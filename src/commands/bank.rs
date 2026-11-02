//! Kürate komut bilgi bankası modelleri (`nova-commands-bank`).

use serde::Deserialize;

/// Katalog `kind` sabiti — Mağaza / yerel PATH ile karıştırılmaz.
pub const BANK_KIND: &str = "nova-commands-bank";

/// Risk seviyesi (YAML/JSON).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    #[default]
    Safe,
    Caution,
    Dangerous,
    Destructive,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Caution => "caution",
            Self::Dangerous => "dangerous",
            Self::Destructive => "destructive",
        }
    }
}

/// Doğrulama yaşam döngüsü — PATH varlığı bunu yükseltmez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    #[default]
    Draft,
    Reviewed,
    Verified,
}

impl VerificationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Reviewed => "reviewed",
            Self::Verified => "verified",
        }
    }
}

/// TR/EN (ve diğer) lokalize metin.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LocalizedString {
    pub tr: Option<String>,
    pub en: Option<String>,
}

impl LocalizedString {
    /// Nova dil koduna göre metin; yoksa `en`, sonra `tr`.
    pub fn pick(&self, lang: &str) -> &str {
        let code = lang.to_ascii_lowercase();
        let primary = match code.as_str() {
            "en" | "en-us" | "en-gb" => self.en.as_deref().or(self.tr.as_deref()),
            _ => self.tr.as_deref().or(self.en.as_deref()),
        };
        primary.unwrap_or("")
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct VerificationInfo {
    #[serde(default)]
    pub status: VerificationStatus,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub last_reviewed: Option<String>,
    #[serde(default)]
    pub reviewer: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CommandFlag {
    pub flag: String,
    #[serde(default)]
    pub meaning: LocalizedString,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CommandExample {
    #[serde(default)]
    pub title: LocalizedString,
    pub command: String,
    #[serde(default)]
    pub explanation: LocalizedString,
    #[serde(default)]
    pub risk: RiskLevel,
    #[serde(default)]
    pub requires_elevated: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Packages {
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub debian: Option<String>,
    #[serde(default)]
    pub ubuntu: Option<String>,
    #[serde(default)]
    pub fedora: Option<String>,
    #[serde(default)]
    pub rhel: Option<String>,
}

impl Packages {
    /// Arch-first tercih (proje prefs).
    pub fn preferred_hint(&self, is_arch: bool) -> Option<&str> {
        if is_arch {
            self.arch
                .as_deref()
                .or(self.debian.as_deref())
                .or(self.ubuntu.as_deref())
                .or(self.fedora.as_deref())
        } else {
            self.debian
                .as_deref()
                .or(self.ubuntu.as_deref())
                .or(self.fedora.as_deref())
                .or(self.arch.as_deref())
                .or(self.rhel.as_deref())
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct NovaHints {
    #[serde(default)]
    pub favorite_eligible: Option<bool>,
    #[serde(default)]
    pub concepts: Vec<String>,
}

/// Tek kürate komut kaydı.
#[derive(Debug, Clone, Deserialize)]
pub struct CommandEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub summary: LocalizedString,
    #[serde(default)]
    pub description: LocalizedString,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub risk: RiskLevel,
    #[serde(default)]
    pub requires_elevated: bool,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub packages: Packages,
    #[serde(default)]
    pub install_notes: LocalizedString,
    #[serde(default)]
    pub flags: Vec<CommandFlag>,
    #[serde(default)]
    pub examples: Vec<CommandExample>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub intent: IntentPhrases,
    #[serde(default)]
    pub verification: VerificationInfo,
    #[serde(default)]
    pub nova: NovaHints,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct IntentPhrases {
    #[serde(default)]
    pub tr: Vec<String>,
    #[serde(default)]
    pub en: Vec<String>,
}

impl IntentPhrases {
    pub fn all_for_lang(&self, lang: &str) -> &[String] {
        let code = lang.to_ascii_lowercase();
        if code.starts_with("en") {
            if self.en.is_empty() {
                &self.tr
            } else {
                &self.en
            }
        } else if self.tr.is_empty() {
            &self.en
        } else {
            &self.tr
        }
    }
}

/// Öğrenme konsepti makalesi.
#[derive(Debug, Clone, Deserialize)]
pub struct ConceptEntry {
    pub id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: LocalizedString,
    #[serde(default)]
    pub summary: LocalizedString,
    #[serde(default)]
    pub difficulty: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub related_commands: Vec<String>,
    #[serde(default)]
    pub related_concepts: Vec<String>,
    #[serde(default)]
    pub body: LocalizedString,
    #[serde(default)]
    pub verification: VerificationInfo,
}

/// `metadata/catalog.json` kök nesnesi.
#[derive(Debug, Clone, Deserialize)]
pub struct CommandBank {
    pub schema_version: u32,
    pub kind: String,
    #[serde(default)]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub content_sha256: Option<String>,
    #[serde(default)]
    pub command_count: Option<usize>,
    #[serde(default)]
    pub concept_count: Option<usize>,
    #[serde(default)]
    pub commands: Vec<CommandEntry>,
    #[serde(default)]
    pub concepts: Vec<ConceptEntry>,
}

impl CommandBank {
    /// `kind` / `schema_version` doğrular.
    pub fn validate_contract(&self) -> Result<(), String> {
        if self.kind != BANK_KIND {
            return Err(format!("beklenen kind={BANK_KIND}, gelen={}", self.kind));
        }
        if self.schema_version != 1 {
            return Err(format!(
                "beklenen schema_version=1, gelen={}",
                self.schema_version
            ));
        }
        Ok(())
    }

    pub fn by_category<'a>(&'a self, category: &str) -> Vec<&'a CommandEntry> {
        self.commands
            .iter()
            .filter(|c| c.category == category)
            .collect()
    }

    pub fn find_by_id_or_name(&self, name: &str) -> Option<&CommandEntry> {
        self.commands
            .iter()
            .find(|c| c.id == name || c.name == name)
    }
}
