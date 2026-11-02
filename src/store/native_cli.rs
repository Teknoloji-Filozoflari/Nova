//! Yerel (native) CLI kurulumları — npm isteğe bağlı bağımlılık tuzağından kaçınır.
//!
//! Özellikle Anthropic Claude Code: resmî yol `downloads.claude.ai` üzerinden
//! platform ikilisini SHA-256 ile indirip `~/.local/bin` altına yerleştirmektir.
//! npm paketindeki `claude` yer tutucu betik, postinstall / optional-deps
//! atlanırsa `Error: claude native binary not installed` veya yanlış mimari
//! dosyasında `Exec format error` üretir; Nova **asla** npm’e düşmez.
//!
//! Risk notu: CDN sürümü dinamiktir (`/latest` + `manifest.json`). İndirme
//! büyük (~300 MB+); ağ/kesinti kırılganlığı vardır. Checksum doğrulanır;
//! `curl | bash` çalıştırılmaz.

use crate::store::http::HttpDownloader;
use crate::store::install::{Downloader, InstallError, InstallOutcome};
use crate::store::net::{self, MAX_TEXT_BYTES};
use crate::store::path::{self, is_broken_cli_placeholder, looks_like_runnable_cli};
use crate::store::platform::CpuArch;
use crate::store::StoreEntry;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const CLAUDE_CDN_BASE: &str = "https://downloads.claude.ai/claude-code-releases";
const ANTHROPIC_NPM_PKG: &str = "@anthropic-ai/claude-code";

/// Paket yöneticisi yerine zorunlu native CDN kurulumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeInstallKind {
    /// Anthropic `downloads.claude.ai` + manifest SHA-256.
    ClaudeCdn,
}

/// Bu kayıt Anthropic Claude Code mı (katalog id / npm paketi / executable)?
pub fn is_claude_code(entry: &StoreEntry) -> bool {
    if entry.id.eq_ignore_ascii_case("claude") || entry.executable.eq_ignore_ascii_case("claude") {
        return true;
    }
    entry
        .install_command
        .as_ref()
        .is_some_and(|argv| argv.iter().any(|a| a == ANTHROPIC_NPM_PKG))
}

/// Claude (ve benzeri) için npm/PM yedek yolu **yok**; yalnızca native CDN.
pub fn native_install_kind(entry: &StoreEntry) -> Option<NativeInstallKind> {
    if is_claude_code(entry) {
        Some(NativeInstallKind::ClaudeCdn)
    } else {
        None
    }
}

/// Host için Claude CDN platform etiketi (`linux-x64`, `linux-arm64`, …-musl).
pub fn claude_platform_tag() -> Result<&'static str, InstallError> {
    let arch = CpuArch::host().ok_or_else(|| {
        InstallError::Security(format!(
            "desteklenmeyen CPU mimarisi: {}",
            std::env::consts::ARCH
        ))
    })?;
    let musl = host_is_musl();
    Ok(match (arch, musl) {
        (CpuArch::X86_64, false) => "linux-x64",
        (CpuArch::X86_64, true) => "linux-x64-musl",
        (CpuArch::Aarch64, false) => "linux-arm64",
        (CpuArch::Aarch64, true) => "linux-arm64-musl",
    })
}

fn host_is_musl() -> bool {
    // ldd çıktısı veya /lib linker adı; yoksa glibc varsay.
    if let Ok(out) = std::process::Command::new("ldd").arg("--version").output() {
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        if text.to_ascii_lowercase().contains("musl") {
            return true;
        }
    }
    Path::new("/lib/ld-musl-x86_64.so.1").exists()
        || Path::new("/lib/ld-musl-aarch64.so.1").exists()
}

/// Resmî Anthropic CDN’den Claude Code ikilisini kurar (kabuksuz).
pub fn install_claude_native(entry: &StoreEntry) -> Result<InstallOutcome, InstallError> {
    let dest = crate::store::store_app_dir(&entry.id).ok_or_else(|| {
        InstallError::Internal("kurulum dizini belirlenemedi ($HOME bulunamadı)".into())
    })?;
    let downloader = HttpDownloader::new();
    install_claude_native_with(&downloader, entry, &dest)
}

