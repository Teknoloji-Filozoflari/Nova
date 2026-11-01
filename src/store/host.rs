//! Host’a göre paket yöneticisi seçimi ve PM güncelleme planı.
//!
//! Katalogda tek `install_method` / `install_command` vardır; alternatifler
//! `install_notes` satırlarında (`apt: …`, `dnf: …`, `brew: …`, `uv: …`, …)
//! tutulur. Bu modül:
//!
//! 1. Host’ta hangi PM’lerin PATH’te olduğunu tespit eder.
//! 2. OS tercihine göre (Linux: apt > dnf > pipx > uv > npm > cargo > brew)
//!    uygun komutu seçer.
//! 3. Kurulu PM uygulamaları için `install` → `upgrade` argv üretir ve
//!    apt/dnf/brew/npm için “outdated” çıktısından güncelleme adaylarını bulur.

use crate::store::{format_install_command, validate_install_command, InstallMethod, StoreEntry};
use std::collections::{BTreeMap, BTreeSet};
use std::process::{Command, Stdio};

/// Çalıştığımız işletim sistemi ailesi (tercih sırası için).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    Linux,
    MacOs,
    Other,
}

/// Host’ta PATH üzerinden görünen paket yöneticileri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HostTools {
    pub apt: bool,
    pub dnf: bool,
    pub brew: bool,
    pub pipx: bool,
    pub uv: bool,
    pub npm: bool,
    pub cargo: bool,
}

impl HostTools {
    /// `which` ile gerçek host’u tarar.
    pub fn detect() -> Self {
        Self {
            apt: which::which("apt").is_ok(),
            dnf: which::which("dnf").is_ok(),
            brew: which::which("brew").is_ok(),
            pipx: which::which("pipx").is_ok(),
            uv: which::which("uv").is_ok(),
            npm: which::which("npm").is_ok(),
            cargo: which::which("cargo").is_ok(),
        }
    }

    /// Testler için sabit bir küme (dnf/uv kapalı).
    pub fn from_flags(apt: bool, brew: bool, pipx: bool, npm: bool, cargo: bool) -> Self {
        Self {
            apt,
            dnf: false,
            brew,
            pipx,
            uv: false,
            npm,
            cargo,
        }
    }

    /// dnf bayrağını ayarlar (builder).
    pub fn with_dnf(mut self, dnf: bool) -> Self {
        self.dnf = dnf;
        self
    }

    /// uv bayrağını ayarlar (builder).
    pub fn with_uv(mut self, uv: bool) -> Self {
        self.uv = uv;
        self
    }

    /// Verilen yöntem host’ta çalıştırılabilir mi?
    pub fn supports(&self, method: InstallMethod) -> bool {
        match method {
            InstallMethod::Apt => self.apt,
            InstallMethod::Dnf => self.dnf,
            InstallMethod::Brew => self.brew,
            InstallMethod::Pipx => self.pipx,
            InstallMethod::Uv => self.uv,
            InstallMethod::Npm => self.npm,
            InstallMethod::Cargo => self.cargo,
            InstallMethod::Manual | InstallMethod::DownloadArchive => false,
        }
    }
}

/// Seçilen kurulum planı (yöntem + kabuksuz argv).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub method: InstallMethod,
    pub argv: Vec<String>,
}

impl InstallPlan {
    pub fn rendered(&self) -> String {
        format_install_command(&self.argv)
    }

    /// Argv `sudo` ile mi başlıyor?
    pub fn uses_sudo(&self) -> bool {
        self.argv.first().map(String::as_str) == Some("sudo")
    }

    /// TUI’yi askıya alıp gerçek TTY ile çalıştırmak gerekir mi?
    ///
    /// `sudo` parola isterse (önbellekte değilse) ham modda parola okunamaz;
    /// apt/dnf planlarında kimlik bilgisi yoksa `true` döner.
    pub fn needs_tty_suspend(&self) -> bool {
        if !self.method.needs_elevated_tty() && !self.uses_sudo() {
            return false;
        }
        if !self.uses_sudo() {
            // Kök kullanıcısı / sudo’suz apt|dnf — TTY askısı gerekmez.
            return false;
        }
        !sudo_credentials_cached()
    }
}

/// Paket yöneticisi ile güncelleme planı (`U` sonrası ↑).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PmUpdatePlan {
    pub method: InstallMethod,
    pub argv: Vec<String>,
    /// UI’de gösterilecek kısa ipucu (ör. paket adı veya “brew upgrade”).
    pub hint: String,
}

