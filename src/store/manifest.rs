//! Mağaza için canlı sürüm takibi (manifest).
//!
//! # Güvenlik modeli
//!
//! Canlı takip bir "uzaktan gelen katalog" değildir. Mağaza kataloğu bir
//! **keşif ipucu** verir, bütünlük ise upstream'in kendisinden gelir:
//!
//! 1. **Depo gömülü katalogda sabitlendir.** Hangi GitHub deposunun
//!    izleneceği `assets/store/*.toml` içinde yazılıdır; ağdan gelen hiçbir
//!    veri bu bilgiyi değiştiremez.
//! 2. **Sürüm GitHub Releases API'sinden okunur.** Bu yalnızca "en güncel
//!    sürüm hangisi?" sorusunu yanıtlar.
//! 3. **Doğrulama özeti upstream'in kendi dosyasından gelir.** Nova, aynı
//!    yayındaki checksum dosyasını indirip ilgili satırı okur.
//! 4. **İndirilen arşiv bu özetle karşılaştırılır.** Uyuşmazlıkta kurulum
//!    durur.
//!
//! Böylece canlı takibi kullanan bir saldırganın yapabileceği tek şey
//! "şu sürümü kontrol et" demektir; çalıştırılacak dosyanın bütünlüğü
//! upstream'in yayımladığı SHA-256 özetiyle korunur.
//!
//! # Önbellek
//!
//! GitHub API'si kimliksiz çağrılarda IP başına saatte 60 istekle sınırlıdır.
//! Bu yüzden çözümlemeler `manifest-cache.toml` dosyasında önbelleklenir ve
//! [`CACHE_TTL_SECS`] boyunca yeniden kullanılır: **taze kayıt ağa hiç
//! çıkmaz**. `GITHUB_TOKEN` ortam değişkeni verilirse istekler kimlikli
//! yapılır ve sınır yükselir (saatte 5000).
//!
//! Yayın sorgusu ayrıca `ETag` ile koşullu atılır; değişiklik yoksa `304`
//! döner ve gövde indirilmez. (Ölçüldüğü kadarıyla `304` yanıtları da hız
//! sınırından düşer; koruma asıl olarak önbellekten gelir.)

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Çözümlenmiş önbelleğin geçerlilik süresi (saniye).
pub const CACHE_TTL_SECS: u64 = 6 * 60 * 60;

/// Bir uygulamanın canlı sürüm kaynağı.
///
/// Depo ve dosya adı desenleri **gömülü katalogda** sabittir; ağdan gelen
/// hiçbir veri bunları değiştiremez.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseSource {
    /// `"owner/repo"` biçiminde GitHub deposu.
    pub repo: String,
    /// Sürüm etiketindeki ön ek (ör. `"v"`). `None` ise etiket olduğu gibi
    /// kullanılır.
    #[serde(default)]
    pub tag_prefix: Option<String>,
    /// Sürüm arşivinin adı; `{version}` yer tutucusu sürümle değiştirilir.
    ///
    /// Yer tutucu içermeyen desenler sabit dosya adı olarak yorumlanır
    /// (bazı projeler sürümü dosya adına koymaz).
    ///
    /// Boş / eksik: yalnızca depo kimliği (yıldız, Depo linki). Canlı sürüm
    /// çözümlemesi için `asset_pattern` gerekir.
    #[serde(default)]
    pub asset_pattern: String,
    /// Doğrulama özeti dosyasının adı deseni.
    ///
    /// `None` ise özet **yayın notlarından** okunur (bazı projeler checksum
    /// listesini yayın notlarının içine gömer).
    #[serde(default)]
    pub checksum_asset_pattern: Option<String>,
}

impl ReleaseSource {
    /// Canlı sürüm / arşiv takibi için asset deseni var mı?
    pub fn tracks_releases(&self) -> bool {
        !self.asset_pattern.trim().is_empty()
    }

