//! Mağaza kurulumlarını ana araç kataloğuna senkronize etme.
//!
//! Mağaza (`assets/store/*.toml`) ile ana katalog (`assets/catalog/*.toml`) ayrıdır. Kurulum
//! sonrası mağazada ✓ görünen bir uygulama AI vb. sol kategoride yoksa
//! kullanıcı oradan başlatamaz. Bu modül:
//!
//! 1. Aynı `id` veya `executable` ile katalogda eşleşme varsa yalnızca
//!    tespit yenilenir (zaten listelenir).
//! 2. Eşleşme yoksa mağaza kaydından kullanıcı araç girişi üretilir
//!    (`state.toml` `custom_tools`) ve ilgili katalog kategorisine eklenir.
//! 3. Kurulu detay için güvenilir proje bağlantıları ve güncelleme güveni
//!    (yalnızca kurulum kaydı / Nova-yönetimli yol / doğrulanmış paket id).

use crate::config::{LaunchKind, ToolAction, ToolEntry};
use crate::store::host::{self, HostOs, HostTools, PmUpdatePlan};
use crate::store::install::InstallFileManifest;
use crate::store::{store_app_dir, store_install_dir, InstallMethod, StoreEntry};
use std::path::Path;

/// Bilinen AI CLI'ler için varsayılan eylemler (mağazadan senkronize edilenler).
fn default_ai_actions(id: &str, executable: &str) -> Vec<ToolAction> {
    let key = if id.is_empty() { executable } else { id };
    let key = key.to_ascii_lowercase();
    let exe = executable.to_ascii_lowercase();
    let known = match key.as_str() {
        "codex" => Some(vec![
            action("open", "Aç", &[]),
            action("resume", "Devam et", &["resume"]),
            action("logout", "Çıkış yap", &["logout"]),
            action("help", "Yardım", &["--help"]),
        ]),
        "claude" => Some(vec![
            action("open", "Aç", &[]),
            action("resume", "Devam et", &["--resume"]),
            action("logout", "Çıkış yap", &["auth", "logout"]),
            action("help", "Yardım", &["--help"]),
        ]),
        "opencode" => Some(vec![
            action("open", "Aç", &[]),
            action("resume", "Devam et", &["-c"]),
            action("logout", "Çıkış yap", &["auth", "logout"]),
            action("help", "Yardım", &["--help"]),
        ]),
        "gemini-cli" | "gemini" => Some(vec![
            action("open", "Aç", &[]),
            action("resume", "Devam et", &["--resume"]),
            action("help", "Yardım", &["--help"]),
        ]),
        _ => None,
    };
    if let Some(actions) = known {
        return actions;
    }
    if exe == "gemini" {
        return vec![
            action("open", "Aç", &[]),
            action("resume", "Devam et", &["--resume"]),
            action("help", "Yardım", &["--help"]),
        ];
    }
    Vec::new()
}

