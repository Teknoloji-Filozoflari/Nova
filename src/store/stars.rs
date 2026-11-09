//! Mağaza / Kurulu detay için GitHub yıldız sayısı.
//!
//! Yalnızca **doğrulanmış** depo adreslerinden okunur:
//! - `release.repo` (`owner/repo` veya `https://github.com/owner/repo`)
//! - isteğe bağlı `repository_url` (yalnızca `https://github.com/owner/repo…`)
//!
//! `source` tek başına yıldız için kullanılmaz (proje sitesi ≠ GitHub deposu).
//! İstekler UI iş parçacığı dışında yapılır; oturum boyunca önbelleğe alınır.
//! `GITHUB_TOKEN` varsa kimlikli istek atılır ama **ekrana yazılmaz**.

use crate::store::StoreEntry;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Oturum içi yıldız önbelleği anahtarı: `owner/repo` (küçük harf owner/repo).
pub type StarsRepoKey = String;

/// Tek bir depo için oturum önbellek kaydı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StarsCacheEntry {
    /// Başarılı sayım.
    Ready(u64),
    /// Ağ / hız sınırı / 404 — oturumda yeniden denenmez (hız sınırı dostu).
    Failed,
}

/// Arka plan yıldız sorgusu sonucu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarsFetchOutcome {
    /// `owner/repo` anahtarı.
    pub key: StarsRepoKey,
    /// Sayı veya hata (hata metni UI’da token içermemelidir).
    pub result: Result<u64, String>,
}

/// Doğrulanmış GitHub deposu (`owner` / `name`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedGithubRepo {
    pub owner: String,
    pub name: String,
}

impl VerifiedGithubRepo {
    /// Önbellek / API yolu anahtarı.
    pub fn key(&self) -> StarsRepoKey {
        format!("{}/{}", self.owner, self.name)
    }

    /// Tarayıcıda açılabilir HTTPS adres.
    pub fn https_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }
}

/// `owner/repo` veya `https://github.com/owner/repo[/…]` → doğrulanmış depo.
///
/// GitHub dışı host’lar ve eksik parçalar reddedilir. Yolun yalnızca
/// `owner/repo` (isteğe bağlı trailing slash / `.git`) kısmı alınır.
pub fn parse_verified_github_repo(raw: &str) -> Option<VerifiedGithubRepo> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let path = if let Some(rest) = raw.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = raw.strip_prefix("http://github.com/") {
        // Yıldız API’si için kabul edilmez; HTTPS zorunlu.
        let _ = rest;
        return None;
    } else if raw.starts_with("https://") || raw.starts_with("http://") {
        return None;
    } else {
        raw
    };

    let path = path.trim_start_matches('/').trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let owner = parts.next()?.trim();
    let name = parts.next()?.trim();
    // Fazladan yol (issues, tree, …) varsa yine owner/repo yeterli — kabul.
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    if owner.contains(' ') || name.contains(' ') {
        return None;
    }
    if !owner
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return None;
    }
    Some(VerifiedGithubRepo {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

/// Mağaza kaydından yıldız için doğrulanmış depo.
///
/// Öncelik: `release.repo` → `repository_url`. `source` kullanılmaz.
pub fn verified_repo_for_store_entry(entry: &StoreEntry) -> Option<VerifiedGithubRepo> {
    if let Some(release) = &entry.release {
        if let Some(repo) = parse_verified_github_repo(&release.repo) {
            return Some(repo);
        }
    }
    entry
        .repository_url
        .as_deref()
        .and_then(parse_verified_github_repo)
}

/// Kurulu araç detayı: özel `repo_url` veya güvenilir Mağaza eşleşmesi.
///
/// Özel `repo_url` yalnızca GitHub ise kabul edilir. Mağaza tarafında
/// `verified_repo_for_store_entry` kuralları geçerlidir.
pub fn verified_repo_for_tool(
    tool_repo_url: Option<&str>,
    store: Option<&StoreEntry>,
) -> Option<VerifiedGithubRepo> {
    if let Some(url) = tool_repo_url {
        if let Some(repo) = parse_verified_github_repo(url) {
            return Some(repo);
        }
    }
    store.and_then(verified_repo_for_store_entry)
}

#[derive(Debug, Deserialize)]
struct RepoApiResponse {
    stargazers_count: u64,
}

/// GitHub depo yıldız sayısını çeker (`GET /repos/{owner}/{repo}`).
///
/// `GITHUB_TOKEN` ortam değişkeni varsa `Authorization` eklenir; değer
/// hiçbir yerde loglanmaz / döndürülmez.
pub fn fetch_stargazers_count(owner: &str, name: &str) -> Result<u64, String> {
    let repo = VerifiedGithubRepo {
        owner: owner.to_string(),
        name: name.to_string(),
    };
    // Yeniden doğrula (çağıran güvenilir olsa da).
    let parsed = parse_verified_github_repo(&repo.key())
        .ok_or_else(|| "geçersiz GitHub deposu".to_string())?;
    let url = format!(
        "https://api.github.com/repos/{}/{}",
        parsed.owner, parsed.name
    );
    crate::store::net::require_https(&url)?;

    let agent = crate::store::net::secure_agent();
    let mut request = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "Nova-desktop-store-stars")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
    }

    let mut response = request
        .call()
        .map_err(|_| "GitHub API isteği başarısız".to_string())?;
    let status = response.status().as_u16();
    if status == 403 {
        let body = response
            .body_mut()
            .with_config()
            .limit(crate::store::net::MAX_TEXT_BYTES)
            .read_to_string()
            .unwrap_or_default();
        if body.contains("rate limit") {
            return Err("GitHub API hız sınırına ulaşıldı".into());
        }
        return Err("GitHub API 403".into());
    }
    if status == 404 {
        return Err("depo bulunamadı".into());
    }
    if !(200..300).contains(&status) {
        return Err(format!("GitHub API {status}"));
    }
    let text = response
        .body_mut()
        .with_config()
        .limit(crate::store::net::MAX_TEXT_BYTES)
        .read_to_string()
        .map_err(|_| "GitHub API yanıtı okunamadı".to_string())?;
    let parsed: RepoApiResponse =
        serde_json::from_str(&text).map_err(|_| "GitHub API yanıtı ayrıştırılamadı".to_string())?;
    Ok(parsed.stargazers_count)
}

