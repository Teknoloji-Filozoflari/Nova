//! Komut Rehberi süzgeçleri — OS / distro / shell / profil / seviye / risk.
//!
//! PATH keşfine uygulanmaz; yalnız banka komut listelerinde.
//! `learning_level` riskten bağımsızdır; belirtilmemiş seviye risk ile doldurulmaz.

use crate::commands::bank::{CommandEntry, CommandExample, LearningLevel, RiskLevel};
use crate::store::host::{detect_host_os, HostOs};

/// Öğrenme seviyesi süzgeci — yalnız banka `learning_level` alanına bakar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LearningLevelFilter {
    #[default]
    All,
    /// Başlangıç ve üstü (beginner + intermediate + advanced).
    AtLeastBeginner,
    Beginner,
    Intermediate,
    Advanced,
    /// Bankada `learning_level` yok / belirtilmemiş.
    Unspecified,
}

impl LearningLevelFilter {
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::AtLeastBeginner,
        Self::Beginner,
        Self::Intermediate,
        Self::Advanced,
        Self::Unspecified,
    ];

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::AtLeastBeginner => "at_least_beginner",
            Self::Beginner => "beginner",
            Self::Intermediate => "intermediate",
            Self::Advanced => "advanced",
            Self::Unspecified => "unspecified",
        }
    }

    /// Süzgeç satırında görünen etiket (TR/EN).
    pub fn label(self, lang: &str) -> &'static str {
        let en = lang.to_ascii_lowercase().starts_with("en");
        match (self, en) {
            (Self::All, false) => "Tümü",
            (Self::All, true) => "All",
            (Self::AtLeastBeginner, false) => "Başlangıç+",
            (Self::AtLeastBeginner, true) => "Beginner+",
            (Self::Beginner, false) => "Başlangıç",
            (Self::Beginner, true) => "Beginner",
            (Self::Intermediate, false) => "Orta",
            (Self::Intermediate, true) => "Intermediate",
            (Self::Advanced, false) => "İleri",
            (Self::Advanced, true) => "Advanced",
            (Self::Unspecified, false) => "Belirtilmedi",
            (Self::Unspecified, true) => "Unspecified",
        }
    }

    fn matches_level(self, level: LearningLevel) -> bool {
        match self {
            Self::All => true,
            Self::AtLeastBeginner => matches!(
                level,
                LearningLevel::Beginner | LearningLevel::Intermediate | LearningLevel::Advanced
            ),
            Self::Beginner => level == LearningLevel::Beginner,
            Self::Intermediate => level == LearningLevel::Intermediate,
            Self::Advanced => level == LearningLevel::Advanced,
            Self::Unspecified => level == LearningLevel::Unspecified,
        }
    }
}

/// Aktif banka süzgeçleri (`None` / `All` = kısıt yok).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandsFilters {
    /// `linux` / `macos` / `windows` / `freebsd` …
    pub os: Option<String>,
    pub distro: Option<String>,
    pub shell: Option<String>,
    /// Profil id — `related_commands` kesişimi.
    pub profile: Option<String>,
    pub learning: LearningLevelFilter,
    pub risk: Option<RiskLevel>,
}

