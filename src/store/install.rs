//! Mağaza kurulum güvenliği modülü.
//!
//! İki kurulum yolu vardır:
//!
//! 1. **Arşiv (`download_archive`)**: yalnızca kullanıcı alanına. `sudo` /
//!    sistem dizinlerine yazma yok. Uzaktan betik shell üzerinden
//!    çalıştırılmaz (`curl | sh` yok). SHA-256 doğrulanır; arşiv yol geçişi
//!    korumalıdır.
//! 2. **Paket yöneticisi (`pipx`/`npm`/`cargo`/`brew`/`apt`/`dnf`/`pacman`/`uv`)**:
//!    katalogdaki `install_command` argv'si (veya host’a göre `install_notes`
//!    alternatifinden seçilen komut), kullanıcı onayından sonra **kabuksuz**
//!    çalıştırılır. `sh -c` / `curl | bash` kabul edilmez. `apt`/`dnf` için
//!    `sudo` argv'nin parçası olabilir; parola gerekiyorsa TUI askıya alınır
//!    ve komut gerçek TTY ile çalışır (sessiz yükseltme yok).
//!
//! Testlerde gerçek ağ veya dosya sistemi erişimi olmadan davranışı sınamak
//! için [`Downloader`] ve [`Installer`] trait'leri taklit edilebilir.

use crate::store::host::{self, HostTools, InstallPlan};
use crate::store::platform::CpuArch;
use crate::store::{format_install_command, validate_install_command, InstallMethod, StoreEntry};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// Bir arşivden çıkarılacak en fazla dosya sayısı (zip bomb koruması).
const MAX_ENTRIES: usize = 10_000;
/// Bir arşivden çıkarılacak en fazla toplam boyut (byte; zip bomb koruması).
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// Çalıştırılabilir dosya aranırken en fazla ineceklebilir dizin derinliği.
const MAX_SEARCH_DEPTH: usize = 4;

/// Mağaza kurulum hataları. Kullanıcıya anlaşılır bir biçimde gösterilir.
#[derive(Debug, Clone)]
pub enum InstallError {
    /// Ağ bağlantısı veya indirme hatası.
    Network(String),
    /// İndirme veya doğrulama hatası (checksum uyuşmazlığı vb.).
    Download(String),
    /// Arşiv hatası (bozuk arşiv, yol geçişi denemesi).
    Archive(String),
    /// İzin hatası.
    Permission(String),
    /// Kullanıcı tarafından reddedilen işlem.
    Cancelled,
    /// Güvenlik kuralı ihlali (ör. sistem dizinine yazma).
    Security(String),
    /// Diğer iç hata.
    Internal(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Network(msg) => write!(f, "ağ hatası: {msg}"),
            InstallError::Download(msg) => write!(f, "indirme hatası: {msg}"),
            InstallError::Archive(msg) => write!(f, "arşiv hatası: {msg}"),
            InstallError::Permission(msg) => write!(f, "izin hatası: {msg}"),
            InstallError::Cancelled => write!(f, "kullanıcı tarafından iptal edildi"),
            InstallError::Security(msg) => write!(f, "güvenlik kuralı ihlali: {msg}"),
            InstallError::Internal(msg) => write!(f, "iç hata: {msg}"),
        }
    }
}

impl std::error::Error for InstallError {}

/// Kurulum sonucu.
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    /// Kurulan uygulamanın kimliği.
    pub app_id: String,
    /// Kurulan uygulamanın adı.
    pub app_name: String,
    /// Kurulan sürüm (gömülü katalogdan veya canlı çözümlemeden).
    pub version: String,
    /// Kurulan çalıştırılabilir dosyanın tam yolu.
    pub executable_path: PathBuf,
    /// Hedef dizin (kullanıcı alanı).
    pub install_dir: PathBuf,
    /// Yapılan işlemin kısa özeti.
    pub summary: String,
}

/// Uzaktan dosya indiren arayüz. Testlerde sahte bir downloader verilebilir.
pub trait Downloader {
    /// `url` adresinden `destination` yoluna indirir. İndirilen dosyanın
    /// yolunu döner.
    fn download(&self, url: &str, destination: &Path) -> Result<PathBuf, InstallError>;
}

/// Uygulamayı kuran arayüz. Testlerde sahte bir installer verilebilir.
pub trait Installer {
    /// `entry` uygulamasını `dest_dir` dizinine kurar. `downloader` ile
    /// gerekli dosyalar indirilir. Kurulumdan önce kullanıcı onayı
    /// alınmış olmalıdır.
    fn install(
        &self,
        entry: &StoreEntry,
        dest_dir: &Path,
        downloader: &dyn Downloader,
    ) -> Result<InstallOutcome, InstallError>;
}

/// Gerçek kurulum uygulaması.
#[derive(Debug, Clone, Copy, Default)]
pub struct StoreInstaller;

impl Installer for StoreInstaller {
    fn install(
        &self,
        entry: &StoreEntry,
        dest_dir: &Path,
        downloader: &dyn Downloader,
    ) -> Result<InstallOutcome, InstallError> {
        install_app(entry, dest_dir, downloader)
    }
}

/// Kurulum onayı sonucu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmDecision {
    /// Kullanıcı onayladı.
    Confirmed,
    /// Kullanıcı reddetti.
    Rejected,
}

/// Kurulum kapsamı: kullanıcı alanı mı, sistem PM mi?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallScope {
    /// `~/.local` / pipx / npm -g kullanıcı / brew vb.
    User,
    /// apt / dnf / pacman (sistem paketleri).
    System,
}

impl InstallScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "kullanıcı alanı",
            Self::System => "sistem (paket yöneticisi)",
        }
    }

    pub fn for_method(method: InstallMethod) -> Self {
        if method.needs_elevated_tty() {
            Self::System
        } else {
            Self::User
        }
    }
}

/// Kurulum onay ekranı için gereken bilgiler.
#[derive(Debug, Clone)]
pub struct InstallConfirmation {
    /// Uygulama adı.
    pub app_name: String,
    /// Uygulama sürümü (bilinmiyorsa boş).
    pub version: String,
    /// Kaynak/proje sayfası.
    pub source: String,
    /// Lisans.
    pub license: String,
    /// Hedef dizin (kullanıcı alanı; arşiv).
    pub install_dir: PathBuf,
    /// Yapılacak işlemlerin kısa açıklaması.
    pub actions: Vec<String>,
    /// Onayda gösterilecek tam komut (argv veya indirme URL’si).
    pub full_command: String,
    /// Paket yöneticisi / yöntem kısa adı.
    pub package_manager: String,
    /// Kullanıcı vs sistem kapsamı.
    pub scope: InstallScope,
    /// `sudo` kullanılacak mı?
    pub uses_sudo: bool,
    /// Üçüncü taraf kod uyarısı.
    pub third_party_risk: String,
    /// Seçilen CPU mimarisi.
    pub arch: String,
}

impl InstallConfirmation {
    /// Onay modalında gösterilecek çok satırlı metin.
    pub fn question_text(&self) -> String {
        let sudo = if self.uses_sudo { "evet" } else { "hayır" };
        let version = if self.version.trim().is_empty() {
            "bilinmiyor".to_string()
        } else {
            self.version.clone()
        };
        let mut lines = vec![
            format!("'{}' kurulsun mu?", self.app_name),
            format!("Komut: {}", self.full_command),
            format!("Paket yöneticisi: {}", self.package_manager),
            format!("Kapsam: {}", self.scope.label()),
            format!("sudo?: {sudo}"),
            format!("Mimari: {}", self.arch),
            format!("Sürüm: {version}"),
            format!("Kaynak: {}", self.source),
            format!("Lisans: {}", self.license),
            format!("Üçüncü taraf: {}", self.third_party_risk),
        ];
        for action in &self.actions {
            if !lines.iter().any(|l| l.contains(action)) {
                lines.push(action.clone());
            }
        }
        lines.push("Enter onaylar, Esc vazgeçer.".into());
        lines.join("\n")
    }
}

/// Arşiv kurulumundan sonra yazılan dosya manifest’i (kaldırma / geri alma).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallFileManifest {
    pub app_id: String,
    pub version: String,
    pub arch: String,
    pub method: String,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
    pub installed_at_unix: u64,
    /// Kurulum dizinine göre göreli dosya yolları.
    pub files: Vec<String>,
    /// `~/.local/bin` shim adı (varsa).
    #[serde(default)]
    pub shim_name: Option<String>,
}

impl InstallFileManifest {
    /// `<store>/<id>/install-manifest.toml` yolu.
    pub fn path_for(app_id: &str) -> Option<PathBuf> {
        Some(crate::store::store_app_dir(app_id)?.join("install-manifest.toml"))
    }

    pub fn load(app_id: &str) -> Option<Self> {
        let path = Self::path_for(app_id)?;
        let text = fs::read_to_string(path).ok()?;
        toml::from_str(&text).ok()
    }

