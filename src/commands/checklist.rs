//! Sunucu güvenlik kontrolü — sıralı checklist UI adımları.
//!
//! Rehber **asla** komut çalıştırmaz / tarama yapmaz. Adımlar banka içeriğinden
//! (katalog `guides` veya güvenlik görevleri + salt-okunur örnekler) gelir.
//! Durum `unchecked` / `ok` / `review` / `na` yerel `state.toml`'da tutulur.

use crate::commands::bank::{
    CommandBank, CommandExample, GuideEntry, GuideStep, LocalizedString, RiskLevel,
};
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
    pub example: Option<CommandExample>,
    pub rollback: LocalizedString,
    pub platforms: Vec<String>,
    pub operating_systems: Vec<String>,
}

impl ChecklistStepView {
    pub fn state_key(&self) -> String {
        format!("{SERVER_SECURITY_GUIDE_ID}::{}", self.id)
    }
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
    ChecklistStepView {
        id: step.id.clone(),
        title: step.title.clone(),
        summary: step.summary.clone(),
        diagnosis: step.diagnosis.clone(),
        risk: step.risk,
        requires_elevated: step.requires_elevated,
        related_commands: step.related_commands.clone(),
        example: step.example.clone(),
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
        // En düşük riskli örnek (tercihen safe) — TaskExample → CommandExample.
        let mut best: Option<CommandExample> = None;
        for impl_ in &task.implementations {
            for ex in &impl_.examples {
                let take = match &best {
                    None => true,
                    Some(b) => (ex.risk as u8) < (b.risk as u8),
                };
                if take {
                    best = Some(CommandExample {
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
                    });
                }
            }
        }
        let elev = best
            .as_ref()
            .map(|b| b.requires_elevated || b.privilege.as_deref() == Some("elevated"))
            .unwrap_or(false);
        let risk = best.as_ref().map(|b| b.risk).unwrap_or(RiskLevel::Caution);
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
            example: best,
            rollback: LocalizedString {
                tr: Some(
                    "Değişiklik yapılmadıysa geri alma gerekmez; yazma örneklerini atlayın.".into(),
                ),
                en: Some("No rollback if nothing changed; skip write/mutate examples.".into()),
                ..Default::default()
            },
            platforms: Vec::new(),
            operating_systems: Vec::new(),
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
            let ex = cmd
                .examples
                .iter()
                .find(|e| e.risk == RiskLevel::Safe)
                .cloned()
                .or_else(|| cmd.examples.first().cloned());
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
                example: ex,
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