/// Test edilebilir kurulum (indirici enjekte edilir).
pub fn install_claude_native_with(
    downloader: &dyn Downloader,
    entry: &StoreEntry,
    dest_dir: &Path,
) -> Result<InstallOutcome, InstallError> {
    if !is_claude_code(entry) {
        return Err(InstallError::Internal(
            "claude native kurulum yalnızca Claude Code için".into(),
        ));
    }
    if !crate::store::install::is_user_area(dest_dir) {
        return Err(InstallError::Security(format!(
            "hedef dizin kullanıcı alanı değil: '{}'",
            dest_dir.display()
        )));
    }

    let platform = claude_platform_tag()?;
    let version = fetch_cdn_text(&format!("{CLAUDE_CDN_BASE}/latest"))?
        .trim()
        .to_string();
    if version.is_empty()
        || version.contains('<')
        || version.chars().any(|c| c.is_whitespace())
        || version.len() > 64
    {
        return Err(InstallError::Download(format!(
            "Claude CDN sürüm yanıtı geçersiz: '{version}'"
        )));
    }

    let manifest_url = format!("{CLAUDE_CDN_BASE}/{version}/manifest.json");
    let manifest = fetch_cdn_text(&manifest_url)?;
    let checksum = checksum_for_platform(&manifest, platform).ok_or_else(|| {
        InstallError::Download(format!(
            "Claude manifest’te '{platform}' platformu yok (sürüm {version})"
        ))
    })?;

    let work = dest_dir.join(".nova-work");
    let bin_dir = dest_dir.join("bin");
    fs::create_dir_all(&work).map_err(|e| {
        InstallError::Permission(format!("'{}' oluşturulamadı: {e}", work.display()))
    })?;
    fs::create_dir_all(&bin_dir).map_err(|e| {
        InstallError::Permission(format!("'{}' oluşturulamadı: {e}", bin_dir.display()))
    })?;

    let staged = work.join(format!("claude-{version}-{platform}"));
    let url = format!("{CLAUDE_CDN_BASE}/{version}/{platform}/claude");
    let downloaded = downloader.download(&url, &staged)?;
    verify_sha256(&downloaded, &checksum)?;

    if !looks_like_runnable_cli(&downloaded) {
        let _ = fs::remove_file(&downloaded);
        return Err(InstallError::Download(
            "Claude CDN dosyası çalıştırılabilir ikili gibi görünmüyor (yanlış mimari veya bozuk indirme)"
                .into(),
        ));
    }

    let final_path = bin_dir.join("claude");
    let _ = fs::remove_file(&final_path);
    fs::copy(&downloaded, &final_path).map_err(|e| {
        InstallError::Permission(format!("'{}' kopyalanamadı: {e}", final_path.display()))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&final_path, fs::Permissions::from_mode(0o755)).map_err(|e| {
            InstallError::Permission(format!(
                "'{}' çalıştırılabilir yapılamadı: {e}",
                final_path.display()
            ))
        })?;
    }
    let _ = fs::remove_file(&downloaded);
    let _ = fs::remove_dir_all(&work);

    // npm-global / eski yer tutucu kalıntılarını yok say / sil; sonra gerçek ikiliye bağla.
    let purged = purge_broken_cli_residuals("claude");
    let _ = path::remove_path_shim("claude");
    let mut executable_path = final_path.clone();
    let mut summary = format!(
        "'{}' kuruldu (Anthropic native CDN {platform} v{version})",
        entry.name
    );
    if purged > 0 {
        summary.push_str(&format!(" · {purged} kırık npm/yer tutucu temizlendi"));
    }
    match path::shim_installed_executable("claude", &final_path) {
        Ok(shim) => {
            executable_path = shim;
            summary.push_str(" · PATH: ~/.local/bin");
        }
        Err(err) => summary.push_str(&format!(" · PATH uyarısı: {err}")),
    }

    Ok(InstallOutcome {
        app_id: entry.id.clone(),
        app_name: entry.name.clone(),
        version,
        executable_path,
        install_dir: dest_dir.to_path_buf(),
        summary,
    })
}

/// Kullanıcı bin’lerindeki kırık yer tutucu / yanlış mimari `exe` dosyalarını siler.
///
/// Dönüş: silinen dosya sayısı. Gerçek ELF/Mach-O veya sağlam shebang’e dokunulmaz.
pub fn purge_broken_cli_residuals(exe_name: &str) -> usize {
    let mut removed = 0usize;
    for path in residual_cli_candidates(exe_name) {
        if !path.exists() {
            continue;
        }
        // Symlink hedefi yer tutucuysa bağı (ve gerekirse hedefi) kaldır.
        let check = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if is_broken_cli_placeholder(&check) || is_broken_cli_placeholder(&path) {
            if fs::remove_file(&path).is_ok() {
                removed += 1;
            }
            if check != path && is_broken_cli_placeholder(&check) {
                let _ = fs::remove_file(&check);
            }
        }
    }
    removed
}

