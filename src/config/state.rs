//! Kalıcı kullanıcı durumu (`state.toml`).
//!
//! Kullanıcının **elle** düzenlediği `config.toml` ile karıştırılmamalıdır:
//! bu dosya yalnızca Nova tarafından yazılır ve favorileri, başlatma
//! geçmişini, proje dizinlerini ve tercihleri taşır.
//!
//! İki kural:
//!
//! 1. **Bozuk dosya veri kaybettirmez.** Ayrıştırma başarısız olursa dosya
//!    `state.toml.bozuk-<saniye>` adıyla yedeklenir ve boş durumla devam
//!    edilir. Çağıran, dönen [`StateLoad`] içindeki uyarıyı kullanıcıya
//!    göstermekle yükümlüdür.
//! 2. **Yazma atomiktir ve özeldir.** Aynı dizinde benzersiz, `create_new` ile
//!    ve Unix'te `0600` izinleriyle açılan geçici dosya `sync_all` ile diske
//!    itilir, sonra `rename` ile yerine konur.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Geçmişte en fazla tutulacak kayıt sayısı (üst sınır).
pub const RECENT_LIMIT_MAX: usize = 100;
/// Geçmişte tutulacak kayıt sayısı (varsayılan).
pub const RECENT_LIMIT_DEFAULT: usize = 20;

/// Başlatma denemesinin launcher'a ulaşıp ulaşmadığı.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    /// Terminal başlatma isteği gönderildi. Harici uygulamanın çalıştığını
    /// doğrulamaz.
    #[default]
    RequestSent,
    /// Deneme araç kontrolü veya terminal isteği sırasında reddedildi.
    Rejected,
}

/// Tek bir başlatma denemesi kaydı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentEntry {
    /// Aracın kimliği.
    pub id: String,
    /// Kayıt anındaki görünen ad (araç katalogdan kalksa da anlaşılsın).
    #[serde(default)]
    pub name: String,
    /// Kayıt anı (UNIX saniye).
    pub at: u64,
    /// Son denemenin sonucu. Eski durum dosyaları geçmişteki isteğin
    /// gönderildiğini varsayar; eski kayıtlar yalnızca başarılı isteklerde
    /// oluşturuluyordu.
    #[serde(default)]
    pub status: AttemptStatus,
}

/// `state.toml`'un kök yapısı.
///
/// Tüm alanlar `#[serde(default)]`: yeni alan eklenince eski dosya okunamaz
/// hâle gelmemelidir.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StateFile {
    /// Favori araç kimlikleri (sıralı ve tekrarsız).
    #[serde(default)]
    pub favorites: BTreeSet<String>,
    /// Son kullanılanlar, **en yeni önce**.
    #[serde(default)]
    pub recents: Vec<RecentEntry>,
    /// `config.toml`'da duran ama kullanıcı tarafından silinen araçlar.
    #[serde(default)]
    pub hidden_tools: BTreeSet<String>,
    /// Kullanıcının form ile eklediği araçlar (kaynak: Nova).
    #[serde(default)]
    pub custom_tools: Vec<super::ToolEntry>,
    /// Favori proje dizinleri (tam yollar).
    #[serde(default)]
    pub project_dirs: Vec<String>,
    /// Kalıcı tercihler.
    #[serde(default)]
    pub prefs: Prefs,
}

/// Kalıcı tercihler.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    /// Desteklenen terminal emülatörleri arasında tercih (ör. `"konsole"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_terminal: Option<String>,
    /// `current`/`home` yerine kullanılacak varsayılan çalışma dizini.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_working_dir: Option<String>,
    /// Başlatma geçmişinde tutulacak en fazla kayıt sayısı.
    ///
    /// Kullanıcı `config.toml`'da `[ui] recent_limit` ile geçersiz kılar.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "recent_limit"
    )]
    pub recent_limit_value: Option<usize>,
    /// Arayüz teması (ör. `"varsayılan"`, `"karanlık"`). `None` ise sistem
    /// varsayılanı kullanılır.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Varsayılan AI komut satırı aracının katalog kimliği (`id`).
    /// Yalnızca kurulu olan AI kategorisindeki araçlar seçilebilir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_ai_tool: Option<String>,
    /// Resim dosyaları için varsayılan uygulama (katalog `id` veya executable adı).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_image_app: Option<String>,
    /// Video dosyaları için varsayılan uygulama (katalog `id` veya executable adı).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_video_app: Option<String>,
}

