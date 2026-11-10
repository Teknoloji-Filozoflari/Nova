//! Nova-Commands tüketimi — **Komut Rehberi**.
//!
//! Üç kaynak **ayrı** tutulur (asla karıştırılmaz):
//!
//! 1. **Komut Bilgi Bankası** — `kind: nova-commands-bank`
//!    (`assets/commands/catalog.json`, Nova-Commands `metadata/`).
//! 2. **Mağaza** — `crate::store` (installable apps; bu modül değil).
//! 3. **Sistemimde Bulunan Komutlar** — `kind: nova-local-commands`
//!    (PATH keşfi stub'ı; bank `verified` miras almaz).
//!
//! Bilgi mimarisi (görev odaklı): Başlangıç → Göreve göre → Kullanım bağlamı
//! → Tüm komutlar → Sistemde bulunanlar. Komutlar asla otomatik çalıştırılmaz.
//!
//! Sözleşme:
//! <https://github.com/Teknoloji-Filozoflari/Nova-Commands/blob/main/docs/NOVA_INTEGRATION.md>

pub mod bank;
pub mod checklist;
pub mod filters;
pub mod loader;
pub mod local;
pub mod repo_sync;

pub use bank::{
    CommandBank, CommandEntry, ConceptEntry, GuideEntry, LocalizedString, ProfileEntry, RiskLevel,
    TaskEntry, VerificationStatus, BANK_KIND,
};
pub use checklist::{
    server_security_steps, server_security_title, ChecklistStepState, ChecklistStepView,
    SERVER_SECURITY_GUIDE_ID,
};
pub use filters::{
    bank_distro_values, bank_os_values, bank_shell_values, command_os_fit, cycle_optional,
    cycle_risk, host_os_bank_id, CommandsFilters, LearningLevelFilter, OsFit,
};
pub use loader::{
    load_bank, load_bank_preferring_cache, load_manifest, load_search_index, CommandsManifest,
    SearchEntry, SearchIndex, SearchName,
};
pub use local::{LocalCommandsSnapshot, LocalEntry, LOCAL_KIND};
pub use repo_sync::{
    format_sync_note, refresh_commands_from_repo, CommandsRepoSync, FetchedCommandsBank,
};

/// Host Arch mi? (`/etc/arch-release`) — paket ipuçlarında pacman önceliği.
pub fn host_is_arch() -> bool {
    std::path::Path::new("/etc/arch-release").exists()
}

/// Sol paneldeki Komut Rehberi sekmesi etiketi (TR varsayılan; i18n `Msg` ile çizilir).
pub const COMMANDS_TAB_LABEL: &str = "Komut Rehberi";

/// Rehber içinde hangi kaynak listeleniyor (PATH ≠ banka).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandsSource {
    /// Kürate banka (`nova-commands-bank`).
    #[default]
    Bank,
    /// Yerel PATH keşfi (`nova-local-commands`) — bank doğrulamasını miras almaz.
    Local,
}

/// Görev odaklı gezinme modu (Bilgi mimarisi 1–6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandsBrowseMode {
    /// 1 · Başlangıç — öğrenme sırası, düşük risk.
    #[default]
    Start,
    /// 2 · Göreve göre — katalog `tasks`, gruplu.
    ByTask,
    /// 3 · Kullanım bağlamı — çoklu profil seçimi.
    Context,
    /// 4 · Tüm komutlar — teknik kategoriler (+ arama).
    AllCommands,
    /// 5 · Sunucu güvenlik kontrolü — sıralı checklist (çalıştırmaz).
    ServerSecurity,
    /// 6 · Sistemde bulunanlar — PATH stub (katalogdan ayrı).
    SystemPath,
}

impl CommandsBrowseMode {
    pub const ALL: [Self; 6] = [
        Self::Start,
        Self::ByTask,
        Self::Context,
        Self::AllCommands,
        Self::ServerSecurity,
        Self::SystemPath,
    ];

    pub fn index(self) -> usize {
        match self {
            Self::Start => 0,
            Self::ByTask => 1,
            Self::Context => 2,
            Self::AllCommands => 3,
            Self::ServerSecurity => 4,
            Self::SystemPath => 5,
        }
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL.get(i).copied().unwrap_or(Self::Start)
    }

    pub fn next(self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }

    pub fn prev(self) -> Self {
        Self::from_index((self.index() + Self::ALL.len() - 1) % Self::ALL.len())
    }

    /// Dual-concept kaynak: yalnız Sistemde bulunanlar → Local.
    pub fn source(self) -> CommandsSource {
        match self {
            Self::SystemPath => CommandsSource::Local,
            _ => CommandsSource::Bank,
        }
    }

    /// İkinci ikincil sütun (grup / profil / teknik kategori) gerekir mi?
    pub fn needs_group_column(self) -> bool {
        matches!(self, Self::ByTask | Self::Context | Self::AllCommands)
    }

    /// Checklist listesi mi (Sunucu güvenlik)?
    pub fn is_checklist(self) -> bool {
        matches!(self, Self::ServerSecurity)
    }
}

impl CommandsSource {
    pub fn next(self) -> Self {
        match self {
            Self::Bank => Self::Local,
            Self::Local => Self::Bank,
        }
    }

    pub fn prev(self) -> Self {
        self.next()
    }

    pub fn index(self) -> usize {
        match self {
            Self::Bank => 0,
            Self::Local => 1,
        }
    }
}

/// Banka kategorileri (Nova-Commands taxonomy §8; katalogdan türetilir).
pub fn bank_categories(bank: &CommandBank) -> Vec<&str> {
    let mut cats: Vec<&str> = bank.commands.iter().map(|c| c.category.as_str()).collect();
    cats.sort_unstable();
    cats.dedup();
    cats
}

/// Başlangıç öğrenme sırası: beginner konseptlerin ilgili komutları (tekil).
///
/// Düşük risk tercih edilir; konsept sırası sabit öğrenme yolu.
pub fn start_learning_command_ids(bank: &CommandBank) -> Vec<String> {
    const PATH_ORDER: &[&str] = &[
        "terminal-basics",
        "path",
        "shell-pipes-redirection",
        "permissions",
        "networking-basics",
        "json-data",
        "package-managers",
        "storage",
        "systemd",
        "debugging",
    ];
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for cid in PATH_ORDER {
        let Some(concept) = bank.concepts.iter().find(|c| c.id == *cid) else {
            continue;
        };
        for cmd_id in &concept.related_commands {
            if !seen.insert(cmd_id.clone()) {
                continue;
            }
            let Some(cmd) = bank.find_by_id_or_name(cmd_id) else {
                continue;
            };
            // Öğrenme: destructive/dangerous atlanır; safe/caution alınır.
            if matches!(cmd.risk, RiskLevel::Destructive | RiskLevel::Dangerous) {
                continue;
            }
            out.push(cmd.id.clone());
        }
    }
    // Konseptte eksik kalan safe komutları doldurma (kısa kuyruk).
    if out.len() < 12 {
        let mut extras: Vec<_> = bank
            .commands
            .iter()
            .filter(|c| c.risk == RiskLevel::Safe && !seen.contains(&c.id))
            .collect();
        extras.sort_by_key(|c| {
            (
                std::cmp::Reverse(c.nova.ui_priority.unwrap_or(0)),
                c.name.as_str(),
            )
        });
        for c in extras.into_iter().take(12usize.saturating_sub(out.len())) {
            out.push(c.id.clone());
        }
    }
    out
}
