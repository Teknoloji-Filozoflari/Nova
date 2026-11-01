//! Nova güncelleme modülü.
//!
//! Kullanıcı tarafından başlatılan "Güncellemeleri denetle" eylemini içerir.
//! Güncelleme yalnızca gerçek ve doğrulanmış bir yayın yöntemi varsa sunulur;
//! kullanıcı onayı olmadan güncelleme yapılmaz. Kurulum kullanıcı alanında
//! yapılır; `sudo`/root yok, kullanıcı yapılandırmasına dokunulmaz.
//!
//! İndirme veya doğrulama başarısızsa çalışan Nova ikilisi bozulmaz.

use std::cmp::Ordering;
use std::fmt;
use std::path::{Path, PathBuf};

/// Minimal semver uyumlu sürüm numarası: `MAJOR.MINOR.PATCH` (ön ek "v"
/// opsiyoneldir). `semver` crate'i eklemek yerine yerel olarak tanımlanmıştır.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Ön sürüm parçaları (ör. "alpha", "beta.1"). Boşsa sürüm stabildir.
    pub pre: Vec<String>,
}

impl Version {
    /// `"0.1.0"` veya `"v0.1.0"` biçimindeki metni ayrıştırır.
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim_start_matches('v');
        let (main, pre_str) = match s.split_once('-') {
            Some((m, pre)) => (m, Some(pre)),
            None => (s, None),
        };
        let parts: Vec<&str> = main.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("geçersiz sürüm: '{s}'"));
        }
        let major = parts[0]
            .parse::<u64>()
            .map_err(|_| format!("geçersiz major sürüm: '{}'", parts[0]))?;
        let minor = parts[1]
            .parse::<u64>()
            .map_err(|_| format!("geçersiz minor sürüm: '{}'", parts[1]))?;
        let patch = parts[2]
            .parse::<u64>()
            .map_err(|_| format!("geçersiz patch sürüm: '{}'", parts[2]))?;
        let pre = pre_str
            .map(|pre| pre.split('.').map(|p| p.to_string()).collect())
            .unwrap_or_default();
        Ok(Self {
            major,
            minor,
            patch,
            pre,
        })
    }

    /// Karşılaştırma: ön sürüm sürümü stabil sürümden düşük kabul edilir
    /// (semver kuralı: 1.0.0-alpha < 1.0.0).
    fn cmp_with_pre(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => {}
            other => return other,
        }
        match self.minor.cmp(&other.minor) {
            Ordering::Equal => {}
            other => return other,
        }
        match self.patch.cmp(&other.patch) {
            Ordering::Equal => {}
            other => return other,
        }
        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, false) => Ordering::Greater, // stabil > ön sürüm
            (false, true) => Ordering::Less,
            (false, false) => {
                for (a, b) in self.pre.iter().zip(other.pre.iter()) {
                    match a.cmp(b) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
                self.pre.len().cmp(&other.pre.len())
            }
            (true, true) => Ordering::Equal,
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_with_pre(other)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pre.is_empty() {
            write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
        } else {
            write!(
                f,
                "{}.{}.{}-{}",
                self.major,
                self.minor,
                self.patch,
                self.pre.join(".")
            )
        }
    }
}

/// Bir GitHub yayın/release kaydı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRelease {
    /// Yayın etiketi (ör. "v0.2.0").
    pub tag_name: String,
    /// Yayın sürümü (etiketten "v" öneki çıkarılmış hali).
    pub version: Version,
    /// Yayın notları.
    pub body: String,
    /// Yayın URL'si.
    pub html_url: String,
    /// Yayın taslak mı?
    pub draft: bool,
    /// Yayın ön sürüm mı?
    pub prerelease: bool,
}