impl StateFile {
    /// Geçmiş sınırını okur (bkz. [`Prefs::recent_limit`]).
    pub fn recent_limit(&self) -> usize {
        self.prefs.recent_limit()
    }
}

impl Prefs {
    /// Geçmiş sınırı; tanımsızsa [`RECENT_LIMIT_DEFAULT`].
    ///
    /// Değer en az 1, en çok [`RECENT_LIMIT_MAX`] olarak kırpılır.
    pub fn recent_limit(&self) -> usize {
        // `recent_limit` 0 ise varsayılan kabul edilir (yanlışlıkla tüm
        // geçmişin susturulmasını önler), aşırı büyük değer kırpılır.
        match self.recent_limit_value {
            Some(0) | None => RECENT_LIMIT_DEFAULT,
            Some(n) => n.min(RECENT_LIMIT_MAX),
        }
    }
}

/// `state.toml`'un yolu (`$XDG_CONFIG_HOME/nova/state.toml`; varsayılanı
/// `~/.config/nova/state.toml`).
///
/// `NOVA_STATE` ortam değişkeni verilmişse o yol kullanılır (yalnızca testler
/// ve taşınabilir kurulumlar için). XDG dizini yoksa `$HOME/.config/nova`
/// kullanılır; ikisi de yoksa `None` döner.
pub fn state_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NOVA_STATE").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    crate::config::user_config_dir().map(|dir| dir.join("state.toml"))
}

/// Durum yükleme sonucu.
#[derive(Debug)]
pub struct StateLoad {
    /// Okunan (ya da boş) durum.
    pub state: StateFile,
    /// Kullanıcıya gösterilecek uyarı (bozuk dosya yedeklendiyse).
    pub warning: Option<String>,
    /// Dosya diskte var mıydı? `false` ise ilk çalıştırma varsayılanları
    /// (gömülü favoriler) tohumlanır.
    pub existed: bool,
}

impl StateLoad {
    /// Dosya yoksa boş durum.
    fn missing() -> Self {
        Self {
            state: StateFile::default(),
            warning: None,
            existed: false,
        }
    }
}

/// Durum dosyasını okur.
///
/// - Dosya yoksa: boş durum, `existed = false`.
/// - Dosya bozuk veya okunamıyorsa: dosya yedeklenir, boş durum ve uyarı.
pub fn load_state(path: &Path) -> StateLoad {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return StateLoad::missing(),
        Err(e) => {
            return backup_broken(path, StateFile::default(), &format!("okunamadı: {e}"));
        }
    };
    match toml::from_str::<StateFile>(&content) {
        Ok(state) => StateLoad {
            state,
            warning: None,
            existed: true,
        },
        Err(e) => backup_broken(path, StateFile::default(), &e.to_string()),
    }
}

/// Bozuk durum dosyasını yanına taşır ve uyarı üretir.
///
/// Taşıma başarısız olursa dosya **silinmez** (veri kaybından kaçınmak için);
/// yalnızca yazma sırasında yeni adla çakışmayacak şekilde durum boş kalır.
fn backup_broken(path: &Path, empty: StateFile, reason: &str) -> StateLoad {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    let backup = if ext.is_empty() {
        path.with_extension(format!("bozuk-{stamp}"))
    } else {
        path.with_extension(format!("{ext}.bozuk-{stamp}"))
    };
    let warning = match fs::rename(path, &backup) {
        Ok(()) => format!(
            "Durum dosyası bozuktu ({reason}); '{}' olarak yedeklendi",
            backup.display()
        ),
        Err(e) => format!(
            "Durum dosyası bozuktu ({reason}) ve yedeklenemedi ({e}); \
             dosya olduğu gibi duruyor"
        ),
    };
    StateLoad {
        state: empty,
        warning: Some(warning),
        existed: false,
    }
}

