//! Mağaza kurulumundan sonra komutların yeni kabuklarda bulunması.
//!
//! Paket yöneticileri (`npm -g`, pipx, cargo, …) ve Nova arşiv kurulumu
//! ikiliyi farklı dizinlere koyabilir. Arch oturumlarında `~/.local/bin`
//! `/etc/profile` ile PATH’e eklenir; bu yüzden kurulum sonrası buraya
//! bir sembolik bağ (shim) yazılır. Böylece kullanıcı `which` / yeni
//! terminalde `qwen` gibi komutları klasör aramadan çalıştırır.

use std::fs;
use std::path::{Path, PathBuf};

/// `$HOME/.local/bin` — kullanıcı alanı PATH hedefi.
pub fn user_local_bin_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".local").join("bin"))
}

/// Mevcut süreç PATH’ine `~/.local/bin` ekler (yoksa öne koyar).
///
/// Tespit ve Nova içinden başlatma, shim yazıldıktan hemen sonra
/// güncel PATH ile çalışsın diye çağrılır. Yeni login kabukları Arch’ta
/// zaten `~/.local/bin` alır.
pub fn ensure_local_bin_on_process_path() {
    let Some(dir) = user_local_bin_dir() else {
        return;
    };
    let Some(dir_s) = dir.to_str().map(str::to_owned) else {
        return;
    };
    let current = std::env::var_os("PATH").unwrap_or_default();
    let already = std::env::split_paths(&current).any(|p| p == dir);
    if already {
        return;
    }
    let current_s = current.to_string_lossy();
    let new_path = if current_s.is_empty() {
        dir_s
    } else {
        format!("{dir_s}:{current_s}")
    };
    std::env::set_var("PATH", new_path);
}

/// `source` ikilisini `~/.local/bin/<exe_name>` altına bağlar.
///
/// - `source` mutlak ve çalıştırılabilir olmalıdır.
/// - Hedef zaten aynı dosyaya işaret ediyorsa dokunulmaz.
/// - Hedef başka bir symlink ise güncellenir; düz dosya ise üzerine
///   yazılmaz (kullanıcı verisi korunur) — o durumda `Ok(hedef)` döner
///   ve süreç PATH’i yine tazelenir.
///
/// Dönüş: shim yolu (veya zaten PATH’te olan kaynak).
pub fn ensure_path_shim(exe_name: &str, source: &Path) -> Result<PathBuf, String> {
    let name = exe_name.trim();
    if name.is_empty() || name.contains('/') || name.contains(char::is_whitespace) {
        return Err(format!("geçersiz executable adı: '{exe_name}'"));
    }
    if !source.is_absolute() {
        return Err(format!(
            "shim için mutlak yol gerekli: '{}'",
            source.display()
        ));
    }
    if !source.is_file() {
        return Err(format!("kaynak ikili yok: '{}'", source.display()));
    }

    let bin_dir = user_local_bin_dir().ok_or_else(|| "HOME bulunamadı".to_string())?;
    fs::create_dir_all(&bin_dir)
        .map_err(|e| format!("'{}' oluşturulamadı: {e}", bin_dir.display()))?;

    let shim = bin_dir.join(name);
    let source_canon = source
        .canonicalize()
        .unwrap_or_else(|_| source.to_path_buf());

    // Kaynak zaten ~/.local/bin altında ise ek bağ gerekmez.
    if source_canon == shim
        || (source_canon
            .parent()
            .is_some_and(|p| p == bin_dir.as_path())
            && source_canon.file_name() == Some(std::ffi::OsStr::new(name)))
    {
        ensure_local_bin_on_process_path();
        return Ok(source_canon);
    }

    match fs::symlink_metadata(&shim) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                let current = fs::read_link(&shim).ok();
                let same = current
                    .as_ref()
                    .and_then(|c| {
                        if c.is_absolute() {
                            c.canonicalize().ok()
                        } else {
                            shim.parent()
                                .unwrap_or(Path::new("."))
                                .join(c)
                                .canonicalize()
                                .ok()
                        }
                    })
                    .is_some_and(|p| p == source_canon);
                if !same {
                    let _ = fs::remove_file(&shim);
                    create_symlink(&source_canon, &shim)?;
                }
            } else {
                // Düz dosya: üzerine yazma; PATH yine de tazelenir.
                ensure_local_bin_on_process_path();
                return Ok(shim);
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            create_symlink(&source_canon, &shim)?;
        }
        Err(err) => {
            return Err(format!("'{}' okunamadı: {err}", shim.display()));
        }
    }

    ensure_local_bin_on_process_path();
    Ok(shim)
}

