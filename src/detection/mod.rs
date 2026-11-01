//! Kurulu komutları tespit etme modülü.
//! PATH üzerinden çalıştırılabilir dosyaların varlığını kontrol eder.
//!
//! Üç sonuç döndürür: kurulu, kurulu değil ve kontrol edilemedi.
//! "Kontrol edilemedi", komut adının geçersiz olduğu veya PATH bilgisinin
//! kullanılamadığı durumları ifade eder; bu ayrım kullanıcıya yanlış
//! "kurulu değil" bilgisi vermemek için gereklidir.

use std::io::ErrorKind;
use std::path::PathBuf;

/// Bir komutun kurulu olma durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstalledStatus {
    /// Komut PATH üzerinde bulundu.
    Installed,
    /// Komut PATH üzerinde yok.
    Missing,
    /// Varlık kontrolü yapılamadı (geçersiz ad, boş PATH, yetki hatası...).
    #[default]
    Unknown,
}

impl InstalledStatus {
    /// Arayüzde gösterilecek kısa işaret.
    pub fn symbol(self) -> &'static str {
        match self {
            InstalledStatus::Installed => "✓",
            InstalledStatus::Missing => "✗",
            InstalledStatus::Unknown => "?",
        }
    }

    /// Durumun Türkçe açıklaması.
    pub fn label(self) -> &'static str {
        match self {
            InstalledStatus::Installed => "Kurulu",
            InstalledStatus::Missing => "Kurulu değil",
            InstalledStatus::Unknown => "Kontrol edilemedi",
        }
    }
}

/// Komutun durumunu ve PATH üzerinde çözümlenen yolunu döner.
pub fn resolve(cmd: &str) -> (InstalledStatus, Option<PathBuf>) {
    let trimmed = cmd.trim();
    // Yalnızca boşluk, boşluk içeren veya boş olan giriş bir komut adı değildir.
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return (InstalledStatus::Unknown, None);
    }

    // `~/yol` biçimindeki girdiler ev dizinine göre çözülür. PATH'te olmayan
    // bir dosya yolu (`~/.minimax-code/bin/mcode` gibi) için de bu genişletme
    // gerekir; `~` yoksa veya `HOME` tanımlıysa orijinal hâl korunur.
    let expanded = if let Some(rest) = trimmed.strip_prefix("~/") {
        match std::env::var_os("HOME") {
            Some(home) if !home.is_empty() => PathBuf::from(home).join(rest),
            _ => PathBuf::from(rest),
        }
    } else if trimmed == "~" {
        std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .unwrap_or_default()
    } else {
        PathBuf::from(trimmed)
    };

    // Komut doğrudan bir yol ise (içinde '/' var) varlığına bak.
    if expanded.as_os_str().to_string_lossy().contains('/') {
        let path = &expanded;
        return match std::fs::metadata(path) {
            Ok(meta) if is_executable(&meta) => {
                (InstalledStatus::Installed, Some(path.to_path_buf()))
            }
            Ok(_) => (InstalledStatus::Missing, None),
            Err(err) if err.kind() == ErrorKind::NotFound => (InstalledStatus::Missing, None),
            Err(_) => (InstalledStatus::Unknown, None),
        };
    }

    // PATH hiç tanımlı değilse arama yapılamaz; bu "kurulu değil" değildir.
    if let Ok(path_var) = std::env::var("PATH") {
        if path_var.trim().is_empty() {
            return (InstalledStatus::Unknown, None);
        }
    } else {
        return (InstalledStatus::Unknown, None);
    }

    match which::which(trimmed) {
        Ok(path) => (InstalledStatus::Installed, Some(path)),
        Err(which::Error::CannotFindBinaryPath) => {
            // PATH'te yoksa Nova mağazasından kurulan uygulamalara bak:
            // mağaza, uygulamayı `<store>/<id>/bin/` altına kurar.
            match resolve_in_store(trimmed) {
                Some(path) => (InstalledStatus::Installed, Some(path)),
                None => (InstalledStatus::Missing, None),
            }
        }
        // Yetki hatası gibi diğer tüm durumlar "kontrol edilemedi" sayılır.
        Err(_) => (InstalledStatus::Unknown, None),
    }
}