    pub fn save(&self, install_dir: &Path) -> Result<(), InstallError> {
        let path = install_dir.join("install-manifest.toml");
        let text = toml::to_string_pretty(self).map_err(|e| {
            InstallError::Internal(format!("kurulum manifest’i serileştirilemedi: {e}"))
        })?;
        fs::write(&path, text).map_err(|e| {
            InstallError::Permission(format!(
                "kurulum manifest’i yazılamadı '{}': {e}",
                path.display()
            ))
        })
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Kısmi arşiv kurulumunu geri alır (yazılan dosyalar + shim + boş dizinler).
pub fn rollback_archive_install(
    install_dir: &Path,
    files: &[String],
    shim_name: Option<&str>,
) -> Result<(), InstallError> {
    if let Some(name) = shim_name {
        let _ = crate::store::path::remove_path_shim(name);
    }
    for rel in files.iter().rev() {
        let path = install_dir.join(rel);
        if path.is_file() || path.is_symlink() {
            let _ = fs::remove_file(&path);
        } else if path.is_dir() {
            let _ = fs::remove_dir_all(&path);
        }
    }
    let manifest = install_dir.join("install-manifest.toml");
    let _ = fs::remove_file(&manifest);
    // Boş kurulum dizinini temizle (üst store/ kalsın).
    let _ = fs::remove_dir(install_dir);
    Ok(())
}

/// Bir dosyanın SHA-256 özetini hesaplar (onaltılık, küçük harf).
pub fn sha256_file(path: &Path) -> Result<String, InstallError> {
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
    Ok(hex(&hasher.finalize()))
}

/// Bayt dizisini onaltılık metne çevirir.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// SHA-256 checksum doğrular.
///
/// `expected` boşsa doğrulama **atlanmaz**, hata döner: indirilebilir bir
/// kayıt için checksum zorunludur (bkz. [`crate::store::Store::from_entries`]).
/// Karşılaştırma büyük/küçük harf duyarsızdır.
pub fn verify_checksum(file: &Path, expected: &str) -> Result<(), InstallError> {
    let expected = expected.trim();
    if expected.is_empty() {
        return Err(InstallError::Security(
            "checksum tanımlanmamış; doğrulanamayan indirme kurulmaz.".into(),
        ));
    }
    if !crate::store::is_sha256_hex(expected) {
        return Err(InstallError::Download(format!(
            "checksum geçersiz: '{}' (SHA-256 64 onaltılık karakter olmalı)",
            expected
        )));
    }
    let actual = sha256_file(file)?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(InstallError::Download(format!(
            "checksum uyuşmazlığı: beklenen {}, bulunan {}",
            expected.to_lowercase(),
            actual
        )))
    }
}

/// `src` dizinindeki dosyaları `dest` dizinine çıkarır.
///
/// # Güvenlik
///
/// - **Yol geçişi (path traversal)**: `..` bileşeni içeren veya mutlak yola
///   çıkan girdiler reddedilir; çıkarma `dest` dışına çıkamaz.
/// - **Sembolik bağ**: sembolik ve hard link girdileri **reddedilir**
///   (dışarıya bağlantı / zip-slip kaçış yolu engellenir).
/// - **Kaynak tüketimi**: girdi sayısı ve toplam boyut sınırlıdır.
/// - Dosya izinleri korunur; çalıştırılabilir biti kaybolmaz.
pub fn extract_archive(src: &Path, dest: &Path) -> Result<(), InstallError> {
    let file = fs::File::open(src)
        .map_err(|e| InstallError::Archive(format!("'{}' açılamadı: {e}", src.display())))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    fs::create_dir_all(dest).map_err(|e| {
        InstallError::Permission(format!("'{}' oluşturulamadı: {e}", dest.display()))
    })?;
    // Çıkarma kökü: bütün yollar buna göre çözülür.
    let root = dest
        .canonicalize()
        .map_err(|e| InstallError::Internal(format!("'{}' çözümlenemedi: {e}", dest.display())))?;

    let mut entries = 0usize;
    let mut total_bytes = 0u64;

    for entry in archive
        .entries()
        .map_err(|e| InstallError::Archive(format!("arşiv okunamadı: {e}")))?
    {
        entries += 1;
        if entries > MAX_ENTRIES {
            return Err(InstallError::Archive(format!(
                "arşiv çok fazla girdi içeriyor ({}'den fazla)",
                MAX_ENTRIES
            )));
        }
        let mut entry =
            entry.map_err(|e| InstallError::Archive(format!("arşiv girdisi okunamadı: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| InstallError::Archive(format!("geçersiz arşiv yolu: {e}")))?
            .into_owned();

        // Sembolik bağ ve hard link reddedilir (zip-slip / escape yolu).
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(InstallError::Security(format!(
                "arşiv sembolik/hard link içeriyor (reddedildi): '{}'",
                path.display()
            )));
        }

        let target = safe_join(&root, &path)?;
        if let Ok(size) = entry.header().size() {
            total_bytes = total_bytes.saturating_add(size);
            if total_bytes > MAX_TOTAL_BYTES {
                return Err(InstallError::Archive(format!(
                    "arşiv çok büyük ({} bayt sınırı aşıldı)",
                    MAX_TOTAL_BYTES
                )));
            }
        }

        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(|e| {
                InstallError::Permission(format!("'{}' oluşturulamadı: {e}", target.display()))
            })?;
            continue;
        }
        if !kind.is_file() {
            // Cihaz, FIFO gibi özel türler yok sayılır.
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                InstallError::Permission(format!("'{}' oluşturulamadı: {e}", parent.display()))
            })?;
        }

        let mut out = fs::File::create(&target).map_err(|e| {
            InstallError::Permission(format!("'{}' yazılamadı: {e}", target.display()))
        })?;
        std::io::copy(&mut entry, &mut out).map_err(|e| {
            InstallError::Archive(format!("'{}' çıkarılamadı: {e}", target.display()))
        })?;
        drop(out);

        // İzinleri koru (çalıştırılabilir bit dâhil).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mode) = entry.header().mode() {
                let mode = mode & 0o777;
                if mode != 0 {
                    let _ = fs::set_permissions(&target, fs::Permissions::from_mode(mode));
                }
            }
        }
    }

    Ok(())
}

/// Bir arşiv yolunu `root` altında güvenli biçimde çözer.
///
/// Mutlak yollar ve `..` bileşenleri reddedilir; sonuç her zaman `root`
/// içinde kalır.
fn safe_join(root: &Path, path: &Path) -> Result<PathBuf, InstallError> {
    let mut target = root.to_path_buf();
    for component in path.components() {
        match component {
            Component::Normal(part) => target.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(InstallError::Security(format!(
                    "arşiv yolu dizin dışına çıkmaya çalışıyor: '{}'",
                    path.display()
                )))
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(InstallError::Security(format!(
                    "arşiv yolu mutlak: '{}'",
                    path.display()
                )))
            }
        }
    }
    // Son savunma: çözülen yol kökün altında kalmalı.
    if !target.starts_with(root) {
        return Err(InstallError::Security(format!(
            "arşiv yolu hedef dizin dışına çıkıyor: '{}'",
            path.display()
        )));
    }
    Ok(target)
}

/// Çıkarılmış ağacın içinde verilen adda çalıştırılabilir dosyayı bulur.
///
/// Arşivlerin çoğu dosyayı bir üst dizine koyar (`ripgrep-15.2.0-.../rg`);
/// bazıları kök dizine (`fzf`). Bu yüzden sınırlı derinlikte bir arama
/// yapılır. Bulunan dosya çalıştırılabilir değilse `None` döner.
pub fn find_executable(root: &Path, name: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
        if depth > MAX_SEARCH_DEPTH {
            return None;
        }
        let entries = fs::read_dir(dir).ok()?;
        let mut dirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name();
            if file_name == OsStr::new(name) && path.is_file() {
                return Some(path);
            }
            if path.is_dir() {
                dirs.push(path);
            }
        }
        for dir in dirs {
            if let Some(found) = walk(&dir, name, depth + 1) {
                return Some(found);
            }
        }
        None
    }
    walk(root, name, 0)
}

/// Hedef dizinin kullanıcı alanında olup olmadığını kontrol eder.
///
/// `dest` ev dizini (`$HOME`) altında veya Nova'nın kendi veri dizini altında
/// olmalıdır. Sistem dizinleri (`/`, `/usr`, `/bin`, `/etc`, `/opt`, `/var`)
/// reddedilir.
///
/// Dizin henüz oluşmamış olabilir (ilk kurulum). Bu yüzden yol önce
/// sözlü olarak normalleştirilir, ardından var olan en derin atası
/// çözümlenerek hedefin gerçek konumu belirlenir.
pub fn is_user_area(dest: &Path) -> bool {
    let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) else {
        return false;
    };
    let Ok(home) = PathBuf::from(home).canonicalize() else {
        return false;
    };
    let Some(full) = resolve_existing_prefix(dest) else {
        return false;
    };
    if full.starts_with(&home) {
        return true;
    }
    // Nova'nın kendi veri dizini (`XDG_DATA_HOME` ev dışında olabilir).
    if let Some(store_dir) = crate::store::store_install_dir() {
        if let Some(store_full) = resolve_existing_prefix(&store_dir) {
            if full.starts_with(&store_full) {
                return true;
            }
        }
    }
    false
}

