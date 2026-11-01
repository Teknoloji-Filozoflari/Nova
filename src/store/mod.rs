//! Mağaza kataloğu modülü.
//!
//! Mağaza kataloğu, uygulamanın mevcut araç kataloğundan (`assets/catalog.toml`)
//! **ayrı** tutulur. Mağaza, henüz Nova'ya eklenmemiş ancak Nova'nın
//! kullanıcı alına kurabileceği uygulamaların listesi tutar.
//!
//! Mağaza kaydı aşağıdaki alanları taşır:
//!
//! - `id` — değişmez ve benzersiz kimlik (birleştirme anahtarı)
//! - `name` — uygulama adı
//! - `description` — Türkçe kısa açıklama
//! - `category` — kısa ve anlaşılır kategori adı
//! - `source` — kaynak/proje sayfası
//! - `license` — lisans bilgisi
//! - `executable` — executable adı
//! - `install_method` — kurulum yöntemi ("manual" | "download_archive")
//! - `supported_platforms` — desteklenen platformlar (ör. "linux-x64")
//! - `checksum` — SHA-256 doğrulama özeti
//! - `checksum_type` — "sha256" gibi
//! - `version` — kurulacak sürüm
//! - `download_url` — indirilecek arşivin `https://` adresi
//! - `install_notes` — `manual` yöntemde gösterilen resmî yönerge
//!
//! Katalog verisi UI koduna gömülmez; ayrı `assets/store.toml` dosyasından
//! `include_str!` ile yüklenir.

pub mod github;
pub mod http;
pub mod install;
pub mod loader;
pub mod manifest;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Mağaza kategorileri (kısa ve anlaşılır).
///
/// tuiui / awesome-tuis kapsamına yaklaşır; mevcut Türkçe adlar korunur:
/// Dashboards→Sistem, Development→Geliştirme, Editors→Editörler,
/// File Managers→Dosya ve Arama, Docker/LXC/K8s→Konteyner,
/// Messaging→Mesajlaşma, Multimedia→Medya, Productivity→Verimlilik,
/// Games→Oyunlar, Miscellaneous→Günlük. Git/Ağ/Veritabanı/Güvenlik Nova’ya özgüdür.
pub const STORE_CATEGORIES: &[&str] = &[
    "AI",
    "Git",
    "Dosya ve Arama",
    "Geliştirme",
    "Editörler",
    "Sistem",
    "Ağ",
    "Konteyner",
    "Veritabanı",
    "Güvenlik",
    "Medya",
    "Mesajlaşma",
    "Verimlilik",
    "Oyunlar",
    "Web",
    "Günlük",
];

/// Mağaza sekmesinin sol panelde görünen adı.
///
/// Mağaza, araç kategorileriyle aynı sekme listesinde yer alır; ayrı bir
/// modal değildir. Bu etiket hem çizimde hem de "hangi sekmedeyim"
/// sorusunun cevabında kullanılır.
pub const STORE_TAB_LABEL: &str = "Mağaza";

/// Kurulum yöntemi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMethod {
    /// Kullanıcının resmî kurulum yönergelerini takip etmesi gerekir.
    /// Nova kurulum yapmaz; yalnızca talimat gösterilir.
    Manual,
    /// Nova indirip kullanıcı alanına çıkarır (checksum doğrulanırsa).
    #[default]
    DownloadArchive,
}

impl InstallMethod {
    /// Bu yöntem için "Kur" eylemi sunulabilir mi?
    ///
    /// Yalnızca [`InstallMethod::DownloadArchive`] için evet; `manual`
    /// yöntemde Nova kurulum yapmaz, resmî talimatı gösterir.
    pub fn is_installable(self) -> bool {
        matches!(self, InstallMethod::DownloadArchive)
    }

    /// Arayüzde gösterilecek kısa açıklama.
    pub fn label(self) -> &'static str {
        match self {
            InstallMethod::Manual => "Resmî talimat (Nova kurmuyor)",
            InstallMethod::DownloadArchive => "Nova kullanıcı alanına indirir",
        }
    }
}