fn action(id: &str, label: &str, args: &[&str]) -> ToolAction {
    ToolAction {
        id: id.into(),
        label: label.into(),
        args: args.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// Mağaza kategorisini ana katalog `CATEGORIES` adına çevirir.
pub fn catalog_category_for_store(store_category: &str) -> &'static str {
    match store_category {
        "AI" => "AI ve Kodlama",
        "Git" | "Geliştirme" | "Editörler" => "Git ve Geliştirici Araçları",
        "Dosya ve Arama" => "Dosya Yönetimi ve Arama",
        "Sistem" | "Paneller" | "Güvenlik" | "Verimlilik" => "Sistem ve Kaynak Kullanımı",
        "Ağ" | "Web" => "Ağ ve Bağlantı",
        "Konteyner" | "Veritabanı" => "Docker ve Sunucu",
        "Günlük" => "Günlük Terminal Yardımcıları",
        "Medya" | "Mesajlaşma" | "Oyunlar" | "Ekran Koruyucular" => "Özel Araçlar",
        _ => "Özel Araçlar",
    }
}

fn https_only(url: Option<&str>) -> Option<String> {
    let u = url.map(str::trim).filter(|s| !s.is_empty())?;
    if u.starts_with("https://") {
        Some(u.to_string())
    } else {
        None
    }
}

/// Mağaza kaydından kalıcı katalog `ToolEntry` üretir.
pub fn tool_entry_from_store(entry: &StoreEntry) -> ToolEntry {
    let category = catalog_category_for_store(&entry.category).to_string();
    let actions = if category == "AI ve Kodlama" {
        default_ai_actions(&entry.id, &entry.executable)
    } else {
        Vec::new()
    };
    let project_url = https_only(Some(&entry.source));
    let repo_url = entry
        .release
        .as_ref()
        .and_then(|r| crate::browser::github_repo_https_url(&r.repo))
        .or_else(|| {
            entry
                .repository_url
                .as_deref()
                .and_then(crate::store::stars::parse_verified_github_repo)
                .map(|r| r.https_url())
        });
    ToolEntry {
        id: entry.id.clone(),
        name: entry.name.clone(),
        description: entry.description.clone(),
        category,
        executable: entry.executable.clone(),
        args: Vec::new(),
        launch_kind: LaunchKind::Interactive,
        launch_mode: "terminal".into(),
        working_directory: "home".into(),
        working_dir_path: None,
        favorite: false,
        help_text: None,
        handlers: Vec::new(),
        info_text: None,
        alternative_to: None,
        examples: vec![
            format!("{}   ← {} arayüzünü açar", entry.executable, entry.name),
            format!("{} --help   ← yardım ve bayraklar", entry.executable),
            format!(
                "Nova’da Enter ile başlatın veya terminalde `{}` yazın.",
                entry.executable
            ),
        ],
        setup_info: Some(format!(
            "Mağaza’dan kuruldu ({}). Terminalde '{}' yazın; detay panelinde kullanım örneklerine bakın.",
            entry.install_method.short_label(),
            entry.executable
        )),
        actions,
        project_url,
        repo_url,
    }
}

/// Katalogda mağaza kaydıyla eşleşen giriş var mı (`id` veya `executable`).
pub fn catalog_matches_store(catalog_id: &str, catalog_exe: &str, store: &StoreEntry) -> bool {
    catalog_id == store.id || (!store.executable.is_empty() && catalog_exe == store.executable)
}

/// Mağaza eşleşme türü — güncelleme güveni için kritik.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreMatchKind {
    /// Katalog `id` == Mağaza `id`.
    ById,
    /// Yalnızca ikili adı eşleşti (id farklı). Güncelleme için tek başına yetmez.
    ByExecutable,
}

/// Kurulu araç için Mağaza kataloğu eşleşmesi (`id`, yoksa `executable`).
///
/// Önce kimlik, sonra ikili adı. Özel argv’li katalog sarmalayıcıları
/// (`cloudflare-speed-test` → `curl` gibi) ikili eşleşmesine girmez; böylece
/// Mağaza’daki asıl uygulamanın açıklaması yanlışlıkla gösterilmez.
pub fn find_store_for_catalog_tool<'a>(
    store_entries: &'a [StoreEntry],
    catalog_id: &str,
    catalog_exe: &str,
    catalog_args: &[String],
) -> Option<&'a StoreEntry> {
    find_store_match(store_entries, catalog_id, catalog_exe, catalog_args).map(|(e, _)| e)
}

/// Eşleşme + tür (id mi, yalnızca ikili mi).
pub fn find_store_match<'a>(
    store_entries: &'a [StoreEntry],
    catalog_id: &str,
    catalog_exe: &str,
    catalog_args: &[String],
) -> Option<(&'a StoreEntry, StoreMatchKind)> {
    if let Some(by_id) = store_entries.iter().find(|e| e.id == catalog_id) {
        return Some((by_id, StoreMatchKind::ById));
    }
    if !catalog_args.is_empty() {
        return None;
    }
    let exe = catalog_exe.trim();
    if exe.is_empty() {
        return None;
    }
    store_entries
        .iter()
        .find(|e| !e.executable.is_empty() && e.executable == exe)
        .map(|e| (e, StoreMatchKind::ByExecutable))
}