/// Önbelleğe yazar; başarı ve kalıcı hatalar oturumda tutulur.
pub fn cache_outcome(
    cache: &mut BTreeMap<StarsRepoKey, StarsCacheEntry>,
    outcome: StarsFetchOutcome,
) {
    let entry = match outcome.result {
        Ok(n) => StarsCacheEntry::Ready(n),
        Err(_) => StarsCacheEntry::Failed,
    };
    cache.insert(outcome.key, entry);
}

/// UI satırı için yıldız metni. Token / ham hata yok.
pub fn stars_display_line(cache: &BTreeMap<StarsRepoKey, StarsCacheEntry>, key: &str) -> String {
    match cache.get(key) {
        Some(StarsCacheEntry::Ready(n)) => format!("Yıldız: {n}"),
        Some(StarsCacheEntry::Failed) => "Yıldız: —".into(),
        None => "Yıldız: …".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InstallMethod;

    fn entry_with_release(repo: &str) -> StoreEntry {
        StoreEntry {
            id: "x".into(),
            name: "x".into(),
            description: "d".into(),
            category: "Git".into(),
            source: "https://example.org/".into(),
            license: "MIT".into(),
            executable: "x".into(),
            install_method: InstallMethod::Manual,
            supported_platforms: vec!["linux-x64".into()],
            checksum: None,
            checksum_type: None,
            version: None,
            download_url: None,
            artifacts: vec![],
            install_command: None,
            install_notes: Some("n".into()),
            release: Some(crate::store::manifest::ReleaseSource {
                repo: repo.into(),
                tag_prefix: None,
                asset_pattern: "a".into(),
                checksum_asset_pattern: None,
            }),
            repository_url: None,
            signature: None,
        }
    }

    #[test]
    fn parses_owner_repo_and_https() {
        assert_eq!(
            parse_verified_github_repo("jesseduffield/lazygit")
                .unwrap()
                .key(),
            "jesseduffield/lazygit"
        );
        assert_eq!(
            parse_verified_github_repo("https://github.com/BurntSushi/ripgrep")
                .unwrap()
                .key(),
            "BurntSushi/ripgrep"
        );
        assert_eq!(
            parse_verified_github_repo("https://github.com/BurntSushi/ripgrep.git")
                .unwrap()
                .key(),
            "BurntSushi/ripgrep"
        );
        assert_eq!(
            parse_verified_github_repo("https://github.com/a/b/issues/1")
                .unwrap()
                .key(),
            "a/b"
        );
    }

    #[test]
    fn rejects_non_github_and_http() {
        assert!(parse_verified_github_repo("https://gitlab.com/a/b").is_none());
        assert!(parse_verified_github_repo("http://github.com/a/b").is_none());
        assert!(parse_verified_github_repo("https://example.org/a/b").is_none());
        assert!(parse_verified_github_repo("").is_none());
        assert!(parse_verified_github_repo("onlyone").is_none());
    }

    #[test]
    fn store_prefers_release_repo_over_repository_url() {
        let mut e = entry_with_release("owner/from-release");
        e.repository_url = Some("https://github.com/owner/from-url".into());
        assert_eq!(
            verified_repo_for_store_entry(&e).unwrap().key(),
            "owner/from-release"
        );
    }

    #[test]
    fn store_falls_back_to_repository_url() {
        let mut e = entry_with_release("bad");
        e.release = None;
        e.repository_url = Some("https://github.com/foo/bar".into());
        assert_eq!(verified_repo_for_store_entry(&e).unwrap().key(), "foo/bar");
        // source GitHub olsa bile tek başına yetmez
        e.repository_url = None;
        e.source = "https://github.com/foo/bar".into();
        assert!(verified_repo_for_store_entry(&e).is_none());
    }

    #[test]
    fn display_never_embeds_tokenish_errors() {
        let mut cache = BTreeMap::new();
        assert_eq!(stars_display_line(&cache, "a/b"), "Yıldız: …");
        cache.insert("a/b".into(), StarsCacheEntry::Ready(42));
        assert_eq!(stars_display_line(&cache, "a/b"), "Yıldız: 42");
        cache.insert("a/b".into(), StarsCacheEntry::Failed);
        assert_eq!(stars_display_line(&cache, "a/b"), "Yıldız: —");
    }
}