/// Bir yolu sözlü olarak normalleştirir (`..` ve `.` bileşenlerini çözer).
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut parts: Vec<OsString> = Vec::new();
    let mut rooted = false;
    for component in path.components() {
        match component {
            Component::Prefix(_) => {}
            Component::RootDir => rooted = true,
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(part) => parts.push(part.to_os_string()),
        }
    }
    let mut out = PathBuf::new();
    if rooted {
        out.push("/");
    }
    for part in parts {
        out.push(part);
    }
    out
}

/// Bir yolun var olan en derin atasını çözümler ve kalan bileşenleri ekler.
///
/// `None` dönerse yolun hiçbir atası çözümlenemedi (ör. kök dizin yok).
fn resolve_existing_prefix(path: &Path) -> Option<PathBuf> {
    let normalized = normalize_lexically(path);
    let mut current = normalized.as_path();
    let mut tail: Vec<&OsStr> = Vec::new();
    loop {
        if current.exists() {
            let mut full = current.canonicalize().ok()?;
            for part in tail.iter().rev() {
                full.push(part);
            }
            return Some(full);
        }
        let name = current.file_name()?;
        let parent = current.parent()?;
        if parent == current {
            return None;
        }
        tail.push(name);
        current = parent;
    }
}

/// Host’a göre seçilmiş kurulum planı (yoksa katalog birincili).
pub fn resolve_entry_install_plan(entry: &StoreEntry) -> Option<InstallPlan> {
    host::resolve_install_plan(entry, &HostTools::detect(), host::detect_host_os())
}

/// Kurulum onay ekranı için bilgi hazırlar.
pub fn prepare_confirmation(entry: &StoreEntry) -> InstallConfirmation {
    let install_dir = crate::store::store_app_dir(&entry.id).unwrap_or_else(|| PathBuf::from("."));
    let arch = CpuArch::host().unwrap_or(CpuArch::X86_64);
    let mut actions = Vec::new();
    let third_party_risk = format!(
        "Üçüncü taraf kod çalışacak (kaynak: {}). Nova yalnızca kataloğu ve \
         (arşivde) SHA-256’yı doğrular; yazılımın kendisine güvenirsiniz.",
        entry.source
    );

    let (full_command, package_manager, scope, uses_sudo) = if entry
        .install_method
        .is_package_manager()
    {
        let plan = resolve_entry_install_plan(entry).unwrap_or(InstallPlan {
            method: entry.install_method,
            argv: entry.install_command.clone().unwrap_or_default(),
        });
        let rendered = plan.rendered();
        actions.push(format!(
            "Onay sonrası kabuksuz komut çalıştırılacak: {rendered}"
        ));
        let host_note = if plan.method != entry.install_method {
            format!(
                " (katalog: {}, host: {})",
                entry.install_method.short_label(),
                plan.method.short_label()
            )
        } else {
            String::new()
        };
        actions.push(format!(
            "Yöntem: {}{host_note} · executable tespiti: '{}'.",
            plan.method.short_label(),
            entry.executable
        ));
        if plan.method.needs_elevated_tty() {
            if plan.needs_tty_suspend() {
                actions.push(
                    "sudo parola gerekecek: onaydan sonra Nova kısa süre TUI’yi \
                     bırakır, parolayı bu terminalde sorar, sonra geri döner."
                        .into(),
                );
            } else if plan.uses_sudo() {
                actions.push(
                    "sudo kimlik bilgisi önbellekte / NOPASSWD; arka planda \
                     kabuksuz çalıştırılacak."
                        .into(),
                );
            }
        }
        if let Some(notes) = entry.install_notes.as_deref() {
            let alt = notes.lines().take(3).collect::<Vec<_>>().join(" · ");
            if !alt.is_empty() {
                actions.push(format!("Notlar: {alt}"));
            }
        }
        let class = entry.trust_class();
        actions.push(format!("Güven: {}", entry.trust_display()));
        if let Some(note) = class.risk_note() {
            actions.push(format!("Risk: {note}"));
        }
        (
            rendered,
            plan.method.short_label().to_string(),
            InstallScope::for_method(plan.method),
            plan.uses_sudo(),
        )
    } else {
        actions.push(format!(
            "'{}' indirilecek ve '{}' dizinine çıkarılacak.",
            entry.name,
            install_dir.display()
        ));
        actions.push(format!(
            "Executable: '{}' (ayrı argüman olarak çalıştırılacak, kabuk yok).",
            entry.executable
        ));
        let (url, sum) = entry.resolve_download_for(arch).unwrap_or_else(|_| {
            (
                entry.download_url.clone().unwrap_or_default(),
                entry.checksum.clone().unwrap_or_default(),
            )
        });
        let full_command = if url.is_empty() {
            format!("download_archive → {}", install_dir.display())
        } else {
            format!("download → {url}")
        };
        if !sum.is_empty() {
            actions.push(format!(
                "İndirme sonrası SHA-256 checksum doğrulanacak: {}.",
                sum.to_lowercase()
            ));
        } else {
            actions.push(
                "Uygulama için upstream checksum sağlanmamış; kurulum onay sonrası yürür.".into(),
            );
        }
        actions
            .push("Kurulan dosyalar install-manifest.toml ile izlenir; kaldırma geri alır.".into());
        let class = entry.trust_class();
        actions.push(format!("Güven: {}", entry.trust_display()));
        if let Some(note) = class.risk_note() {
            actions.push(format!("Risk: {note}"));
        }
        if let Some(sig) = &entry.signature {
            actions.push(format!(
                "Upstream imza ({}) mümkünse doğrulanacak.",
                sig.kind.label()
            ));
        }
        (
            full_command,
            entry.install_method.short_label().to_string(),
            InstallScope::User,
            false,
        )
    };
    InstallConfirmation {
        app_name: entry.name.clone(),
        version: entry.version.clone().unwrap_or_default(),
        source: entry.source.clone(),
        license: if entry.license.trim().is_empty() {
            "belirtilmemiş".into()
        } else {
            entry.license.clone()
        },
        install_dir,
        actions,
        full_command,
        package_manager,
        scope,
        uses_sudo,
        third_party_risk,
        arch: arch.as_str().to_string(),
    }
}

/// `entry` uygulamasını `dest_dir` dizinine kurar (**arşiv yolu**).
///
/// Paket yöneticisi kayıtları için [`install_via_package_manager`] kullanın.
///
/// # Güvenlik kuralları
///
/// 1. `dest_dir` kullanıcı alanında olmalıdır ([`is_user_area`]).
/// 2. `sudo`/root kullanılmaz, sistem dizinlerine yazılmaz.
/// 3. Kabuk (`sh -c`) çalıştırılmaz; program ve argümanlar ayrı ayrı geçirilir.
/// 4. Checksum **doğrulanmadan** hiçbir dosya kurulmaz.
/// 5. Arşiv çıkarma yol geçişi korumalıdır ([`extract_archive`]).
///
/// `downloader` gerçek ağ erişimi olmadan test edilebilir.
pub fn install_app(
    entry: &StoreEntry,
    dest_dir: &Path,
    downloader: &dyn Downloader,
) -> Result<InstallOutcome, InstallError> {
    if entry.install_method.is_package_manager() {
        return Err(InstallError::Internal(
            "paket yöneticisi kaydı için install_via_package_manager kullanın".into(),
        ));
    }
    let source = InstallSource::from_entry(entry)?;
    let version = entry.version.clone().unwrap_or_default();
    install_with_source(entry, dest_dir, &source, &version, downloader)
}

/// PM sürecinin stdio bağlama biçimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmIoMode {
    /// stdout/stderr yakalanır (pipx/npm/cargo/brew ve önbellekli sudo).
    Captured,
    /// stdin/stdout/stderr miras alınır (sudo parola / gerçek TTY).
    Inherited,
}

/// Katalog / host planındaki argv'yi kabuksuz çalıştırır.
///
/// Kullanıcı onayı **çağıran** tarafta alınmış olmalıdır. `curl | bash`,
/// `sh -c` ve benzeri kalıplar [`validate_install_command`] ile reddedilir.
/// Host’ta birden fazla seçenek varsa [`resolve_entry_install_plan`] ile
/// OS tercihine göre komut seçilir.
///
/// TTY gerektiren planlar için çağıran önce TUI’yi askıya almalı ve
/// [`PmIoMode::Inherited`] geçirmelidir.
pub fn install_via_package_manager(entry: &StoreEntry) -> Result<InstallOutcome, InstallError> {
    if !entry.install_method.is_package_manager() {
        return Err(InstallError::Security(format!(
            "'{}' paket yöneticisi yöntemi değil.",
            entry.name
        )));
    }
    // Claude Code: npm optional-deps/postinstall kırılgan; Anthropic CDN native
    // ikili tercih edilir (gerçek ELF, doğru mimari, kırık shim yok).
    if crate::store::native_cli::is_claude_code(entry) {
        // Ağ/CDN hatasında npm’e düşülür; npm sonrası ikili yine doğrulanır.
        if let Ok(outcome) = crate::store::native_cli::install_claude_native(entry) {
            return Ok(outcome);
        }
    }
    let plan = resolve_entry_install_plan(entry).ok_or_else(|| {
        InstallError::Internal(format!(
            "'{}' için host’a uygun kurulum komutu bulunamadı.",
            entry.name
        ))
    })?;
    let io = if plan.needs_tty_suspend() {
        PmIoMode::Inherited
    } else {
        PmIoMode::Captured
    };
    run_package_manager_argv(entry, &plan, PmOperation::Install, io)
}