fn residual_cli_candidates(exe_name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) else {
        return out;
    };
    let home = PathBuf::from(home);
    out.push(home.join(".local").join("bin").join(exe_name));
    out.push(home.join(".npm-global").join("bin").join(exe_name));
    out.push(home.join("bin").join(exe_name));
    // npm önekleri bazen farklı (prefix -g).
    if let Ok(prefix) = std::env::var("npm_config_prefix") {
        if !prefix.is_empty() {
            out.push(PathBuf::from(prefix).join("bin").join(exe_name));
        }
    }
    out
}

fn fetch_cdn_text(url: &str) -> Result<String, InstallError> {
    net::require_https(url).map_err(InstallError::Security)?;
    let agent = net::secure_agent();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| InstallError::Network(format!("'{url}' alınamadı: {e}")))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(InstallError::Network(format!(
            "'{url}' sunucusu {status} yanıtı verdi"
        )));
    }
    let mut reader = response.body_mut().as_reader();
    let mut buf = Vec::new();
    reader
        .by_ref()
        .take(MAX_TEXT_BYTES.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| InstallError::Network(format!("'{url}' okunamadı: {e}")))?;
    if buf.len() as u64 > MAX_TEXT_BYTES {
        return Err(InstallError::Download(format!(
            "'{url}' metin yanıtı çok büyük"
        )));
    }
    String::from_utf8(buf).map_err(|e| InstallError::Download(format!("'{url}' UTF-8 değil: {e}")))
}

/// Manifest JSON içinden platform checksum’unu çeker (jq gerekmez).
pub fn checksum_for_platform(manifest: &str, platform: &str) -> Option<String> {
    // "linux-x64": { ... "checksum": "<64 hex>" ... }
    let key = format!("\"{platform}\"");
    let start = manifest.find(&key)?;
    let region = &manifest[start..];
    let end = region.find('}').unwrap_or(region.len().min(800));
    let region = &region[..end];
    let marker = "\"checksum\"";
    let cpos = region.find(marker)?;
    let after = &region[cpos + marker.len()..];
    let q1 = after.find('"')?;
    let rest = &after[q1 + 1..];
    let q2 = rest.find('"')?;
    let hex = &rest[..q2];
    if crate::store::is_sha256_hex(hex) {
        Some(hex.to_ascii_lowercase())
    } else {
        None
    }
}

