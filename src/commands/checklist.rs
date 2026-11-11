//! Sunucu güvenlik kontrolü — sıralı checklist UI adımları.
//!
//! Rehber **asla** komut çalıştırmaz / tarama yapmaz. Adımlar banka içeriğinden
//! (katalog `guides` veya güvenlik görevleri + salt-okunur örnekler) gelir.
//! Durum `unchecked` / `ok` / `review` / `na` yerel `state.toml`'da tutulur.
//!
//! Örnekler platforma göre ayrı tutulur; OS/distro/shell süzgeci başka OS
//! komutunu sessizce yerine koymaz.

use crate::commands::bank::{
    CommandBank, CommandExample, GuideEntry, GuideStep, LocalizedString, RiskLevel, TaskExample,
    TaskImplementation,
};
use crate::commands::filters::CommandsFilters;
use serde::{Deserialize, Serialize};

/// Checklist adım durumu (kalıcı).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecklistStepState {
    #[default]
    Unchecked,
    Ok,
    Review,
    Na,
}

impl ChecklistStepState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unchecked => "unchecked",
            Self::Ok => "ok",
            Self::Review => "review",
            Self::Na => "na",
        }
    }

    pub fn marker(self) -> &'static str {
        match self {
            Self::Unchecked => "[ ]",
            Self::Ok => "[✓]",
            Self::Review => "[?]",
            Self::Na => "[–]",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Unchecked => Self::Ok,
            Self::Ok => Self::Review,
            Self::Review => Self::Na,
            Self::Na => Self::Unchecked,
        }
    }

    pub fn label(self, lang: &str) -> &'static str {
        let en = lang.to_ascii_lowercase().starts_with("en");
        match (self, en) {
            (Self::Unchecked, false) => "işaretlenmedi",
            (Self::Unchecked, true) => "unchecked",
            (Self::Ok, false) => "tamam",
            (Self::Ok, true) => "ok",
            (Self::Review, false) => "gözden geçir",
            (Self::Review, true) => "review",
            (Self::Na, false) => "uygulanmaz",
            (Self::Na, true) => "n/a",
        }
    }
}

/// Sabit rehber kimliği (state anahtarı öneki).
pub const SERVER_SECURITY_GUIDE_ID: &str = "server-security-check";

/// Varsayılan checklist hedefi (yerel / tek host).
///
/// İlerleme anahtarı: `{SERVER_SECURITY_GUIDE_ID}::{target}::{step_id}`.
/// Farklı hedefler birbirine sızmaz.
pub const CHECKLIST_DEFAULT_TARGET: &str = "local";

/// Checklist adımına bağlı tek örnek — TaskImplementation alanları korunur.
#[derive(Debug, Clone)]
pub struct ChecklistExampleView {
    pub implementation_id: String,
    pub platforms: Vec<String>,
    pub shells: Vec<String>,
    pub distributions: Vec<String>,
    pub example: CommandExample,
}

impl ChecklistExampleView {
    fn from_task_impl(impl_: &TaskImplementation, ex: &TaskExample) -> Self {
        let mut example = CommandExample {
            title: LocalizedString::default(),
            command: ex.command.clone(),
            explanation: ex.explanation.clone(),
            risk: ex.risk,
            requires_elevated: ex.requires_elevated,
            shell: ex.shell.clone(),
            privilege: ex.privilege.clone(),
            platforms: impl_.platforms.clone(),
            difficulty: None,
            warnings: Vec::new(),
        };
        if example.shell.is_none() {
            example.shell = impl_.shells.first().cloned();
        }
        let distributions = infer_distributions(impl_);
        let mut shells = impl_.shells.clone();
        if let Some(ref sh) = ex.shell {
            if !shells.iter().any(|s| s.eq_ignore_ascii_case(sh)) {
                shells.push(sh.clone());
            }
        }
        Self {
            implementation_id: impl_.id.clone(),
            platforms: impl_.platforms.clone(),
            shells,
            distributions,
            example,
        }
    }

    fn from_command_example(ex: CommandExample, platforms: Vec<String>) -> Self {
        let shell = ex.shell.clone();
        let mut shells = Vec::new();
        if let Some(ref sh) = shell {
            shells.push(sh.clone());
        }
        let platforms = if ex.platforms.is_empty() {
            platforms
        } else {
            ex.platforms.clone()
        };
        Self {
            implementation_id: String::new(),
            platforms: platforms.clone(),
            shells,
            distributions: Vec::new(),
            example: CommandExample { platforms, ..ex },
        }
    }
}

