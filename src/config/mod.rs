//! Yapılandırma modülü.
//!
//! İki veri kaynağı bilinçli olarak ayrıdır:
//!
//! 1. **Varsayılanlar**: uygulamayla birlikte gelen gömülü katalog
//!    (`assets/catalog/*.toml`, kategoriye göre ayrılmış; `include_str!` ile
//!    derlenir). Kullanıcı bunu düzenlemez; güncelleme varsayılanları
//!    değiştirebilir.
//! 2. **Kullanıcı verisi**: `$XDG_CONFIG_HOME/nova/config.toml` (varsayılanı
//!    `~/.config/nova/config.toml`; bkz.
//!    [`user_config_path`]). Yalnızca kullanıcının değiştirdiği/eklediği
//!    şeyleri içerir: kimliğe göre geçersiz kılmalar, favoriler ve
//!    kullanıcının eklediği araçlar.
//!
//! İkisi [`crate::catalog::loader`] içinde kimliğe (`id`) göre birleştirilir.
//!
//! Bu modül yalnızca **serde tiplerini** tanımlar; birleştirme ve
//! doğrulama mantığı `catalog` tarafındadır. Böylece `catalog` ile `config`
//! arasında döngüsel bağımlılık oluşmaz.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub mod state;

/// Bir aracın başlatma türü.
///
/// Bu, *nasıl* başlatılacağını değil, *ne* başlatılacağını anlatır; gerçek
/// terminal açma biçimi (`launch_mode`) ayrı bir alandır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchKind {
    /// Aracı yeni sekmede doğrudan çalıştır (TUI araçları).
    #[default]
    Interactive,
    /// Yeni sekmede aracın sabit kullanım metnini göster.
    ///
    /// Metin TOML'da yazılır; araç `--help` ile **çalıştırılmaz**. Metin tek
    /// sayfaya sığmıyorsa `handlers` listesindeki ilk bulunan sayfalayıcı
    /// (örn. `less`) ile gösterilir.
    CommandHelp,
    /// Araç kurulu olsa bile Nova'dan başlatılamaz; kabuk entegrasyonu
    /// gerektiğini anlatan bir bilgi görünümü açılır.
    ShellIntegrationInfo,
    /// Kullanıcının TOML'da verdiği komut ve argümanlarla çalıştır.
    CustomCommand,
}

impl LaunchKind {
    /// TOML'daki metin değeri (yazma için).
    pub fn as_str(self) -> &'static str {
        match self {
            LaunchKind::Interactive => "interactive",
            LaunchKind::CommandHelp => "command_help",
            LaunchKind::ShellIntegrationInfo => "shell_integration_info",
            LaunchKind::CustomCommand => "custom_command",
        }
    }

    /// Arayüzde gösterilecek kısa Türkçe etiket.
    pub fn label(self) -> &'static str {
        match self {
            LaunchKind::Interactive => "etkileşimli",
            LaunchKind::CommandHelp => "kullanım bilgisi",
            LaunchKind::ShellIntegrationInfo => "kabuk entegrasyonu gerekir",
            LaunchKind::CustomCommand => "özel komut",
        }
    }

    /// Arayüzde gösterilecek kısa işaret; etkileşimli araçlar (çoğunluk)
    /// herhangi bir işaret taşımaz, yalnızca özel türler taşır.
    pub fn mark(self) -> Option<&'static str> {
        match self {
            LaunchKind::Interactive => None,
            other => Some(other.label()),
        }
    }

    /// Bu tür çalıştırılabilir bir program gerektirir mi?
    ///
    /// `shell_integration_info` yalnızca bilgi gösterir; araç kurulu olmasa
    /// da çalışabilir.
    pub fn runs_program(self) -> bool {
        !matches!(self, LaunchKind::ShellIntegrationInfo)
    }
}