impl CommandsFilters {
    pub fn is_active(&self) -> bool {
        self.os.is_some()
            || self.distro.is_some()
            || self.shell.is_some()
            || self.profile.is_some()
            || self.learning != LearningLevelFilter::All
            || self.risk.is_some()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Komut bu süzgeçlere uyar mı?
    ///
    /// Seviye ve risk bağımsızdır: `safe` + `advanced` advanced süzgecinde görünür;
    /// yalnız `dangerous` advanced sayılmaz.
    pub fn matches(&self, cmd: &CommandEntry) -> bool {
        if let Some(ref risk) = self.risk {
            if cmd.risk != *risk {
                return false;
            }
        }
        if let Some(ref os) = self.os {
            if !command_covers_os(cmd, os) {
                return false;
            }
        }
        if let Some(ref distro) = self.distro {
            if !command_covers_distro(cmd, distro) {
                return false;
            }
        }
        if let Some(ref shell) = self.shell {
            if !command_covers_shell(cmd, shell) {
                return false;
            }
        }
        if let Some(ref profile) = self.profile {
            // Profil süzgeci App tarafında related set ile de uygulanır;
            // burada id etiketi tutarlılığı için no-op bırakılabilir.
            let _ = profile;
        }
        self.learning.matches_level(cmd.learning_level)
    }

    /// Durum satırı özeti (kısa).
    pub fn summary_chips(&self) -> Vec<String> {
        let mut chips = Vec::new();
        if let Some(ref os) = self.os {
            chips.push(format!("os:{os}"));
        }
        if let Some(ref d) = self.distro {
            chips.push(format!("distro:{d}"));
        }
        if let Some(ref s) = self.shell {
            chips.push(format!("shell:{s}"));
        }
        if let Some(ref p) = self.profile {
            chips.push(format!("profile:{p}"));
        }
        if self.learning != LearningLevelFilter::All {
            chips.push(format!("level:{}", self.learning.as_str()));
        }
        if let Some(r) = self.risk {
            chips.push(format!("risk:{}", r.as_str()));
        }
        chips
    }
}

fn command_covers_os(cmd: &CommandEntry, os: &str) -> bool {
    let os = os.to_ascii_lowercase();
    let hay: Vec<String> = cmd
        .operating_systems
        .iter()
        .chain(cmd.platforms.iter())
        .map(|s| s.to_ascii_lowercase())
        .collect();
    if hay.is_empty() {
        // Boş platform = genel / çapraz — süzgeçte göster (sessizce “native” deme).
        return true;
    }
    hay.iter().any(|h| h == &os || h.contains(&os))
}

fn command_covers_distro(cmd: &CommandEntry, distro: &str) -> bool {
    let d = distro.to_ascii_lowercase();
    let has_refs = cmd.package_refs.iter().any(|r| !r.distributions.is_empty());
    let has_legacy = !cmd.packages.is_empty();
    if !has_refs && !has_legacy {
        // Distro meta yok = genel komut; sessizce Arch sayma, eleme de.
        return true;
    }
    // Exact distribution id match only — no Arch-first collapse / substring.
    if cmd
        .package_refs
        .iter()
        .any(|r| r.distributions.iter().any(|x| x.to_ascii_lowercase() == d))
    {
        return true;
    }
    // Legacy `packages` keys: exact field match. Never fall back Arch↔Debian.
    match d.as_str() {
        "arch" => cmd.packages.arch.as_ref().is_some_and(|s| !s.is_empty()),
        "debian" => cmd.packages.debian.as_ref().is_some_and(|s| !s.is_empty()),
        "ubuntu" => cmd.packages.ubuntu.as_ref().is_some_and(|s| !s.is_empty()),
        "fedora" => cmd.packages.fedora.as_ref().is_some_and(|s| !s.is_empty()),
        "rhel" => cmd.packages.rhel.as_ref().is_some_and(|s| !s.is_empty()),
        _ => {
            // Yalnız package_refs ile bilinen id'ler; legacy’de yoksa eşleşme yok.
            false
        }
    }
}

/// Banka komut örneklerini aktif OS/shell süzgecine göre seç (ikame yok).
///
/// Distro süzgeci örneklerde ayrı alan olmadığı için komut düzeyinde kalır;
/// örnek `platforms` / `shell` boşsa genel kabul edilir.
pub fn select_command_examples<'a>(
    examples: &'a [CommandExample],
    filters: &CommandsFilters,
) -> Vec<&'a CommandExample> {
    examples
        .iter()
        .filter(|ex| command_example_matches_filters(ex, filters))
        .collect()
}

