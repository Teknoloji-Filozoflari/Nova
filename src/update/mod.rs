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
    /// Linux x86_64 kurulum varlığının adı (varsa).
    pub asset_name: Option<String>,
    /// Linux x86_64 kurulum varlığının https indirme adresi (varsa).
    pub download_url: Option<String>,
}

/// Güncelleme denetimi sonucu.
///
/// Nova **iki** yolu destekler (birbirinin yerine geçmez):
/// 1. GitHub Releases ikili self-update (varsa)
/// 2. Repo `main` üzerinden `cargo install --git` (ağ + derleme gerekir)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheck {
    /// Güncelleme yok; mevcut sürüm yayınlanan sürümle aynı veya daha yeni.
    /// Repo yolu hâlâ `cargo install --git` ile denenebilir.
    UpToDate { current: Version },
    /// Depoda henüz (kararlı) GitHub Release yok — ikili self-update yapılamaz.
    /// Repo yolu (`cargo install --git`) önerilir.
    NoPublishedRelease { current: Version },
    /// Güncelleme mevcut.
    UpdateAvailable {
        /// Mevcut sürüm.
        current: Version,
        /// Yeni sürüm.
        latest: Version,
        /// Yayın kaydı.
        release: Box<GitHubRelease>,
    },
    /// Güncelleme denetimi başarısız oldu (ağ, doğrulama vb.).
    Error(String),
}

/// Repo (kaynak) güncellemesinin özeti — UI’da dürüstçe gösterilir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoUpdatePlan {
    /// Git HTTPS URL.
    pub git_url: String,
    /// Dal adı.
    pub branch: String,
    /// Çalıştırılacak komut (görüntü / onay).
    pub command_display: String,
    /// Kullanıcıya uyarı: ağ + derleme gerekir.
    pub warnings: Vec<String>,
}

impl Default for RepoUpdatePlan {
    fn default() -> Self {
        Self::for_nova_main()
    }
}

impl RepoUpdatePlan {
    /// Nova `main` dalı için varsayılan plan.
    pub fn for_nova_main() -> Self {
        let git_url = std::env::var("NOVA_GIT_SOURCE_URL")
            .unwrap_or_else(|_| NOVA_GIT_SOURCE_URL.to_string());
        let branch = std::env::var("NOVA_GIT_BRANCH")
            .unwrap_or_else(|_| NOVA_GIT_DEFAULT_BRANCH.to_string());
        let command_display =
            format!("cargo install --git {git_url} --branch {branch} --force --locked");
        Self {
            git_url,
            branch,
            command_display,
            warnings: vec![
                "Ağ gerekir: GitHub’dan kaynak çekilir.".into(),
                "Derleme gerekir: Rust toolchain (cargo) ile yerelde derlenir; birkaç dakika sürebilir.".into(),
                "Kurulum hedefi genellikle ~/.cargo/bin/nova (PATH’te olmalı).".into(),
                "GitHub Releases ikili yolu ayrıdır; bu komut Releases etiketi beklemez.".into(),
            ],
        }
    }
}

/// `cargo install --git` ile Nova’yı main (veya seçili dal) üzerinden kurar.
///
/// Kullanıcı onayı alınmış olmalıdır. `sudo` kullanılmaz.
pub fn install_nova_from_git(plan: &RepoUpdatePlan) -> Result<UpdateOutcome, String> {
    which_cargo()?;
    let status = std::process::Command::new("cargo")
        .args([
            "install",
            "--git",
            &plan.git_url,
            "--branch",
            &plan.branch,
            "--force",
            "--locked",
        ])
        .status()
        .map_err(|e| format!("cargo başlatılamadı: {e}"))?;
    if !status.success() {
        return Err(format!(
            "cargo install başarısız (çıkış {:?}). Ağ / toolchain / derleme hatası olabilir.",
            status.code()
        ));
    }
    let version = Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_default();
    let bin_dir = crate::store::path::user_local_bin_dir()
        .or_else(|| {
            std::env::var_os("CARGO_HOME")
                .map(PathBuf::from)
                .map(|p| p.join("bin"))
        })
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".cargo/bin")
        });
    Ok(UpdateOutcome {
        version,
        install_dir: bin_dir,
        summary: format!(
            "Nova kaynak güncellemesi tamamlandı (`{}`). Yeni kabuk veya PATH yenilemesi gerekebilir.",
            plan.command_display
        ),
    })
}