impl PmUpdatePlan {
    pub fn rendered(&self) -> String {
        format_install_command(&self.argv)
    }

    pub fn as_install_plan(&self) -> InstallPlan {
        InstallPlan {
            method: self.method,
            argv: self.argv.clone(),
        }
    }

    pub fn needs_tty_suspend(&self) -> bool {
        self.as_install_plan().needs_tty_suspend()
    }
}

/// `sudo -n true` ile parola önbelleği / NOPASSWD kontrolü.
pub fn sudo_credentials_cached() -> bool {
    Command::new("sudo")
        .args(["-n", "true"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Derleme hedefinden host OS tahmini.
pub fn detect_host_os() -> HostOs {
    if cfg!(target_os = "linux") {
        HostOs::Linux
    } else if cfg!(target_os = "macos") {
        HostOs::MacOs
    } else {
        HostOs::Other
    }
}

/// OS’e göre tercih sırası (soldan sağa yüksek öncelik).
///
/// Linux: **apt > dnf > pipx > uv > npm > cargo > brew**.
/// macOS’ta brew öne alınır; uv pipx’ten sonra gelir.
pub fn preferred_methods(os: HostOs) -> &'static [InstallMethod] {
    match os {
        HostOs::Linux | HostOs::Other => &[
            InstallMethod::Apt,
            InstallMethod::Dnf,
            InstallMethod::Pipx,
            InstallMethod::Uv,
            InstallMethod::Npm,
            InstallMethod::Cargo,
            InstallMethod::Brew,
        ],
        HostOs::MacOs => &[
            InstallMethod::Brew,
            InstallMethod::Pipx,
            InstallMethod::Uv,
            InstallMethod::Npm,
            InstallMethod::Cargo,
            InstallMethod::Apt,
            InstallMethod::Dnf,
        ],
    }
}

/// `install_notes` satırlarından desteklenen PM seçeneklerini çıkarır.
///
/// Tanınan etiketler: `apt`, `dnf`, `brew`, `pipx`, `uv`, `npm`, `cargo`.
/// `curl` / `go` / `pip` / `gh` / `Resmî` yok sayılır.
pub fn parse_notes_options(notes: &str) -> Vec<InstallPlan> {
    let mut out = Vec::new();
    for raw in notes.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((label, rest)) = line.split_once(':') else {
            continue;
        };
        let label = label.trim().to_ascii_lowercase();
        let method = match label.as_str() {
            "apt" => InstallMethod::Apt,
            "dnf" => InstallMethod::Dnf,
            "brew" => InstallMethod::Brew,
            "pipx" => InstallMethod::Pipx,
            "uv" => InstallMethod::Uv,
            "npm" => InstallMethod::Npm,
            "cargo" => InstallMethod::Cargo,
            _ => continue,
        };
        let cmd = rest.trim();
        if cmd.is_empty() {
            continue;
        }
        let mut argv = split_command_line(cmd);
        if argv.is_empty() {
            continue;
        }
        if method == InstallMethod::Apt {
            ensure_sys_yes(&mut argv, "install");
        }
        if method == InstallMethod::Dnf {
            ensure_sys_yes(&mut argv, "install");
        }
        if validate_install_command(method, &argv).is_ok() {
            out.push(InstallPlan { method, argv });
        }
    }
    out
}

/// Katalog birincili + notes alternatiflerinden host’a uyan planı seçer.
///
/// Host’ta hiç uygun PM yoksa birincil planı (katalog) döner; kurulum
/// sırasında “PM bulunamadı” hatası yine anlaşılır kalır.
pub fn resolve_install_plan(
    entry: &StoreEntry,
    host: &HostTools,
    os: HostOs,
) -> Option<InstallPlan> {
    if !entry.install_method.is_package_manager() {
        return None;
    }
    let mut by_method: BTreeMap<InstallMethod, InstallPlan> = BTreeMap::new();

    if let Some(argv) = entry.install_command.as_ref() {
        if validate_install_command(entry.install_method, argv).is_ok() {
            by_method.insert(
                entry.install_method,
                InstallPlan {
                    method: entry.install_method,
                    argv: argv.clone(),
                },
            );
        }
    }
    if let Some(notes) = entry.install_notes.as_deref() {
        for plan in parse_notes_options(notes) {
            by_method.entry(plan.method).or_insert(plan);
        }
    }

    for method in preferred_methods(os) {
        if host.supports(*method) {
            if let Some(plan) = by_method.remove(method) {
                return Some(plan);
            }
        }
    }

    // Hiçbiri host’ta yok: katalog birinciline düş.
    by_method.remove(&entry.install_method).or_else(|| {
        entry.install_command.as_ref().map(|argv| InstallPlan {
            method: entry.install_method,
            argv: argv.clone(),
        })
    })
}

/// Kurulum argv’sinden güncelleme argv’si üretir.
pub fn upgrade_argv(method: InstallMethod, install_argv: &[String]) -> Option<Vec<String>> {
    if install_argv.is_empty() {
        return None;
    }
    match method {
        InstallMethod::Pipx => {
            // pipx install [flags…] PKG → pipx upgrade PKG
            let pkg = package_token(install_argv, "install")?;
            Some(vec!["pipx".into(), "upgrade".into(), pkg])
        }
        InstallMethod::Uv => {
            // uv tool install [flags…] PKG → uv tool upgrade PKG
            let pkg = uv_package_name(install_argv)?;
            Some(vec!["uv".into(), "tool".into(), "upgrade".into(), pkg])
        }
        InstallMethod::Npm => {
            // npm install|-i -g PKG → npm update -g PKG
            let pkg = package_token(install_argv, "install")
                .or_else(|| package_token(install_argv, "i"))?;
            Some(vec!["npm".into(), "update".into(), "-g".into(), pkg])
        }
        InstallMethod::Brew => {
            // brew install FORMULA → brew upgrade FORMULA
            let formula = package_token(install_argv, "install")?;
            Some(vec!["brew".into(), "upgrade".into(), formula])
        }
        InstallMethod::Apt => {
            // sudo apt install -y PKG → sudo apt install --only-upgrade -y PKG
            let pkg = sys_package_name(install_argv)?;
            let mut out = Vec::new();
            if install_argv.first().map(String::as_str) == Some("sudo") {
                out.push("sudo".into());
            }
            out.extend([
                "apt".into(),
                "install".into(),
                "--only-upgrade".into(),
                "-y".into(),
                pkg,
            ]);
            Some(out)
        }
        InstallMethod::Dnf => {
            // sudo dnf install -y PKG → sudo dnf upgrade -y PKG
            let pkg = sys_package_name(install_argv)?;
            let mut out = Vec::new();
            if install_argv.first().map(String::as_str) == Some("sudo") {
                out.push("sudo".into());
            }
            out.extend(["dnf".into(), "upgrade".into(), "-y".into(), pkg]);
            Some(out)
        }
        InstallMethod::Cargo => {
            // cargo install … → aynı argv + --force (yoksa)
            let mut out = install_argv.to_vec();
            if !out.iter().any(|a| a == "--force") {
                let pos = out.iter().position(|a| a == "install")?;
                out.insert(pos + 1, "--force".into());
            }
            Some(out)
        }
        InstallMethod::Manual | InstallMethod::DownloadArchive => None,
    }
}

/// apt/dnf/brew/npm “outdated” çıktılarından paket adı kümeleri.
#[derive(Debug, Clone, Default)]
pub struct OutdatedPackages {
    pub apt: BTreeSet<String>,
    pub dnf: BTreeSet<String>,
    pub brew: BTreeSet<String>,
    pub npm: BTreeSet<String>,
    /// pipx/uv/cargo için güvenilir toplu denetim yok; `true` ise kurulu
    /// uygulamalar güncelleme adayı sayılır (iyimser).
    pub optimistic_pipx: bool,
    pub optimistic_uv: bool,
    pub optimistic_cargo: bool,
}

impl OutdatedPackages {
    /// Gerçek host komutlarıyla doldurur (ağ/IO; arka plan işinde çağrılır).
    pub fn probe(host: &HostTools) -> Self {
        let mut out = Self {
            optimistic_pipx: host.pipx,
            optimistic_uv: host.uv,
            optimistic_cargo: host.cargo,
            ..Self::default()
        };
        if host.apt {
            if let Ok(text) = run_capture("apt", &["list", "--upgradable"]) {
                out.apt = parse_apt_upgradable(&text);
            }
        }
        if host.dnf {
            if let Ok(text) = run_capture("dnf", &["check-update", "-q"]) {
                out.dnf = parse_dnf_check_update(&text);
            }
        }
        if host.brew {
            if let Ok(text) = run_capture("brew", &["outdated", "--quiet"]) {
                out.brew = parse_brew_outdated(&text);
            }
        }
        if host.npm {
            if let Ok(text) = run_capture("npm", &["outdated", "-g", "--parseable"]) {
                out.npm = parse_npm_outdated(&text);
            }
        }
        out
    }

    /// Testler için boş (iyimser bayraklar kapalı) küme.
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Kurulu PM uygulamaları için güncelleme planları üretir.
///
/// - apt/dnf/brew/npm: yalnızca outdated listesinde görünen paketler.
/// - pipx/uv/cargo: host’ta PM varsa ve uygulama kuruluysa iyimser aday
///   (güvenilir toplu “outdated” yok).
pub fn collect_pm_updates(
    entries: &[StoreEntry],
    installed_ids: &BTreeSet<String>,
    host: &HostTools,
    os: HostOs,
    outdated: &OutdatedPackages,
) -> BTreeMap<String, PmUpdatePlan> {
    let mut plans = BTreeMap::new();
    for entry in entries {
        if !entry.install_method.is_package_manager() {
            continue;
        }
        if !installed_ids.contains(&entry.id) {
            continue;
        }
        let Some(install) = resolve_install_plan(entry, host, os) else {
            continue;
        };
        if !host.supports(install.method) {
            continue;
        }
        let Some(pkg) = package_name_for_method(install.method, &install.argv) else {
            continue;
        };
        let wanted = match install.method {
            InstallMethod::Apt => {
                outdated.apt.contains(&pkg)
                    || outdated.apt.contains(pkg.split('/').next().unwrap_or(&pkg))
            }
            InstallMethod::Dnf => {
                let short = pkg.split('.').next().unwrap_or(&pkg);
                outdated.dnf.contains(&pkg) || outdated.dnf.contains(short)
            }
            InstallMethod::Brew => {
                let short = pkg.rsplit('/').next().unwrap_or(&pkg);
                outdated.brew.contains(&pkg) || outdated.brew.contains(short)
            }
            InstallMethod::Npm => outdated.npm.contains(&pkg),
            InstallMethod::Pipx => outdated.optimistic_pipx,
            InstallMethod::Uv => outdated.optimistic_uv,
            InstallMethod::Cargo => outdated.optimistic_cargo,
            InstallMethod::Manual | InstallMethod::DownloadArchive => false,
        };
        if !wanted {
            continue;
        }
        let Some(argv) = upgrade_argv(install.method, &install.argv) else {
            continue;
        };
        if validate_upgrade_command(install.method, &argv).is_err() {
            continue;
        }
        plans.insert(
            entry.id.clone(),
            PmUpdatePlan {
                method: install.method,
                argv,
                hint: pkg,
            },
        );
    }
    plans
}

/// Güncelleme argv doğrulaması (install’tan biraz daha geniş fiiller).
pub fn validate_upgrade_command(method: InstallMethod, argv: &[String]) -> Result<(), String> {
    if argv.is_empty() {
        return Err("upgrade argv boş".into());
    }
    let program = argv[0].as_str();
    match method {
        InstallMethod::Pipx
            if program == "pipx" && argv.get(1).map(String::as_str) == Some("upgrade") =>
        {
            Ok(())
        }
        InstallMethod::Uv => validate_install_command(method, argv),
        InstallMethod::Npm
            if program == "npm" && argv.get(1).map(String::as_str) == Some("update") =>
        {
            Ok(())
        }
        InstallMethod::Brew
            if program == "brew" && argv.get(1).map(String::as_str) == Some("upgrade") =>
        {
            Ok(())
        }
        InstallMethod::Apt => {
            let (prog, rest) = if program == "sudo" {
                (argv.get(1).map(String::as_str), &argv[2..])
            } else {
                (Some(program), &argv[1..])
            };
            if prog != Some("apt") {
                return Err("apt bekleniyor".into());
            }
            if rest.first().map(String::as_str) != Some("install") {
                return Err("apt install bekleniyor".into());
            }
            if !rest.iter().any(|a| a == "--only-upgrade") {
                return Err("--only-upgrade bekleniyor".into());
            }
            Ok(())
        }
        InstallMethod::Dnf => {
            let (prog, rest) = if program == "sudo" {
                (argv.get(1).map(String::as_str), &argv[2..])
            } else {
                (Some(program), &argv[1..])
            };
            if prog != Some("dnf") {
                return Err("dnf bekleniyor".into());
            }
            if rest.first().map(String::as_str) != Some("upgrade") {
                return Err("dnf upgrade bekleniyor".into());
            }
            Ok(())
        }
        InstallMethod::Cargo if program == "cargo" => validate_install_command(method, argv),
        _ => Err(format!(
            "'{}' için upgrade argv geçersiz: {}",
            method.short_label(),
            format_install_command(argv)
        )),
    }
}

fn package_name_for_method(method: InstallMethod, argv: &[String]) -> Option<String> {
    match method {
        InstallMethod::Apt | InstallMethod::Dnf => sys_package_name(argv),
        InstallMethod::Brew => package_token(argv, "install"),
        InstallMethod::Pipx => package_token(argv, "install"),
        InstallMethod::Uv => uv_package_name(argv),
        InstallMethod::Npm => package_token(argv, "install").or_else(|| package_token(argv, "i")),
        InstallMethod::Cargo => cargo_package_name(argv),
        InstallMethod::Manual | InstallMethod::DownloadArchive => None,
    }
}

fn sys_package_name(argv: &[String]) -> Option<String> {
    // Son konumsal argüman paket adıdır (-y / --yes / --only-upgrade atlanır).
    argv.iter()
        .rev()
        .find(|a| {
            !a.starts_with('-')
                && !matches!(
                    a.as_str(),
                    "apt" | "dnf" | "sudo" | "install" | "upgrade" | "update"
                )
        })
        .cloned()
}

fn uv_package_name(argv: &[String]) -> Option<String> {
    // uv tool install|upgrade [flags…] PKG
    let mut seen_tool = false;
    let mut after_verb = false;
    for a in argv {
        if !seen_tool {
            if a == "tool" {
                seen_tool = true;
            }
            continue;
        }
        if !after_verb {
            if a == "install" || a == "upgrade" {
                after_verb = true;
            }
            continue;
        }
        if a.starts_with('-') {
            continue;
        }
        return Some(a.clone());
    }
    None
}

fn cargo_package_name(argv: &[String]) -> Option<String> {
    let mut after_install = false;
    for a in argv {
        if !after_install {
            if a == "install" {
                after_install = true;
            }
            continue;
        }
        if a.starts_with('-') {
            continue;
        }
        return Some(a.clone());
    }
    None
}

fn package_token(argv: &[String], verb: &str) -> Option<String> {
    let mut after = false;
    for a in argv {
        if !after {
            if a == verb {
                after = true;
            }
            continue;
        }
        if a.starts_with('-') {
            continue;
        }
        return Some(a.clone());
    }
    None
}

fn ensure_sys_yes(argv: &mut Vec<String>, verb: &str) {
    let has_yes = argv
        .iter()
        .any(|a| a == "-y" || a == "--yes" || a == "--assumeyes");
    if has_yes {
        return;
    }
    if let Some(pos) = argv.iter().position(|a| a == verb) {
        argv.insert(pos + 1, "-y".into());
    }
}

fn split_command_line(cmd: &str) -> Vec<String> {
    // Katalog notları tırnaksız basit argv kullanır; boşlukla bölmek yeterli.
    cmd.split_whitespace()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .collect()
}

fn run_capture(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    // apt list / dnf check-update bazen exit 0 dışı döner; stdout’u yine de oku.
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `apt list --upgradable` çıktısından paket adları.
pub fn parse_apt_upgradable(text: &str) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Listing") {
            continue;
        }
        // htop/noble 3.3.0-1 [upgradable from: …]
        if let Some(name) = line.split('/').next() {
            let name = name.trim();
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_graphic()) {
                set.insert(name.to_string());
            }
        }
    }
    set
}

