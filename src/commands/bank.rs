//! Kürate komut bilgi bankası modelleri (`nova-commands-bank`).

use serde::Deserialize;

/// Katalog `kind` sabiti — Mağaza / yerel PATH ile karıştırılmaz.
pub const BANK_KIND: &str = "nova-commands-bank";

/// Desteklenen banka / manifest / search-index `schema_version` değerleri.
///
/// v1 (Linux-era) önbellekleri hâlâ okunur; canlı banka v2 (`package_refs`).
pub const SUPPORTED_BANK_SCHEMA_VERSIONS: &[u32] = &[1, 2];

pub(crate) fn validate_bank_schema_version(version: u32) -> Result<(), String> {
    if SUPPORTED_BANK_SCHEMA_VERSIONS.contains(&version) {
        Ok(())
    } else {
        Err(format!(
            "desteklenen schema_version={SUPPORTED_BANK_SCHEMA_VERSIONS:?}, gelen={version}"
        ))
    }
}

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

    /// Liste/detay rozeti (TR/EN kısa etiket).
    pub fn label(self, lang: &str) -> &'static str {
        let en = lang.to_ascii_lowercase().starts_with("en");
        match (self, en) {
            (Self::Safe, false) => "güvenli",
            (Self::Safe, true) => "safe",
            (Self::Caution, false) => "dikkat",
            (Self::Caution, true) => "caution",
            (Self::Dangerous, false) => "tehlikeli",
            (Self::Dangerous, true) => "dangerous",
            (Self::Destructive, false) => "yıkıcı",
            (Self::Destructive, true) => "destructive",
        }
    }

    /// Liste satırında kısa işaret (safe → boş).
    pub fn marker(self) -> &'static str {
        match self {
            Self::Safe => "",
            Self::Caution => " !",
            Self::Dangerous => " !!",
            Self::Destructive => " !!!",
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

/// Katalog / rehber lokalize metin (tr, en, ru, az, uz, kk, tk).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LocalizedString {
    pub tr: Option<String>,
    pub en: Option<String>,
    #[serde(default)]
    pub ru: Option<String>,
    #[serde(default)]
    pub az: Option<String>,
    #[serde(default)]
    pub uz: Option<String>,
    #[serde(default)]
    pub kk: Option<String>,
    #[serde(default)]
    pub tk: Option<String>,
}

impl LocalizedString {
    /// Nova dil koduna göre metin; yoksa `en`, sonra `tr`.
    pub fn pick(&self, lang: &str) -> &str {
        let code = lang.to_ascii_lowercase();
        let specific = match code.as_str() {
            "en" | "en-us" | "en-gb" => self.en.as_deref(),
            "ru" | "ru-ru" => self.ru.as_deref(),
            "az" | "az-az" => self.az.as_deref(),
            "uz" | "uz-uz" => self.uz.as_deref(),
            "kk" | "kk-kz" => self.kk.as_deref(),
            "tk" | "tk-tm" => self.tk.as_deref(),
            "tr" | "tr-tr" => self.tr.as_deref(),
            _ => None,
        };
        specific
            .or(self.en.as_deref())
            .or(self.tr.as_deref())
            .or(self.ru.as_deref())
            .or(self.az.as_deref())
            .or(self.uz.as_deref())
            .or(self.kk.as_deref())
            .or(self.tk.as_deref())
            .unwrap_or("")
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
    /// Küratör notu (düz metin; çoğu kayıt agent-draft açıklaması).
    #[serde(default)]
    pub notes: Option<String>,
}

/// Komut güvenlik / yetki notları (Windows UAC vb.) — bankadan salt okuma.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CommandSecurity {
    #[serde(default)]
    pub notes: LocalizedString,
    #[serde(default)]
    pub requires_admin: bool,
    #[serde(default)]
    pub uac_prompt: bool,
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
    /// v2 additive — yoksa yok sayılır.
    #[serde(default)]
    pub shell: Option<String>,
    /// v2 additive (`user` / `elevated` / …) — yoksa yok sayılır.
    #[serde(default)]
    pub privilege: Option<String>,
    /// v2 additive — örnek platform kısıtı.
    #[serde(default)]
    pub platforms: Vec<String>,
    /// Yaygın hatalar / uyarılar (öğretici).
    #[serde(default)]
    pub warnings: Vec<LocalizedString>,
}