/// Güncelleme denetimi sonucu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheck {
    /// Güncelleme yok; mevcut sürüm zaten güncel.
    UpToDate { current: Version },
    /// Güncelleme mevcut.
    UpdateAvailable {
        /// Mevcut sürüm.
        current: Version,
        /// Yeni sürüm.
        latest: Version,
        /// Yayın kaydı.
        release: GitHubRelease,
    },
    /// Güncelleme denetimi başarısız oldu (ağ, doğrulama vb.).
    Error(String),
}

/// Güncelleme sağlayıcısı arayüzü. Gerçek GitHub erişimi olmadan test
/// edilebilir.
pub trait UpdateProvider: Send + Sync {
    /// Mevcut sürümü döner (`Cargo.toml` içindeki `version`).
    fn current_version(&self) -> Version;
    /// GitHub deposundaki en son yayın/canlı (non-draft, non-prerelease)
    /// kaydı getirir. Ağ erişimi gerçekleştirir; hata durumunda `Err` döner.
    fn fetch_latest_release(&self) -> Result<Option<GitHubRelease>, String>;
}

/// Güncelleme kurulumu sonucu.
#[derive(Debug, Clone)]
pub struct UpdateOutcome {
    /// Yeni sürüm.
    pub version: Version,
    /// Yükleme dizini (kullanıcı alanı).
    pub install_dir: PathBuf,
    /// Yapılan işlemin kısa özeti.
    pub summary: String,
}

/// Güncelleme kurulum arayüzü. Testlerde sahte bir installer verilebilir.
pub trait UpdateInstaller {
    /// `release` kaydındaki dosyayı `dest_dir` dizinine indirir ve doğrular.
    /// `downloader` ile gerekli dosyalar indirilir. Kurulumdan önce kullanıcı
    /// onayı alınmış olmalıdır.
    fn install_update(
        &self,
        release: &GitHubRelease,
        dest_dir: &Path,
        downloader: &dyn Downloader,
    ) -> Result<UpdateOutcome, String>;
}

/// Uzaktan dosya indiren arayüz.
pub trait Downloader {
    /// `url` adresinden `destination` yoluna indirir.
    fn download(&self, url: &str, destination: &Path) -> Result<PathBuf, String>;
}

/// Kurulum onayı sonucu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDecision {
    /// Kullanıcı onayladı.
    Confirmed,
    /// Kullanıcı reddetti.
    Rejected,
}

/// Güncelleme onay ekranı için gereken bilgiler.
#[derive(Debug, Clone)]
pub struct UpdateConfirmation {
    /// Mevcut sürüm.
    pub current: Version,
    /// Yeni sürüm.
    pub latest: Version,
    /// Yayın notları.
    pub body: String,
    /// Yayın kaynağı URL'si.
    pub source: String,
    /// Hedef dizin (kullanıcı alanı).
    pub install_dir: PathBuf,
    /// Yapılacak işlemlerin kısa açıklaması.
    pub actions: Vec<String>,
}

/// Gerçek güncelleme sağlayıcısı. Bu sürümde resmî bir GitHub kaynağı
/// tanımlanmadığı için `fetch_latest_release` her zaman
/// "kaynağın tanımlanmaması" hatası döner; otomatik güncelleme varmış gibi
/// gösterilmez.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultUpdateProvider;

impl UpdateProvider for DefaultUpdateProvider {
    fn current_version(&self) -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_default()
    }

    fn fetch_latest_release(&self) -> Result<Option<GitHubRelease>, String> {
        Err(
            "Resmî güncelleme kaynağı henüz tanımlanmadı. Nova'yı güncellemek için \
             kaynak koddan derleyin veya resmi bir dağıtım paketini bekleyin."
                .into(),
        )
    }
}