/// `dnf check-update -q` çıktısından paket adları (`htop.x86_64 …` → `htop`).
pub fn parse_dnf_check_update(text: &str) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Last metadata") || line.starts_with("Security:") {
            continue;
        }
        let Some(first) = line.split_whitespace().next() else {
            continue;
        };
        // name.arch veya name
        let name = first.split('.').next().unwrap_or(first).trim();
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '+')
        {
            set.insert(name.to_string());
        }
    }
    set
}

/// `brew outdated --quiet` satırları.
pub fn parse_brew_outdated(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// `npm outdated -g --parseable`: `path:wanted:current:latest[:depType]`
pub fn parse_npm_outdated(text: &str) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Son path bileşeni paket adıdır; scoped (@a/b) için path’ten çıkar.
        let path = line.split(':').next().unwrap_or("");
        let name = path
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        // Scoped: …/node_modules/@scope/pkg
        if path.contains("/@") {
            let parts: Vec<_> = path.split('/').collect();
            if let Some(i) = parts.iter().rposition(|p| p.starts_with('@')) {
                if let Some(pkg) = parts.get(i + 1) {
                    set.insert(format!("{}/{}", parts[i], pkg));
                    continue;
                }
            }
        }
        if let Some(n) = name {
            set.insert(n);
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pm_entry(id: &str, method: InstallMethod, argv: &[&str], notes: &str) -> StoreEntry {
        StoreEntry {
            id: id.into(),
            name: id.into(),
            description: "t".into(),
            category: "Sistem".into(),
            source: "https://example.org".into(),
            license: "MIT".into(),
            executable: id.into(),
            install_method: method,
            supported_platforms: vec!["linux-x64".into()],
            checksum: None,
            checksum_type: None,
            version: None,
            download_url: None,
            install_command: Some(argv.iter().map(|s| (*s).to_string()).collect()),
            install_notes: Some(notes.into()),
            release: None,
        }
    }

    #[test]
    fn parse_notes_skips_unsupported_and_comments() {
        let notes = "\
apt: sudo apt install bat  # bazen batcat
brew: brew install bat
cargo: cargo install bat
dnf: sudo dnf install bat
uv: uv tool install bat
curl: curl -fsSL https://x | bash
Resmî: https://example.org/
go: go install example@latest
";
        let opts = parse_notes_options(notes);
        let methods: Vec<_> = opts.iter().map(|p| p.method).collect();
        assert!(methods.contains(&InstallMethod::Apt));
        assert!(methods.contains(&InstallMethod::Brew));
        assert!(methods.contains(&InstallMethod::Cargo));
        assert!(methods.contains(&InstallMethod::Dnf));
        assert!(methods.contains(&InstallMethod::Uv));
        assert_eq!(opts.len(), 5);
        let apt = opts
            .iter()
            .find(|p| p.method == InstallMethod::Apt)
            .unwrap();
        assert!(apt.argv.iter().any(|a| a == "-y"));
        assert_eq!(apt.argv.last().map(String::as_str), Some("bat"));
        let dnf = opts
            .iter()
            .find(|p| p.method == InstallMethod::Dnf)
            .unwrap();
        assert!(dnf.argv.iter().any(|a| a == "-y"));
        let uv = opts.iter().find(|p| p.method == InstallMethod::Uv).unwrap();
        assert_eq!(uv.argv, vec!["uv", "tool", "install", "bat"]);
    }

    #[test]
    fn linux_prefers_apt_when_available() {
        let entry = pm_entry(
            "htop",
            InstallMethod::Brew,
            &["brew", "install", "htop"],
            "apt: sudo apt install htop\nbrew: brew install htop\n",
        );
        let host = HostTools::from_flags(true, true, false, false, false);
        let plan = resolve_install_plan(&entry, &host, HostOs::Linux).unwrap();
        assert_eq!(plan.method, InstallMethod::Apt);
        assert_eq!(plan.argv[0], "sudo");
    }

    #[test]
    fn linux_prefers_dnf_when_apt_missing() {
        let entry = pm_entry(
            "htop",
            InstallMethod::Brew,
            &["brew", "install", "htop"],
            "apt: sudo apt install htop\ndnf: sudo dnf install htop\nbrew: brew install htop\n",
        );
        let host = HostTools::from_flags(false, true, false, false, false).with_dnf(true);
        let plan = resolve_install_plan(&entry, &host, HostOs::Linux).unwrap();
        assert_eq!(plan.method, InstallMethod::Dnf);
        assert_eq!(plan.argv[1], "dnf");
        assert!(plan.argv.iter().any(|a| a == "-y"));
    }

    #[test]
    fn linux_prefers_uv_over_npm_when_pipx_missing() {
        let entry = pm_entry(
            "llm",
            InstallMethod::Pipx,
            &["pipx", "install", "llm"],
            "pipx: pipx install llm\nuv: uv tool install llm\nnpm: npm install -g llm\n",
        );
        let host = HostTools::from_flags(false, false, false, true, false).with_uv(true);
        let plan = resolve_install_plan(&entry, &host, HostOs::Linux).unwrap();
        assert_eq!(plan.method, InstallMethod::Uv);
        assert_eq!(plan.argv, vec!["uv", "tool", "install", "llm"]);
    }

    #[test]
    fn linux_falls_back_to_cargo_without_apt() {
        let entry = pm_entry(
            "gitui",
            InstallMethod::Apt,
            &["sudo", "apt", "install", "-y", "gitui"],
            "cargo: cargo install gitui\nbrew: brew install gitui\napt: sudo apt install gitui\n",
        );
        let host = HostTools::from_flags(false, false, false, false, true);
        let plan = resolve_install_plan(&entry, &host, HostOs::Linux).unwrap();
        assert_eq!(plan.method, InstallMethod::Cargo);
        assert_eq!(plan.argv, vec!["cargo", "install", "gitui"]);
    }

    #[test]
    fn macos_prefers_brew() {
        let entry = pm_entry(
            "htop",
            InstallMethod::Apt,
            &["sudo", "apt", "install", "-y", "htop"],
            "apt: sudo apt install htop\nbrew: brew install htop\n",
        );
        let host = HostTools::from_flags(true, true, false, false, false);
        let plan = resolve_install_plan(&entry, &host, HostOs::MacOs).unwrap();
        assert_eq!(plan.method, InstallMethod::Brew);
    }

    #[test]
    fn no_host_pm_returns_catalog_primary() {
        let entry = pm_entry(
            "crush",
            InstallMethod::Brew,
            &["brew", "install", "charmbracelet/tap/crush"],
            "brew: brew install charmbracelet/tap/crush\ngo: go install x@latest\n",
        );
        let host = HostTools::from_flags(false, false, false, false, false);
        let plan = resolve_install_plan(&entry, &host, HostOs::Linux).unwrap();
        assert_eq!(plan.method, InstallMethod::Brew);
    }

    #[test]
    fn upgrade_argv_transforms() {
        assert_eq!(
            upgrade_argv(
                InstallMethod::Pipx,
                &["pipx".into(), "install".into(), "aider-chat".into()]
            ),
            Some(vec!["pipx".into(), "upgrade".into(), "aider-chat".into()])
        );
        assert_eq!(
            upgrade_argv(
                InstallMethod::Uv,
                &["uv".into(), "tool".into(), "install".into(), "llm".into()]
            ),
            Some(vec![
                "uv".into(),
                "tool".into(),
                "upgrade".into(),
                "llm".into()
            ])
        );
        assert_eq!(
            upgrade_argv(
                InstallMethod::Npm,
                &["npm".into(), "i".into(), "-g".into(), "@a/b".into()]
            ),
            Some(vec![
                "npm".into(),
                "update".into(),
                "-g".into(),
                "@a/b".into()
            ])
        );
        assert_eq!(
            upgrade_argv(
                InstallMethod::Brew,
                &["brew".into(), "install".into(), "htop".into()]
            ),
            Some(vec!["brew".into(), "upgrade".into(), "htop".into()])
        );
        assert_eq!(
            upgrade_argv(
                InstallMethod::Apt,
                &[
                    "sudo".into(),
                    "apt".into(),
                    "install".into(),
                    "-y".into(),
                    "htop".into()
                ]
            ),
            Some(vec![
                "sudo".into(),
                "apt".into(),
                "install".into(),
                "--only-upgrade".into(),
                "-y".into(),
                "htop".into()
            ])
        );
        assert_eq!(
            upgrade_argv(
                InstallMethod::Dnf,
                &[
                    "sudo".into(),
                    "dnf".into(),
                    "install".into(),
                    "-y".into(),
                    "htop".into()
                ]
            ),
            Some(vec![
                "sudo".into(),
                "dnf".into(),
                "upgrade".into(),
                "-y".into(),
                "htop".into()
            ])
        );
        let cargo = upgrade_argv(
            InstallMethod::Cargo,
            &[
                "cargo".into(),
                "install".into(),
                "--locked".into(),
                "yazi-fm".into(),
            ],
        )
        .unwrap();
        assert!(cargo.iter().any(|a| a == "--force"));
        assert!(cargo.iter().any(|a| a == "yazi-fm"));
    }

    #[test]
    fn parse_outdated_outputs() {
        let apt = parse_apt_upgradable(
            "Listing...\nhtop/noble 3.3.0-1 amd64 [upgradable from: 3.2.0-1]\nfd-find/noble 1 [upgradable from: 0]\n",
        );
        assert!(apt.contains("htop"));
        assert!(apt.contains("fd-find"));

        let dnf = parse_dnf_check_update(
            "htop.x86_64                  3.3.0-1.fc40           updates\nbat.x86_64 1.0-1 updates\n",
        );
        assert!(dnf.contains("htop"));
        assert!(dnf.contains("bat"));

        let brew = parse_brew_outdated("htop\nbat\n");
        assert!(brew.contains("htop") && brew.contains("bat"));

        let npm = parse_npm_outdated(
            "/usr/lib/node_modules/fx:3.0.0:2.0.0:3.0.0\n/usr/lib/node_modules/@openai/codex:1.2.0:1.0.0:1.2.0\n",
        );
        assert!(npm.contains("fx"));
        assert!(npm.contains("@openai/codex"));
    }

    #[test]
    fn collect_pm_updates_respects_outdated_and_installed() {
        let htop = pm_entry(
            "htop",
            InstallMethod::Apt,
            &["sudo", "apt", "install", "-y", "htop"],
            "apt: sudo apt install htop\nbrew: brew install htop\n",
        );
        let fx = pm_entry(
            "fx",
            InstallMethod::Npm,
            &["npm", "install", "-g", "fx"],
            "npm: npm install -g fx\n",
        );
        let host = HostTools::from_flags(true, false, false, true, false);
        let mut outdated = OutdatedPackages::empty();
        outdated.apt.insert("htop".into());
        // fx outdated değil → plan yok
        let installed = BTreeSet::from(["htop".into(), "fx".into()]);
        let plans = collect_pm_updates(&[htop, fx], &installed, &host, HostOs::Linux, &outdated);
        assert!(plans.contains_key("htop"));
        assert!(!plans.contains_key("fx"));
        assert_eq!(
            plans["htop"].argv,
            vec!["sudo", "apt", "install", "--only-upgrade", "-y", "htop"]
        );
    }

    #[test]
    fn collect_pm_updates_dnf_and_optimistic_uv() {
        let htop = pm_entry(
            "htop",
            InstallMethod::Brew,
            &["brew", "install", "htop"],
            "dnf: sudo dnf install htop\nbrew: brew install htop\n",
        );
        let llm = pm_entry(
            "llm",
            InstallMethod::Pipx,
            &["pipx", "install", "llm"],
            "uv: uv tool install llm\npipx: pipx install llm\n",
        );
        let host = HostTools::from_flags(false, false, false, false, false)
            .with_dnf(true)
            .with_uv(true);
        let mut outdated = OutdatedPackages::empty();
        outdated.dnf.insert("htop".into());
        outdated.optimistic_uv = true;
        let installed = BTreeSet::from(["htop".into(), "llm".into()]);
        let plans = collect_pm_updates(&[htop, llm], &installed, &host, HostOs::Linux, &outdated);
        assert_eq!(
            plans["htop"].argv,
            vec!["sudo", "dnf", "upgrade", "-y", "htop"]
        );
        assert_eq!(plans["llm"].argv, vec!["uv", "tool", "upgrade", "llm"]);
    }

    #[test]
    fn collect_pm_updates_optimistic_pipx_when_enabled() {
        let aider = pm_entry(
            "aider",
            InstallMethod::Pipx,
            &["pipx", "install", "aider-chat"],
            "pipx: pipx install aider-chat\n",
        );
        let host = HostTools::from_flags(false, false, true, false, false);
        let mut outdated = OutdatedPackages::empty();
        outdated.optimistic_pipx = true;
        let installed = BTreeSet::from(["aider".into()]);
        let plans = collect_pm_updates(&[aider], &installed, &host, HostOs::Linux, &outdated);
        assert_eq!(plans["aider"].argv, vec!["pipx", "upgrade", "aider-chat"]);
    }

    #[test]
    fn plan_needs_tty_suspend_only_for_uncached_sudo() {
        let no_sudo = InstallPlan {
            method: InstallMethod::Apt,
            argv: vec!["apt".into(), "install".into(), "-y".into(), "htop".into()],
        };
        assert!(!no_sudo.needs_tty_suspend());

        let pipx = InstallPlan {
            method: InstallMethod::Pipx,
            argv: vec!["pipx".into(), "install".into(), "x".into()],
        };
        assert!(!pipx.needs_tty_suspend());
    }
}