/// Mağazadan kurulan bir uygulamanın çalıştırılabilir dosyasını bulur.
///
/// Nova mağaza uygulamalarını `$XDG_DATA_HOME/nova/store/<id>/bin/` altına
/// kurar. Bu dizin PATH'te olmadığı için normal PATH araması onları bulamaz;
/// burada mağaza dizini taranır.
fn resolve_in_store(cmd: &str) -> Option<PathBuf> {
    let store_dir = crate::store::store_install_dir()?;
    let entries = std::fs::read_dir(&store_dir).ok()?;
    for entry in entries.flatten() {
        let candidate = entry.path().join("bin").join(cmd);
        if let Ok(meta) = std::fs::metadata(&candidate) {
            if is_executable(&meta) {
                return Some(candidate);
            }
        }
    }
    None
}

/// Unix'te çalıştırılabilirlik bitini kontrol eder.
#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.is_file() && meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    meta.is_file()
}

/// Verilen komutun kurulu olup olmadığını döner. "Kontrol edilemedi"
/// durumunu da kesin olumsuz olarak raporlar; kesin ayrım için
/// `resolve` kullanılmalıdır.
pub fn is_installed(cmd: &str) -> bool {
    resolve(cmd).0 == InstalledStatus::Installed
}

/// Birden fazla komutun durumunu döner.
pub fn detect_many<'a>(cmds: &'a [&'a str]) -> Vec<(&'a str, InstalledStatus)> {
    cmds.iter().map(|cmd| (*cmd, resolve(cmd).0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_command_is_unknown() {
        assert_eq!(resolve("").0, InstalledStatus::Unknown);
        assert_eq!(resolve("   ").0, InstalledStatus::Unknown);
    }

    #[test]
    fn command_with_spaces_is_unknown() {
        assert_eq!(resolve("ls -la").0, InstalledStatus::Unknown);
    }

    #[test]
    fn nonexistent_command_is_missing() {
        assert_eq!(
            resolve("bu-comut-kesinlikle-yok-xyz123").0,
            InstalledStatus::Missing
        );
        assert!(!is_installed("bu-comut-kesinlikle-yok-xyz123"));
    }

    #[test]
    fn existing_command_is_installed_on_posix() {
        if cfg!(unix) {
            let (status, path) = resolve("sh");
            if which::which("sh").is_ok() {
                assert_eq!(status, InstalledStatus::Installed);
                assert!(path.is_some());
            }
        }
    }

    #[test]
    fn absolute_path_to_missing_file_is_missing() {
        assert_eq!(
            resolve("/bu/yol/kesinlikle/yok/xyz").0,
            InstalledStatus::Missing
        );
    }

    #[test]
    fn absolute_path_to_non_executable_file_is_missing() {
        let tmp = std::env::temp_dir().join("nova-detection-test.txt");
        std::fs::write(&tmp, "x").expect("test dosyası yazılamadı");
        let result = resolve(tmp.to_str().unwrap()).0;
        std::fs::remove_file(&tmp).ok();
        if cfg!(unix) {
            assert_eq!(result, InstalledStatus::Missing);
        }
    }

    #[test]
    fn store_installed_binary_is_detected() {
        // Mağazadan kurulan uygulamalar PATH'te olmasa da "kurulu" görünmeli.
        let store = crate::store::test_store_dir();
        let app_dir = store.join("test-detect-app");
        let bin = app_dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("nova-test-probe");
        std::fs::write(&exe, b"x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let (status, path) = resolve("nova-test-probe");
        assert_eq!(status, InstalledStatus::Installed);
        assert_eq!(path, Some(exe));
        let _ = std::fs::remove_dir_all(&app_dir);
    }

    #[test]
    fn detect_many_returns_pairs() {
        let result = detect_many(&["sh", "bu-comut-yok-xyz"]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "sh");
        assert_eq!(result[1].1, InstalledStatus::Missing);
    }

    #[test]
    fn symbol_and_label_are_consistent() {
        assert_eq!(InstalledStatus::Installed.symbol(), "✓");
        assert_eq!(InstalledStatus::Missing.label(), "Kurulu değil");
        assert_eq!(InstalledStatus::Unknown.label(), "Kontrol edilemedi");
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .expect("HOME gerekli");
        let test_file = PathBuf::from(home.as_os_str()).join(".nova-tilde-test-file");
        std::fs::write(&test_file, "x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&test_file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let (status, path) = resolve("~/.nova-tilde-test-file");
        assert_eq!(status, InstalledStatus::Installed);
        assert_eq!(path.as_deref(), Some(test_file.as_path()));
        let _ = std::fs::remove_file(&test_file);
    }
}