/// UI’da gösterilen adım (bankadan çözülmüş).
#[derive(Debug, Clone)]
pub struct ChecklistStepView {
    pub id: String,
    pub title: LocalizedString,
    pub summary: LocalizedString,
    pub diagnosis: LocalizedString,
    pub risk: RiskLevel,
    pub requires_elevated: bool,
    pub related_commands: Vec<String>,
    /// Tüm platform örnekleri (çökertilmez). Süzgeç ile [`select_checklist_examples`].
    pub examples: Vec<ChecklistExampleView>,
    pub rollback: LocalizedString,
    pub platforms: Vec<String>,
    pub operating_systems: Vec<String>,
}

impl ChecklistStepView {
    /// Hedef + adım için kalıcı durum anahtarı.
    pub fn state_key_for(&self, target: &str) -> String {
        checklist_progress_key(target, &self.id)
    }

    /// Varsayılan hedef (`local`) için durum anahtarı.
    pub fn state_key(&self) -> String {
        self.state_key_for(CHECKLIST_DEFAULT_TARGET)
    }

    /// Geriye dönük: süzgeçsiz ilk örnek (tercihen Arch / düşük risk).
    pub fn primary_example(&self) -> Option<&CommandExample> {
        select_checklist_examples(&self.examples, &CommandsFilters::default())
            .into_iter()
            .next()
            .map(|e| &e.example)
    }
}

/// `{guide}::{target}::{step}` — hedef / adım ayrımı.
pub fn checklist_progress_key(target: &str, step_id: &str) -> String {
    let target = normalize_checklist_target(target);
    format!("{SERVER_SECURITY_GUIDE_ID}::{target}::{step_id}")
}

fn normalize_checklist_target(target: &str) -> String {
    let t = target.trim();
    if t.is_empty() {
        CHECKLIST_DEFAULT_TARGET.to_string()
    } else {
        t.to_ascii_lowercase()
    }
}