/// Tek bir mağaza kaydı.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct StoreEntry {
    /// Değişmez ve benzersiz kimlik.
    pub id: String,
    /// Uygulama adı.
    pub name: String,
    /// Türkçe kısa açıklama.
    pub description: String,
    /// Kategori (bkz. `STORE_CATEGORIES`).
    pub category: String,
    /// Kaynak/proje sayfası.
    pub source: String,
    /// Lisans bilgisi.
    pub license: String,
    /// Executable adı.
    pub executable: String,
    /// Kurulum yöntemi.
    #[serde(default)]
    pub install_method: InstallMethod,
    /// Desteklenen platformlar.
    #[serde(default)]
    pub supported_platforms: Vec<String>,
    /// SHA-256 doğrulama özeti (opsiyonel).
    #[serde(default)]
    pub checksum: Option<String>,
    /// Checksum türü (opsiyonel).
    #[serde(default)]
    pub checksum_type: Option<String>,
    /// Kurulacak sürüm (ör. "15.2.0"). Yalnızca `download_archive` için.
    #[serde(default)]
    pub version: Option<String>,
    /// İndirilecek arşivin adresi.
    ///
    /// Yalnızca `download_archive` yöntemde zorunludur; `https://` şemasında
    /// olmalıdır. Nova bu adresi `curl | sh` gibi bir kabuk betiği olarak
    /// çalıştırmaz, dosyayı indirip doğrular.
    #[serde(default)]
    pub download_url: Option<String>,
    /// `manual` yöntemde gösterilecek resmî kurulum yönergesi.
    #[serde(default)]
    pub install_notes: Option<String>,
    /// Canlı sürüm takibi için izlenecek GitHub deposu.
    ///
    /// Depo **gömülü katalogda sabittir**; ağdan gelen veri yalnızca sürüm
    /// ipucu verir, bütünlük upstream'in yayımladığı checksum dosyasından
    /// gelir. `None` ise bu uygulama canlı olarak takip edilmez.
    #[serde(default)]
    pub release: Option<manifest::ReleaseSource>,
}

/// Mağaza dosyasının kök yapısı.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreFile {
    #[serde(default)]
    pub apps: Vec<StoreEntry>,
}

/// Mağaza katalogı.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Store {
    entries: Vec<StoreEntry>,
}

impl Store {
    /// Girişlerden mağaza katalogu oluşturur.
    ///
    /// # Doğrulama kuralları
    ///
    /// 1. Kimlikler benzersiz olmalıdır.
    /// 2. Kategori `STORE_CATEGORIES` listesinden biri olmalıdır.
    /// 3. `download_archive` yöntemli bir kaydın **hem** `download_url`
    ///    **hem** `checksum` alanı olmalıdır. Doğrulanamayan bir indirme
    ///    için Nova "Kur" eylemi sunmaz; böylece eksik metadata nedeniyle
    ///    checksum'siz dosya çalıştırılmaz.
    /// 4. `download_url` yalnızca `https://` şemasında olabilir.
    pub fn from_entries(entries: Vec<StoreEntry>) -> Result<Self, String> {
        if entries.is_empty() {
            return Err("mağaza kataloğu boş olamaz".into());
        }
        let mut seen = std::collections::HashSet::new();
        for entry in &entries {
            if !seen.insert(entry.id.clone()) {
                return Err(format!(
                    "mağaza kataloğunda yinelenen kimlik: '{}'",
                    entry.id
                ));
            }
            if !STORE_CATEGORIES.contains(&entry.category.as_str()) {
                return Err(format!(
                    "'{}' için bilinmeyen kategori '{}'; geçerli kategoriler: {}",
                    entry.id,
                    entry.category,
                    STORE_CATEGORIES.join(", ")
                ));
            }
            if entry.install_method.is_installable() {
                let url = entry.download_url.as_deref().unwrap_or_default();
                if url.trim().is_empty() {
                    return Err(format!(
                        "'{}' için download_url eksik; indirilebilir kayıtlar \
                         doğrulanabilir bir adres taşımalı.",
                        entry.id
                    ));
                }
                if !url.starts_with("https://") {
                    return Err(format!(
                        "'{}' için download_url yalnızca https:// şemasında olabilir: '{}'",
                        entry.id, url
                    ));
                }
                match entry.checksum.as_deref() {
                    None => {
                        return Err(format!(
                            "'{}' için checksum eksik; indirilebilir kayıtlar SHA-256 \
                             doğrulama özeti taşımalı.",
                            entry.id
                        ))
                    }
                    Some(sum) if !is_sha256_hex(sum) => {
                        return Err(format!(
                            "'{}' için checksum SHA-256 değil: '{}'",
                            entry.id, sum
                        ))
                    }
                    Some(_) => {}
                }
            }
            // Canlı takip kaynağı varsa kendi kurallarına uymalıdır.
            if let Some(release) = &entry.release {
                release.validate().map_err(|e| {
                    format!("'{}' için canlı sürüm kaynağı geçersiz: {e}", entry.id)
                })?;
            }
        }
        Ok(Self { entries })
    }

