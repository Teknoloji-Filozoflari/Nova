//! Yerel (native) CLI kurulumları — npm isteğe bağlı bağımlılık tuzağından kaçınır.
//!
//! Özellikle Anthropic Claude Code: resmî yol `downloads.claude.ai` üzerinden
//! platform ikilisini SHA-256 ile indirip `~/.local/bin` altına yerleştirmektir.
//! npm paketindeki `claude` yer tutucu betik, postinstall / optional-deps
//! atlanırsa `Error: claude native binary not installed` veya yanlış mimari
//! dosyasında `Exec format error` üretir; Nova bu yolu tercih etmez.
//!
//! Risk notu: CDN sürümü dinamiktir (`/latest` + `manifest.json`). İndirme
//! büyük (~300 MB+); ağ/kesinti kırılganlığı vardır. Checksum doğrulanır;
//! `curl | bash` çalıştırılmaz.

use crate::store::http::HttpDownloader;
use crate::store::install::{Downloader, InstallError, InstallOutcome};
use crate::store::net::{self, MAX_TEXT_BYTES};
use crate::store::path::{self, looks_like_runnable_cli};
use crate::store::platform::CpuArch;
use crate::store::StoreEntry;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

const CLAUDE_CDN_BASE: &str = "https://downloads.claude.ai/claude-code-releases";
const ANTHROPIC_NPM_PKG: &str = "@anthropic-ai/claude-code";

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

    // Eski kırık npm shim’ini temizle; sonra gerçek ikiliye bağla.
    let _ = path::remove_path_shim("claude");
    let mut executable_path = final_path.clone();
    let mut summary = format!(
        "'{}' kuruldu (Anthropic native CDN {platform} v{version})",
        entry.name
    );
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

    #[test]
    fn detects_claude_catalog_shapes() {
        let mut e = StoreEntry {
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
        };
        assert!(is_claude_code(&e));
        e.id = "other".into();
        e.executable = "other".into();
        assert!(is_claude_code(&e), "npm paket adından tanımalı");
        e.install_command = Some(vec![
            "npm".into(),
            "install".into(),
            "-g".into(),
            "fx".into(),
        ]);
        assert!(!is_claude_code(&e));
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
}