/// Eski `command_checklist` kayıtlarını hedef-kapsamlı anahtarlara taşı.
///
/// - `server-security-check::{step}` → `server-security-check::local::{step}`
/// - çıplak `{step}` (önek yok) → aynı varsayılan hedef altına
/// - zaten 3 parçalı anahtarlar (`guide::target::step`) olduğu gibi kalır
///
/// Dönüş: taşınan / yeniden yazılan kayıt sayısı.
pub fn migrate_checklist_progress(map: &mut std::collections::BTreeMap<String, String>) -> usize {
    let mut rewritten = 0usize;
    let legacy: Vec<(String, String)> = map
        .iter()
        .filter(|(k, _)| !is_target_scoped_checklist_key(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (old_key, value) in legacy {
        let Some(step_id) = legacy_checklist_step_id(&old_key) else {
            continue;
        };
        let new_key = checklist_progress_key(CHECKLIST_DEFAULT_TARGET, step_id);
        map.remove(&old_key);
        // Hedefte zaten yeni kayıt varsa eskiyi düş; üzerine yazma.
        map.entry(new_key).or_insert(value);
        rewritten += 1;
    }
    rewritten
}

fn is_target_scoped_checklist_key(key: &str) -> bool {
    let parts: Vec<&str> = key.splitn(3, "::").collect();
    parts.len() == 3 && parts[0] == SERVER_SECURITY_GUIDE_ID && !parts[1].is_empty()
}

fn legacy_checklist_step_id(key: &str) -> Option<&str> {
    if let Some(rest) = key.strip_prefix(&format!("{SERVER_SECURITY_GUIDE_ID}::")) {
        // Eski 2 parçalı: guide::step (step içinde :: yok).
        if !rest.is_empty() && !rest.contains("::") {
            return Some(rest);
        }
        return None;
    }
    // Çıplak adım id (yanlışlıkla öneksiz yazılmış eski kayıt).
    if !key.is_empty() && !key.contains("::") {
        return Some(key);
    }
    None
}

fn guide_from_bank(bank: &CommandBank) -> Option<&GuideEntry> {
    bank.guides.iter().find(|g| {
        g.id == SERVER_SECURITY_GUIDE_ID
            || g.kind.as_deref() == Some("checklist")
                && g.tags
                    .iter()
                    .any(|t| t == "server-security" || t == "security")
    })
}

fn step_from_guide(step: &GuideStep) -> ChecklistStepView {
    let examples = step
        .example
        .clone()
        .map(|ex| {
            ChecklistExampleView::from_command_example(
                ex,
                if step.platforms.is_empty() {
                    step.operating_systems.clone()
                } else {
                    step.platforms.clone()
                },
            )
        })
        .into_iter()
        .collect();
    ChecklistStepView {
        id: step.id.clone(),
        title: step.title.clone(),
        summary: step.summary.clone(),
        diagnosis: step.diagnosis.clone(),
        risk: step.risk,
        requires_elevated: step.requires_elevated,
        related_commands: step.related_commands.clone(),
        examples,
        rollback: step.rollback.clone(),
        platforms: step.platforms.clone(),
        operating_systems: step.operating_systems.clone(),
    }
}

/// Bankada `guides` yoksa güvenlik / sunucu görevlerinden sıralı adım üret.
fn derived_steps(bank: &CommandBank) -> Vec<ChecklistStepView> {
    // Tercih: Commands `security-check-*` görevleri (Homelab related_tasks sırası).
    // Yoksa eski salt-okunur teşhis görevlerine düş.
    const SECURITY_CHECK_ORDER: &[&str] = &[
        "security-check-os-accounts",
        "security-check-updates",
        "security-check-ssh",
        "security-check-firewall-ports",
        "security-check-users-sudo",
        "security-check-docker",
        "security-check-reverse-proxy-tls",
        "security-check-dns-vpn",
        "security-check-backup-restore",
        "security-check-logs-incident",
    ];
    const FALLBACK_ORDER: &[&str] = &[
        "check-firewall-status",
        "check-open-ports",
        "list-running-services",
        "inspect-system-logs",
        "check-disk-space",
        "show-local-ip",
        "check-dns",
        "check-internet-connectivity",
        "connect-ssh",
        "update-system",
    ];

    let mut order: Vec<String> = SECURITY_CHECK_ORDER
        .iter()
        .filter(|id| bank.find_task(id).is_some())
        .map(|s| (*s).to_string())
        .collect();
    if let Some(profile) = bank.find_profile("homelab") {
        for tid in &profile.related_tasks {
            if tid.starts_with("security-check-")
                && !order.iter().any(|x| x == tid)
                && bank.find_task(tid).is_some()
            {
                order.push(tid.clone());
            }
        }
    }
    if order.is_empty() {
        order = FALLBACK_ORDER.iter().map(|s| (*s).to_string()).collect();
    }

    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for tid in &order {
        let Some(task) = bank.find_task(tid.as_str()) else {
            continue;
        };
        if !seen.insert(task.id.clone()) {
            continue;
        }
        // Tüm uygulamaları koru — platformları tek örneğe çökertme.
        let mut examples: Vec<ChecklistExampleView> = Vec::new();
        let mut step_platforms: Vec<String> = Vec::new();
        for impl_ in &task.implementations {
            for p in &impl_.platforms {
                if !step_platforms.iter().any(|x| x.eq_ignore_ascii_case(p)) {
                    step_platforms.push(p.clone());
                }
            }
            for ex in &impl_.examples {
                examples.push(ChecklistExampleView::from_task_impl(impl_, ex));
            }
        }
        let elev = examples.iter().any(|e| {
            e.example.requires_elevated || e.example.privilege.as_deref() == Some("elevated")
        });
        let risk = select_checklist_examples(&examples, &CommandsFilters::default())
            .into_iter()
            .map(|e| e.example.risk)
            .min_by_key(|r| *r as u8)
            .or_else(|| {
                examples
                    .iter()
                    .map(|e| e.example.risk)
                    .min_by_key(|r| *r as u8)
            })
            .unwrap_or(RiskLevel::Caution);
        out.push(ChecklistStepView {
            id: task.id.clone(),
            title: task.name.clone(),
            summary: task.summary.clone(),
            diagnosis: LocalizedString {
                tr: Some(
                    "Salt okunur teşhis — Nova komutu çalıştırmaz; çıktıyı kendiniz yorumlayın."
                        .into(),
                ),
                en: Some(
                    "Read-only diagnosis — Nova never runs the command; interpret output yourself."
                        .into(),
                ),
                ..Default::default()
            },
            risk,
            requires_elevated: elev,
            related_commands: task.related_commands.clone(),
            examples,
            rollback: LocalizedString {
                tr: Some(
                    "Değişiklik yapılmadıysa geri alma gerekmez; yazma örneklerini atlayın.".into(),
                ),
                en: Some("No rollback if nothing changed; skip write/mutate examples.".into()),
                ..Default::default()
            },
            platforms: step_platforms.clone(),
            operating_systems: step_platforms,
        });
    }

    // Homelab profilinden eksik kalan güvenli ağ komutları.
    if let Some(profile) = bank
        .find_profile("homelab")
        .or_else(|| bank.find_profile("linux-server"))
    {
        for cmd_id in ["ss", "ufw", "systemctl", "journalctl", "ssh"] {
            if out
                .iter()
                .any(|s| s.related_commands.iter().any(|c| c == cmd_id))
            {
                continue;
            }
            let Some(cmd) = bank.find_by_id_or_name(cmd_id) else {
                continue;
            };
            if !profile
                .related_commands
                .iter()
                .any(|c| c == cmd_id || c == &cmd.id)
            {
                continue;
            }
            let plats = if cmd.operating_systems.is_empty() {
                cmd.platforms.clone()
            } else {
                cmd.operating_systems.clone()
            };
            let examples: Vec<ChecklistExampleView> = cmd
                .examples
                .iter()
                .cloned()
                .map(|ex| ChecklistExampleView::from_command_example(ex, plats.clone()))
                .collect();
            let step_id = format!("cmd-{cmd_id}");
            if !seen.insert(step_id.clone()) {
                continue;
            }
            out.push(ChecklistStepView {
                id: step_id,
                title: LocalizedString {
                    tr: Some(cmd.name.clone()),
                    en: Some(cmd.name.clone()),
                    ..Default::default()
                },
                summary: cmd.summary.clone(),
                diagnosis: cmd.description.clone(),
                risk: cmd.risk,
                requires_elevated: cmd.requires_elevated,
                related_commands: vec![cmd.id.clone()],
                examples,
                rollback: LocalizedString::default(),
                platforms: cmd.platforms.clone(),
                operating_systems: cmd.operating_systems.clone(),
            });
            if out.len() >= 14 {
                break;
            }
        }
    }

    out
}

/// Sunucu güvenlik kontrol listesi adımları (katalog guide veya türetim).
pub fn server_security_steps(bank: &CommandBank) -> Vec<ChecklistStepView> {
    if let Some(guide) = guide_from_bank(bank) {
        let steps: Vec<_> = guide.steps.iter().map(step_from_guide).collect();
        if !steps.is_empty() {
            return steps;
        }
    }
    derived_steps(bank)
}

/// Rehber başlığı (katalog veya varsayılan).
pub fn server_security_title(bank: &CommandBank, lang: &str) -> String {
    if let Some(guide) = guide_from_bank(bank) {
        let name = guide.name.pick(lang);
        if !name.is_empty() {
            return name.to_string();
        }
    }
    if lang.to_ascii_lowercase().starts_with("en") {
        "Server security check".into()
    } else {
        "Sunucu güvenlik kontrolü".into()
    }
}

/// OS / distro / shell süzgecine uyan checklist örnekleri.
///
/// Eşleşme yoksa boş döner — başka platform komutu **sessizce** seçilmez.
/// Distro süzgeci yokken Linux paket yöneticisi örnekleri yarışırsa Arch/pacman tercih edilir.
pub fn select_checklist_examples<'a>(
    examples: &'a [ChecklistExampleView],
    filters: &CommandsFilters,
) -> Vec<&'a ChecklistExampleView> {
    let mut matched: Vec<&ChecklistExampleView> = examples
        .iter()
        .filter(|ex| example_matches_filters(ex, filters))
        .collect();

    if filters.distro.is_none() {
        matched = prefer_arch_among_distro_variants(matched);
    }

    // Aynı platformda birden fazla örnek: düşük risk önce.
    matched.sort_by(|a, b| {
        (a.example.risk as u8)
            .cmp(&(b.example.risk as u8))
            .then_with(|| a.implementation_id.cmp(&b.implementation_id))
            .then_with(|| a.example.command.cmp(&b.example.command))
    });
    matched
}