/// Paket yöneticisi işlemi türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmOperation {
    /// Kurulum.
    Install,
    /// Güncelleme.
    Upgrade,
    /// Kaldırma.
    Uninstall,
}

/// Önceden çözülmüş PM argv’sini çalıştırır (kurulum, güncelleme veya kaldırma).
pub fn run_package_manager_argv(
    entry: &StoreEntry,
    plan: &InstallPlan,
    op: PmOperation,
    io: PmIoMode,
) -> Result<InstallOutcome, InstallError> {
    let argv = &plan.argv;
    match op {
        PmOperation::Upgrade => {
            host::validate_upgrade_command(plan.method, argv).map_err(InstallError::Security)?;
        }
        PmOperation::Uninstall => {
            host::validate_uninstall_command(plan.method, argv).map_err(InstallError::Security)?;
        }
        PmOperation::Install => {
            validate_install_command(plan.method, argv).map_err(InstallError::Security)?;
        }
    }
    let program = &argv[0];
    let args = &argv[1..];

    // sudo dışındaki programın PATH'te olması gerekir; yoksa anlaşılır hata.
    let lookup = if program == "sudo" {
        argv.get(1)
            .map(String::as_str)
            .unwrap_or(plan.method.short_label())
    } else {
        program.as_str()
    };
    if which::which(lookup).is_err() {
        let alts = entry
            .install_notes
            .as_deref()
            .map(|n| {
                format!(
                    " Alternatifler: {}",
                    n.lines().take(3).collect::<Vec<_>>().join(" · ")
                )
            })
            .unwrap_or_default();
        return Err(InstallError::Internal(format!(
            "'{lookup}' bulunamadı (PATH).{alts}"
        )));
    }

    let rendered = format_install_command(argv);
    let mut cmd = Command::new(program);
    cmd.args(args);
    // npm native CLI paketleri optionalDeps + postinstall ister; kullanıcının
    // --omit=optional / ignore-scripts ortamını ezmemek için yalnızca
    // kurulumda açıkça optional’ı aç.
    if plan.method == InstallMethod::Npm
        && matches!(op, PmOperation::Install | PmOperation::Upgrade)
    {
        cmd.env("npm_config_optional", "true");
        cmd.env("npm_config_ignore_scripts", "false");
    }

    let status = match io {
        PmIoMode::Captured => {
            // `output()` stdout/stderr’i yakalar; parola istenen sudo için uygun değil.
            let output = cmd
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .map_err(|e| InstallError::Internal(format!("'{rendered}' başlatılamadı: {e}")))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let detail = if !stderr.trim().is_empty() {
                    stderr.trim().to_string()
                } else if !stdout.trim().is_empty() {
                    stdout.trim().to_string()
                } else {
                    format!("çıkış kodu {:?}", output.status.code())
                };
                let hint = if plan.method.needs_elevated_tty() || plan.uses_sudo() {
                    " (sudo parola veya TTY gerekebilir; Nova onayda TUI’yi bırakır — \
                      gerekirse komutu ayrı bir terminalde deneyin)"
                } else {
                    ""
                };
                return Err(InstallError::Internal(format!(
                    "'{rendered}' başarısız{hint}: {detail}"
                )));
            }
            output.status
        }
        PmIoMode::Inherited => {
            // `status()` stdio’yu korur; parola istemi /dev/tty veya inherit ile çalışır.
            if attach_controlling_tty(&mut cmd).is_err() {
                cmd.stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit());
            }
            let status = cmd
                .status()
                .map_err(|e| InstallError::Internal(format!("'{rendered}' başlatılamadı: {e}")))?;
            if !status.success() {
                return Err(InstallError::Internal(format!(
                    "'{rendered}' başarısız (çıkış kodu {:?}). sudo parola reddedildiyse \
                     veya paket yoksa komutu bir terminalde deneyin.",
                    status.code()
                )));
            }
            status
        }
    };
    let _ = status;

    let install_dir = crate::store::store_app_dir(&entry.id).unwrap_or_else(|| PathBuf::from("."));
    let mut executable_path = crate::detection::resolve(&entry.executable)
        .1
        .unwrap_or_else(|| PathBuf::from(&entry.executable));
    let mut summary = {
        let verb = match op {
            PmOperation::Upgrade => "güncellendi",
            PmOperation::Uninstall => "kaldırıldı",
            PmOperation::Install => "kuruldu",
        };
        format!(
            "'{}' {verb} ({}): {rendered}",
            entry.name,
            plan.method.short_label()
        )
    };
    if op == PmOperation::Uninstall {
        let _ = crate::store::path::remove_path_shim(&entry.executable);
        return Ok(InstallOutcome {
            app_id: entry.id.clone(),
            app_name: entry.name.clone(),
            version: entry.version.clone().unwrap_or_default(),
            executable_path,
            install_dir,
            summary,
        });
    }
    // npm native CLI’ler: yer tutucu / yanlış mimariyi reddet; kırık shim bırakma.
    if plan.method == InstallMethod::Npm {
        crate::store::native_cli::reject_broken_npm_cli(&entry.executable, &executable_path)?;
    }
    // Yeni kabuklarda `which qwen` bulunsun diye ~/.local/bin shim.
    match crate::store::path::shim_installed_executable(&entry.executable, &executable_path) {
        Ok(shim) => {
            executable_path = shim;
            summary.push_str(" · PATH: ~/.local/bin");
        }
        Err(err) => {
            summary.push_str(&format!(" · PATH uyarısı: {err}"));
        }
    }
    Ok(InstallOutcome {
        app_id: entry.id.clone(),
        app_name: entry.name.clone(),
        version: entry.version.clone().unwrap_or_default(),
        executable_path,
        install_dir,
        summary,
    })
}

/// Controlling TTY’yi (`/dev/tty`) sürecin stdio’suna bağlar.
fn attach_controlling_tty(cmd: &mut Command) -> Result<(), ()> {
    use std::fs::OpenOptions;
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|_| ())?;
    let stdout = tty.try_clone().map_err(|_| ())?;
    let stderr = tty.try_clone().map_err(|_| ())?;
    cmd.stdin(Stdio::from(tty))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    Ok(())
}

/// `U` ile bulunan PM güncelleme planını çalıştırır.
pub fn upgrade_via_package_manager(
    entry: &StoreEntry,
    plan: &host::PmUpdatePlan,
) -> Result<InstallOutcome, InstallError> {
    let install = plan.as_install_plan();
    let io = if install.needs_tty_suspend() {
        PmIoMode::Inherited
    } else {
        PmIoMode::Captured
    };
    run_package_manager_argv(entry, &install, PmOperation::Upgrade, io)
}

/// Arşivle kurulan uygulamayı kullanıcı alanından kaldırır.
///
/// Varsa `install-manifest.toml` ile dosya listesi + shim geri alınır;
/// yoksa dizin toplu silinir (eski kurulumlar).
pub fn uninstall_archive_app(entry: &StoreEntry) -> Result<InstallOutcome, InstallError> {
    let Some(dir) = crate::store::store_app_dir(&entry.id) else {
        return Err(InstallError::Internal(
            "kurulum dizini belirlenemedi ($HOME bulunamadı)".into(),
        ));
    };
    if let Some(manifest) = InstallFileManifest::load(&entry.id) {
        let shim = manifest
            .shim_name
            .as_deref()
            .unwrap_or(entry.executable.as_str());
        rollback_archive_install(&dir, &manifest.files, Some(shim))?;
        // Manifest’te listelenmeyen artıkları da temizle.
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
    } else {
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| {
                InstallError::Permission(format!("'{}' silinemedi: {e}", dir.display()))
            })?;
        }
        let _ = crate::store::path::remove_path_shim(&entry.executable);
    }
    Ok(InstallOutcome {
        app_id: entry.id.clone(),
        app_name: entry.name.clone(),
        version: entry.version.clone().unwrap_or_default(),
        executable_path: PathBuf::from(&entry.executable),
        install_dir: dir,
        summary: format!(
            "'{}' kullanıcı alanı kurulumundan kaldırıldı (manifest geri alma).",
            entry.name
        ),
    })
}

/// Host’a göre PM kaldırma planını çalıştırır.
pub fn uninstall_via_package_manager(
    entry: &StoreEntry,
    plan: &InstallPlan,
) -> Result<InstallOutcome, InstallError> {
    let io = if plan.needs_tty_suspend() {
        PmIoMode::Inherited
    } else {
        PmIoMode::Captured
    };
    run_package_manager_argv(entry, plan, PmOperation::Uninstall, io)
}