/// Kurulu detayda gösterilecek açıklama: Mağaza eşleşmesi varsa onun metni,
/// yoksa katalog açıklaması. Boş Mağaza metninde kataloğa düşülür; uydurma yok.
pub fn detail_description_for_tool<'a>(
    catalog_description: &'a str,
    store: Option<&'a StoreEntry>,
) -> &'a str {
    match store
        .map(|e| e.description.trim())
        .filter(|d| !d.is_empty())
    {
        Some(desc) => desc,
        None => catalog_description,
    }
}

/// Detay paneli proje / depo bağlantısı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolProjectLink {
    pub label: &'static str,
    pub url: String,
}

/// Kurulu araç için tarayıcıda açılabilir bağlantılar.
///
/// Öncelik: özel araç alanları (`project_url` / `repo_url`) → güvenilir Mağaza
/// eşleşmesinin `source` / `release.repo`. Adres **uydurulmaz**; yalnızca
/// `https://` kabul edilir.
pub fn tool_project_links(
    project_url: Option<&str>,
    repo_url: Option<&str>,
    store: Option<&StoreEntry>,
) -> Vec<ToolProjectLink> {
    let mut out = Vec::new();
    if let Some(url) = https_only(project_url) {
        out.push(ToolProjectLink {
            label: "Kaynak",
            url,
        });
    }
    if let Some(url) = https_only(repo_url) {
        if out.iter().all(|l| l.url != url) {
            out.push(ToolProjectLink { label: "Depo", url });
        }
    }
    if out.is_empty() {
        if let Some(entry) = store {
            if let Some(url) = https_only(Some(&entry.source)) {
                out.push(ToolProjectLink {
                    label: "Kaynak",
                    url,
                });
            }
            if let Some(repo) = crate::store::stars::verified_repo_for_store_entry(entry) {
                let url = repo.https_url();
                if out.iter().all(|l| l.url != url) {
                    out.push(ToolProjectLink { label: "Depo", url });
                }
            }
        }
    }
    out
}

/// Kurulu araç güncelleme güveninin kaynağı.
///
/// Ad / ikili Mağaza eşleşmesi **tek başına** yeterli değildir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateTrust {
    /// `<store>/<id>/install-manifest.toml` var (Nova arşiv kurulumu).
    InstallRecord,
    /// Çözülen ikili Nova mağaza dizininde (`store/<id>/…`).
    NovaManagedPath,
    /// Katalog id == Mağaza id ve paket adı install argv’den doğrulanabilir.
    VerifiedPkgId,
}

impl UpdateTrust {
    pub fn label(self) -> &'static str {
        match self {
            Self::InstallRecord => "kurulum kaydı",
            Self::NovaManagedPath => "Nova yolu",
            Self::VerifiedPkgId => "doğrulanmış paket",
        }
    }
}

/// Çözülen yol Nova mağaza kurulum kökü altında mı? Varsa mağaza `id`.
pub fn nova_managed_store_id(resolved_path: &Path) -> Option<String> {
    let root = store_install_dir()?;
    let root = root.canonicalize().unwrap_or(root);
    let path = resolved_path
        .canonicalize()
        .unwrap_or_else(|_| resolved_path.to_path_buf());
    let rel = path.strip_prefix(&root).ok()?;
    let mut comps = rel.components();
    let id = comps.next()?.as_os_str().to_str()?.to_string();
    if id.is_empty() || id.starts_with('.') {
        return None;
    }
    // En azından `id/…` altında bir şey olmalı (genelde `bin/exe`).
    comps.next()?;
    Some(id)
}

/// Güncelleme güveni: kurulum kaydı / Nova yolu / doğrulanmış paket id.
///
/// Yalnızca ad veya ikili Mağaza eşleşmesi → `None`.
pub fn update_trust_for_tool<'a>(
    store_entries: &'a [StoreEntry],
    catalog_id: &str,
    catalog_exe: &str,
    catalog_args: &[String],
    resolved_path: Option<&Path>,
) -> Option<(UpdateTrust, &'a StoreEntry)> {
    update_trust_for_tool_with_host(
        store_entries,
        catalog_id,
        catalog_exe,
        catalog_args,
        resolved_path,
        &HostTools::detect(),
        host::detect_host_os(),
    )
}