fn example_matches_filters(ex: &ChecklistExampleView, filters: &CommandsFilters) -> bool {
    if let Some(ref os) = filters.os {
        if !platforms_cover_os(&ex.platforms, os) && !platforms_cover_os(&ex.example.platforms, os)
        {
            return false;
        }
    }
    if let Some(ref shell) = filters.shell {
        if !shells_cover(&ex.shells, ex.example.shell.as_deref(), shell) {
            return false;
        }
    }
    if let Some(ref distro) = filters.distro {
        if !distributions_cover(&ex.distributions, distro) {
            // Distro etiketi yoksa OS düzeyinde genel örnek kabul (distro-agnostic).
            if !ex.distributions.is_empty() {
                return false;
            }
        }
    }
    true
}

fn platforms_cover_os(platforms: &[String], os: &str) -> bool {
    let os = normalize_os_token(os);
    if platforms.is_empty() {
        // Boş = genel / çapraz — süzgeçte göster (başka OS yerine koyma değil).
        return true;
    }
    platforms.iter().any(|p| {
        let p = normalize_os_token(p);
        p == os
            || p.contains(&os)
            || os.contains(&p)
            || p == "any"
            || p == "all"
            || p == "cross"
            || p == "portable"
    })
}

fn normalize_os_token(raw: &str) -> String {
    let s = raw.to_ascii_lowercase();
    match s.as_str() {
        "freebsd" | "openbsd" | "netbsd" | "dragonfly" => "bsd".into(),
        "osx" | "mac" | "darwin" => "macos".into(),
        "win" | "win32" | "win64" => "windows".into(),
        other => other.to_string(),
    }
}