fn command_example_matches_filters(ex: &CommandExample, filters: &CommandsFilters) -> bool {
    if let Some(ref os) = filters.os {
        let os = os.to_ascii_lowercase();
        if !ex.platforms.is_empty() {
            let hit = ex.platforms.iter().any(|p| {
                let p = p.to_ascii_lowercase();
                p == os
                    || p.contains(&os)
                    || os.contains(&p)
                    || p == "any"
                    || p == "all"
                    || p == "cross"
                    || p == "portable"
            });
            if !hit {
                return false;
            }
        }
    }
    if let Some(ref shell) = filters.shell {
        let s = shell.to_ascii_lowercase();
        if let Some(ref ex_shell) = ex.shell {
            if !ex_shell.is_empty() {
                let h = ex_shell.to_ascii_lowercase();
                if !(h == s || h.contains(&s) || s.contains(&h)) {
                    return false;
                }
            }
        }
    }
    true
}

/// Aktif distro süzgecine uyan paket satırları — eşleşme yoksa boş (ikame yok).
pub fn package_lines_for_filter(
    cmd: &CommandEntry,
    filters: &CommandsFilters,
    arch_host: bool,
) -> Vec<(String, String)> {
    let all = cmd.package_labeled_pairs(arch_host && filters.distro.is_none());
    let Some(ref distro) = filters.distro else {
        return all;
    };
    let d = distro.to_ascii_lowercase();
    let mut out = Vec::new();
    if !cmd.package_refs.is_empty() {
        for r in &cmd.package_refs {
            if r.package.is_empty() {
                continue;
            }
            if r.distributions.iter().any(|x| x.to_ascii_lowercase() == d) {
                out.push((r.label(), r.package.clone()));
            }
        }
        return out;
    }
    // Legacy packages — yalnızca seçilen anahtar; Arch’a düşme.
    let legacy = match d.as_str() {
        "arch" => cmd
            .packages
            .arch
            .as_deref()
            .map(|p| ("arch/pacman".into(), p.to_string())),
        "debian" => cmd
            .packages
            .debian
            .as_deref()
            .map(|p| ("debian".into(), p.to_string())),
        "ubuntu" => cmd
            .packages
            .ubuntu
            .as_deref()
            .map(|p| ("ubuntu".into(), p.to_string())),
        "fedora" => cmd
            .packages
            .fedora
            .as_deref()
            .map(|p| ("fedora".into(), p.to_string())),
        "rhel" => cmd
            .packages
            .rhel
            .as_deref()
            .map(|p| ("rhel".into(), p.to_string())),
        _ => None,
    };
    if let Some(pair) = legacy {
        out.push(pair);
    }
    out
}

fn command_covers_shell(cmd: &CommandEntry, shell: &str) -> bool {
    let s = shell.to_ascii_lowercase();
    if cmd.shells.is_empty()
        && cmd
            .examples
            .iter()
            .all(|ex| ex.shell.as_ref().map(|x| x.is_empty()).unwrap_or(true))
    {
        // Kabuk belirtilmemiş = genel kabuk-agnostik.
        return true;
    }
    cmd.shells
        .iter()
        .any(|x| x.to_ascii_lowercase() == s || x.to_ascii_lowercase().contains(&s))
        || cmd.examples.iter().any(|ex| {
            ex.shell
                .as_ref()
                .is_some_and(|x| x.to_ascii_lowercase() == s || x.to_ascii_lowercase().contains(&s))
        })
}

/// Host OS → banka `operating_systems` id.
pub fn host_os_bank_id() -> &'static str {
    match detect_host_os() {
        HostOs::Arch | HostOs::Linux => "linux",
        HostOs::MacOs => "macos",
        HostOs::Other => {
            if cfg!(target_os = "windows") {
                "windows"
            } else if cfg!(target_os = "freebsd") {
                "freebsd"
            } else {
                "other"
            }
        }
    }
}