/// [`update_trust_for_tool`] — testlerde host/os enjekte edilebilir.
pub fn update_trust_for_tool_with_host<'a>(
    store_entries: &'a [StoreEntry],
    catalog_id: &str,
    catalog_exe: &str,
    catalog_args: &[String],
    resolved_path: Option<&Path>,
    host: &HostTools,
    os: HostOs,
) -> Option<(UpdateTrust, &'a StoreEntry)> {
    // 1) Nova-yönetimli yol: dizin yapısından id (eşleşmeden bağımsız güven).
    if let Some(path) = resolved_path {
        if let Some(managed_id) = nova_managed_store_id(path) {
            if let Some(store) = store_entries.iter().find(|e| e.id == managed_id) {
                return Some((UpdateTrust::NovaManagedPath, store));
            }
        }
    }

    let (store, kind) = find_store_match(store_entries, catalog_id, catalog_exe, catalog_args)?;

    // 2) Kurulum kaydı (arşiv manifest) — Mağaza id bilinince.
    if InstallFileManifest::load(&store.id).is_some() {
        return Some((UpdateTrust::InstallRecord, store));
    }
    // Yol store_app_dir altında ama canonicalize farklı olabilir.
    if let Some(path) = resolved_path {
        if let Some(app_dir) = store_app_dir(&store.id) {
            let app_dir = app_dir.canonicalize().unwrap_or(app_dir);
            let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            if path.starts_with(&app_dir) {
                return Some((UpdateTrust::NovaManagedPath, store));
            }
        }
    }

    // 3) Doğrulanmış paket id: id eşleşmesi + PM paket adı argv’den.
    //    Yalnızca ikili eşleşmesi (ByExecutable) burada kabul edilmez.
    if kind == StoreMatchKind::ById && store.install_method.is_package_manager() {
        if let Some(argv) = store.install_command.as_ref() {
            if host::package_name_for_method(store.install_method, argv).is_some() {
                return Some((UpdateTrust::VerifiedPkgId, store));
            }
        }
        if let Some(plan) = host::resolve_install_plan(store, host, os) {
            if host::package_name_for_method(plan.method, &plan.argv).is_some() {
                return Some((UpdateTrust::VerifiedPkgId, store));
            }
        }
    }

    None
}

/// Kurulu detayda sunulabilecek güncelleme teklifi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolUpdateOffer {
    /// Arşiv / GitHub Releases (önceden çözülmüş sürüm gerekir; probe yok).
    Archive {
        store_id: String,
        name: String,
        from_version: String,
        to_version: String,
        command: String,
    },
    /// Paket-kapsamlı PM upgrade (outdated probe yok; argv bilinen kayıttan).
    PackageManager {
        store_id: String,
        name: String,
        plan: PmUpdatePlan,
        trust: UpdateTrust,
    },
}

impl ToolUpdateOffer {
    pub fn store_id(&self) -> &str {
        match self {
            Self::Archive { store_id, .. } | Self::PackageManager { store_id, .. } => store_id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Archive { name, .. } | Self::PackageManager { name, .. } => name,
        }
    }

    pub fn full_command(&self) -> String {
        match self {
            Self::Archive { command, .. } => command.clone(),
            Self::PackageManager { plan, .. } => plan.rendered(),
        }
    }

    pub fn confirm_question(&self) -> String {
        match self {
            Self::Archive {
                name,
                from_version,
                to_version,
                command,
                ..
            } => format!(
                "'{name}' {from_version} → {to_version} güncellensin mi?\n\
                 Komut: {command}\n\
                 Enter onaylar, Esc vazgeçer."
            ),
            Self::PackageManager {
                name, plan, trust, ..
            } => format!(
                "'{name}' paket yöneticisiyle güncellensin mi? ({}, {})\n\
                 Komut: {}\n\
                 Enter onaylar, Esc vazgeçer.",
                plan.method.short_label(),
                trust.label(),
                plan.rendered()
            ),
        }
    }
}