    /// Tüm girişlere salt okunur erişim.
    pub fn entries(&self) -> &[StoreEntry] {
        &self.entries
    }

    /// Kimliğe göre giriş döner.
    pub fn get_by_id(&self, id: &str) -> Option<&StoreEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Kurulum sonrası bir kaydın **bilinen** sürümünü günceller.
    ///
    /// Böylece "yeni sürüm var" göstergesi kurulumdan sonra söner. Gömülü
    /// katalog dosyası değişmez; bu yalnızca bellekteki bilgiyi tazeler.
    pub fn mark_installed(
        &mut self,
        id: &str,
        version: &str,
        checksum: &str,
        download_url: &str,
    ) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) else {
            return false;
        };
        if !version.trim().is_empty() {
            entry.version = Some(version.to_string());
        }
        // Yalnızca geçerli değerler yazılır: bozuk özet ya da şemasız adres
        // kaydı geçersiz hâle getirir.
        if is_sha256_hex(checksum) {
            entry.checksum = Some(checksum.to_lowercase());
        }
        if download_url.starts_with("https://") {
            entry.download_url = Some(download_url.to_string());
        }
        true
    }

    /// Kategoriye göre filtreler.
    pub fn by_category(&self, category: &str) -> Vec<&StoreEntry> {
        self.entries
            .iter()
            .filter(|e| e.category == category)
            .collect()
    }

    /// İsim, açıklama, kategori ve kaynakta arama yapar.
    pub fn search(&self, query: &str) -> Vec<&StoreEntry> {
        if query.is_empty() {
            return Vec::new();
        }
        let q = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.name.to_lowercase().contains(&q)
                    || e.description.to_lowercase().contains(&q)
                    || e.category.to_lowercase().contains(&q)
                    || e.source.to_lowercase().contains(&q)
            })
            .collect()
    }
}

/// Mağaza kurulum hedef dizini (kullanıcı alanı).
///
/// Nova'nın kendi veri dizininden ayrı, ancak kullanıcının ev dizini altında
/// tutulan bir konumdur. Sistem dizinlerine yazılmaz.
///
/// `NOVA_STORE_DIR` ortam değişkeni verilmişse o yol kullanılır (yalnızca
/// testler ve taşınabilir kurulumlar için; `NOVA_STATE` ile aynı desendir).
#[allow(clippy::question_mark)]
pub fn store_install_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("NOVA_STORE_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let xdg_data = std::env::var_os("XDG_DATA_HOME");
    let home = std::env::var_os("HOME");
    let xdg = xdg_data.filter(|v| !v.is_empty());
    let home = home.filter(|v| !v.is_empty());
    let base = match (xdg, home) {
        (Some(xdg), _) => Path::new(xdg.as_os_str()).join("nova"),
        (None, Some(h)) => PathBuf::from(h.as_os_str())
            .join(".local")
            .join("share")
            .join("nova"),
        (None, None) => return None,
    };
    Some(base.join("store"))
}

/// Bir mağaza uygulamasının kurulum dizini (`<store>/<id>`).
pub fn store_app_dir(id: &str) -> Option<PathBuf> {
    Some(store_install_dir()?.join(id))
}

/// Bir mağaza uygulamasının çalıştırılabilir dosyalarının dizini
/// (`<store>/<id>/bin`).
///
/// Nova, mağazadan kurulan uygulamaları buradan tespit eder; böylece
/// kurulumdan sonra araç "kurulu" görünür.
pub fn store_app_bin_dir(id: &str) -> Option<PathBuf> {
    Some(store_app_dir(id)?.join("bin"))
}

/// `checksum` alanının SHA-256 biçiminde olup olmadığını kontrol eder.
///
/// 64 onaltılık karakter olmalıdır; küçük/büyük harf ikisi de kabul edilir.
pub fn is_sha256_hex(value: &str) -> bool {
    let v = value.trim();
    v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit())
}