/// `entry` uygulamasını **canlı çözülmüş sürümden** kurar veya günceller.
///
/// Kaynak, gömülü katalogdaki sabit değerlerin yerine GitHub Releases'tan
/// çözülen adres ve özeti kullanır. Doğrulama kuralı değişmez: indirilen
/// dosya, upstream'in bu yayın için yayımladığı özetle eşleşmelidir.
pub fn install_release(
    entry: &StoreEntry,
    dest_dir: &Path,
    release: &crate::store::manifest::ResolvedRelease,
    downloader: &dyn Downloader,
) -> Result<InstallOutcome, InstallError> {
    let source = InstallSource::from_release(release)?;
    install_with_source(entry, dest_dir, &source, &release.version, downloader)
}

/// Kurulumda kullanılacak somut indirme bilgisi.
///
/// Kaynak iki yerden gelebilir: gömülü katalog (`from_entry`) veya canlı
/// çözümlenmiş yayın (`from_release`). İkisi de **doğrulanabilir** olmak
/// zorundadır: `https://` adres ve geçerli SHA-256 özeti.
#[derive(Debug, Clone)]
pub struct InstallSource {
    /// İndirilecek arşivin adresi.
    pub url: String,
    /// Arşivin beklenen SHA-256 özeti.
    pub checksum: String,
}

impl InstallSource {
    /// Gömülü katalogdaki kayıttan indirme bilgisi üretir (host mimarisi).
    pub fn from_entry(entry: &StoreEntry) -> Result<Self, InstallError> {
        let arch = CpuArch::host().ok_or_else(|| {
            InstallError::Security(format!(
                "desteklenmeyen CPU mimarisi: {}",
                std::env::consts::ARCH
            ))
        })?;
        Self::from_entry_for_arch(entry, arch)
    }

    /// Belirli mimari için gömülü katalogdan indirme bilgisi üretir.
    pub fn from_entry_for_arch(entry: &StoreEntry, arch: CpuArch) -> Result<Self, InstallError> {
        if !entry.supports_arch(arch) {
            return Err(InstallError::Security(format!(
                "'{}' {} mimarisini desteklemiyor",
                entry.name,
                arch.as_str()
            )));
        }
        let (url, checksum) = entry
            .resolve_download_for(arch)
            .map_err(InstallError::Security)?;
        Self::new(url.trim(), checksum.trim())
    }

    /// Canlı çözülmüş yayından indirme bilgisi üretir.
    pub fn from_release(
        release: &crate::store::manifest::ResolvedRelease,
    ) -> Result<Self, InstallError> {
        Self::new(&release.download_url, &release.checksum)
    }

    /// Adres ve özeti doğrulayarak indirme bilgisi oluşturur.
    pub fn new(url: &str, checksum: &str) -> Result<Self, InstallError> {
        if !url.starts_with("https://") {
            return Err(InstallError::Security(format!(
                "indirme adresi yalnızca https:// olabilir: '{}'",
                url
            )));
        }
        if !crate::store::is_sha256_hex(checksum) {
            return Err(InstallError::Security(format!(
                "geçersiz SHA-256 özeti: '{}'",
                checksum
            )));
        }
        Ok(Self {
            url: url.to_string(),
            checksum: checksum.to_lowercase(),
        })
    }
}