/// Güncelleme teklifi üretir. Sürüm probe’u **çalıştırmaz**.
///
/// - Arşiv: yalnızca `pending_archive` (Mağaza `U` sonucu) verilmişse.
/// - PM: bilinen install argv → paket-kapsamlı upgrade; host/os test için
///   enjekte edilebilir.
#[allow(clippy::too_many_arguments)]
pub fn tool_update_offer(
    store_entries: &[StoreEntry],
    catalog_id: &str,
    catalog_exe: &str,
    catalog_args: &[String],
    resolved_path: Option<&Path>,
    pending_archive: Option<&crate::store::manifest::ResolvedRelease>,
    pending_pm: Option<&PmUpdatePlan>,
    host: &HostTools,
    os: HostOs,
) -> Option<ToolUpdateOffer> {
    let (trust, store) = update_trust_for_tool_with_host(
        store_entries,
        catalog_id,
        catalog_exe,
        catalog_args,
        resolved_path,
        host,
        os,
    )?;

    if store.install_method.is_archive() {
        let release = pending_archive?;
        let current = store.version.clone().unwrap_or_default();
        if !crate::store::manifest::is_newer(&release.version, &current) {
            return None;
        }
        return Some(ToolUpdateOffer::Archive {
            store_id: store.id.clone(),
            name: store.name.clone(),
            from_version: if current.is_empty() {
                "bilinmiyor".into()
            } else {
                current
            },
            to_version: release.version.clone(),
            command: format!(
                "download+verify {} → {}",
                release.download_url, release.version
            ),
        });
    }

    if store.install_method.is_package_manager() {
        let plan = pending_pm
            .cloned()
            .or_else(|| host::resolve_upgrade_plan(store, host, os))?;
        return Some(ToolUpdateOffer::PackageManager {
            store_id: store.id.clone(),
            name: store.name.clone(),
            plan,
            trust,
        });
    }

    // manual / bilinmeyen — güncelleme yok
    let _ = InstallMethod::Manual;
    None
}

