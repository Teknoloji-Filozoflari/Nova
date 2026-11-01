//! GitHub Releases tabanlı canlı sürüm çözümleyici.
//!
//! Bu modül "uzaktan gelen katalog" **değildir**. Yalnızca iki soruyu
//! yanıtlar:
//!
//! 1. "Bu deponun en güncel yayını hangisi?" → GitHub Releases API.
//! 2. "Bu yayının doğrulama özeti ne?" → **yayının kendi** checksum dosyası
//!    (ya da yayın notları).
//!
//! Depo adı gömülü katalogda sabit olduğu için, ağdan gelen veri yalnızca
//! sürüm ipucu verir; çalıştırılacak dosyanın bütünlüğü upstream'in
//! yayımladığı SHA-256 özetiyle korunur.
//!
//! # Hız sınırı
//!
//! `api.github.com` kimliksiz çağrılarda IP başına saatte 60 istekle
//! sınırlıdır. Bu yüzden:
//!
//! - Çözümlemeler 6 saatlik önbellekte tutulur; **taze kayıt API'ye hiç
//!   gitmez**. Sınır korumasının asıl kaynağı budur.
//! - Yayın sorgusu yine de `ETag` ile **koşullu** atılır; `304 Not Modified`
//!   döndüğünde gövde indirilmez (bant genişliği tasarrufu). Ancak ölçüldüğü
//!   kadarıyla `304` yanıtları da sınırdan düşer, yani tekrarlar ücretsiz
//!   sayılmaz.
//! - `GITHUB_TOKEN` ortam değişkeni verilirse istekler kimlikli yapılır ve
//!   sınır saatte 5000'e yükselir.
//! - Checksum dosyası `github.com` üzerinden **indirilir**; bu indirmeler API
//!   sınırından sayılmaz.

use crate::store::manifest::{
    parse_checksum_file, ConditionalRelease, ReleaseProvider, ReleaseSource, ResolvedRelease,
};
use serde::Deserialize;
use std::time::Duration;

/// GitHub API kök adresi.
const API_ROOT: &str = "https://api.github.com";
/// Tek bir isteğin zaman sınırı.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);

/// Koşullu bir metin isteğinin sonucu.
#[derive(Debug, Clone)]
enum TextFetch {
    /// Gövde metni (ve varsa `ETag`).
    Body {
        /// Yanıt gövdesi.
        text: String,
        /// Sunucunun verdiği `ETag`.
        etag: Option<String>,
    },
    /// Değişiklik yok (HTTP 304).
    NotModified,
}

/// Koşullu bir yayın isteğinin sonucu.
#[derive(Debug, Clone)]
pub enum ReleaseFetch {
    /// Yeni yayın bilgisi.
    Fresh {
        /// Yayın kaydı.
        release: ReleaseResponse,
        /// Sunucunun verdiği `ETag`.
        etag: Option<String>,
    },
    /// Değişiklik yok (HTTP 304).
    NotModified,
}

/// GitHub Releases API yanıtının ilgilenen kısmı.
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseResponse {
    /// Yayın etiketi (ör. `"v15.2.0"`).
    pub tag_name: String,
    /// Taslak mı?
    #[serde(default)]
    pub draft: bool,
    /// Ön sürüm mü?
    #[serde(default)]
    pub prerelease: bool,
    /// Yayın notları (gövde). Boş olabilir.
    #[serde(default)]
    pub body: Option<String>,
    /// Yayındaki dosyalar.
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// Yayındaki tek bir dosya.
#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    /// Dosya adı.
    pub name: String,
    /// İndirme adresi.
    pub browser_download_url: String,
}

/// GitHub Releases çözümleyicisi.
#[derive(Debug, Clone)]
pub struct GitHubReleaseProvider {
    /// İstekleri yapan ajan.
    agent: ureq::Agent,
    /// Kimlik doğrulama token'ı (varsa).
    token: Option<String>,
}

impl Default for GitHubReleaseProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHubReleaseProvider {
    /// Yeni bir çözümleyici oluşturur.
    ///
    /// `GITHUB_TOKEN` ortam değişkeni varsa kullanılır: kimlikli istekler
    /// API hız sınırını yükseltir.
    pub fn new() -> Self {
        let token = std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        Self::with_token(token)
    }

    /// Belirtilen token ile çözümleyici oluşturur.
    pub fn with_token(token: Option<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build();
        Self {
            agent: config.into(),
            token,
        }
    }

