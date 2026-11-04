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
//! Sözleşme:
//! <https://github.com/Teknoloji-Filozoflari/Nova-Commands/blob/main/docs/NOVA_INTEGRATION.md>

pub mod bank;
pub mod loader;
pub mod local;
pub mod repo_sync;

pub use bank::{
    CommandBank, CommandEntry, ConceptEntry, LocalizedString, RiskLevel, VerificationStatus,
    BANK_KIND,
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

/// Rehber içinde hangi kaynak listeleniyor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandsSource {
    /// Kürate banka (`nova-commands-bank`).
    #[default]
    Bank,
    /// Yerel PATH keşfi (`nova-local-commands`) — bank doğrulamasını miras almaz.
    Local,
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