/// Kurulum sonrası: çözülen ikiliyi shim’le; yol yoksa atla.
///
/// Hata kurulumu bozmaz; çağıran özet metnine not ekleyebilir.
pub fn shim_installed_executable(exe_name: &str, source: &Path) -> Result<PathBuf, String> {
    if !source.is_absolute() || !source.is_file() {
        // PM kurulumu bazen yalnızca adı döner; detection ile yeniden dene.
        if let Some(found) = crate::detection::resolve(exe_name).1 {
            return ensure_path_shim(exe_name, &found);
        }
        return Err(format!(
            "'{exe_name}' için shim yolu bulunamadı (kaynak: {})",
            source.display()
        ));
    }
    ensure_path_shim(exe_name, source)
}

#[cfg(unix)]
fn create_symlink(source: &Path, shim: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(source, shim)
        .map_err(|e| format!("symlink '{}' → '{}': {e}", shim.display(), source.display()))
}

#[cfg(not(unix))]
fn create_symlink(source: &Path, shim: &Path) -> Result<(), String> {
    // Windows’ta symlink izin gerektirebilir; kopya yedek yol.
    fs::copy(source, shim)
        .map(|_| ())
        .map_err(|e| format!("kopya '{}' ← '{}': {e}", shim.display(), source.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home<F: FnOnce(&Path)>(f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "nova-path-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let old_home = std::env::var_os("HOME");
        let old_path = std::env::var_os("PATH");
        std::env::set_var("HOME", &dir);
        // PATH’ten local/bin’i bilerek çıkar; shim sonrası eklendiğini doğrula.
        if let Some(ref p) = old_path {
            let filtered: Vec<_> = std::env::split_paths(p)
                .filter(|x| !x.ends_with(".local/bin") && !x.starts_with(&dir))
                .collect();
            let joined = std::env::join_paths(filtered).unwrap_or_default();
            std::env::set_var("PATH", joined);
        }
        f(&dir);
        match old_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        match old_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shim_creates_local_bin_symlink() {
        with_temp_home(|home| {
            let real_dir = home.join("somewhere/bin");
            fs::create_dir_all(&real_dir).unwrap();
            let real = real_dir.join("qwen");
            fs::write(&real, b"#!/bin/sh\necho ok\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&real, fs::Permissions::from_mode(0o755)).unwrap();
            }
            let shim = ensure_path_shim("qwen", &real).expect("shim");
            assert_eq!(shim, home.join(".local/bin/qwen"));
            assert!(shim.exists());
            #[cfg(unix)]
            {
                let target = fs::read_link(&shim).unwrap();
                assert_eq!(target, real.canonicalize().unwrap());
            }
            let path = std::env::var("PATH").unwrap();
            assert!(
                path.split(':')
                    .any(|p| p == home.join(".local/bin").to_str().unwrap()),
                "PATH={path}"
            );
            // which artık bulmalı (PATH güncellendi).
            assert!(which::which("qwen").is_ok());
        });
    }

    #[test]
    fn shim_is_idempotent_for_same_target() {
        with_temp_home(|home| {
            let real_dir = home.join("pkg/bin");
            fs::create_dir_all(&real_dir).unwrap();
            let real = real_dir.join("gemini");
            fs::write(&real, b"x").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&real, fs::Permissions::from_mode(0o755)).unwrap();
            }
            let a = ensure_path_shim("gemini", &real).unwrap();
            let b = ensure_path_shim("gemini", &real).unwrap();
            assert_eq!(a, b);
        });
    }

    #[test]
    fn shim_rejects_relative_source() {
        with_temp_home(|_| {
            let err = ensure_path_shim("x", Path::new("relative/x")).unwrap_err();
            assert!(err.contains("mutlak"), "{err}");
        });
    }
}