/// Testler için mağaza dizinini geçici bir konuma sabitler.
///
/// `NOVA_STORE_DIR` ortam değişkeni süreç boyunca **bir kez** ayarlanır.
/// Böylece mağaza testleri gerçek kullanıcı dizinine yazmaz; her test
/// kendi alt dizinini kullanır ve sonunda siler.
#[cfg(test)]
pub fn test_store_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("nova-store-test-{}", std::process::id()));
        std::env::set_var("NOVA_STORE_DIR", &dir);
        dir
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test kataloğundan indirilebilir bir kayıt üretir.
    fn installable_entry(id: &str) -> StoreEntry {
        StoreEntry {
            id: id.into(),
            name: id.into(),
            description: "d".into(),
            category: "Günlük".into(),
            source: "https://example.org/".into(),
            license: "MIT".into(),
            executable: "rg".into(),
            install_method: InstallMethod::DownloadArchive,
            supported_platforms: vec!["linux-x64".into()],
            checksum: Some("a".repeat(64)),
            checksum_type: Some("sha256".into()),
            version: Some("1.0.0".into()),
            download_url: Some("https://example.org/a.tar.gz".into()),
            install_notes: None,
            release: None,
        }
    }

    #[test]
    fn is_sha256_hex_accepts_only_64_hex_chars() {
        assert!(is_sha256_hex(&"a".repeat(64)));
        assert!(is_sha256_hex(&"A".repeat(64)));
        assert!(!is_sha256_hex(&"a".repeat(63)));
        assert!(!is_sha256_hex(&"g".repeat(64)));
        assert!(!is_sha256_hex(""));
    }

    #[test]
    fn download_archive_entry_requires_download_url() {
        let mut entry = installable_entry("a");
        entry.download_url = None;
        let err = Store::from_entries(vec![entry]).unwrap_err();
        assert!(err.contains("download_url"), "{err}");
    }

    #[test]
    fn download_archive_entry_requires_checksum() {
        let mut entry = installable_entry("a");
        entry.checksum = None;
        let err = Store::from_entries(vec![entry]).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
    }

    #[test]
    fn download_archive_entry_requires_valid_sha256() {
        let mut entry = installable_entry("a");
        entry.checksum = Some("kisa".into());
        let err = Store::from_entries(vec![entry]).unwrap_err();
        assert!(err.contains("SHA-256"), "{err}");
    }

    #[test]
    fn download_url_must_use_https() {
        let mut entry = installable_entry("a");
        entry.download_url = Some("http://example.org/a.tar.gz".into());
        let err = Store::from_entries(vec![entry]).unwrap_err();
        assert!(err.contains("https"), "{err}");
    }

    #[test]
    fn manual_entry_needs_no_url_or_checksum() {
        let mut entry = installable_entry("a");
        entry.install_method = InstallMethod::Manual;
        entry.download_url = None;
        entry.checksum = None;
        assert!(Store::from_entries(vec![entry]).is_ok());
    }

    #[test]
    fn manual_entries_are_not_installable() {
        assert!(!InstallMethod::Manual.is_installable());
        assert!(InstallMethod::DownloadArchive.is_installable());
    }

    #[test]
    fn mark_installed_updates_the_known_version() {
        let mut store = Store::from_entries(vec![installable_entry("a")]).unwrap();
        let sum = "b".repeat(64);
        assert!(store.mark_installed("a", "2.0.0", &sum, "https://example.org/yeni.tar.gz"));
        let entry = store.get_by_id("a").unwrap();
        assert_eq!(entry.version.as_deref(), Some("2.0.0"));
        assert_eq!(entry.checksum.as_deref(), Some(sum.as_str()));
        assert_eq!(
            entry.download_url.as_deref(),
            Some("https://example.org/yeni.tar.gz")
        );
    }

    #[test]
    fn mark_installed_never_writes_invalid_values() {
        let mut store = Store::from_entries(vec![installable_entry("a")]).unwrap();
        let sum = "b".repeat(64);
        store.mark_installed("a", "2.0.0", &sum, "https://example.org/yeni.tar.gz");
        // Bozuk özet ve şemasız adres yok sayılır.
        store.mark_installed("a", "3.0.0", "bozuk", "http://guvensiz");
        let entry = store.get_by_id("a").unwrap();
        assert_eq!(entry.version.as_deref(), Some("3.0.0"));
        assert_eq!(entry.checksum.as_deref(), Some(sum.as_str()));
        assert_eq!(
            entry.download_url.as_deref(),
            Some("https://example.org/yeni.tar.gz")
        );
        // Katalog hâlâ geçerli olmalı.
        assert!(Store::from_entries(store.entries().to_vec()).is_ok());
    }

    #[test]
    fn mark_installed_ignores_unknown_ids() {
        let mut store = Store::from_entries(vec![installable_entry("a")]).unwrap();
        assert!(!store.mark_installed("yok", "1.0.0", &"c".repeat(64), "https://x/y"));
    }
}