fn shells_cover(shells: &[String], example_shell: Option<&str>, want: &str) -> bool {
    let want = want.to_ascii_lowercase();
    let mut hay: Vec<String> = shells.iter().map(|s| s.to_ascii_lowercase()).collect();
    if let Some(sh) = example_shell {
        if !sh.is_empty() {
            hay.push(sh.to_ascii_lowercase());
        }
    }
    if hay.is_empty() {
        // Kabuk belirtilmemiş = kabuk-agnostik.
        return true;
    }
    hay.iter()
        .any(|h| h == &want || h.contains(&want) || want.contains(h.as_str()))
}

fn distributions_cover(distributions: &[String], want: &str) -> bool {
    let want = want.to_ascii_lowercase();
    let aliases = distro_aliases(&want);
    distributions.iter().any(|d| {
        let d = d.to_ascii_lowercase();
        aliases
            .iter()
            .any(|a| d == *a || d.contains(a) || a.contains(&d))
    })
}

fn distro_aliases(want: &str) -> Vec<&str> {
    match want {
        "arch" | "archlinux" | "endeavouros" | "manjaro" => {
            vec!["arch", "archlinux", "pacman"]
        }
        "debian" => vec!["debian", "apt"],
        "ubuntu" | "pop" | "popos" => vec!["ubuntu", "debian", "apt"],
        "fedora" => vec!["fedora", "dnf"],
        "rhel" | "centos" | "rocky" | "alma" => vec!["rhel", "centos", "fedora", "dnf"],
        other => vec![other],
    }
}

/// Linux paket yöneticisi yarışında Arch/pacman tercih et; distro-agnostik örnekleri koru.
fn prefer_arch_among_distro_variants(
    examples: Vec<&ChecklistExampleView>,
) -> Vec<&ChecklistExampleView> {
    let any_tagged = examples.iter().any(|e| !e.distributions.is_empty());
    if !any_tagged {
        return examples;
    }
    let has_arch = examples
        .iter()
        .any(|e| distributions_cover(&e.distributions, "arch"));
    if !has_arch {
        return examples;
    }
    examples
        .into_iter()
        .filter(|e| e.distributions.is_empty() || distributions_cover(&e.distributions, "arch"))
        .collect()
}