    /// Depo kimliğini doğrular (`owner/repo`). Asset deseni zorunlu değildir
    /// (Nova-Store’da yıldız-only `release.repo` kayıtları).
    pub fn validate(&self) -> Result<(), String> {
        let parts: Vec<&str> = self.repo.split('/').collect();
        if parts.len() != 2 || parts.iter().any(|p| p.trim().is_empty()) {
            return Err(format!(
                "depo '{}' 'owner/repo' biçiminde olmalı",
                self.repo
            ));
        }
        if let Some(pattern) = &self.checksum_asset_pattern {
            if pattern.trim().is_empty() {
                return Err("checksum_asset_pattern boş olamaz".into());
            }
            if !self.tracks_releases() {
                return Err("checksum_asset_pattern asset_pattern olmadan kullanılamaz".into());
            }
        }
        Ok(())
    }

    /// Canlı sürüm çözümlemesi için doğrular (asset deseni zorunlu).
    pub fn validate_for_resolve(&self) -> Result<(), String> {
        self.validate()?;
        if !self.tracks_releases() {
            return Err("asset_pattern boş olamaz".into());
        }
        Ok(())
    }

    /// Sürüm etiketini biçimlendirir (ör. `15.2.0` → `v15.2.0`).
    pub fn tag_for(&self, version: &str) -> String {
        match &self.tag_prefix {
            Some(prefix) => format!("{prefix}{version}"),
            None => version.to_string(),
        }
    }

    /// Sürüm etiketinden sürüm numarasını çıkarır.
    ///
    /// `"v15.2.0"` → `"15.2.0"`. Önek eşleşmezse etiket olduğu gibi döner.
    pub fn version_from_tag(&self, tag: &str) -> String {
        match &self.tag_prefix {
            Some(prefix) => tag.strip_prefix(prefix.as_str()).unwrap_or(tag).to_string(),
            None => tag.to_string(),
        }
    }

    /// Sürüm verildiğinde indirilecek arşivin adı.
    pub fn asset_name(&self, version: &str) -> String {
        self.asset_pattern.replace("{version}", version)
    }

    /// Sürüm verildiğinde checksum dosyasının adı.
    ///
    /// Checksum yayın notlarından okunacaksa `None` döner.
    pub fn checksum_asset_name(&self, version: &str) -> Option<String> {
        self.checksum_asset_pattern
            .as_ref()
            .map(|pattern| pattern.replace("{version}", version))
    }
}

/// Canlı takiple çözülmüş sürüm bilgisi.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedRelease {
    /// Yayın sürümü (ör. `"15.2.0"`).
    pub version: String,
    /// Arşivin indirileceği `https://` adresi.
    pub download_url: String,
    /// Arşivin SHA-256 özeti (upstream'in yayımladığı değer).
    pub checksum: String,
    /// Özetin okunduğu dosyanın adresi (denetim izi için).
    pub checksum_url: String,
}

impl ResolvedRelease {
    /// Çözümlenmiş sürümün kullanılabilir olup olmadığını doğrular.
    ///
    /// Adres `https://` olmalı, özet geçerli bir SHA-256 hex olmalıdır.
    pub fn validate(&self) -> Result<(), String> {
        if !self.download_url.starts_with("https://") {
            return Err(format!(
                "indirme adresi yalnızca https:// olabilir: '{}'",
                self.download_url
            ));
        }
        if !crate::store::is_sha256_hex(&self.checksum) {
            return Err(format!("geçersiz SHA-256 özeti: '{}'", self.checksum));
        }
        Ok(())
    }
}

/// Koşullu çözümlemenin sonucu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionalRelease {
    /// Yeni veri geldi.
    Fresh {
        /// Çözülen sürüm.
        release: ResolvedRelease,
        /// Sunucunun verdiği `ETag` (sonraki koşullu istekler için).
        etag: Option<String>,
    },
    /// Değişiklik yok (HTTP 304); önbellekteki kayıt hâlâ geçerli.
    ///
    /// Gövde yeniden indirilmez. Not: GitHub `304` yanıtlarını da hız
    /// sınırından düşürür; sınır koruması önbellekten (`CACHE_TTL_SECS`)
    /// gelir, `ETag`'den değil.
    NotModified,
}

