//! Sistem tarayıcısında HTTPS URL açma (`xdg-open`).
//!
//! Kabuk kullanılmaz; `xdg-open` ve URL ayrı argüman olarak verilir.
//! Nova yalnızca sürecin başlatıldığını doğrular.

use std::process::{Command, Stdio};

/// `url` adresini varsayılan tarayıcıda açmayı dener (`xdg-open`).
///
/// Yalnızca `https://` kabul edilir (Nova'nın ağ güven sınırıyla uyumlu).
pub fn open_https_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("açılacak adres boş".into());
    }
    if !url.starts_with("https://") {
        return Err(format!(
            "yalnızca https:// adresleri açılır (verilen: {})",
            truncate_for_err(url, 48)
        ));
    }
    Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("xdg-open başlatılamadı: {e}"))?;
    Ok(())
}

/// `owner/repo` veya tam HTTPS GitHub adresini tarayıcı URL'sine çevirir.
pub fn github_repo_https_url(repo: &str) -> Option<String> {
    let repo = repo.trim();
    if repo.is_empty() {
        return None;
    }
    if repo.starts_with("https://") {
        return Some(repo.to_string());
    }
    if repo.starts_with("http://") {
        return None;
    }
    // "owner/repo" kısa biçimi
    if repo.contains('/') && !repo.contains(' ') {
        return Some(format!("https://github.com/{repo}"));
    }
    None
}

fn truncate_for_err(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_https() {
        assert!(open_https_url("http://example.com").is_err());
        assert!(open_https_url("ftp://example.com").is_err());
        assert!(open_https_url("").is_err());
    }

    #[test]
    fn github_short_form_becomes_https() {
        assert_eq!(
            github_repo_https_url("jesseduffield/lazygit").as_deref(),
            Some("https://github.com/jesseduffield/lazygit")
        );
        assert_eq!(
            github_repo_https_url("https://github.com/a/b").as_deref(),
            Some("https://github.com/a/b")
        );
        assert!(github_repo_https_url("http://evil.example").is_none());
    }
}