impl std::fmt::Display for LaunchKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Katalog dosyasındaki tek bir araç (hem gömülü hem kullanıcı dosyası).
///
/// Tüm alanlar `#[serde(default)]` taşır: eski bir yapılandırma yeni bir alan
/// eklenince okunamaz hâle gelmemelidir.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ToolEntry {
    /// Değişmez ve benzersiz kimlik (birleştirme anahtarı).
    pub id: String,
    /// Ekranda gösterilecek ad.
    #[serde(default)]
    pub name: String,
    /// Kısa ve anlaşılır açıklama.
    #[serde(default)]
    pub description: String,
    /// Ait olduğu kategori (bkz. `CATEGORIES`).
    #[serde(default)]
    pub category: String,
    /// Çalıştırılacak program (PATH üzerinde aranır). Boşsa `id` kullanılır.
    #[serde(default, alias = "command")]
    pub executable: String,
    /// Ayrı argüman listesi (kabuk yok, genişletme yok).
    #[serde(default)]
    pub args: Vec<String>,
    /// Çalıştırma türü; verilmezse `interactive`.
    #[serde(default)]
    pub launch_kind: LaunchKind,
    /// Çalıştırma biçimi: "terminal", "background" veya "detached".
    #[serde(default)]
    pub launch_mode: String,
    /// Çalışma dizini: "current", "home", "custom" veya "project".
    #[serde(default)]
    pub working_directory: String,
    /// `working_directory` "custom" veya "project" ise kullanılacak yol.
    #[serde(default)]
    pub working_dir_path: Option<String>,
    /// Favori durumu.
    #[serde(default)]
    pub favorite: bool,
    /// `command_help` için gösterilecek sabit metin.
    #[serde(default)]
    pub help_text: Option<String>,
    /// `command_help` metnini ekranda tutmak için kullanılabilecek
    /// sayfalayıcılar (bulunan ilki kullanılır). Kabuk yok.
    #[serde(default)]
    pub handlers: Vec<String>,
    /// `shell_integration_info` için gösterilecek açıklama.
    #[serde(default)]
    pub info_text: Option<String>,
    /// Bir alternatif ise, asıl aracın kimliği (örn. `gdu` → `duf`).
    #[serde(default)]
    pub alternative_to: Option<String>,
    /// Doğrulanmış kullanım örnekleri / kısayollar (katalog metaverisi).
    /// Kullanıcı açıkça başlatmadıkça Nova bunları çalıştırmaz.
    #[serde(default)]
    pub examples: Vec<String>,
    /// Kabuk entegrasyonu veya ek kurulum bilgisi (ör. shell entegrasyonu
    /// kurulum talimatları). Gömülü katalogdan veya kullanıcı yapılandırmasından
    /// gelir; Nova tarafından yürütülmez.
    #[serde(default)]
    pub setup_info: Option<String>,
}

/// Bir katalog girişinin nereden geldiği.
///
/// Yalnızca bellekte kullanılır; dosyaya yazılmaz (birleştirme zaten `id`ye
/// göre yapılır).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EntryOrigin {
    /// Uygulamayla gelen gömülü katalog.
    #[default]
    Builtin,
    /// Kullanıcı yapılandırması.
    User,
}

/// Katalog dosyasının kök yapısı.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CatalogFile {
    #[serde(default)]
    pub tools: Vec<ToolEntry>,
}

/// Arayüz yapılandırması.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiConfig {
    #[serde(default)]
    pub theme: String,
    /// Başlatma geçmişinde tutulacak en fazla kayıt sayısı.
    #[serde(default)]
    pub recent_limit: Option<usize>,
}

/// Kullanıcı yapılandırmasının kök yapısı.
///
/// `tools` içinde aynı `id`ye sahip bir gömülü kayıt varsa alanlar
/// **birleştirilir** (kullanıcı alanı kazanır); yoksa kayıt yeni araç olarak
/// eklenir.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserConfig {
    #[serde(default)]
    pub tools: Vec<ToolEntry>,
    #[serde(default)]
    pub ui: UiConfig,
}

/// Nova kullanıcı yapılandırma dizini.
///
/// Mutlak `$XDG_CONFIG_HOME` varsa onu kullanır; değişken boş, göreli veya
/// tanımsızsa `$HOME/.config` yoluna döner. Göreli XDG yolu, XDG temel dizin
/// kuralı gereği geçersiz sayılır.
pub(crate) fn user_config_dir() -> Option<PathBuf> {
    let xdg = env::var_os("XDG_CONFIG_HOME");
    let home = env::var_os("HOME");
    user_config_dir_from(xdg.as_deref(), home.as_deref())
}

fn user_config_dir_from(
    xdg: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if let Some(xdg) = xdg.filter(|value| !value.is_empty()) {
        let path = Path::new(xdg);
        if path.is_absolute() {
            return Some(path.join("nova"));
        }
    }
    home.filter(|value| !value.is_empty())
        .map(|value| PathBuf::from(value).join(".config").join("nova"))
}

/// Kullanıcı yapılandırma dosyasının yolu (`$XDG_CONFIG_HOME/nova/config.toml`;
/// XDG değişkeni yoksa `~/.config/nova/config.toml`).
///
/// `$HOME` yoksa `None` döner; çağıran bunu anlaşılır bir hataya çevirir.
pub fn user_config_path() -> Option<PathBuf> {
    user_config_dir().map(|dir| dir.join("config.toml"))
}