/// Sürüm sağlayıcısı.
///
/// Testlerde sahte bir sağlayıcı verilebilir; gerçek uygulama
/// [`crate::store::github::GitHubReleaseProvider`]'dır.
pub trait ReleaseProvider: Send + Sync {
    /// Verilen kaynak için en güncel sürümü ve doğrulama özetini çözer.
    ///
    /// `etag` verilirse koşullu istek atılır (`If-None-Match`); sunucu
    /// değişiklik yoksa [`ConditionalRelease::NotModified`] döner ve API hız
    /// sınırı harcanmaz.
    fn resolve(
        &self,
        source: &ReleaseSource,
        etag: Option<&str>,
    ) -> Result<ConditionalRelease, String>;
}

/// Önbelleklenmiş bir çözümleme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedRelease {
    /// Çözümlenen sürüm.
    #[serde(flatten)]
    pub release: ResolvedRelease,
    /// Çözümlemenin yapıldığı UNIX zamanı (saniye).
    pub checked_at: u64,
    /// Sunucunun verdiği `ETag`; koşullu sonraki istekler için.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
}

impl CachedRelease {
    /// Önbellek kaydı hâlâ taze mi?
    pub fn is_fresh(&self, now: u64) -> bool {
        now.saturating_sub(self.checked_at) < CACHE_TTL_SECS
    }
}

/// Mağaza manifest önbelleği.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ManifestCache {
    /// Uygulama kimliği → son çözümleme.
    #[serde(default)]
    pub entries: BTreeMap<String, CachedRelease>,
}

impl ManifestCache {
    /// Belirtilen uygulamanın taze kaydını döner.
    pub fn fresh(&self, app_id: &str, now: u64) -> Option<&CachedRelease> {
        self.entries.get(app_id).filter(|c| c.is_fresh(now))
    }

    /// Uygulamanın kaydını (tazeliği önemsiz) döner.
    pub fn get(&self, app_id: &str) -> Option<&CachedRelease> {
        self.entries.get(app_id)
    }

    /// Bir çözümlemeyi kaydeder.
    pub fn put(&mut self, app_id: &str, release: ResolvedRelease, now: u64, etag: Option<String>) {
        self.entries.insert(
            app_id.to_string(),
            CachedRelease {
                release,
                checked_at: now,
                etag,
            },
        );
    }
}

/// Önbellek dosyasının yolu (`<store>/manifest-cache.toml`).
pub fn cache_path() -> Option<PathBuf> {
    Some(crate::store::store_install_dir()?.join("manifest-cache.toml"))
}

/// Önbelleği okur. Dosya yoksa boş önbellek döner; bozuk dosya sessizce
/// silinmez, yalnızca yok sayılır.
pub fn load_cache(path: &Path) -> ManifestCache {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).unwrap_or_default(),
        Err(_) => ManifestCache::default(),
    }
}

/// Önbelleği atomik olarak yazar.
pub fn save_cache(path: &Path, cache: &ManifestCache) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("'{}' oluşturulamadı: {e}", parent.display()))?;
    }
    let text =
        toml::to_string_pretty(cache).map_err(|e| format!("önbellek TOML'a çevrilemedi: {e}"))?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("'{}' yazılamadı: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("'{}' yerine konamadı: {e}", path.display()))
}

/// Şu anki UNIX zamanı (saniye).
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// İki sürüm arasındaki ilişkiyi karşılaştırır.
///
/// Sürümler `MAJOR.MINOR.PATCH` biçiminde olmalıdır; karşılaştırma semver
/// kurallarına uyar (ön sürüm, kararlı sürümden küçüktür). Biçim dışı
/// sürümler "eşit değil" sayılır.
pub fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let va = crate::update::Version::parse(a).ok()?;
    let vb = crate::update::Version::parse(b).ok()?;
    Some(va.cmp(&vb))
}

/// Bir uygulamanın canlı sürümü, gömülü katalogdaki sürümünden **yeni** mi?
///
/// Sürümler karşılaştırılamıyorsa (biçim dışı) `false` döner: Nova
/// bilmediği bir sürümü "yeni" ilan etmez.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!(
        compare_versions(candidate, current),
        Some(std::cmp::Ordering::Greater)
    )
}