/// Ortak kurulum yolu: indir → doğrula → çıkar → yerleştir → manifest → temizle.
fn install_with_source(
    entry: &StoreEntry,
    dest_dir: &Path,
    source: &InstallSource,
    version: &str,
    downloader: &dyn Downloader,
) -> Result<InstallOutcome, InstallError> {
    if !entry.install_method.is_archive() {
        return Err(InstallError::Security(format!(
            "'{}' için arşiv kurulumu yok; resmî kurulum yönergelerini takip edin \
             veya paket yöneticisi yolunu kullanın.",
            entry.name
        )));
    }
    if !is_user_area(dest_dir) {
        return Err(InstallError::Security(format!(
            "hedef dizin kullanıcı alanı değil: '{}'",
            dest_dir.display()
        )));
    }
    let arch = CpuArch::host().unwrap_or(CpuArch::X86_64);
    if !entry.supports_arch(arch) {
        return Err(InstallError::Security(format!(
            "'{}' {} mimarisini desteklemiyor",
            entry.name,
            arch.as_str()
        )));
    }

    // 1) Geçici çalışma alanı (kullanıcı alanında).
    let work_dir = dest_dir.join(".nova-work");
    let bin_dir = dest_dir.join("bin");
    fs::create_dir_all(&work_dir).map_err(|e| {
        InstallError::Permission(format!("'{}' oluşturulamadı: {e}", work_dir.display()))
    })?;
    let archive_path = work_dir.join("download.tar.gz");
    let staging_dir = work_dir.join("staging");

    // Önceki başarısız kurulumdan kalmış olabilecek geçici içeriği temizle.
    let _ = fs::remove_dir_all(&staging_dir);

    let mut placed_files: Vec<String> = Vec::new();
    let mut shim_written: Option<String> = None;

    let result = (|| -> Result<InstallOutcome, InstallError> {
        // 2) İndir.
        let downloaded = downloader.download(&source.url, &archive_path)?;

        // 3) SHA-256 doğrula — uyuşmazlık kurulumu keser.
        verify_checksum(&downloaded, &source.checksum)?;

        // 3b) Upstream imza — araç varsa doğrula, yoksa atla (when possible).
        let signature_note = match crate::store::signature::verify_signature_when_possible(
            &downloaded,
            entry.signature.as_ref(),
            version,
            &work_dir,
            downloader,
        )? {
            crate::store::signature::SignatureCheck::Verified => {
                Some("upstream imza doğrulandı".to_string())
            }
            crate::store::signature::SignatureCheck::SkippedMissingTool(tool) => {
                Some(format!("upstream imza atlandı ({tool} PATH'te yok)"))
            }
            crate::store::signature::SignatureCheck::NotRequested => None,
        };

        // 4) Güvenli çıkarma.
        extract_archive(&downloaded, &staging_dir)?;

        // 5) Çalıştırılabilir dosyayı bul ve yerleştir.
        let found = find_executable(&staging_dir, &entry.executable).ok_or_else(|| {
            InstallError::Archive(format!(
                "arşivde '{}' çalıştırılabilir dosyası bulunamadı.",
                entry.executable
            ))
        })?;

        fs::create_dir_all(&bin_dir).map_err(|e| {
            InstallError::Permission(format!("'{}' oluşturulamadı: {e}", bin_dir.display()))
        })?;
        let final_path = bin_dir.join(&entry.executable);
        let _ = fs::remove_file(&final_path);
        fs::copy(&found, &final_path).map_err(|e| {
            InstallError::Permission(format!("'{}' kopyalanamadı: {e}", final_path.display()))
        })?;
        placed_files.push(format!("bin/{}", entry.executable));
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

        let mut executable_path = final_path.clone();
        let mut summary = format!(
            "'{}' başarıyla kuruldu: {}",
            entry.name,
            final_path.display()
        );
        if let Some(note) = signature_note {
            summary.push_str(" · ");
            summary.push_str(&note);
        }
        match crate::store::path::shim_installed_executable(&entry.executable, &final_path) {
            Ok(shim) => {
                executable_path = shim;
                shim_written = Some(entry.executable.clone());
                summary.push_str(" · PATH: ~/.local/bin");
            }
            Err(err) => {
                summary.push_str(&format!(" · PATH uyarısı: {err}"));
            }
        }

        // 6) Dosya manifest’i — kaldırma / geri alma için.
        let manifest = InstallFileManifest {
            app_id: entry.id.clone(),
            version: version.to_string(),
            arch: arch.as_str().to_string(),
            method: entry.install_method.short_label().to_string(),
            source_url: Some(source.url.clone()),
            checksum: Some(source.checksum.clone()),
            installed_at_unix: unix_now(),
            files: placed_files.clone(),
            shim_name: shim_written.clone(),
        };
        manifest.save(dest_dir)?;
        placed_files.push("install-manifest.toml".into());

        Ok(InstallOutcome {
            app_id: entry.id.clone(),
            app_name: entry.name.clone(),
            version: version.to_string(),
            executable_path,
            install_dir: dest_dir.to_path_buf(),
            summary,
        })
    })();

    // 7) Geçici dosyaları temizle (hata durumunda da).
    let _ = fs::remove_dir_all(&staging_dir);
    let _ = fs::remove_file(&archive_path);
    let _ = fs::remove_dir(&work_dir);

    // 8) Kalıcı dosya yazıldıktan sonra hata → rollback.
    if result.is_err() && !placed_files.is_empty() {
        let _ = rollback_archive_install(dest_dir, &placed_files, shim_written.as_deref());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InstallMethod;
    use std::cell::RefCell;
    use std::io::Write;
    use std::rc::Rc;
    use std::sync::Mutex;

    /// PATH shim `HOME/.local/bin` yazar; paralel testler aynı `rg` adına
    /// yarışmasın diye HOME değiştiren testler bu kilidi alır.
    static SHIM_HOME_LOCK: Mutex<()> = Mutex::new(());

    fn with_isolated_home<F: FnOnce()>(f: F) {
        let _guard = SHIM_HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("shim-home");
        let old_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &dir);
        f();
        match old_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    /// Sahte downloader: indirmeyi kaydeder, gerçek ağ erişimi yok.
    struct FakeDownloader {
        calls: Rc<RefCell<Vec<(String, PathBuf)>>>,
        fail: bool,
        payload: Option<Vec<u8>>,
    }

    impl FakeDownloader {
        fn ok() -> Self {
            Self {
                calls: Rc::new(RefCell::new(Vec::new())),
                fail: false,
                payload: None,
            }
        }
        fn failing() -> Self {
            Self {
                calls: Rc::new(RefCell::new(Vec::new())),
                fail: true,
                payload: None,
            }
        }
        fn with_payload(payload: Vec<u8>) -> Self {
            Self {
                calls: Rc::new(RefCell::new(Vec::new())),
                fail: false,
                payload: Some(payload),
            }
        }
    }

    impl Downloader for FakeDownloader {
        fn download(&self, url: &str, destination: &Path) -> Result<PathBuf, InstallError> {
            self.calls
                .borrow_mut()
                .push((url.to_string(), destination.to_path_buf()));
            if self.fail {
                return Err(InstallError::Network("bağlantı yok".into()));
            }
            if let Some(payload) = &self.payload {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).ok();
                }
                let mut file = fs::File::create(destination)
                    .map_err(|e| InstallError::Download(format!("yazılamadı: {e}")))?;
                file.write_all(payload)
                    .map_err(|e| InstallError::Download(format!("yazılamadı: {e}")))?;
            }
            Ok(destination.to_path_buf())
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nova-store-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Gerçek bir `tar.gz` arşivi üretir (testler gerçek çıkarma yolunu dener).
    fn make_tar_gz(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        gzip(&make_tar(entries))
    }

    /// Çıplak (sıkıştırılmamış) tar üretir.
    fn make_tar(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, content, executable) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(if *executable { 0o755 } else { 0o644 });
            header.set_cksum();
            builder
                .append_data(&mut header, name, *content)
                .expect("arşiv girdisi eklenemedi");
        }
        builder.into_inner().expect("arşiv tamamlanamadı")
    }

    /// Ham tar verisini gzip ile sıkıştırır.
    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).expect("sıkıştırma başarısız");
        encoder.finish().expect("sıkıştırma tamamlanamadı")
    }

    /// Bir tar başlığının checksum'unu hesaplar (checksum alanı boşluk sayılır).
    fn tar_checksum(header: &[u8]) -> u64 {
        let mut sum = 0u64;
        for (i, b) in header.iter().enumerate() {
            if (148..156).contains(&i) {
                sum += 0x20;
            } else {
                sum += *b as u64;
            }
        }
        sum
    }

    /// Bir tar girdisinin adını değiştirir (checksum yeniden hesaplanır).
    ///
    /// `tar` kütüphanesi `..` ve mutlak yolları **yazarken** reddettiği için,
    /// saldırgan bir arşivi bu yolla üretmek gerekir: yol alanı doğrudan
    /// bayt düzeyinde değiştirilir.
    fn patch_entry_name(tar_bytes: &[u8], old: &str, new: &str) -> Vec<u8> {
        let mut out = tar_bytes.to_vec();
        let mut offset = 0usize;
        while offset + 512 <= out.len() {
            let block = &mut out[offset..offset + 512];
            if block.iter().all(|&b| b == 0) {
                break; // arşiv sonu
            }
            let name_end = block[..100].iter().position(|&b| b == 0).unwrap_or(100);
            let name = String::from_utf8_lossy(&block[..name_end]).to_string();
            if name == old {
                for b in block[..100].iter_mut() {
                    *b = 0;
                }
                block[..new.len()].copy_from_slice(new.as_bytes());
                let sum = tar_checksum(block);
                let chk = format!("{sum:06o}\0 ");
                block[148..156].copy_from_slice(chk.as_bytes());
            }
            offset += 512;
        }
        out
    }

    /// Test kataloğundan indirilebilir bir kayıt üretir.
    /// Test ikili adı — gerçek `rg` / sistem PATH shim’leriyle çakışmaz.
    const TEST_EXE: &str = "nova-tst-bin";

    fn installable_entry(checksum: &str) -> StoreEntry {
        StoreEntry {
            id: "ornek".into(),
            name: "Örnek".into(),
            description: "d".into(),
            category: "Günlük".into(),
            source: "https://example.org/".into(),
            license: "MIT".into(),
            executable: TEST_EXE.into(),
            install_method: crate::store::InstallMethod::DownloadArchive,
            supported_platforms: vec!["linux-x64".into()],
            checksum: Some(checksum.into()),
            checksum_type: Some("sha256".into()),
            version: Some("1.0.0".into()),
            download_url: Some("https://example.org/ornek.tar.gz".into()),
            artifacts: vec![],
            install_command: None,
            install_notes: None,
            release: None,
            signature: None,
        }
    }

    fn package_entry(method: InstallMethod, argv: Vec<&str>) -> StoreEntry {
        StoreEntry {
            id: "ornek-pm".into(),
            name: "Örnek PM".into(),
            description: "d".into(),
            category: "Günlük".into(),
            source: "https://example.org/".into(),
            license: "MIT".into(),
            executable: "ornek-pm".into(),
            install_method: method,
            supported_platforms: vec!["linux-x64".into()],
            checksum: None,
            checksum_type: None,
            version: None,
            download_url: None,
            artifacts: vec![],
            install_command: Some(argv.into_iter().map(str::to_string).collect()),
            install_notes: Some("not".into()),
            release: None,
            signature: None,
        }
    }

    #[test]
    fn sha256_matches_known_vector() {
        let dir = temp_dir("sha");
        let file = dir.join("a.txt");
        fs::write(&file, b"abc").unwrap();
        let sum = sha256_file(&file).unwrap();
        assert_eq!(
            sum,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_checksum_accepts_matching_sum_in_any_case() {
        let dir = temp_dir("esit");
        let file = dir.join("a.txt");
        fs::write(&file, b"abc").unwrap();
        let sum = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
        assert!(verify_checksum(&file, sum).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_checksum_rejects_mismatch() {
        let dir = temp_dir("uyusmaz");
        let file = dir.join("a.txt");
        fs::write(&file, b"abc").unwrap();
        let err = verify_checksum(&file, &"0".repeat(64)).unwrap_err();
        assert!(matches!(err, InstallError::Download(_)), "hata: {err}");
        assert!(err.to_string().contains("uyuşmazlığı"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_checksum_rejects_malformed_sum() {
        let dir = temp_dir("bozuk");
        let file = dir.join("a.txt");
        fs::write(&file, b"abc").unwrap();
        let err = verify_checksum(&file, "kisa").unwrap_err();
        assert!(matches!(err, InstallError::Download(_)), "hata: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_checksum_requires_a_sum() {
        let dir = temp_dir("sumsuz");
        let file = dir.join("a.txt");
        fs::write(&file, b"abc").unwrap();
        let err = verify_checksum(&file, "").unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_archive_writes_files_and_keeps_executable_bit() {
        let dir = temp_dir("cikar");
        let archive = dir.join("a.tar.gz");
        let payload = make_tar_gz(&[
            ("paket/rg", b"#!/bin/sh\necho rg\n", true),
            ("paket/OKUBENI", b"aciklama", false),
        ]);
        fs::write(&archive, &payload).unwrap();
        let dest = dir.join("cikarilan");
        extract_archive(&archive, &dest).unwrap();
        assert_eq!(
            fs::read(dest.join("paket/rg")).unwrap(),
            b"#!/bin/sh\necho rg\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dest.join("paket/rg"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111;
            assert_ne!(mode, 0, "çalıştırılabilir bit korunmalı");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_archive_rejects_path_traversal() {
        let dir = temp_dir("gecis");
        let archive = dir.join("a.tar.gz");
        // Saldırgan arşiv: yol alanı bayt düzeyinde "../kaçis" yapılır.
        let raw = make_tar(&[("paket/rg", b"kotu", false)]);
        let malicious = gzip(&patch_entry_name(&raw, "paket/rg", "../kacis"));
        fs::write(&archive, &malicious).unwrap();
        let dest = dir.join("cikarilan");
        let err = extract_archive(&archive, &dest).unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
        assert!(!dir.join("kacis").exists(), "hedef dışına yazılmamalı");
        assert!(!dest.join("kacis").exists(), "hedef dışına yazılmamalı");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Klasik zip-slip: `../` ile hedef kök dışına yazma denemesi.
    #[test]
    fn extract_archive_rejects_zip_slip() {
        let dir = temp_dir("zipslip");
        let archive = dir.join("a.tar.gz");
        let raw = make_tar(&[("safe/file", b"x", false)]);
        let malicious = gzip(&patch_entry_name(
            &raw,
            "safe/file",
            "safe/../../zipslip-escaped",
        ));
        fs::write(&archive, &malicious).unwrap();
        let dest = dir.join("cikarilan");
        let err = extract_archive(&archive, &dest).unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
        assert!(
            !dir.join("zipslip-escaped").exists(),
            "zip-slip hedef dışına yazmamalı"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_archive_rejects_absolute_paths() {
        let dir = temp_dir("mutlak");
        let archive = dir.join("a.tar.gz");
        let raw = make_tar(&[("paket/rg", b"kotu", false)]);
        let malicious = gzip(&patch_entry_name(&raw, "paket/rg", "/etc/kotu"));
        fs::write(&archive, &malicious).unwrap();
        let dest = dir.join("cikarilan");
        let err = extract_archive(&archive, &dest).unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
        assert!(
            !Path::new("/etc/kotu").exists(),
            "sistem dizinine yazılmamalı"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_archive_rejects_symlink_entries() {
        let dir = temp_dir("symlink");
        let archive = dir.join("a.tar.gz");
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_cksum();
        builder
            .append_link(&mut header, "escape-link", "/etc/passwd")
            .expect("symlink girdisi");
        // Yanına zararsız bir dosya da koy; link önce gelince tüm çıkarma kesilmeli.
        let mut file_header = tar::Header::new_gnu();
        file_header.set_size(1);
        file_header.set_mode(0o644);
        file_header.set_cksum();
        builder
            .append_data(&mut file_header, "paket/ok", &b"x"[..])
            .expect("dosya");
        let raw = builder.into_inner().expect("tar");
        fs::write(&archive, gzip(&raw)).unwrap();
        let dest = dir.join("cikarilan");
        let err = extract_archive(&archive, &dest).unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
        assert!(
            format!("{err}").contains("link"),
            "sembolik/hard link mesajı beklenir: {err}"
        );
        assert!(
            !dest.join("escape-link").exists(),
            "symlink oluşturulmamalı"
        );
        assert!(
            !dest.join("paket/ok").exists(),
            "link reddi sonrası devam edilmemeli"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_archive_rejects_deep_path_traversal() {
        let dir = temp_dir("derin");
        let archive = dir.join("a.tar.gz");
        let raw = make_tar(&[("paket/rg", b"kotu", false)]);
        let malicious = gzip(&patch_entry_name(&raw, "paket/rg", "paket/../../kacis"));
        fs::write(&archive, &malicious).unwrap();
        let dest = dir.join("cikarilan");
        let err = extract_archive(&archive, &dest).unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_archive_rejects_broken_archive() {
        let dir = temp_dir("bozuk");
        let archive = dir.join("a.tar.gz");
        fs::write(&archive, b"bu bir arsiv degil").unwrap();
        let dest = dir.join("cikarilan");
        let err = extract_archive(&archive, &dest).unwrap_err();
        assert!(matches!(err, InstallError::Archive(_)), "hata: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_executable_finds_nested_file() {
        let dir = temp_dir("ara");
        fs::create_dir_all(dir.join("paket/bin")).unwrap();
        fs::write(dir.join("paket/bin/rg"), b"x").unwrap();
        assert_eq!(find_executable(&dir, "rg"), Some(dir.join("paket/bin/rg")));
        assert_eq!(find_executable(&dir, "yok"), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_executable_finds_root_level_file() {
        let dir = temp_dir("kok");
        fs::write(dir.join("fzf"), b"x").unwrap();
        assert_eq!(find_executable(&dir, "fzf"), Some(dir.join("fzf")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_user_area_accepts_home_subdir_even_when_missing() {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .expect("HOME gerekli");
        // Dizin henüz yok: ilk kurulum da kabul edilmeli.
        let path = PathBuf::from(home).join(".local/share/nova/store/yok-boyle-bir-dizin");
        assert!(is_user_area(&path));
    }

    #[test]
    fn is_user_area_rejects_system_dirs() {
        assert!(!is_user_area(Path::new("/usr/bin")));
        assert!(!is_user_area(Path::new("/etc")));
        assert!(!is_user_area(Path::new("/opt")));
        assert!(!is_user_area(Path::new("/var")));
        assert!(!is_user_area(Path::new("/usr/local/bin")));
    }

    #[test]
    fn is_user_area_rejects_parent_escape() {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .expect("HOME gerekli");
        let escape = PathBuf::from(home).join("..").join("etc");
        assert!(!is_user_area(&escape));
    }

    #[test]
    fn install_app_rejects_system_dir_before_downloading() {
        let entry = installable_entry(&"0".repeat(64));
        let downloader = FakeDownloader::ok();
        let err = install_app(&entry, Path::new("/usr/bin/nova-store"), &downloader).unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
        assert!(downloader.calls.borrow().is_empty(), "indirme yapılmamalı");
    }

    #[test]
    fn install_app_rejects_manual_entries() {
        let mut entry = installable_entry(&"0".repeat(64));
        entry.install_method = crate::store::InstallMethod::Manual;
        let dir = temp_dir("manuel");
        let downloader = FakeDownloader::ok();
        let err = install_app(&entry, &dir, &downloader).unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
        assert!(downloader.calls.borrow().is_empty(), "indirme yapılmamalı");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_app_reports_network_failure() {
        let entry = installable_entry(&"0".repeat(64));
        let dest = crate::store::test_store_dir().join("test-download-fail");
        let downloader = FakeDownloader::failing();
        let err = install_app(&entry, &dest, &downloader).unwrap_err();
        assert!(matches!(err, InstallError::Network(_)), "hata: {err}");
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn install_app_rejects_checksum_mismatch_and_installs_nothing() {
        let payload = make_tar_gz(&[(&format!("paket/{TEST_EXE}"), b"#!/bin/sh\n" as &[u8], true)]);
        let entry = installable_entry(&"0".repeat(64));
        let dest = crate::store::test_store_dir().join("test-checksum");
        let downloader = FakeDownloader::with_payload(payload);
        let err = install_app(&entry, &dest, &downloader).unwrap_err();
        assert!(matches!(err, InstallError::Download(_)), "hata: {err}");
        assert!(
            !dest.join("bin").join(TEST_EXE).exists(),
            "checksum uyuşmazlığında hiçbir dosya kurulmamalı"
        );
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn install_app_succeeds_and_places_executable_in_bin() {
        with_isolated_home(|| {
            let payload = make_tar_gz(&[(
                &format!("paket/{TEST_EXE}"),
                b"#!/bin/sh\necho ok\n" as &[u8],
                true,
            )]);
            let sum = hex(&Sha256::digest(&payload));
            let entry = installable_entry(&sum);
            let dest = crate::store::test_store_dir().join("test-success");
            let downloader = FakeDownloader::with_payload(payload);
            let outcome = install_app(&entry, &dest, &downloader).unwrap();
            assert_eq!(outcome.app_id, "ornek");
            let store_bin = dest.join("bin").join(TEST_EXE);
            assert!(store_bin.is_file(), "store bin yazılmalı");
            // PATH shim ~/.local/bin/<exe> olabilir; yoksa store yolu kalır.
            let shim_suffix = PathBuf::from(".local/bin").join(TEST_EXE);
            assert!(
                outcome.executable_path == store_bin
                    || outcome.executable_path.ends_with(&shim_suffix),
                "beklenen store veya ~/.local/bin shim, got {}",
                outcome.executable_path.display()
            );
            // Shim hedefi store bin’e işaret etmeli (dangling değil).
            if outcome.executable_path != store_bin {
                let target = fs::read_link(&outcome.executable_path)
                    .unwrap_or_else(|_| outcome.executable_path.clone());
                let resolved = if target.is_absolute() {
                    target
                } else {
                    outcome
                        .executable_path
                        .parent()
                        .unwrap_or(Path::new("."))
                        .join(target)
                };
                assert!(
                    resolved == store_bin
                        || resolved.canonicalize().ok() == store_bin.canonicalize().ok(),
                    "shim {} → {} store bin’e gitmeli",
                    outcome.executable_path.display(),
                    resolved.display()
                );
            }
            assert!(outcome.summary.contains("başarıyla"));
            // Geçici çalışma alanı temizlenmeli.
            assert!(!dest.join(".nova-work").exists());
            let _ = crate::store::path::remove_path_shim(TEST_EXE);
            let _ = fs::remove_dir_all(&dest);
        });
    }

    #[test]
    fn install_app_reports_missing_executable_in_archive() {
        let payload = make_tar_gz(&[("paket/okubeni", b"aciklama", false)]);
        let sum = hex(&Sha256::digest(&payload));
        let entry = installable_entry(&sum);
        let dest = crate::store::test_store_dir().join("test-missing-exe");
        let downloader = FakeDownloader::with_payload(payload);
        let err = install_app(&entry, &dest, &downloader).unwrap_err();
        assert!(matches!(err, InstallError::Archive(_)), "hata: {err}");
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn install_app_uses_the_entry_download_url() {
        with_isolated_home(|| {
            let payload =
                make_tar_gz(&[(&format!("paket/{TEST_EXE}"), b"#!/bin/sh\n" as &[u8], true)]);
            let sum = hex(&Sha256::digest(&payload));
            let entry = installable_entry(&sum);
            let dest = crate::store::test_store_dir().join("test-url");
            let downloader = FakeDownloader::with_payload(payload);
            install_app(&entry, &dest, &downloader).unwrap();
            let calls = downloader.calls.borrow();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].0, "https://example.org/ornek.tar.gz");
            let _ = crate::store::path::remove_path_shim(TEST_EXE);
            let _ = fs::remove_dir_all(&dest);
        });
    }

    #[test]
    fn prepare_confirmation_lists_version_and_checksum() {
        let entry = installable_entry(&"a".repeat(64));
        let confirm = prepare_confirmation(&entry);
        assert_eq!(confirm.version, "1.0.0");
        assert!(confirm.actions.iter().any(|a| a.contains("SHA-256")));
        assert!(confirm.actions.iter().any(|a| a.contains("kabuk yok")));
        let q = confirm.question_text();
        assert!(q.contains("SHA-256"), "{q}");
        assert!(q.contains("Kaynak:"));
        assert!(q.contains("Lisans: MIT"));
        assert!(q.contains("sudo?: hayır"));
        assert!(q.contains("Kapsam: kullanıcı alanı"));
        assert!(q.contains("Üçüncü taraf"));
        assert_eq!(confirm.scope, InstallScope::User);
        assert!(!confirm.uses_sudo);
        assert_eq!(confirm.package_manager, "archive");
    }

    #[test]
    fn prepare_confirmation_for_package_manager_shows_command() {
        let entry = package_entry(InstallMethod::Pipx, vec!["pipx", "install", "ornek-pm"]);
        let confirm = prepare_confirmation(&entry);
        assert!(
            confirm
                .actions
                .iter()
                .any(|a| a.contains("pipx install ornek-pm")),
            "{:?}",
            confirm.actions
        );
        let q = confirm.question_text();
        assert!(q.contains("pipx install ornek-pm"), "{q}");
        assert!(q.contains("Paket yöneticisi: pipx"), "{q}");
        assert!(q.contains("Kaynak:"));
        assert!(q.contains("Lisans:"));
        assert_eq!(confirm.scope, InstallScope::User);
        assert!(!confirm.uses_sudo);
    }

    #[test]
    fn prepare_confirmation_marks_system_sudo_for_pacman() {
        let entry = package_entry(
            InstallMethod::Pacman,
            vec!["sudo", "pacman", "-S", "--noconfirm", "htop"],
        );
        let confirm = prepare_confirmation(&entry);
        assert_eq!(confirm.scope, InstallScope::System);
        assert!(confirm.uses_sudo);
        let q = confirm.question_text();
        assert!(q.contains("sudo?: evet"), "{q}");
        assert!(q.contains("sistem"), "{q}");
    }

    #[test]
    fn archive_install_writes_manifest_and_uninstall_rolls_back() {
        with_isolated_home(|| {
            let payload =
                make_tar_gz(&[(&format!("paket/{TEST_EXE}"), b"#!/bin/sh\n" as &[u8], true)]);
            let sum = hex(&Sha256::digest(&payload));
            let entry = installable_entry(&sum);
            let dest = crate::store::store_app_dir(&entry.id).expect("store app dir");
            let downloader = FakeDownloader::with_payload(payload);
            install_app(&entry, &dest, &downloader).unwrap();
            assert!(dest.join("bin").join(TEST_EXE).is_file());
            let manifest = InstallFileManifest::load(&entry.id).expect("manifest yazılmalı");
            assert_eq!(manifest.app_id, entry.id);
            assert!(manifest.files.iter().any(|f| f.contains(TEST_EXE)));
            assert_eq!(manifest.checksum.as_deref(), Some(sum.as_str()));
            uninstall_archive_app(&entry).unwrap();
            assert!(!dest.exists(), "kurulum dizini rollback sonrası silinmeli");
            let _ = crate::store::path::remove_path_shim(TEST_EXE);
        });
    }

    #[test]
    fn install_source_picks_artifact_for_arch() {
        let mut entry = installable_entry(&"a".repeat(64));
        entry.supported_platforms = vec!["linux-x64".into(), "linux-aarch64".into()];
        entry.artifacts = vec![
            crate::store::ArchArtifact {
                arch: "x86_64".into(),
                download_url: "https://example.org/x.tar.gz".into(),
                checksum: "b".repeat(64),
                checksum_type: Some("sha256".into()),
                asset_pattern: Some("x-{version}.tar.gz".into()),
                checksum_asset_pattern: None,
            },
            crate::store::ArchArtifact {
                arch: "aarch64".into(),
                download_url: "https://example.org/a.tar.gz".into(),
                checksum: "c".repeat(64),
                checksum_type: Some("sha256".into()),
                asset_pattern: Some("a-{version}.tar.gz".into()),
                checksum_asset_pattern: None,
            },
        ];
        entry.download_url = None;
        entry.checksum = None;
        let x = InstallSource::from_entry_for_arch(&entry, CpuArch::X86_64).unwrap();
        assert_eq!(x.url, "https://example.org/x.tar.gz");
        assert_eq!(x.checksum, "b".repeat(64));
        let a = InstallSource::from_entry_for_arch(&entry, CpuArch::Aarch64).unwrap();
        assert_eq!(a.url, "https://example.org/a.tar.gz");
        let src = entry.release_source_for(CpuArch::Aarch64);
        // no release on installable_entry
        assert!(src.is_none());
    }

    #[test]
    fn package_manager_rejects_curl_bash_argv() {
        let entry = package_entry(
            InstallMethod::Pipx,
            vec!["curl", "-fsSL", "https://x", "|", "bash"],
        );
        let err = install_via_package_manager(&entry).unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
    }

    #[test]
    fn package_manager_rejects_manual_method() {
        let mut entry = package_entry(InstallMethod::Pipx, vec!["pipx", "install", "x"]);
        entry.install_method = InstallMethod::Manual;
        let err = install_via_package_manager(&entry).unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
    }

    #[test]
    fn package_manager_reports_missing_program() {
        let entry = package_entry(
            InstallMethod::Brew,
            vec!["brew", "install", "kesinlikle-yok-xyz"],
        );
        // brew yoksa erken hata; varsa paket yokluğu da Internal olur.
        let err = install_via_package_manager(&entry).unwrap_err();
        assert!(matches!(err, InstallError::Internal(_)), "hata: {err}");
    }

    #[test]
    fn install_source_requires_https_and_valid_checksum() {
        assert!(InstallSource::new("https://example.org/a.tar.gz", &"a".repeat(64)).is_ok());
        assert!(InstallSource::new("http://example.org/a.tar.gz", &"a".repeat(64)).is_err());
        assert!(InstallSource::new("https://example.org/a.tar.gz", "kisa").is_err());
        assert!(InstallSource::new("https://example.org/a.tar.gz", "").is_err());
    }

    #[test]
    fn install_release_uses_the_resolved_url_and_checksum() {
        // Canlı çözümlemede gelen adres/özet, gömülü katalogdakinden farklı
        // olabilir; kurulum **çözümlenen** değerleri kullanmalıdır.
        with_isolated_home(|| {
            let payload =
                make_tar_gz(&[(&format!("paket/{TEST_EXE}"), b"#!/bin/sh\n" as &[u8], true)]);
            let sum = hex(&Sha256::digest(&payload));
            let release = crate::store::manifest::ResolvedRelease {
                version: "2.0.0".into(),
                download_url: "https://example.org/yeni.tar.gz".into(),
                checksum: sum.clone(),
                checksum_url: "https://example.org/yeni.sha256".into(),
            };
            let mut entry = installable_entry(&"0".repeat(64));
            entry.download_url = Some("https://example.org/eski.tar.gz".into());

            let dest = crate::store::test_store_dir().join("test-release");
            let downloader = FakeDownloader::with_payload(payload);
            let outcome = install_release(&entry, &dest, &release, &downloader).unwrap();
            let store_bin = dest.join("bin").join(TEST_EXE);
            assert!(store_bin.is_file());
            let shim_suffix = PathBuf::from(".local/bin").join(TEST_EXE);
            assert!(
                outcome.executable_path == store_bin
                    || outcome.executable_path.ends_with(&shim_suffix),
                "got {}",
                outcome.executable_path.display()
            );

            let calls = downloader.calls.borrow();
            assert_eq!(calls.len(), 1);
            assert_eq!(
                calls[0].0, "https://example.org/yeni.tar.gz",
                "canlı çözümlenen adres kullanılmalı"
            );
            let _ = crate::store::path::remove_path_shim(TEST_EXE);
            let _ = fs::remove_dir_all(&dest);
        });
    }

    #[test]
    fn install_release_rejects_a_mismatched_checksum() {
        let payload = make_tar_gz(&[(&format!("paket/{TEST_EXE}"), b"#!/bin/sh\n" as &[u8], true)]);
        let release = crate::store::manifest::ResolvedRelease {
            version: "2.0.0".into(),
            download_url: "https://example.org/yeni.tar.gz".into(),
            checksum: "0".repeat(64),
            checksum_url: "https://example.org/yeni.sha256".into(),
        };
        let entry = installable_entry(&"0".repeat(64));
        let dest = crate::store::test_store_dir().join("test-release-bad");
        let downloader = FakeDownloader::with_payload(payload);
        let err = install_release(&entry, &dest, &release, &downloader).unwrap_err();
        assert!(matches!(err, InstallError::Download(_)), "hata: {err}");
        assert!(!dest.join("bin").join(TEST_EXE).exists());
        let _ = fs::remove_dir_all(&dest);
    }
}