/// Kullanıcı yapılandırmasını dosyadan okur.
///
/// Dosya yoksa boş yapılandırma döner (yalnızca gömülü katalog kullanılır).
pub fn load_user_config(path: &Path) -> Result<UserConfig, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(UserConfig::default());
    }
    let content =
        fs::read_to_string(path).map_err(|e| format!("'{}' okunamadı: {e}", path.display()))?;
    toml::from_str(&content)
        .map_err(|e| format!("'{}' geçerli TOML değil: {e}", path.display()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_config_dir_prefers_absolute_xdg_and_rejects_relative_xdg() {
        let home = std::ffi::OsStr::new("/home/example");
        assert_eq!(
            user_config_dir_from(Some(std::ffi::OsStr::new("/tmp/xdg")), Some(home)),
            Some(PathBuf::from("/tmp/xdg/nova"))
        );
        assert_eq!(
            user_config_dir_from(Some(std::ffi::OsStr::new("relative")), Some(home)),
            Some(PathBuf::from("/home/example/.config/nova"))
        );
        assert_eq!(
            user_config_dir_from(Some(std::ffi::OsStr::new("")), Some(home)),
            Some(PathBuf::from("/home/example/.config/nova"))
        );
        assert_eq!(
            user_config_dir_from(Some(std::ffi::OsStr::new("/tmp/xdg")), None),
            Some(PathBuf::from("/tmp/xdg/nova"))
        );
        assert_eq!(
            user_config_dir_from(None, Some(home)),
            Some(PathBuf::from("/home/example/.config/nova"))
        );
        assert_eq!(user_config_dir_from(None, None), None);
        assert_eq!(
            user_config_dir_from(
                Some(std::ffi::OsStr::new("")),
                Some(std::ffi::OsStr::new(""))
            ),
            None
        );
    }

    #[test]
    fn default_entry_is_builtin_and_interactive() {
        let entry = ToolEntry::default();
        assert_eq!(entry.launch_kind, LaunchKind::Interactive);
        assert_eq!(entry.launch_mode, "");
        assert_eq!(entry.working_directory, "");
        assert!(entry.working_dir_path.is_none());
        assert!(!entry.favorite);
        assert!(entry.handlers.is_empty());
        assert!(entry.alternative_to.is_none());
    }

    #[test]
    fn launch_kind_round_trips_through_toml() {
        for kind in [
            LaunchKind::Interactive,
            LaunchKind::CommandHelp,
            LaunchKind::ShellIntegrationInfo,
            LaunchKind::CustomCommand,
        ] {
            let text = format!("id = \"x\"\nlaunch_kind = \"{}\"", kind.as_str());
            let entry: ToolEntry = toml::from_str(&text).unwrap();
            assert_eq!(entry.launch_kind, kind);
        }
    }

    #[test]
    fn unknown_launch_kind_is_a_clear_error() {
        let text = "id = \"x\"\nlaunch_kind = \"uçan\"\n";
        let err = toml::from_str::<ToolEntry>(text).unwrap_err().to_string();
        assert!(err.contains("launch_kind"), "{err}");
        assert!(err.contains("uçan"), "hata değeri anmalı: {err}");
    }

    #[test]
    fn working_dir_path_round_trips_through_toml() {
        let entry = ToolEntry {
            id: "lazygit".into(),
            working_directory: "custom".into(),
            working_dir_path: Some("/home/kullanici/git/depo".into()),
            ..ToolEntry::default()
        };
        let cfg = UserConfig {
            tools: vec![entry],
            ..UserConfig::default()
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: UserConfig = toml::from_str(&text).unwrap();
        assert_eq!(
            back.tools[0].working_dir_path.as_deref(),
            Some("/home/kullanici/git/depo")
        );
        assert_eq!(back.tools[0].working_directory, "custom");
    }

    #[test]
    fn saved_config_has_no_runtime_only_fields() {
        let cfg = UserConfig {
            tools: vec![ToolEntry {
                id: "x".into(),
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(!text.contains("origin"), "{text}");
        assert!(!text.contains("status"), "{text}");
        assert!(!text.contains("resolved_path"), "{text}");
    }

    #[test]
    fn older_configs_without_new_fields_still_load() {
        let text = r#"
[[tools]]
id = "htop"
name = "htop"
command = "htop"
category = "Sistem ve Kaynak Kullanımı"
description = "t"
args = []
working_directory = "home"
"#;
        let cfg: UserConfig = toml::from_str(text).unwrap();
        let tool = &cfg.tools[0];
        assert!(tool.working_dir_path.is_none());
        assert_eq!(tool.executable, "htop", "eski 'command' alanı okunmalı");
        assert_eq!(tool.launch_kind, LaunchKind::Interactive);
    }

    #[test]
    fn missing_user_config_is_not_an_error() {
        let dir = std::env::temp_dir().join(format!("nova-cfg-{}", std::process::id()));
        let path = dir.join("yok").join("config.toml");
        let cfg = load_user_config(&path).expect("eksik dosya hata değil");
        assert!(cfg.tools.is_empty());
    }

    #[test]
    fn broken_user_config_error_names_the_file() {
        let dir = std::env::temp_dir().join(format!("nova-cfg-bozuk-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "[[tools]\nid = \"x\"\n").unwrap();
        let err = load_user_config(&path).unwrap_err().to_string();
        assert!(err.contains("config.toml"), "{err}");
        assert!(err.contains("TOML"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }
}