/// Bir checksum dosyasının içinden verilen dosya adına ait özeti bulur.
///
/// Upstream checksum dosyalarının yaygın biçimi:
/// ```text
/// <64 hex>  <dosya adı>
/// ```
/// Bazıları `*` önekiyle (ikili mod) yazar; bazıları `./` öneki kullanır.
/// Hepsini desteklenir. Dosya adı bulunamazsa `None` döner.
pub fn parse_checksum_file(content: &str, asset_name: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(sum) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        if !crate::store::is_sha256_hex(sum) {
            continue;
        }
        let name = name.trim_start_matches('*');
        // `./target/bin-0.23.5/eza_...` gibi yolların son bileşeni de eşleşebilir.
        let base = name.rsplit('/').next().unwrap_or(name);
        if name == asset_name || base == asset_name {
            return Some(sum.to_lowercase());
        }
    }
    None
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
    fn release_source_validates_repo_and_patterns() {
        assert!(source().validate().is_ok());

        let mut bad = source();
        bad.repo = "ripgrep".into();
        assert!(bad.validate().is_err());

        let mut bad = source();
        bad.repo = "owner/".into();
        assert!(bad.validate().is_err());

        let mut repo_only = source();
        repo_only.asset_pattern = String::new();
        repo_only.checksum_asset_pattern = None;
        assert!(repo_only.validate().is_ok());
        assert!(!repo_only.tracks_releases());
        assert!(repo_only.validate_for_resolve().is_err());

        let mut bad = source();
        bad.checksum_asset_pattern = Some("   ".into());
        assert!(bad.validate().is_err());
    }

    #[test]
    fn release_notes_checksum_source_needs_no_asset() {
        let mut s = source();
        s.checksum_asset_pattern = None;
        assert!(s.validate().is_ok());
        assert_eq!(s.checksum_asset_name("15.2.0"), None);
    }

    #[test]
    fn tag_and_asset_names_follow_the_pattern() {
        let s = source();
        assert_eq!(s.tag_for("15.2.0"), "v15.2.0");
        assert_eq!(s.version_from_tag("v15.2.0"), "15.2.0");
        assert_eq!(
            s.asset_name("15.2.0"),
            "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz"
        );
        assert_eq!(
            s.checksum_asset_name("15.2.0").as_deref(),
            Some("ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz.sha256")
        );
    }

    #[test]
    fn version_from_tag_keeps_unmatched_prefix() {
        let mut s = source();
        s.tag_prefix = Some("v".into());
        assert_eq!(s.version_from_tag("15.2.0"), "15.2.0");
    }

    #[test]
    fn resolved_release_requires_https_and_valid_checksum() {
        let ok = ResolvedRelease {
            version: "1.0.0".into(),
            download_url: "https://example.org/a.tar.gz".into(),
            checksum: "a".repeat(64),
            checksum_url: "https://example.org/a.sha256".into(),
        };
        assert!(ok.validate().is_ok());

        let mut bad = ok.clone();
        bad.download_url = "http://example.org/a.tar.gz".into();
        assert!(bad.validate().is_err());

        let mut bad = ok.clone();
        bad.checksum = "kisa".into();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn checksum_file_is_parsed_by_asset_name() {
        let linux = "a".repeat(64);
        let darwin = "b".repeat(64);
        let content = format!(
            "{linux}  fzf-0.74.5-linux_amd64.tar.gz\n\
             {darwin}  fzf-0.74.5-darwin_amd64.tar.gz\n"
        );
        assert_eq!(
            parse_checksum_file(&content, "fzf-0.74.5-linux_amd64.tar.gz"),
            Some(linux.clone())
        );
        assert_eq!(
            parse_checksum_file(&content, "fzf-0.74.5-darwin_amd64.tar.gz"),
            Some(darwin)
        );
        assert_eq!(
            parse_checksum_file(&content, "fzf-0.74.5-windows_amd64.zip"),
            None
        );
    }

    #[test]
    fn checksum_file_accepts_binary_marker_and_paths() {
        let sum = "abcdef".repeat(10) + "abcd";
        let content = format!("{sum}  ./target/bin-0.23.5/eza_x86_64-unknown-linux-musl.tar.gz\n");
        assert_eq!(
            parse_checksum_file(&content, "eza_x86_64-unknown-linux-musl.tar.gz"),
            Some(sum)
        );
    }

    #[test]
    fn checksum_file_skips_comments_and_invalid_lines() {
        let content = "# yorum\n\nnotahash  a.tar.gz\n";
        assert_eq!(parse_checksum_file(content, "a.tar.gz"), None);
    }

    #[test]
    fn cache_entries_expire_after_ttl() {
        let now = 1_000_000u64;
        let mut cache = ManifestCache::default();
        cache.put(
            "ripgrep",
            ResolvedRelease {
                version: "15.2.0".into(),
                download_url: "https://example.org/a.tar.gz".into(),
                checksum: "a".repeat(64),
                checksum_url: "https://example.org/a.sha256".into(),
            },
            now,
            None,
        );
        assert!(cache.fresh("ripgrep", now).is_some());
        assert!(cache.fresh("ripgrep", now + CACHE_TTL_SECS - 1).is_some());
        assert!(cache.fresh("ripgrep", now + CACHE_TTL_SECS).is_none());
        assert!(cache.fresh("başka", now).is_none());
    }

    #[test]
    fn cache_round_trips_through_toml() {
        let dir = std::env::temp_dir().join(format!("nova-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest-cache.toml");
        let mut cache = ManifestCache::default();
        cache.put(
            "fzf",
            ResolvedRelease {
                version: "0.74.5".into(),
                download_url: "https://example.org/fzf.tar.gz".into(),
                checksum: "b".repeat(64),
                checksum_url: "https://example.org/fzf.sha256".into(),
            },
            42,
            Some("W/\"abc\"".into()),
        );
        save_cache(&path, &cache).unwrap();
        let loaded = load_cache(&path);
        assert_eq!(loaded, cache);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_cache_is_not_an_error() {
        let dir = std::env::temp_dir().join(format!("nova-cache-yok-{}", std::process::id()));
        let cache = load_cache(&dir.join("yok.toml"));
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn version_comparison_detects_newer_versions() {
        assert!(is_newer("15.2.1", "15.2.0"));
        assert!(!is_newer("15.2.0", "15.2.0"));
        assert!(!is_newer("15.1.9", "15.2.0"));
        assert!(is_newer("16.0.0", "15.9.9"));
        // Biçim dışı sürüm "yeni" sayılmaz.
        assert!(!is_newer("son", "15.2.0"));
    }

    #[test]
    fn cache_keeps_the_etag_for_conditional_requests() {
        let mut cache = ManifestCache::default();
        cache.put(
            "ripgrep",
            ResolvedRelease {
                version: "15.2.0".into(),
                download_url: "https://example.org/a.tar.gz".into(),
                checksum: "a".repeat(64),
                checksum_url: "https://example.org/a.sha256".into(),
            },
            100,
            Some("W/\"abc123\"".into()),
        );
        let entry = cache.get("ripgrep").unwrap();
        assert_eq!(entry.etag.as_deref(), Some("W/\"abc123\""));
    }

    #[test]
    fn etag_survives_a_toml_round_trip() {
        let dir = std::env::temp_dir().join(format!("nova-etag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest-cache.toml");
        let mut cache = ManifestCache::default();
        cache.put(
            "fzf",
            ResolvedRelease {
                version: "0.74.5".into(),
                download_url: "https://example.org/fzf.tar.gz".into(),
                checksum: "b".repeat(64),
                checksum_url: "https://example.org/fzf.sha256".into(),
            },
            42,
            Some("W/\"xyz\"".into()),
        );
        save_cache(&path, &cache).unwrap();
        let loaded = load_cache(&path);
        assert_eq!(
            loaded.get("fzf").unwrap().etag.as_deref(),
            Some("W/\"xyz\"")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