/// Test yardımcısı: yolun mağaza kökü altında olup olmadığını sınar.
#[cfg(test)]
pub fn path_under_store_root(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut comps = rel.components();
    let id = comps.next()?.as_os_str().to_str()?.to_string();
    comps.next()?;
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InstallMethod;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn sample(id: &str, category: &str, exe: &str) -> StoreEntry {
        StoreEntry {
            id: id.into(),
            name: id.into(),
            description: "d".into(),
            category: category.into(),
            source: "https://example.org/".into(),
            license: "MIT".into(),
            executable: exe.into(),
            install_method: InstallMethod::Npm,
            supported_platforms: vec!["linux-x64".into()],
            version: None,
            download_url: None,
            checksum: None,
            checksum_type: None,
            artifacts: vec![],
            install_command: Some(vec![
                "npm".into(),
                "install".into(),
                "-g".into(),
                "x".into(),
            ]),
            install_notes: None,
            release: None,
            repository_url: None,
            signature: None,
        }
    }

    #[test]
    fn ai_store_maps_to_ai_catalog_category() {
        assert_eq!(catalog_category_for_store("AI"), "AI ve Kodlama");
    }

    #[test]
    fn tool_entry_keeps_store_id_and_executable() {
        let e = sample("gemini-cli", "AI", "gemini");
        let t = tool_entry_from_store(&e);
        assert_eq!(t.id, "gemini-cli");
        assert_eq!(t.executable, "gemini");
        assert_eq!(t.category, "AI ve Kodlama");
        assert_eq!(t.launch_kind, LaunchKind::Interactive);
        assert_eq!(t.project_url.as_deref(), Some("https://example.org/"));
        assert!(
            t.actions.iter().any(|a| a.id == "resume"),
            "AI mağaza senkronu bilinen aksiyonları eklemeli: {:?}",
            t.actions
        );
    }

    #[test]
    fn match_by_executable_even_when_ids_differ() {
        let store = sample("qwen-code", "AI", "qwen");
        assert!(catalog_matches_store("other-id", "qwen", &store));
        assert!(!catalog_matches_store("other", "other-exe", &store));
        assert!(catalog_matches_store("qwen-code", "anything", &store));
    }

    #[test]
    fn find_store_prefers_id_then_executable() {
        let entries = vec![
            sample("bottom", "Sistem", "btm"),
            sample("curl", "Ağ", "curl"),
        ];
        let by_id = find_store_for_catalog_tool(&entries, "bottom", "btm", &[]).unwrap();
        assert_eq!(by_id.id, "bottom");

        let by_exe = find_store_for_catalog_tool(&entries, "btm", "btm", &[]).unwrap();
        assert_eq!(by_exe.id, "bottom");
        assert_eq!(by_exe.executable, "btm");

        let (_, kind) = find_store_match(&entries, "btm", "btm", &[]).unwrap();
        assert_eq!(kind, StoreMatchKind::ByExecutable);
        let (_, kind) = find_store_match(&entries, "bottom", "btm", &[]).unwrap();
        assert_eq!(kind, StoreMatchKind::ById);
    }

    #[test]
    fn find_store_skips_executable_match_when_catalog_has_args() {
        let entries = vec![sample("curl", "Ağ", "curl")];
        let args = vec!["-fsSL".into(), "https://example.com".into()];
        assert!(
            find_store_for_catalog_tool(&entries, "cloudflare-speed-test", "curl", &args).is_none(),
            "özel argv’li sarmalayıcı curl Mağaza kaydına bağlanmamalı"
        );
    }

    #[test]
    fn detail_description_uses_store_when_present() {
        let mut entry = sample("fzf", "Dosya", "fzf");
        entry.description = "Mağaza uzun açıklaması.".into();
        assert_eq!(
            detail_description_for_tool("katalog kısa", Some(&entry)),
            "Mağaza uzun açıklaması."
        );
        assert_eq!(
            detail_description_for_tool("katalog kısa", None),
            "katalog kısa"
        );
        entry.description = "   ".into();
        assert_eq!(
            detail_description_for_tool("katalog kısa", Some(&entry)),
            "katalog kısa"
        );
    }

    #[test]
    fn project_links_prefer_custom_then_store_https_only() {
        let mut store = sample("fx", "Ağ", "fx");
        store.source = "https://github.com/antonmedv/fx".into();
        store.release = Some(crate::store::manifest::ReleaseSource {
            repo: "antonmedv/fx".into(),
            tag_prefix: Some("v".into()),
            asset_pattern: "fx-{version}.tar.gz".into(),
            checksum_asset_pattern: None,
        });

        let custom = tool_project_links(
            Some("https://example.com/my-tool"),
            Some("https://github.com/me/my-tool"),
            Some(&store),
        );
        assert_eq!(custom.len(), 2);
        assert_eq!(custom[0].url, "https://example.com/my-tool");
        assert_eq!(custom[1].label, "Depo");

        let from_store = tool_project_links(None, None, Some(&store));
        assert_eq!(from_store[0].url, "https://github.com/antonmedv/fx");
        // source ile release.repo aynı HTTPS adrese çözülünce tek satır kalır.
        assert_eq!(from_store.len(), 1);

        store.source = "https://fx.example.org/".into();
        let distinct = tool_project_links(None, None, Some(&store));
        assert_eq!(distinct.len(), 2);
        assert!(distinct.iter().any(|l| l.label == "Depo"));

        assert!(tool_project_links(Some("http://insecure"), None, None).is_empty());
        assert!(tool_project_links(None, None, None).is_empty());
    }

    #[test]
    fn update_trust_rejects_exe_only_magaza_match() {
        let mut bottom = sample("bottom", "Sistem", "btm");
        bottom.install_method = InstallMethod::Cargo;
        bottom.install_command = Some(vec!["cargo".into(), "install".into(), "bottom".into()]);
        let entries = vec![bottom];
        // Katalog id ≠ mağaza id; yalnızca ikili eşleşmesi → güncelleme yok.
        assert!(
            update_trust_for_tool(&entries, "btm", "btm", &[], None).is_none(),
            "yalnızca ikili Mağaza eşleşmesi Update vermemeli"
        );
    }

    #[test]
    fn update_trust_accepts_verified_pkg_id_match() {
        let mut htop = sample("htop", "Sistem", "htop");
        htop.install_method = InstallMethod::Apt;
        htop.install_command = Some(vec![
            "sudo".into(),
            "apt".into(),
            "install".into(),
            "-y".into(),
            "htop".into(),
        ]);
        let entries = vec![htop];
        let host = HostTools::from_flags(true, false, false, false, false);
        let (trust, store) = update_trust_for_tool_with_host(
            &entries,
            "htop",
            "htop",
            &[],
            None,
            &host,
            HostOs::Linux,
        )
        .expect("id eşleşmesi + apt paket adı → VerifiedPkgId");
        assert_eq!(trust, UpdateTrust::VerifiedPkgId);
        assert_eq!(store.id, "htop");

        let offer = tool_update_offer(
            &entries,
            "htop",
            "htop",
            &[],
            None,
            None,
            None,
            &host,
            HostOs::Linux,
        )
        .expect("PM upgrade planı üretilmeli");
        assert!(matches!(offer, ToolUpdateOffer::PackageManager { .. }));
        assert!(offer.full_command().contains("htop"));
        assert!(offer.confirm_question().contains("Komut:"));
        let _ = BTreeMap::<String, String>::new();
    }

    #[test]
    fn path_under_store_identifies_app_id() {
        let root = PathBuf::from("/data/nova/store");
        let path = root.join("lazygit").join("bin").join("lazygit");
        assert_eq!(
            path_under_store_root(&path, &root).as_deref(),
            Some("lazygit")
        );
        assert!(path_under_store_root(&root.join("lazygit"), &root).is_none());
    }

    #[test]
    fn tool_update_offer_archive_needs_pending_release() {
        let mut entry = sample("yazi", "Dosya", "yazi");
        entry.install_method = InstallMethod::DownloadArchive;
        entry.version = Some("0.1.0".into());
        entry.download_url = Some("https://example.org/yazi.tar.gz".into());
        entry.checksum = Some("a".repeat(64));
        entry.install_command = None;
        // Kurulum kaydı yok → trust yok → offer yok (exe match yetmez).
        let host = HostTools::from_flags(false, false, false, false, false);
        let offer = tool_update_offer(
            &[entry.clone()],
            "other",
            "yazi",
            &[],
            None,
            Some(&crate::store::manifest::ResolvedRelease {
                version: "0.2.0".into(),
                download_url: "https://example.org/yazi-0.2.tar.gz".into(),
                checksum: "b".repeat(64),
                checksum_url: "https://example.org/SHA256".into(),
            }),
            None,
            &host,
            HostOs::Linux,
        );
        assert!(offer.is_none(), "exe-only + archive pending → Update yok");
    }

    #[test]
    fn tool_update_offer_pm_prefers_pending_plan_command() {
        let mut htop = sample("htop", "Sistem", "htop");
        htop.install_method = InstallMethod::Apt;
        htop.install_command = Some(vec![
            "sudo".into(),
            "apt".into(),
            "install".into(),
            "-y".into(),
            "htop".into(),
        ]);
        let plan = PmUpdatePlan {
            method: InstallMethod::Apt,
            argv: vec![
                "sudo".into(),
                "apt".into(),
                "install".into(),
                "--only-upgrade".into(),
                "-y".into(),
                "htop".into(),
            ],
            hint: "htop".into(),
        };
        let host = HostTools::from_flags(true, false, false, false, false);
        let offer = tool_update_offer(
            &[htop],
            "htop",
            "htop",
            &[],
            None,
            None,
            Some(&plan),
            &host,
            HostOs::Linux,
        )
        .expect("verified pkg id + PM plan → Update");
        assert_eq!(offer.store_id(), "htop");
        assert_eq!(
            offer.full_command(),
            "sudo apt install --only-upgrade -y htop"
        );
    }

    #[test]
    fn resolve_upgrade_plan_is_package_scoped() {
        let mut entry = sample("fx", "Ağ", "fx");
        entry.install_method = InstallMethod::Npm;
        entry.install_command = Some(vec![
            "npm".into(),
            "install".into(),
            "-g".into(),
            "fx".into(),
        ]);
        let host = HostTools::from_flags(false, false, false, true, false);
        let plan = host::resolve_upgrade_plan(&entry, &host, HostOs::Linux).unwrap();
        assert_eq!(plan.argv, vec!["npm", "update", "-g", "fx"]);
        assert_eq!(plan.hint, "fx");
    }
}