/// Güvenilir host OS/distro süzgeç varsayılanı (düzenlenebilir; sessiz Arch çökertmesi yok).
///
/// - Arch → OS `linux` + distro `arch` (Arch ≠ tüm Linux; birincil hedef)
/// - diğer Linux → yalnız OS `linux` (distro meta seyrek; yanlış eleme yok)
/// - macOS / Windows / FreeBSD → yalnız OS
/// - bilinmeyen → `(None, None)` — varsayılan dayatma yok
pub fn host_filter_defaults(bank_distros: &[String]) -> (Option<String>, Option<String>) {
    let _ = bank_distros;
    match detect_host_os() {
        HostOs::Arch => (Some("linux".into()), Some("arch".into())),
        HostOs::MacOs => (Some("macos".into()), None),
        HostOs::Linux => (Some("linux".into()), None),
        HostOs::Other => {
            if cfg!(target_os = "windows") {
                (Some("windows".into()), None)
            } else if cfg!(target_os = "freebsd") {
                (Some("freebsd".into()), None)
            } else {
                (None, None)
            }
        }
    }
}

/// Komutun host OS ile ilişkisi — asla sessizce “yerel” ima etme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsFit {
    /// Host OS listede.
    Native,
    /// Platform boş / genel.
    General,
    /// Başka OS için; eşdeğer bu host’ta yok sayılmamalı.
    OtherPlatform,
    /// Bilinçli olarak bu OS’ta karşılık yok (liste dolu, host yok).
    NoEquivalent,
}

pub fn command_os_fit(cmd: &CommandEntry) -> OsFit {
    let host = host_os_bank_id();
    let oss: Vec<String> = cmd
        .operating_systems
        .iter()
        .chain(cmd.platforms.iter())
        .map(|s| s.to_ascii_lowercase())
        .collect();
    if oss.is_empty() {
        return OsFit::General;
    }
    if oss.iter().any(|o| o == host || o.contains(host)) {
        return OsFit::Native;
    }
    // "cross" / "any" işaretleri
    if oss
        .iter()
        .any(|o| o == "any" || o == "all" || o == "cross" || o == "portable")
    {
        return OsFit::General;
    }
    if oss.len() == 1 {
        OsFit::OtherPlatform
    } else {
        OsFit::NoEquivalent
    }
}