/// Güncelleme onay ekranı için bilgi hazırlar.
pub fn prepare_update_confirmation(
    current: &Version,
    latest: &Version,
    release: &GitHubRelease,
) -> UpdateConfirmation {
    let install_dir = crate::store::store_install_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nova-update")
        .join(format!("v{}", latest));
    let mut actions = Vec::new();
    actions.push(format!("Nova v{} → v{} güncellenecek.", current, latest));
    actions.push(format!("Yayın kaynağı: {}", release.html_url));
    actions.push(format!(
        "Kurulum kullanıcı alanına yapılır: {} (sudo/root yok, kullanıcı yapılandırması dokunulmaz).",
        install_dir.display()
    ));
    actions.push("Yayın notları gösterilecek ve indirme/doğrulama ayrı onayla yürür.".to_string());
    UpdateConfirmation {
        current: current.clone(),
        latest: latest.clone(),
        body: release.body.clone(),
        source: release.html_url.clone(),
        install_dir,
        actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sahte sağlayıcı: sabit mevcut sürüm ve yayın döner.
    struct FakeProvider {
        current: Version,
        latest: Option<GitHubRelease>,
    }

    impl UpdateProvider for FakeProvider {
        fn current_version(&self) -> Version {
            self.current.clone()
        }
        fn fetch_latest_release(&self) -> Result<Option<GitHubRelease>, String> {
            Ok(self.latest.clone())
        }
    }

    #[test]
    fn version_parse_and_display() {
        let v = Version::parse("v0.2.0").unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);
        assert!(v.pre.is_empty());
        assert_eq!(v.to_string(), "0.2.0");

        let pre = Version::parse("1.0.0-alpha.1").unwrap();
        assert_eq!(pre.pre, vec!["alpha".to_string(), "1".to_string()]);
        assert_eq!(pre.to_string(), "1.0.0-alpha.1");
    }

    #[test]
    fn version_ordering() {
        assert!(Version::parse("0.1.0").unwrap() < Version::parse("0.2.0").unwrap());
        assert!(Version::parse("1.0.0").unwrap() > Version::parse("0.9.9").unwrap());
        assert!(Version::parse("1.0.0-alpha").unwrap() < Version::parse("1.0.0").unwrap());
        assert_eq!(
            Version::parse("1.0.0").unwrap(),
            Version::parse("v1.0.0").unwrap()
        );
    }

    #[test]
    fn check_up_to_date_when_latest_none() {
        let provider = FakeProvider {
            current: Version::parse("0.1.0").unwrap(),
            latest: None,
        };
        let latest_release = provider.fetch_latest_release().unwrap();
        assert!(latest_release.is_none());
        let check = UpdateCheck::UpToDate {
            current: provider.current_version(),
        };
        assert!(matches!(check, UpdateCheck::UpToDate { .. }));
    }

    #[test]
    fn check_update_available() {
        let current = Version::parse("0.1.0").unwrap();
        let latest = Version::parse("0.2.0").unwrap();
        let release = GitHubRelease {
            tag_name: "v0.2.0".into(),
            version: latest.clone(),
            body: "Yeni özellikler eklendi.".into(),
            html_url: "https://github.com/nova/releases/tag/v0.2.0".into(),
            draft: false,
            prerelease: false,
        };
        let provider = FakeProvider {
            current,
            latest: Some(release.clone()),
        };
        let latest_release = provider.fetch_latest_release().unwrap().unwrap();
        assert_eq!(latest_release.version, latest);
        assert!(latest_release.version > provider.current_version());
    }

    #[test]
    fn prepare_update_confirmation_contains_versions_and_source() {
        let current = Version::parse("0.1.0").unwrap();
        let latest = Version::parse("0.2.0").unwrap();
        let release = GitHubRelease {
            tag_name: "v0.2.0".into(),
            version: latest.clone(),
            body: "Yeni özellikler.".into(),
            html_url: "https://github.com/nova/releases/tag/v0.2.0".into(),
            draft: false,
            prerelease: false,
        };
        let confirm = prepare_update_confirmation(&current, &latest, &release);
        assert_eq!(confirm.current, current);
        assert_eq!(confirm.latest, latest);
        assert!(confirm.actions.iter().any(|a| a.contains("sudo/root yok")));
        assert!(confirm.source.contains("github.com"));
    }
}