fn verify_sha256(path: &Path, expected: &str) -> Result<(), InstallError> {
    let mut file = fs::File::open(path)
        .map_err(|e| InstallError::Download(format!("'{}' açılamadı: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| InstallError::Download(format!("'{}' okunamadı: {e}", path.display())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected.to_ascii_lowercase() {
        return Err(InstallError::Download(format!(
            "SHA-256 uyuşmazlığı: beklenen {expected}, alınan {actual}"
        )));
    }
    Ok(())
}

/// npm kurulumundan sonra kırık yer tutucu / yanlış mimariyi reddeder.
pub fn reject_broken_npm_cli(exe_name: &str, path: &Path) -> Result<(), InstallError> {
    if looks_like_runnable_cli(path) {
        return Ok(());
    }
    let _ = path::remove_path_shim(exe_name);
    Err(InstallError::Internal(format!(
        "'{exe_name}' kurulumu kırık veya yer tutucu bıraktı (npm postinstall / optional deps?). \
         Kaynak: {}. Kırık ~/.local/bin shim temizlendi. Claude için Anthropic native CDN yolu tercih edilir.",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn sample_claude_entry() -> StoreEntry {
        StoreEntry {
            id: "claude".into(),
            name: "Claude".into(),
            description: "d".into(),
            category: "AI".into(),
            source: "https://code.claude.com".into(),
            license: "Proprietary".into(),
            executable: "claude".into(),
            install_method: crate::store::InstallMethod::Npm,
            supported_platforms: vec!["linux-x64".into()],
            version: None,
            download_url: None,
            checksum: None,
            checksum_type: None,
            artifacts: vec![],
            install_command: Some(vec![
                "npm".into(),
                "install".into(),
                "-g".into(),
                ANTHROPIC_NPM_PKG.into(),
            ]),
            install_notes: None,
            release: None,
            signature: None,
        }
    }

    fn with_temp_home<F: FnOnce(&Path)>(f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "nova-native-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let old_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &dir);
        f(&dir);
        match old_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_claude_catalog_shapes() {
        let mut e = sample_claude_entry();
        assert!(is_claude_code(&e));
        assert_eq!(native_install_kind(&e), Some(NativeInstallKind::ClaudeCdn));
        e.id = "other".into();
        e.executable = "other".into();
        assert!(is_claude_code(&e), "npm paket adından tanımalı");
        assert_eq!(native_install_kind(&e), Some(NativeInstallKind::ClaudeCdn));
        e.install_command = Some(vec![
            "npm".into(),
            "install".into(),
            "-g".into(),
            "fx".into(),
        ]);
        assert!(!is_claude_code(&e));
        assert_eq!(native_install_kind(&e), None);
    }

    #[test]
    fn claude_never_uses_npm_fallback_strategy() {
        // Kurulum yönlendirmesi: Claude → yalnızca CDN; npm argv’ye düşülmez.
        let e = sample_claude_entry();
        assert!(
            native_install_kind(&e).is_some(),
            "Claude native CDN zorunlu olmalı"
        );
        // Katalog hâlâ npm method gösterebilir; strateji yine CDN.
        assert_eq!(e.install_method, crate::store::InstallMethod::Npm);
    }

    #[test]
    fn parses_checksum_from_manifest_snippet() {
        let manifest = r#"{
  "version": "2.1.316",
  "platforms": {
    "linux-x64": {
      "binary": "claude",
      "checksum": "fd13c70000fec3f2116dfbcbb6121210b4a29328bf0e46210a59dd594a67d824",
      "size": 328262000
    },
    "linux-arm64": {
      "binary": "claude",
      "checksum": "55cdb8e162d190432696ea54010e0fca2e18d06fa7a7127fd563b9ffd4f22624",
      "size": 1
    }
  }
}"#;
        assert_eq!(
            checksum_for_platform(manifest, "linux-x64").as_deref(),
            Some("fd13c70000fec3f2116dfbcbb6121210b4a29328bf0e46210a59dd594a67d824")
        );
        assert_eq!(
            checksum_for_platform(manifest, "linux-arm64").as_deref(),
            Some("55cdb8e162d190432696ea54010e0fca2e18d06fa7a7127fd563b9ffd4f22624")
        );
        assert!(checksum_for_platform(manifest, "win32-x64").is_none());
    }

    #[test]
    fn platform_tag_is_linux_family() {
        let tag = claude_platform_tag().expect("ci linux");
        assert!(
            tag.starts_with("linux-"),
            "beklenen linux-* etiketi, got {tag}"
        );
    }

    #[test]
    fn detects_placeholder_vs_real_binary_and_purges_npm_residual() {
        with_temp_home(|home| {
            let npm_bin = home.join(".npm-global").join("bin");
            fs::create_dir_all(&npm_bin).unwrap();
            let placeholder = npm_bin.join("claude");
            fs::write(
                &placeholder,
                b"#!/usr/bin/env node\nthrow new Error('claude native binary not installed');\n",
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&placeholder, fs::Permissions::from_mode(0o755)).unwrap();
            }
            assert!(is_broken_cli_placeholder(&placeholder));
            assert!(!looks_like_runnable_cli(&placeholder));

            let real_dir = home
                .join(".local")
                .join("share")
                .join("nova")
                .join("store")
                .join("claude")
                .join("bin");
            fs::create_dir_all(&real_dir).unwrap();
            let real = real_dir.join("claude");
            let mut elf = vec![0x7f, b'E', b'L', b'F'];
            elf.extend_from_slice(&[2, 1, 1, 0]);
            elf.extend_from_slice(&[0; 56]);
            fs::write(&real, &elf).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&real, fs::Permissions::from_mode(0o755)).unwrap();
            }
            assert!(!is_broken_cli_placeholder(&real));
            assert!(looks_like_runnable_cli(&real));

            let n = purge_broken_cli_residuals("claude");
            assert!(n >= 1, "yer tutucu silinmeliydi");
            assert!(!placeholder.exists(), "npm-global yer tutucu kalmamalı");
            assert!(real.exists(), "gerçek ELF silinmemeli");

            let err = reject_broken_npm_cli("claude", &real);
            assert!(err.is_ok(), "ELF reddedilmemeli");
        });
    }

    #[test]
    fn reject_broken_npm_cli_clears_local_shim_on_placeholder() {
        with_temp_home(|home| {
            let npm = home.join(".npm-global").join("bin");
            fs::create_dir_all(&npm).unwrap();
            let bad = npm.join("claude");
            fs::write(
                &bad,
                b"#!/usr/bin/env node\nError: claude native binary not installed.\n",
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&bad, fs::Permissions::from_mode(0o755)).unwrap();
            }
            let local = home.join(".local").join("bin");
            fs::create_dir_all(&local).unwrap();
            let shim = local.join("claude");
            #[cfg(unix)]
            std::os::unix::fs::symlink(&bad, &shim).unwrap();
            #[cfg(not(unix))]
            fs::copy(&bad, &shim).unwrap();

            let err = reject_broken_npm_cli("claude", &bad).unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains("yer tutucu") || msg.contains("kırık"), "{msg}");
            assert!(msg.contains("Anthropic native CDN"), "{msg}");
        });
    }
}