fn infer_distributions(impl_: &TaskImplementation) -> Vec<String> {
    let id = impl_.id.to_ascii_lowercase();
    let cmds = impl_.commands.join(" ").to_ascii_lowercase();
    let blob = format!(
        "{id} {cmds} {}",
        impl_
            .examples
            .iter()
            .map(|e| e.command.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    );
    let mut out = Vec::new();
    let push = |out: &mut Vec<String>, tag: &str| {
        if !out.iter().any(|x| x == tag) {
            out.push(tag.into());
        }
    };
    if id.contains("arch")
        || blob.contains("pacman")
        || blob.contains("checkupdates")
        || blob.contains("archlinux")
    {
        push(&mut out, "arch");
    }
    if id.contains("debian")
        || id.contains("ubuntu")
        || blob.contains("apt ")
        || blob.contains("apt-get")
        || blob.contains("apt list")
    {
        push(&mut out, "debian");
        if id.contains("ubuntu") || blob.contains("ubuntu") {
            push(&mut out, "ubuntu");
        }
    }
    if id.contains("fedora") || blob.contains("dnf ") || blob.contains("dnf check") {
        push(&mut out, "fedora");
    }
    if id.contains("rhel") || blob.contains("yum ") {
        push(&mut out, "rhel");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::bank::{IntentPhrases, TaskEntry, VerificationInfo, VerificationStatus};

    fn ex(cmd: &str, risk: RiskLevel, shell: &str) -> TaskExample {
        TaskExample {
            command: cmd.into(),
            explanation: LocalizedString::default(),
            risk,
            requires_elevated: false,
            shell: Some(shell.into()),
            privilege: Some("none".into()),
        }
    }

    fn impl_(
        id: &str,
        platforms: &[&str],
        shells: &[&str],
        examples: Vec<TaskExample>,
    ) -> TaskImplementation {
        TaskImplementation {
            id: id.into(),
            commands: Vec::new(),
            examples,
            platforms: platforms.iter().map(|s| (*s).into()).collect(),
            shells: shells.iter().map(|s| (*s).into()).collect(),
        }
    }

    fn firewall_task() -> TaskEntry {
        TaskEntry {
            id: "security-check-firewall-ports".into(),
            kind: Some("task".into()),
            name: LocalizedString {
                en: Some("Firewall".into()),
                ..Default::default()
            },
            summary: LocalizedString::default(),
            category: "security".into(),
            intent: IntentPhrases::default(),
            implementations: vec![
                impl_(
                    "linux-ss-ufw",
                    &["linux"],
                    &["bash"],
                    vec![
                        ex("ss -tulpn", RiskLevel::Safe, "bash"),
                        ex("sudo ufw status verbose", RiskLevel::Safe, "bash"),
                    ],
                ),
                impl_(
                    "windows-net",
                    &["windows"],
                    &["powershell"],
                    vec![ex(
                        "Get-NetTCPConnection -State Listen",
                        RiskLevel::Safe,
                        "powershell",
                    )],
                ),
                impl_(
                    "bsd-pf",
                    &["bsd", "macos"],
                    &["bash"],
                    vec![ex("sudo pfctl -s info", RiskLevel::Caution, "bash")],
                ),
            ],
            verification: VerificationInfo {
                status: VerificationStatus::Verified,
                ..Default::default()
            },
            tags: vec!["security".into()],
            related_commands: vec!["ss".into(), "ufw".into()],
            related_tasks: Vec::new(),
        }
    }

    fn updates_task() -> TaskEntry {
        TaskEntry {
            id: "security-check-updates".into(),
            kind: Some("task".into()),
            name: LocalizedString {
                en: Some("Updates".into()),
                ..Default::default()
            },
            summary: LocalizedString::default(),
            category: "security".into(),
            intent: IntentPhrases::default(),
            implementations: vec![
                impl_(
                    "arch-check",
                    &["linux"],
                    &["bash"],
                    vec![ex("checkupdates || pacman -Qu", RiskLevel::Safe, "bash")],
                ),
                impl_(
                    "debian-check",
                    &["linux"],
                    &["bash"],
                    vec![ex("apt list --upgradable", RiskLevel::Safe, "bash")],
                ),
                impl_(
                    "windows-winget",
                    &["windows"],
                    &["powershell"],
                    vec![ex("winget upgrade", RiskLevel::Safe, "powershell")],
                ),
            ],
            verification: VerificationInfo {
                status: VerificationStatus::Verified,
                ..Default::default()
            },
            tags: vec!["security".into()],
            related_commands: Vec::new(),
            related_tasks: Vec::new(),
        }
    }

    fn bank_with(tasks: Vec<TaskEntry>) -> CommandBank {
        CommandBank {
            schema_version: 2,
            kind: crate::commands::bank::BANK_KIND.into(),
            generated_at: None,
            content_sha256: None,
            command_count: Some(0),
            concept_count: Some(0),
            task_count: Some(tasks.len()),
            profile_count: Some(0),
            commands: Vec::new(),
            concepts: Vec::new(),
            tasks,
            profiles: Vec::new(),
            guides: Vec::new(),
        }
    }

    #[test]
    fn checklist_keeps_per_platform_examples_no_collapse() {
        let bank = bank_with(vec![firewall_task()]);
        let steps = server_security_steps(&bank);
        let step = steps
            .iter()
            .find(|s| s.id == "security-check-firewall-ports")
            .expect("firewall step");
        assert!(
            step.examples.len() >= 3,
            "expected multi-platform examples, got {}",
            step.examples.len()
        );
        let plats: Vec<_> = step
            .examples
            .iter()
            .flat_map(|e| e.platforms.iter().cloned())
            .collect();
        assert!(plats.iter().any(|p| p == "linux"));
        assert!(plats.iter().any(|p| p == "windows"));
        assert!(plats.iter().any(|p| p == "bsd" || p == "macos"));
    }

    #[test]
    fn checklist_os_filter_windows_excludes_ufw() {
        let bank = bank_with(vec![firewall_task()]);
        let steps = server_security_steps(&bank);
        let step = steps
            .iter()
            .find(|s| s.id == "security-check-firewall-ports")
            .unwrap();
        let filters = CommandsFilters {
            os: Some("windows".into()),
            ..Default::default()
        };
        let matched = select_checklist_examples(&step.examples, &filters);
        assert!(!matched.is_empty(), "windows should have an example");
        for m in &matched {
            assert!(
                !m.example.command.contains("ufw") && !m.example.command.contains("ss -"),
                "windows filter must not surface linux cmds: {}",
                m.example.command
            );
            assert!(
                platforms_cover_os(&m.platforms, "windows"),
                "matched example platforms={:?}",
                m.platforms
            );
        }
    }

    #[test]
    fn checklist_os_filter_no_match_returns_empty() {
        let bank = bank_with(vec![updates_task()]);
        let steps = server_security_steps(&bank);
        let step = steps
            .iter()
            .find(|s| s.id == "security-check-updates")
            .unwrap();
        // updates_task has no macos impl — must not fall back to linux/windows.
        let filters = CommandsFilters {
            os: Some("macos".into()),
            ..Default::default()
        };
        let matched = select_checklist_examples(&step.examples, &filters);
        assert!(
            matched.is_empty(),
            "macos has no example; must not substitute. got {:?}",
            matched
                .iter()
                .map(|m| m.example.command.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn checklist_linux_prefers_arch_pacman_over_debian() {
        let bank = bank_with(vec![updates_task()]);
        let steps = server_security_steps(&bank);
        let step = steps
            .iter()
            .find(|s| s.id == "security-check-updates")
            .unwrap();
        let filters = CommandsFilters {
            os: Some("linux".into()),
            ..Default::default()
        };
        let matched = select_checklist_examples(&step.examples, &filters);
        assert!(
            matched.iter().any(|m| m.example.command.contains("pacman")
                || m.example.command.contains("checkupdates")),
            "arch/pacman preferred: {:?}",
            matched
                .iter()
                .map(|m| m.example.command.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            matched
                .iter()
                .all(|m| !m.example.command.contains("apt list")),
            "debian must not win when arch competes"
        );
    }

    #[test]
    fn checklist_distro_filter_selects_debian() {
        let bank = bank_with(vec![updates_task()]);
        let steps = server_security_steps(&bank);
        let step = steps
            .iter()
            .find(|s| s.id == "security-check-updates")
            .unwrap();
        let filters = CommandsFilters {
            os: Some("linux".into()),
            distro: Some("debian".into()),
            ..Default::default()
        };
        let matched = select_checklist_examples(&step.examples, &filters);
        assert_eq!(matched.len(), 1);
        assert!(matched[0].example.command.contains("apt"));
    }

    #[test]
    fn checklist_shell_filter_powershell() {
        let bank = bank_with(vec![firewall_task()]);
        let steps = server_security_steps(&bank);
        let step = steps
            .iter()
            .find(|s| s.id == "security-check-firewall-ports")
            .unwrap();
        let filters = CommandsFilters {
            shell: Some("powershell".into()),
            ..Default::default()
        };
        let matched = select_checklist_examples(&step.examples, &filters);
        assert!(!matched.is_empty());
        assert!(matched
            .iter()
            .all(|m| m.example.command.contains("Get-NetTCPConnection")));
    }

    #[test]
    fn checklist_progress_isolated_between_targets() {
        let step = ChecklistStepView {
            id: "security-check-ssh".into(),
            title: LocalizedString::default(),
            summary: LocalizedString::default(),
            diagnosis: LocalizedString::default(),
            risk: RiskLevel::Safe,
            requires_elevated: false,
            related_commands: Vec::new(),
            examples: Vec::new(),
            rollback: LocalizedString::default(),
            platforms: Vec::new(),
            operating_systems: Vec::new(),
        };
        let local = step.state_key_for("local");
        let prod = step.state_key_for("prod-host");
        assert_ne!(local, prod, "farklı hedefler ayrı anahtar kullanmalı");
        assert_eq!(
            local,
            format!("{SERVER_SECURITY_GUIDE_ID}::local::security-check-ssh")
        );
        assert_eq!(
            prod,
            format!("{SERVER_SECURITY_GUIDE_ID}::prod-host::security-check-ssh")
        );

        let mut map = std::collections::BTreeMap::new();
        map.insert(local.clone(), "ok".into());
        assert_eq!(map.get(&local).map(String::as_str), Some("ok"));
        assert!(
            !map.contains_key(&prod),
            "local ilerlemesi prod-host'a sızmamalı"
        );
    }

    #[test]
    fn checklist_progress_migrates_legacy_two_part_and_bare_keys() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            format!("{SERVER_SECURITY_GUIDE_ID}::security-check-ssh"),
            "ok".into(),
        );
        map.insert("security-check-firewall-ports".into(), "review".into());
        // Zaten hedef-kapsamlı — dokunulmamalı.
        map.insert(
            format!("{SERVER_SECURITY_GUIDE_ID}::prod::security-check-docker"),
            "na".into(),
        );

        let n = migrate_checklist_progress(&mut map);
        assert_eq!(n, 2, "iki eski kayıt taşınmalı, got {n}");
        assert!(
            !map.contains_key(&format!("{SERVER_SECURITY_GUIDE_ID}::security-check-ssh")),
            "eski 2 parçalı anahtar kalkmalı"
        );
        assert!(
            !map.contains_key("security-check-firewall-ports"),
            "çıplak adım anahtarı kalkmalı"
        );
        assert_eq!(
            map.get(&checklist_progress_key(
                CHECKLIST_DEFAULT_TARGET,
                "security-check-ssh"
            ))
            .map(String::as_str),
            Some("ok")
        );
        assert_eq!(
            map.get(&checklist_progress_key(
                CHECKLIST_DEFAULT_TARGET,
                "security-check-firewall-ports"
            ))
            .map(String::as_str),
            Some("review")
        );
        assert_eq!(
            map.get(&format!(
                "{SERVER_SECURITY_GUIDE_ID}::prod::security-check-docker"
            ))
            .map(String::as_str),
            Some("na"),
            "mevcut hedef kaydı korunmalı"
        );
    }

    #[test]
    fn checklist_progress_migrate_does_not_clobber_existing_target_value() {
        let mut map = std::collections::BTreeMap::new();
        let modern = checklist_progress_key(CHECKLIST_DEFAULT_TARGET, "security-check-ssh");
        map.insert(modern.clone(), "review".into());
        map.insert(
            format!("{SERVER_SECURITY_GUIDE_ID}::security-check-ssh"),
            "ok".into(),
        );
        migrate_checklist_progress(&mut map);
        assert_eq!(
            map.get(&modern).map(String::as_str),
            Some("review"),
            "mevcut modern değer eski kayıtla ezilmemeli"
        );
    }
}