/// Bankadan bilinen OS id’leri (süzgeç döngüsü).
pub fn bank_os_values(commands: &[CommandEntry]) -> Vec<String> {
    let mut v: Vec<String> = commands
        .iter()
        .flat_map(|c| c.operating_systems.iter().chain(c.platforms.iter()))
        .map(|s| s.to_ascii_lowercase())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

pub fn bank_shell_values(commands: &[CommandEntry]) -> Vec<String> {
    let mut v: Vec<String> = commands
        .iter()
        .flat_map(|c| {
            c.shells.iter().cloned().chain(
                c.examples
                    .iter()
                    .filter_map(|ex| ex.shell.clone())
                    .filter(|s| !s.is_empty()),
            )
        })
        .map(|s| s.to_ascii_lowercase())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

pub fn bank_distro_values(commands: &[CommandEntry]) -> Vec<String> {
    let mut v: Vec<String> = commands
        .iter()
        .flat_map(|c| {
            c.package_refs
                .iter()
                .flat_map(|r| r.distributions.iter().cloned())
        })
        .map(|s| s.to_ascii_lowercase())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// `None` ↔ değerler arasında döngü.
pub fn cycle_optional(current: &Option<String>, values: &[String]) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    match current {
        None => Some(values[0].clone()),
        Some(cur) => match values.iter().position(|v| v == cur) {
            Some(i) if i + 1 < values.len() => Some(values[i + 1].clone()),
            _ => None,
        },
    }
}

pub fn cycle_risk(current: Option<RiskLevel>) -> Option<RiskLevel> {
    match current {
        None => Some(RiskLevel::Safe),
        Some(RiskLevel::Safe) => Some(RiskLevel::Caution),
        Some(RiskLevel::Caution) => Some(RiskLevel::Dangerous),
        Some(RiskLevel::Dangerous) => Some(RiskLevel::Destructive),
        Some(RiskLevel::Destructive) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::bank::{sort_examples_by_difficulty, CommandExample};

    fn cmd(risk: RiskLevel, learning: LearningLevel) -> CommandEntry {
        serde_json::from_value(serde_json::json!({
            "id": "demo",
            "name": "demo",
            "risk": risk.as_str(),
            "learning_level": learning.as_str(),
        }))
        .expect("command fixture")
    }

    #[test]
    fn learning_filter_ignores_risk_and_excludes_unspecified() {
        let advanced_safe = cmd(RiskLevel::Safe, LearningLevel::Advanced);
        let dangerous_unset = cmd(RiskLevel::Dangerous, LearningLevel::Unspecified);
        let dangerous_advanced = cmd(RiskLevel::Dangerous, LearningLevel::Advanced);
        let beginner_caution = cmd(RiskLevel::Caution, LearningLevel::Beginner);

        let mut f = CommandsFilters {
            learning: LearningLevelFilter::Advanced,
            ..CommandsFilters::default()
        };
        assert!(
            f.matches(&advanced_safe),
            "safe-but-advanced advanced süzgecinde olmalı"
        );
        assert!(
            !f.matches(&dangerous_unset),
            "dangerous + belirtilmemiş advanced sayılmamalı"
        );
        assert!(f.matches(&dangerous_advanced));
        assert!(!f.matches(&beginner_caution));

        f.learning = LearningLevelFilter::Beginner;
        assert!(f.matches(&beginner_caution));
        assert!(!f.matches(&advanced_safe));
        assert!(
            !f.matches(&dangerous_unset),
            "belirtilmemiş beginner uydurulmamalı"
        );

        f.learning = LearningLevelFilter::Unspecified;
        assert!(f.matches(&dangerous_unset));
        assert!(!f.matches(&advanced_safe));
    }

    #[test]
    fn risk_filter_independent_of_learning_level() {
        let advanced_safe = cmd(RiskLevel::Safe, LearningLevel::Advanced);
        let advanced_dangerous = cmd(RiskLevel::Dangerous, LearningLevel::Advanced);
        let mut f = CommandsFilters {
            risk: Some(RiskLevel::Safe),
            learning: LearningLevelFilter::Advanced,
            ..CommandsFilters::default()
        };
        assert!(f.matches(&advanced_safe));
        assert!(!f.matches(&advanced_dangerous));

        f.risk = Some(RiskLevel::Dangerous);
        assert!(!f.matches(&advanced_safe));
        assert!(f.matches(&advanced_dangerous));

        f.learning = LearningLevelFilter::All;
        f.risk = Some(RiskLevel::Dangerous);
        let dangerous_unset = cmd(RiskLevel::Dangerous, LearningLevel::Unspecified);
        assert!(f.matches(&dangerous_unset));
    }

    #[test]
    fn example_order_uses_difficulty_never_risk() {
        let mut examples = vec![
            CommandExample {
                command: "risky-easy".into(),
                risk: RiskLevel::Dangerous,
                difficulty: Some(LearningLevel::Beginner),
                ..Default::default()
            },
            CommandExample {
                command: "safe-hard".into(),
                risk: RiskLevel::Safe,
                difficulty: Some(LearningLevel::Advanced),
                ..Default::default()
            },
            CommandExample {
                command: "mid".into(),
                risk: RiskLevel::Caution,
                difficulty: Some(LearningLevel::Intermediate),
                ..Default::default()
            },
        ];
        sort_examples_by_difficulty(&mut examples);
        assert_eq!(examples[0].command, "risky-easy");
        assert_eq!(examples[1].command, "mid");
        assert_eq!(examples[2].command, "safe-hard");

        // Difficulty yoksa risk ile sıralama yok — katalog sırası korunur.
        let mut no_diff = vec![
            CommandExample {
                command: "b".into(),
                risk: RiskLevel::Destructive,
                ..Default::default()
            },
            CommandExample {
                command: "a".into(),
                risk: RiskLevel::Safe,
                ..Default::default()
            },
        ];
        sort_examples_by_difficulty(&mut no_diff);
        assert_eq!(no_diff[0].command, "b");
        assert_eq!(no_diff[1].command, "a");
    }

    #[test]
    fn learning_filter_cycle_includes_unspecified() {
        let mut v = LearningLevelFilter::All;
        let mut seen = Vec::new();
        for _ in 0..LearningLevelFilter::ALL.len() {
            v = v.next();
            seen.push(v);
        }
        assert!(seen.contains(&LearningLevelFilter::Unspecified));
        assert_eq!(v, LearningLevelFilter::All);
    }

    fn cmd_json(v: serde_json::Value) -> CommandEntry {
        serde_json::from_value(v).expect("command fixture")
    }

    #[test]
    fn os_filter_selects_matching_platform_ids() {
        let linux = cmd_json(serde_json::json!({
            "id": "ufw-demo",
            "name": "ufw",
            "platforms": ["linux"],
            "risk": "safe",
            "learning_level": "beginner",
        }));
        let windows = cmd_json(serde_json::json!({
            "id": "netsh-demo",
            "name": "netsh",
            "platforms": ["windows"],
            "risk": "safe",
            "learning_level": "beginner",
        }));
        let f = CommandsFilters {
            os: Some("windows".into()),
            ..Default::default()
        };
        assert!(
            f.matches(&windows),
            "windows süzgeci netsh-demo id'sini tutmalı"
        );
        assert!(
            !f.matches(&linux),
            "windows süzgeci ufw-demo (linux) elemmeli"
        );
    }

    #[test]
    fn distro_filter_selects_package_ref_distributions() {
        let arch = cmd_json(serde_json::json!({
            "id": "pacman-demo",
            "name": "pacman",
            "platforms": ["linux"],
            "risk": "safe",
            "learning_level": "beginner",
            "package_refs": [{"manager": "pacman", "package": "pacman", "distributions": ["arch"]}],
        }));
        let debian = cmd_json(serde_json::json!({
            "id": "apt-demo",
            "name": "apt",
            "platforms": ["linux"],
            "risk": "safe",
            "learning_level": "beginner",
            "package_refs": [{"manager": "apt", "package": "apt", "distributions": ["debian"]}],
        }));
        let f = CommandsFilters {
            distro: Some("arch".into()),
            ..Default::default()
        };
        assert!(f.matches(&arch), "distro=arch pacman-demo id'sini tutmalı");
        assert!(!f.matches(&debian), "distro=arch apt-demo elemmeli");
    }

    #[test]
    fn shell_filter_selects_shell_metadata_and_examples() {
        let bash = cmd_json(serde_json::json!({
            "id": "bash-demo",
            "name": "bash-demo",
            "shells": ["bash"],
            "risk": "safe",
            "learning_level": "beginner",
        }));
        let pwsh = cmd_json(serde_json::json!({
            "id": "pwsh-demo",
            "name": "pwsh-demo",
            "shells": ["powershell"],
            "examples": [{"command": "Get-Help", "shell": "powershell", "risk": "safe"}],
            "risk": "safe",
            "learning_level": "beginner",
        }));
        let f = CommandsFilters {
            shell: Some("powershell".into()),
            ..Default::default()
        };
        assert!(
            f.matches(&pwsh),
            "shell=powershell pwsh-demo id'sini tutmalı"
        );
        assert!(!f.matches(&bash), "shell=powershell bash-demo elemmeli");
    }

    #[test]
    fn risk_filter_selects_exact_risk_not_learning() {
        let safe_adv = cmd(RiskLevel::Safe, LearningLevel::Advanced);
        let caution_beg = cmd(RiskLevel::Caution, LearningLevel::Beginner);
        let f = CommandsFilters {
            risk: Some(RiskLevel::Safe),
            ..Default::default()
        };
        assert!(f.matches(&safe_adv), "risk=safe advanced komutu tutmalı");
        assert!(
            !f.matches(&caution_beg),
            "risk=safe beginner/caution elemmeli — seviye risk yerine geçmez"
        );
    }

    #[test]
    fn distro_filter_does_not_collapse_to_arch_or_substitute() {
        let debian_only = cmd_json(serde_json::json!({
            "id": "apt-only",
            "name": "apt-only",
            "platforms": ["linux"],
            "risk": "safe",
            "packages": {"debian": "coreutils"},
        }));
        let both = cmd_json(serde_json::json!({
            "id": "multi-pm",
            "name": "multi-pm",
            "platforms": ["linux"],
            "risk": "safe",
            "packages": {"arch": "coreutils", "debian": "coreutils"},
        }));
        let arch_f = CommandsFilters {
            distro: Some("arch".into()),
            ..Default::default()
        };
        assert!(
            !arch_f.matches(&debian_only),
            "debian-only komut distro=arch ile sessizce Arch sayılmamalı"
        );
        assert!(arch_f.matches(&both));

        let deb_f = CommandsFilters {
            distro: Some("debian".into()),
            ..Default::default()
        };
        assert!(deb_f.matches(&debian_only));
        let arch_pkgs = package_lines_for_filter(&both, &arch_f, true);
        assert_eq!(arch_pkgs.len(), 1);
        assert!(arch_pkgs[0].0.starts_with("arch"));
        let empty = package_lines_for_filter(&debian_only, &arch_f, true);
        assert!(
            empty.is_empty(),
            "distro=arch iken debian paketi ikame edilmemeli: {empty:?}"
        );
    }

    #[test]
    fn command_examples_filter_by_os_without_substitute() {
        let linux = CommandExample {
            command: "ss -tuln".into(),
            platforms: vec!["linux".into()],
            ..Default::default()
        };
        let windows = CommandExample {
            command: "Get-NetTCPConnection".into(),
            platforms: vec!["windows".into()],
            shell: Some("powershell".into()),
            ..Default::default()
        };
        let general = CommandExample {
            command: "echo hi".into(),
            ..Default::default()
        };
        let examples = vec![linux, windows, general];
        let win = CommandsFilters {
            os: Some("windows".into()),
            ..Default::default()
        };
        let matched = select_command_examples(&examples, &win);
        let cmds: Vec<_> = matched.iter().map(|e| e.command.as_str()).collect();
        assert!(cmds.contains(&"Get-NetTCPConnection"));
        assert!(cmds.contains(&"echo hi"), "platform boş = genel kalır");
        assert!(
            !cmds.contains(&"ss -tuln"),
            "windows süzgeci linux örneğini ikame etmemeli"
        );
        let none = CommandsFilters {
            os: Some("plan9".into()),
            ..Default::default()
        };
        let only_general = select_command_examples(&examples, &none);
        assert_eq!(only_general.len(), 1);
        assert_eq!(only_general[0].command, "echo hi");
    }

    #[test]
    fn host_filter_defaults_arch_keeps_os_and_distro_separate() {
        // Sözleşme: Arch → linux + arch (Arch ≠ tüm Linux sessiz çökertmesi yok).
        let bank = vec!["arch".into(), "debian".into()];
        let (os, distro) = host_filter_defaults(&bank);
        match crate::store::host::detect_host_os() {
            crate::store::host::HostOs::Arch => {
                assert_eq!(os.as_deref(), Some("linux"));
                assert_eq!(distro.as_deref(), Some("arch"));
            }
            crate::store::host::HostOs::Linux => {
                assert_eq!(os.as_deref(), Some("linux"));
                assert!(
                    distro.is_none(),
                    "non-Arch Linux distro dayatmaz: {distro:?}"
                );
            }
            crate::store::host::HostOs::MacOs => {
                assert_eq!(os.as_deref(), Some("macos"));
                assert!(distro.is_none());
            }
            crate::store::host::HostOs::Other => {
                // windows/freebsd/unknown — OS opsiyonel; distro yok.
                assert!(distro.is_none());
            }
        }
    }
}