    /// Bir `https://` adresinden metin indirir.
    fn get_text(&self, url: &str) -> Result<String, String> {
        match self.get_text_conditional(url, None)? {
            TextFetch::Body { text, .. } => Ok(text),
            TextFetch::NotModified => Err(format!("'{url}' için koşullu istek gereksizdi")),
        }
    }

    /// Bir `https://` adresinden metin indirir; `ETag` ile koşullu istek
    /// atabilir.
    fn get_text_conditional(&self, url: &str, etag: Option<&str>) -> Result<TextFetch, String> {
        if !url.starts_with("https://") {
            return Err(format!("yalnızca https:// adresleri okunabilir: '{url}'"));
        }
        let mut request = self.agent.get(url);
        if let Some(token) = &self.token {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }
        let mut response = request
            .call()
            .map_err(|e| format!("'{url}' okunamadı: {e}"))?;
        let status = response.status().as_u16();
        if status == 304 {
            return Ok(TextFetch::NotModified);
        }
        if status == 403 {
            let body = response.body_mut().read_to_string().unwrap_or_default();
            if body.contains("rate limit") {
                return Err(
                    "GitHub API hız sınırına ulaşıldı. 'GITHUB_TOKEN' ortam değişkenini \
                     ayarlayarak kimlikli isteklerle sınırı yükseltebilir ya da birazdan \
                     yeniden deneyebilirsin."
                        .into(),
                );
            }
            return Err(format!("'{url}' sunucusu 403 yanıtı verdi"));
        }
        if !(200..300).contains(&status) {
            return Err(format!("'{url}' sunucusu {status} yanıtı verdi"));
        }
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_string());
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("'{url}' okunamadı: {e}"))?;
        Ok(TextFetch::Body { text, etag })
    }

    /// Deponun en güncel (taslak/ön sürüm olmayan) yayınını getirir.
    pub fn latest_release(&self, repo: &str) -> Result<ReleaseResponse, String> {
        match self.latest_release_conditional(repo, None)? {
            ReleaseFetch::Fresh { release, .. } => Ok(release),
            ReleaseFetch::NotModified => Err(format!("'{repo}' için koşullu istek gereksizdi")),
        }
    }

    /// Deponun en güncel yayınını `ETag` ile koşullu olarak getirir.
    pub fn latest_release_conditional(
        &self,
        repo: &str,
        etag: Option<&str>,
    ) -> Result<ReleaseFetch, String> {
        let url = format!("{API_ROOT}/repos/{repo}/releases/latest");
        let (body, response_etag) = match self.get_text_conditional(&url, etag)? {
            TextFetch::Body { text, etag } => (text, etag),
            TextFetch::NotModified => return Ok(ReleaseFetch::NotModified),
        };
        let release: ReleaseResponse =
            serde_json::from_str(&body).map_err(|e| format!("'{url}' yanıtı okunamadı: {e}"))?;
        if release.draft {
            return Err(format!("'{repo}' için taslak yayın kullanılamaz"));
        }
        if release.prerelease {
            return Err(format!("'{repo}' için en güncel yayın bir ön sürüm"));
        }
        Ok(ReleaseFetch::Fresh {
            release,
            etag: response_etag,
        })
    }

    /// Yayın gövdesinden (yayın notları) checksum satırını okur.
    fn checksum_from_notes(&self, release: &ReleaseResponse, asset_name: &str) -> Option<String> {
        release
            .body
            .as_deref()
            .and_then(|body| parse_checksum_file(body, asset_name))
    }
}

impl ReleaseProvider for GitHubReleaseProvider {
    fn resolve(
        &self,
        source: &ReleaseSource,
        etag: Option<&str>,
    ) -> Result<ConditionalRelease, String> {
        source.validate()?;
        let (release, release_etag) = match self.latest_release_conditional(&source.repo, etag)? {
            ReleaseFetch::NotModified => return Ok(ConditionalRelease::NotModified),
            ReleaseFetch::Fresh { release, etag } => (release, etag),
        };
        let version = source.version_from_tag(&release.tag_name);
        let asset_name = source.asset_name(&version);

        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| {
                format!(
                    "'{}' yayınında '{}' adlı dosya bulunamadı",
                    source.repo, asset_name
                )
            })?;