/// Durumu atomik olarak yazar (üst dizini oluşturur).
pub fn save_state(path: &Path, state: &StateFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("'{}' dizini oluşturulamadı: {e}", parent.display()))?;
    }
    let content =
        toml::to_string_pretty(state).map_err(|e| format!("Durum TOML'a çevrilemedi: {e}"))?;

    let (tmp, mut file) = create_private_temp_sibling(path).map_err(|e| {
        format!(
            "'{}' için güvenli geçici dosya oluşturulamadı: {e}",
            path.display()
        )
    })?;
    if let Err(e) = file
        .write_all(content.as_bytes())
        .and_then(|()| file.sync_all())
    {
        let _ = fs::remove_file(&tmp);
        return Err(format!("'{}' yazılamadı: {e}", tmp.display()));
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("'{}' yerine konamadı: {e}", path.display())
    })
}

fn create_private_temp_sibling(path: &Path) -> std::io::Result<(PathBuf, File)> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("state"))
        .to_string_lossy();
    for _ in 0..100 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = parent.join(format!(
            ".{file_name}.tmp-{}-{nanos}-{counter}",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "benzersiz geçici dosya adı oluşturulamadı",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nova-state-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_state_is_not_an_error() {
        let dir = temp_dir("eksik");
        let load = load_state(&dir.join("state.toml"));
        assert!(!load.existed);
        assert!(load.warning.is_none());
        assert!(load.state.favorites.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_round_trips() {
        let dir = temp_dir("tur");
        let path = dir.join("state.toml");
        let mut state = StateFile::default();
        state.favorites.insert("opencode".into());
        state.recents.push(RecentEntry {
            id: "opencode".into(),
            name: "OpenCode".into(),
            at: 1_700_000_000,
            status: AttemptStatus::RequestSent,
        });
        state.project_dirs.push("/home/x/proje".into());
        save_state(&path, &state).unwrap();

        let load = load_state(&path);
        assert!(load.existed);
        assert_eq!(load.state.favorites, state.favorites);
        assert_eq!(load.state.recents, state.recents);
        assert_eq!(load.state.project_dirs, state.project_dirs);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_state_is_backed_up_and_not_lost() {
        let dir = temp_dir("bozuk");
        let path = dir.join("state.toml");
        fs::write(&path, "favorites = [ 'a'\n[[[\n").unwrap();

        let load = load_state(&path);
        assert!(!load.existed);
        assert!(load.warning.as_deref().unwrap_or("").contains("yedeklendi"));
        assert!(!path.exists(), "bozuk dosya yerinde kalmamalı");
        let backups: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("bozuk"))
            .collect();
        assert_eq!(backups.len(), 1, "tek yedek olmalı");
        let kept = fs::read_to_string(backups[0].path()).unwrap();
        assert!(kept.contains("favorites"), "yedek içerik korunmalı");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_leaves_no_temp_file() {
        let dir = temp_dir("gecici");
        let path = dir.join("state.toml");
        save_state(&path, &StateFile::default()).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn saved_state_is_private_to_the_current_user() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("izin");
        let path = dir.join("state.toml");
        save_state(&path, &StateFile::default()).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode & 0o077, 0, "durum dosyası başkalarına kapalı olmalı");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_state_without_new_fields_loads() {
        let state: StateFile = toml::from_str("favorites = [\"btop\"]\n").unwrap();
        assert!(state.favorites.contains("btop"));
        assert!(state.recents.is_empty());
        assert_eq!(state.recent_limit(), RECENT_LIMIT_DEFAULT);
    }

    #[test]
    fn old_recent_entries_default_to_request_sent() {
        let state: StateFile =
            toml::from_str("[[recents]]\nid = \"btop\"\nname = \"btop\"\nat = 10\n").unwrap();
        assert_eq!(state.recents[0].status, AttemptStatus::RequestSent);
    }

    #[test]
    fn recent_limit_is_clamped() {
        assert_eq!(Prefs::default().recent_limit(), RECENT_LIMIT_DEFAULT);
        let p: Prefs = toml::from_str("recent_limit = 0\n").unwrap();
        assert_eq!(p.recent_limit(), RECENT_LIMIT_DEFAULT);
        let p: Prefs = toml::from_str("recent_limit = 500\n").unwrap();
        assert_eq!(p.recent_limit(), RECENT_LIMIT_MAX);
        let p: Prefs = toml::from_str("recent_limit = 5\n").unwrap();
        assert_eq!(p.recent_limit(), 5);
    }
}