fn which_cargo() -> Result<PathBuf, String> {
    which::which("cargo").map_err(|_| {
        "cargo bulunamadı — kaynak güncellemesi için Rust toolchain kurulu olmalı".into()
    })
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

/// Nova'nın izlediği GitHub deposu.
pub const NOVA_GITHUB_REPO: &str = "Teknoloji-Filozoflari/Nova";
/// Kaynak (repo) güncellemesi için varsayılan dal.
pub const NOVA_GIT_DEFAULT_BRANCH: &str = "main";
/// `cargo install --git` kaynak URL’si.
pub const NOVA_GIT_SOURCE_URL: &str = "https://github.com/Teknoloji-Filozoflari/Nova.git";

/// Gerçek güncelleme sağlayıcısı: GitHub Releases (`NOVA_GITHUB_REPO`).
///
/// Yayın yoksa (`404`) `Ok(None)` döner — bu "güncelsiniz / yayın yok"
/// anlamına gelir; hata sayılmaz. İkili indirme için yayın varlıkları
/// arasından Linux x86_64 adayı seçilir ([`pick_linux_asset`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultUpdateProvider;

impl UpdateProvider for DefaultUpdateProvider {
    fn current_version(&self) -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_default()
    }

    fn fetch_latest_release(&self) -> Result<Option<GitHubRelease>, String> {
        fetch_nova_latest_release(NOVA_GITHUB_REPO)
    }
}

/// GitHub Releases API'den Nova yayınını okur.
pub fn fetch_nova_latest_release(repo: &str) -> Result<Option<GitHubRelease>, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(45)))
        .build()
        .into();
    let mut request = agent
        .get(&url)
        .header("User-Agent", "nova-update")
        .header("Accept", "application/vnd.github+json");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.trim().is_empty() {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
    }
    let mut response = match request.call() {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            // Yayın yoksa GitHub 404 döner; bunu hata değil "yayın yok" say.
            if msg.contains("404") {
                return Ok(None);
            }
            return Err(format!("GitHub yayınları okunamadı ({repo}): {e}"));
        }
    };
    let status = response.status().as_u16();
    if status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(format!(
            "GitHub yayınları okunamadı ({repo}): HTTP {status}"
        ));
    }
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("GitHub yanıtı okunamadı: {e}"))?;
    parse_github_release_json(&body)
}

/// GitHub release JSON gövdesini [`GitHubRelease`]'a çevirir.
pub fn parse_github_release_json(body: &str) -> Result<Option<GitHubRelease>, String> {
    #[derive(serde::Deserialize)]
    struct ApiRelease {
        tag_name: String,
        #[serde(default)]
        body: Option<String>,
        html_url: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
        #[serde(default)]
        assets: Vec<ApiAsset>,
    }
    #[derive(serde::Deserialize)]
    struct ApiAsset {
        name: String,
        browser_download_url: String,
    }
    let api: ApiRelease =
        serde_json::from_str(body).map_err(|e| format!("GitHub JSON okunamadı: {e}"))?;
    if api.draft || api.prerelease {
        return Ok(None);
    }
    let version = Version::parse(&api.tag_name)
        .map_err(|e| format!("yayın etiketi ayrıştırılamadı '{}': {e}", api.tag_name))?;
    let (asset_name, download_url) = pick_linux_asset_from(
        api.assets
            .iter()
            .map(|a| (a.name.as_str(), a.browser_download_url.as_str())),
    )
    .map(|(n, u)| (Some(n), Some(u)))
    .unwrap_or((None, None));
    Ok(Some(GitHubRelease {
        tag_name: api.tag_name,
        version,
        body: api.body.unwrap_or_default(),
        html_url: api.html_url,
        draft: api.draft,
        prerelease: api.prerelease,
        asset_name,
        download_url,
    }))
}

/// Yayın JSON'undan Linux x86_64 ikili/arşiv adayını seçer.
pub fn pick_linux_asset(body: &str) -> Result<(String, String), String> {
    #[derive(serde::Deserialize)]
    struct ApiRelease {
        #[serde(default)]
        assets: Vec<ApiAsset>,
    }
    #[derive(serde::Deserialize)]
    struct ApiAsset {
        name: String,
        browser_download_url: String,
    }
    let api: ApiRelease =
        serde_json::from_str(body).map_err(|e| format!("GitHub JSON okunamadı: {e}"))?;
    pick_linux_asset_from(
        api.assets
            .iter()
            .map(|a| (a.name.as_str(), a.browser_download_url.as_str())),
    )
}

/// Varlık listesinden en uygun Linux x86_64 adayını seçer.
pub fn pick_linux_asset_from<'a, I>(assets: I) -> Result<(String, String), String>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut scored: Vec<(i32, String, String)> = Vec::new();
    for (name, url) in assets {
        let n = name.to_lowercase();
        if !(n.contains("nova")
            || n.ends_with(".tar.gz")
            || n.ends_with(".tgz")
            || n == "nova"
            || n.ends_with(".zip"))
        {
            continue;
        }
        let mut score = 0;
        if n.contains("linux") {
            score += 5;
        }
        if n.contains("x86_64") || n.contains("amd64") || n.contains("x64") {
            score += 5;
        }
        if n.contains("nova") {
            score += 3;
        }
        if n.ends_with(".tar.gz") || n.ends_with(".tgz") {
            score += 2;
        }
        if n.contains("musl") || n.contains("gnu") {
            score += 1;
        }
        if n.contains("darwin") || n.contains("windows") || n.contains("macos") {
            score -= 20;
        }
        scored.push((score, name.to_string(), url.to_string()));
    }
    scored.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored
        .into_iter()
        .find(|(s, _, _)| *s > 0)
        .map(|(_, name, url)| (name, url))
        .ok_or_else(|| {
            "Yayında Linux x86_64 Nova ikilisi/arşivi bulunamadı. \
             Kaynak koddan derleyin veya uygun bir GitHub release varlığı ekleyin."
                .into()
        })
}