/// Schema v2 paket eşlemesi (`metadata/package-managers.yaml` id + paket adı).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PackageRef {
    pub manager: String,
    pub package: String,
    #[serde(default)]
    pub distributions: Vec<String>,
    #[serde(default)]
    pub notes: LocalizedString,
}

impl PackageRef {
    /// UI etiketi — pacman için mevcut `arch/pacman` işaretine uyum.
    pub fn label(&self) -> String {
        match self.manager.as_str() {
            "pacman" => "arch/pacman".into(),
            other => other.to_string(),
        }
    }
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

    /// Detay paneli için (etiket, paket) çiftleri — Arch önce.
    pub fn labeled_pairs(&self, arch_first: bool) -> Vec<(&'static str, &str)> {
        let order: &[(&'static str, &Option<String>)] = if arch_first {
            &[
                ("arch/pacman", &self.arch),
                ("debian", &self.debian),
                ("ubuntu", &self.ubuntu),
                ("fedora", &self.fedora),
                ("rhel", &self.rhel),
            ]
        } else {
            &[
                ("debian", &self.debian),
                ("ubuntu", &self.ubuntu),
                ("fedora", &self.fedora),
                ("arch/pacman", &self.arch),
                ("rhel", &self.rhel),
            ]
        };
        order
            .iter()
            .filter_map(|(label, v)| {
                v.as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|pkg| (*label, pkg))
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.arch.is_none()
            && self.debian.is_none()
            && self.ubuntu.is_none()
            && self.fedora.is_none()
            && self.rhel.is_none()
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct NovaHints {
    #[serde(default)]
    pub favorite_eligible: Option<bool>,
    #[serde(default)]
    pub ui_priority: Option<u32>,
    #[serde(default)]
    pub concepts: Vec<String>,
}

/// Tek kürate komut kaydı.
#[derive(Debug, Clone, Deserialize)]
pub struct CommandEntry {
    pub id: String,
    pub name: String,
    /// v2: `executable` / `package_manager` / `shell_builtin` (yoksa yok sayılır).
    #[serde(default)]
    pub kind: Option<String>,
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
    /// v2 additive OS registry id'leri (`linux` / `macos` / `windows` / …).
    #[serde(default)]
    pub operating_systems: Vec<String>,
    /// Desteklenen kabuklar (`bash` / `zsh` / `powershell` / …).
    #[serde(default)]
    pub shells: Vec<String>,
    /// Alternatif adlar (PowerShell alias vb.).
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub packages: Packages,
    /// v2 tercih edilen paket eşlemesi; doluysa legacy `packages` yerine kullanılır.
    #[serde(default)]
    pub package_refs: Vec<PackageRef>,
    #[serde(default)]
    pub install_notes: LocalizedString,
    #[serde(default)]
    pub flags: Vec<CommandFlag>,
    #[serde(default)]
    pub examples: Vec<CommandExample>,
    #[serde(default)]
    pub related: Vec<String>,
    /// Man / docs çapraz referansları.
    #[serde(default)]
    pub see_also: Vec<String>,
    #[serde(default)]
    pub security: CommandSecurity,
    #[serde(default)]
    pub intent: IntentPhrases,
    #[serde(default)]
    pub verification: VerificationInfo,
    #[serde(default)]
    pub nova: NovaHints,
}

impl CommandEntry {
    /// Kurulum ipucu satırları: `package_refs` varsa onu, yoksa legacy `packages`.
    ///
    /// Arch-first: `pacman` / `arch` önce.
    pub fn package_labeled_pairs(&self, arch_first: bool) -> Vec<(String, String)> {
        if !self.package_refs.is_empty() {
            let mut refs = self.package_refs.clone();
            refs.sort_by_key(|r| {
                let is_pacman = r.manager == "pacman";
                if arch_first {
                    if is_pacman {
                        0u8
                    } else {
                        1
                    }
                } else if is_pacman {
                    1
                } else {
                    0
                }
            });
            refs.into_iter()
                .filter(|r| !r.package.is_empty())
                .map(|r| (r.label(), r.package))
                .collect()
        } else {
            self.packages
                .labeled_pairs(arch_first)
                .into_iter()
                .map(|(label, pkg)| (label.to_string(), pkg.to_string()))
                .collect()
        }
    }

    pub fn has_package_hints(&self) -> bool {
        !self.package_refs.is_empty() || !self.packages.is_empty()
    }
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

/// Görev uygulaması örneği (task → komut köprüsü).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TaskExample {
    pub command: String,
    #[serde(default)]
    pub explanation: LocalizedString,
    #[serde(default)]
    pub risk: RiskLevel,
    #[serde(default)]
    pub requires_elevated: bool,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub privilege: Option<String>,
}

/// Görev içinde bir uygulama yolu (ör. rsync / restic).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TaskImplementation {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub examples: Vec<TaskExample>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub shells: Vec<String>,
}

/// Katalog görevi (`kind: task`) — kullanıcı niyeti.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskEntry {
    pub id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub name: LocalizedString,
    #[serde(default)]
    pub summary: LocalizedString,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub intent: IntentPhrases,
    #[serde(default)]
    pub implementations: Vec<TaskImplementation>,
    #[serde(default)]
    pub verification: VerificationInfo,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub related_commands: Vec<String>,
    #[serde(default)]
    pub related_tasks: Vec<String>,
}

/// Kullanım bağlamı profili (`kind: profile`).
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileEntry {
    pub id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub name: LocalizedString,
    #[serde(default)]
    pub summary: LocalizedString,
    #[serde(default)]
    pub description: LocalizedString,
    #[serde(default)]
    pub verification: VerificationInfo,
    #[serde(default)]
    pub operating_systems: Vec<String>,
    #[serde(default)]
    pub distributions: Vec<String>,
    #[serde(default)]
    pub related_commands: Vec<String>,
    #[serde(default)]
    pub related_tasks: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
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
    pub task_count: Option<usize>,
    #[serde(default)]
    pub profile_count: Option<usize>,
    #[serde(default)]
    pub commands: Vec<CommandEntry>,
    #[serde(default)]
    pub concepts: Vec<ConceptEntry>,
    #[serde(default)]
    pub tasks: Vec<TaskEntry>,
    #[serde(default)]
    pub profiles: Vec<ProfileEntry>,
    /// İsteğe bağlı rehberler (Sunucu güvenlik kontrolü vb.).
    ///
    /// Commands içeriği gelene kadar boş olabilir; Nova o zaman banka
    /// görevlerinden türetilmiş adımları kullanır.
    #[serde(default)]
    pub guides: Vec<GuideEntry>,
}

/// Katalog rehberi (`kind: checklist` vb.).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct GuideEntry {
    pub id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub name: LocalizedString,
    #[serde(default)]
    pub summary: LocalizedString,
    #[serde(default)]
    pub steps: Vec<GuideStep>,
    #[serde(default)]
    pub verification: VerificationInfo,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Rehber adımı — komut çalıştırmaz; salt öğretici.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct GuideStep {
    pub id: String,
    #[serde(default)]
    pub title: LocalizedString,
    #[serde(default)]
    pub summary: LocalizedString,
    #[serde(default)]
    pub diagnosis: LocalizedString,
    #[serde(default)]
    pub risk: RiskLevel,
    #[serde(default)]
    pub requires_elevated: bool,
    #[serde(default)]
    pub related_commands: Vec<String>,
    #[serde(default)]
    pub related_tasks: Vec<String>,
    #[serde(default)]
    pub example: Option<CommandExample>,
    #[serde(default)]
    pub rollback: LocalizedString,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub operating_systems: Vec<String>,
}

impl CommandBank {
    /// `kind` / `schema_version` doğrular.
    pub fn validate_contract(&self) -> Result<(), String> {
        if self.kind != BANK_KIND {
            return Err(format!("beklenen kind={BANK_KIND}, gelen={}", self.kind));
        }
        validate_bank_schema_version(self.schema_version)?;
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

    pub fn find_task(&self, id: &str) -> Option<&TaskEntry> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn find_profile(&self, id: &str) -> Option<&ProfileEntry> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// Görev kategorileri (sıralı, tekil).
    pub fn task_categories(&self) -> Vec<&str> {
        let mut cats: Vec<&str> = self.tasks.iter().map(|t| t.category.as_str()).collect();
        cats.sort_unstable();
        cats.dedup();
        cats
    }

    pub fn tasks_in_category<'a>(&'a self, category: &str) -> Vec<&'a TaskEntry> {
        self.tasks
            .iter()
            .filter(|t| t.category == category)
            .collect()
    }
}