        // Doğrulama özeti daima aynı yayından gelir.
        let (checksum, checksum_url) = match source.checksum_asset_name(&version) {
            Some(checksum_name) => {
                let checksum_asset = release
                    .assets
                    .iter()
                    .find(|a| a.name == checksum_name)
                    .ok_or_else(|| {
                        format!(
                            "'{}' yayınında '{}' checksum dosyası bulunamadı",
                            source.repo, checksum_name
                        )
                    })?;
                let content = self.get_text(&checksum_asset.browser_download_url)?;
                let sum = parse_checksum_file(&content, &asset_name).ok_or_else(|| {
                    format!(
                        "'{}' içinde '{}' dosyasına ait SHA-256 özeti yok",
                        checksum_name, asset_name
                    )
                })?;
                (sum, checksum_asset.browser_download_url.clone())
            }
            // Checksum dosyası yok: özet yayın notlarından okunur.
            None => {
                let sum = self
                    .checksum_from_notes(&release, &asset_name)
                    .ok_or_else(|| {
                        format!(
                            "'{}' yayın notlarında '{}' dosyasına ait SHA-256 özeti yok",
                            source.repo, asset_name
                        )
                    })?;
                (sum, format!("{}#{}", source.repo, release.tag_name))
            }
        };

        let resolved = ResolvedRelease {
            version,
            download_url: asset.browser_download_url.clone(),
            checksum,
            checksum_url,
        };
        resolved.validate()?;
        Ok(ConditionalRelease::Fresh {
            release: resolved,
            etag: release_etag,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> ReleaseSource {
        ReleaseSource {
            repo: "BurntSushi/ripgrep".into(),
            tag_prefix: Some("v".into()),
            asset_pattern: "ripgrep-{version}-x86_64-unknown-linux-musl.tar.gz".into(),
            checksum_asset_pattern: Some(
                "ripgrep-{version}-x86_64-unknown-linux-musl.tar.gz.sha256".into(),
            ),
        }
    }

    #[test]
    fn release_response_is_parsed() {
        let json = r#"{
            "tag_name": "v15.2.0",
            "draft": false,
            "prerelease": false,
            "body": "notlar",
            "assets": [
                {"name": "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz",
                 "browser_download_url": "https://github.com/x/y.tar.gz"}
            ]
        }"#;
        let release: ReleaseResponse = serde_json::from_str(json).unwrap();
        assert_eq!(release.tag_name, "v15.2.0");
        assert!(!release.draft);
        assert!(!release.prerelease);
        assert_eq!(release.assets.len(), 1);
        assert_eq!(
            release.assets[0].name,
            "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz"
        );
    }

    #[test]
    fn release_response_tolerates_missing_optional_fields() {
        let json = r#"{"tag_name": "v1.0.0"}"#;
        let release: ReleaseResponse = serde_json::from_str(json).unwrap();
        assert!(release.assets.is_empty());
        assert!(release.body.is_none());
    }

    #[test]
    fn checksum_from_notes_is_found_by_asset_name() {
        let provider = GitHubReleaseProvider::with_token(None);
        let release = ReleaseResponse {
            tag_name: "v0.23.5".into(),
            draft: false,
            prerelease: false,
            body: Some(
                "```\n1c01b578b5bd3f23b7de5a4b41936cde20fb16ff16a03e63266317ac1eb821e0  \
                 ./target/bin-0.23.5/eza_x86_64-unknown-linux-musl.tar.gz\n```"
                    .into(),
            ),
            assets: Vec::new(),
        };
        let sum = provider
            .checksum_from_notes(&release, "eza_x86_64-unknown-linux-musl.tar.gz")
            .unwrap();
        assert_eq!(
            sum,
            "1c01b578b5bd3f23b7de5a4b41936cde20fb16ff16a03e63266317ac1eb821e0"
        );
        assert!(provider
            .checksum_from_notes(&release, "olmayan.tar.gz")
            .is_none());
    }

    #[test]
    fn non_https_urls_are_refused() {
        let provider = GitHubReleaseProvider::with_token(None);
        let err = provider.get_text("http://example.org/a").unwrap_err();
        assert!(err.contains("https"), "{err}");
    }

    #[test]
    fn resolve_rejects_invalid_source_before_any_request() {
        let provider = GitHubReleaseProvider::with_token(None);
        let mut bad = source();
        bad.repo = "ripgrep".into();
        let err = provider.resolve(&bad, None).unwrap_err();
        assert!(err.contains("owner/repo"), "{err}");
    }
}