/// Kullanıcı alanına Nova ikilisi kurar (`~/.local/bin/nova`).
pub fn install_nova_from_url(
    version: &Version,
    download_url: &str,
    asset_name: &str,
) -> Result<UpdateOutcome, String> {
    if !download_url.starts_with("https://") {
        return Err("yalnızca https:// indirme adresleri kabul edilir".into());
    }
    let install_root = crate::store::store_install_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nova-update")
        .join(format!("v{version}"));
    std::fs::create_dir_all(&install_root)
        .map_err(|e| format!("kurulum dizini oluşturulamadı: {e}"))?;
    let archive_path = install_root.join(asset_name);
    crate::store::install::Downloader::download(
        &crate::store::http::HttpDownloader::new(),
        download_url,
        &archive_path,
    )
    .map_err(|e| format!("Nova indirme başarısız: {e}"))?;

    let bin_dir = crate::store::path::user_local_bin_dir().unwrap_or_else(dirs_fallback_local_bin);
    std::fs::create_dir_all(&bin_dir).map_err(|e| format!("~/.local/bin oluşturulamadı: {e}"))?;
    let dest = bin_dir.join("nova");

    let lower = asset_name.to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        extract_nova_binary_from_tar_gz(&archive_path, &dest)?;
    } else if lower.ends_with(".zip") {
        return Err("zip arşivleri henüz desteklenmiyor; tar.gz kullanın".into());
    } else {
        std::fs::copy(&archive_path, &dest).map_err(|e| format!("ikili kopyalanamadı: {e}"))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)
            .map_err(|e| format!("izinler okunamadı: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms)
            .map_err(|e| format!("çalıştırma izni verilemedi: {e}"))?;
    }

    Ok(UpdateOutcome {
        version: version.clone(),
        install_dir: install_root,
        summary: format!(
            "Nova v{version} kuruldu: {}. Yeni sürüm için yeni bir kabuk veya PATH yenilemesi gerekebilir.",
            dest.display()
        ),
    })
}

fn dirs_fallback_local_bin() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/bin")
}

fn extract_nova_binary_from_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("arşiv açılamadı: {e}"))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let mut found = false;
    for entry in archive
        .entries()
        .map_err(|e| format!("arşiv okunamadı: {e}"))?
    {
        let mut entry = entry.map_err(|e| format!("arşiv girişi okunamadı: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("arşiv yolu okunamadı: {e}"))?
            .to_path_buf();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name != "nova" {
            continue;
        }
        {
            let tmp = dest.with_extension("tmp");
            {
                let mut out = std::fs::File::create(&tmp)
                    .map_err(|e| format!("geçici ikili yazılamadı: {e}"))?;
                std::io::copy(&mut entry, &mut out)
                    .map_err(|e| format!("ikili çıkarılamadı: {e}"))?;
            }
            std::fs::rename(&tmp, dest).map_err(|e| {
                let _ = std::fs::remove_file(&tmp);
                format!("ikili yerine konamadı: {e}")
            })?;
        }
        found = true;
        break;
    }
    if !found {
        return Err("arşivde 'nova' adlı ikili bulunamadı".into());
    }
    Ok(())
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
            asset_name: Some("nova-v0.2.0-x86_64-unknown-linux-musl.tar.gz".into()),
            download_url: Some(
                "https://github.com/nova/releases/download/v0.2.0/nova.tar.gz".into(),
            ),
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
            asset_name: None,
            download_url: None,
        };
        let confirm = prepare_update_confirmation(&current, &latest, &release);
        assert_eq!(confirm.current, current);
        assert_eq!(confirm.latest, latest);
        assert!(confirm.actions.iter().any(|a| a.contains("sudo/root yok")));
        assert!(confirm.source.contains("github.com"));
    }

    #[test]
    fn pick_linux_asset_prefers_linux_x86_64_nova() {
        let (name, url) = pick_linux_asset_from([
            (
                "nova-v1.0.0-aarch64-apple-darwin.tar.gz",
                "https://example/darwin",
            ),
            (
                "nova-v1.0.0-x86_64-unknown-linux-musl.tar.gz",
                "https://example/linux",
            ),
            ("NOTES.txt", "https://example/notes"),
        ])
        .unwrap();
        assert!(name.contains("linux"));
        assert_eq!(url, "https://example/linux");
    }

    #[test]
    fn parse_release_json_without_assets_is_ok() {
        let body = r#"{
            "tag_name": "v0.3.0",
            "html_url": "https://github.com/Teknoloji-Filozoflari/Nova/releases/tag/v0.3.0",
            "body": "deneme",
            "draft": false,
            "prerelease": false,
            "assets": []
        }"#;
        let release = parse_github_release_json(body).unwrap().unwrap();
        assert_eq!(release.version.to_string(), "0.3.0");
        assert!(release.download_url.is_none());
    }
}
