//! Katalog yükleyici.
//!
//! İki kaynağı birleştirir:
//!
//! 1. Uygulamayla **gömülü** varsayılan katalog (`assets/catalog/*.toml`,
//!    kategoriye göre ayrılmış). Güncellemede değişebilir; kullanıcı bunu
//!    düzenlemez.
//! 2. Kullanıcı yapılandırması (`$XDG_CONFIG_HOME/nova/config.toml`, yoksa
//!    `~/.config/nova/config.toml`): yalnızca
//!    favoriler, kimliğe göre geçersiz kılmalar ve kullanıcının eklediği
//!    araçlar.
//!
//! Birleştirme **kimliğe (`id`) göre** yapılır. Bu sayede gömülü katalog
//! güncellendiğinde kullanıcının favori işaretleri ve eklediği kendi araçları
//! korunur.
//!
//! Yükleme sırasında hiçbir araç **çalıştırılmaz**: yalnızca dosyalar okunur,
//! alanlar doğrulanır ve `PATH` üzerinde dosya aranır.

use crate::catalog::{
    catalog_entry_from_tool, working_directory_from, Catalog, CatalogEntry, WorkingDirChoice,
    CATEGORIES,
};
use crate::config::{CatalogFile, EntryOrigin, LaunchKind, ToolEntry, UserConfig};
use std::path::Path;

/// Gömülü katalog kaynakları: `(görünen ad, içerik)`.
///
/// Sıra [`CATEGORIES`] ile uyumludur; her dosya bir kategoriye karşılık gelir.
pub const BUILTIN_CATALOG_SOURCES: &[(&str, &str)] = &[
    (
        "assets/catalog/ai.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/catalog/ai.toml"
        )),
    ),
    (
        "assets/catalog/git.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/catalog/git.toml"
        )),
    ),
    (
        "assets/catalog/files.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/catalog/files.toml"
        )),
    ),
    (
        "assets/catalog/system.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/catalog/system.toml"
        )),
    ),
    (
        "assets/catalog/network.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/catalog/network.toml"
        )),
    ),
    (
        "assets/catalog/docker.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/catalog/docker.toml"
        )),
    ),
    (
        "assets/catalog/helpers.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/catalog/helpers.toml"
        )),
    ),
    (
        "assets/catalog/custom.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/catalog/custom.toml"
        )),
    ),
];

/// Gömülü katalogun hata mesajlarında görünen ortak adı.
pub const BUILTIN_NAME: &str = "assets/catalog/*.toml (gömülü)";

/// Bir katalog kaynağını (dosya içeriği) ayrıştırır ve doğrular.
///
/// `name` yalnızca hata mesajlarında kullanılır. TOML sözdizimi hatası,
/// dosya adını ve satır/sütun numarasını içerir; böylece kullanıcı hatanın
/// yerini bulabilir.
///
/// `require_nonempty`: `true` ise hiç araç yoksa hata döner. Kategoriye göre
/// ayrılmış gömülü dosyalarda (ör. boş `custom.toml`) `false` kullanılır;
/// birleşik katalog yine boş olamaz.
fn parse_source(
    name: &str,
    text: &str,
    require_nonempty: bool,
) -> Result<Vec<CatalogEntry>, String> {
    let file: CatalogFile =
        toml::from_str(text).map_err(|e| format!("'{name}' geçerli TOML değil: {e}"))?;
    let mut entries = Vec::with_capacity(file.tools.len());
    for tool in &file.tools {
        let entry = catalog_entry_from_tool(tool)
            .map_err(|e| format!("'{name}' içindeki kayıt geçersiz: {e}"))?;
        entries.push(entry);
    }
    if require_nonempty && entries.is_empty() {
        return Err(format!(
            "'{name}' hiç araç içermiyor; en az bir araç gerekli"
        ));
    }
    Ok(entries)
}

/// Gömülü varsayılan kataloğu ayrıştırır.
///
/// Hata ancak uygulama paketlenirken gömülü dosya bozulursa oluşur; yine de
/// hata sessizce yutulmaz.
pub fn builtin_catalog() -> Result<Vec<CatalogEntry>, String> {
    let mut entries = Vec::new();
    for (name, text) in BUILTIN_CATALOG_SOURCES {
        entries.extend(parse_source(name, text, false)?);
    }
    if entries.is_empty() {
        return Err(format!(
            "'{BUILTIN_NAME}' hiç araç içermiyor; en az bir araç gerekli"
        ));
    }
    Ok(entries)
}

/// Birleştirilmiş tek bir girişin tutarlılığını kontrol eder.
fn validate_merged(entry: &CatalogEntry) -> Result<(), String> {
    if !CATEGORIES.contains(&entry.category.as_str()) {
        return Err(format!(
            "'{}' için bilinmeyen kategori '{}'; geçerli kategoriler: {}",
            entry.id,
            entry.category,
            CATEGORIES.join(", ")
        ));
    }
    if entry.launch_kind.runs_program() && entry.executable.contains('/') {
        return Err(format!(
            "'{}' için 'executable' yalnızca program adı olabilir (yol değil): '{}'",
            entry.id, entry.executable
        ));
    }
    if entry.launch_kind == LaunchKind::CommandHelp
        && entry.help_text.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(format!(
            "'{}' launch_kind = \"command_help\" ama 'help_text' boş",
            entry.id
        ));
    }
    Ok(())
}

/// Kullanıcı kaydını gömülü bir girişe uygular.
///
/// `CatalogEntry::merge_user` boş alanları "kullanıcı dokunmadı" sayar; burada
/// ilk olarak kullanıcı kaydının kendi içinde tutarlı olduğu doğrulanır:
/// `custom`/`project` yol olmadan verilmişse sessizce yoksayılmaz, açık hata
/// döner. Bilinmeyen `launch_mode` değerleri de birleştirme öncesi reddedilir.
fn apply_user(base: &mut CatalogEntry, tool: &ToolEntry) -> Result<(), String> {
    if let WorkingDirChoice::Invalid(err) = working_directory_from(tool) {
        return Err(err);
    }
    base.merge_user(tool)?;
    Ok(())
}

/// Bir kullanıcı yapılandırmasını gömülü katalogla birleştirir.
///
/// - Aynı `id`ye sahip kayıt varsa alanlar birleştirilir (bkz.
///   [`CatalogEntry::merge_user`]); giriş gömülü kalır.
/// - Yeni `id` ise kayıt kullanıcının aracı olarak eklenir.
pub fn merge_with_user(
    mut builtin: Vec<CatalogEntry>,
    user: &UserConfig,
) -> Result<Catalog, String> {
    let mut custom: Vec<CatalogEntry> = Vec::new();
    for tool in &user.tools {
        // Tamamen boş bir kayıt (`[[tools]]` başlığı altında hiçbir alan)
        // yazım hatası sayılmaz, yok sayılır.
        if tool.id.trim().is_empty()
            && tool.name.is_empty()
            && tool.executable.is_empty()
            && tool.category.is_empty()
        {
            continue;
        }
        if let Some(existing) = builtin.iter_mut().find(|e| e.id == tool.id) {
            apply_user(existing, tool)?;
            validate_merged(existing)?;
            continue;
        }
        let mut entry = catalog_entry_from_tool(tool)
            .map_err(|e| format!("kullanıcı yapılandırmasındaki kayıt geçersiz: {e}"))?;
        entry.origin = EntryOrigin::User;
        custom.push(entry);
    }

    builtin.extend(custom);
    Catalog::from_entries(builtin).map_err(|e| e.to_string())
}

/// Kullanıcı yapılandırma dosyasını okuyup gömülü katalogla birleştirir.
///
/// Verilen yol yoksa yalnızca gömülü katalog kullanılır (bu bir hata değil).
/// Dosya varsa ama bozuksa açık bir hata döner ve **uygulama açılmaz** —
/// kullanıcının yapılandırması sessizce yok sayılmamalıdır.
pub fn load(user_config_path: Option<&Path>) -> Result<Catalog, String> {
    let entries = builtin_catalog()?;
    load_with_builtin(entries, user_config_path)
}

/// [`load`] gibi çalışır ama varsayılan katalog dışarıdan verilir.
///
/// Testlerin kullanıcının gerçek dosyasına ve gömülü içeriğe bağlı kalmadan
/// kullanıcı dosyası davranışını sınamasını sağlar.
pub fn load_with_builtin(
    entries: Vec<CatalogEntry>,
    user_config_path: Option<&Path>,
) -> Result<Catalog, String> {
    let user = match user_config_path {
        // Dosya yoksa yalnızca varsayılanlar geçerlidir.
        Some(path) if path.exists() => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("'{}' okunamadı: {e}", path.display()))?;
            let name = path.display().to_string();
            toml::from_str::<UserConfig>(&text)
                .map_err(|e| format!("'{name}' geçerli TOML değil: {e}"))?
        }
        _ => UserConfig::default(),
    };
    // Boş bir kullanıcı dosyası (yalnızca `[ui]` gibi) bir hata değildir:
    // kullanıcı henüz araç eklememiş olabilir. Katılacak araç yoksa hata
    // üretmemek için birleştirme atlanır.
    if user.tools.is_empty() {
        return Catalog::from_entries(entries).map_err(|e| e.to_string());
    }
    merge_with_user(entries, &user)
}

/// Gömülü katalog ve kullanıcı yapılandırma yolundan katalog yükler.
///
/// `$HOME` yoksa yalnızca gömülü katalog kullanılır: kullanıcı dosyası
/// aranamaz, bu da bir hata değildir (kurulumlar `HOME` olmadan da
/// çalışabilmeli).
pub fn load_default() -> Result<Catalog, String> {
    let path = crate::config::user_config_path();
    load(path.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::InstalledStatus;

    const MINI: &str = r#"
[[tools]]
id = "alpha"
name = "Alpha"
category = "Sistem ve Kaynak Kullanımı"
executable = "alpha-xyz-olmayan"
description = "test"
"#;

    fn user_tool(id: &str) -> ToolEntry {
        ToolEntry {
            id: id.into(),
            name: "Yeni".into(),
            category: "Özel Araçlar".into(),
            ..ToolEntry::default()
        }
    }

    fn parse_one(name: &str, text: &str) -> Result<Vec<CatalogEntry>, String> {
        parse_source(name, text, true)
    }

    #[test]
    fn builtin_catalog_loads_and_uses_known_categories() {
        let entries = builtin_catalog().expect("gömülü katalog okunmalı");
        assert!(!entries.is_empty());
        for entry in &entries {
            assert!(
                CATEGORIES.contains(&entry.category.as_str()),
                "{} kategorisi geçersiz: {}",
                entry.id,
                entry.category
            );
        }
    }

    #[test]
    fn builtin_catalog_sources_cover_every_category_file() {
        assert_eq!(
            BUILTIN_CATALOG_SOURCES.len(),
            8,
            "her kategori dosyası loader listesinde olmalı"
        );
        for (name, text) in BUILTIN_CATALOG_SOURCES {
            assert!(name.starts_with("assets/catalog/"), "{name}");
            assert!(name.ends_with(".toml"), "{name}");
            // Her dosya geçerli TOML olmalı (boş tools kabul).
            parse_source(name, text, false).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }

    #[test]
    fn builtin_catalog_has_no_zoxide_or_httpie() {
        let entries = builtin_catalog().unwrap();
        for entry in &entries {
            let haystack = format!("{} {} {}", entry.id, entry.name, entry.executable);
            let lowered = haystack.to_lowercase();
            assert!(!lowered.contains("zoxide"), "zoxide olmamalı: {haystack}");
            assert!(!lowered.contains("httpie"), "httpie olmamalı: {haystack}");
            assert_ne!(entry.executable, "http", "{haystack}");
        }
    }

    #[test]
    fn builtin_catalog_contains_required_tools() {
        let entries = builtin_catalog().unwrap();
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        for required in [
            "opencode",
            "codex",
            "claude",
            "lazygit",
            "btop",
            "fastfetch",
            "procs",
            "btm",
            "dust",
            "duf",
            "spf",
            "yazi",
            "gdu",
            "fzf",
            "rg",
            "fd",
            "bat",
            "eza",
            "bandwhich",
            "trip",
            "cloudflare-speed-test",
            "mtr",
            "curl",
            "lazydocker",
            "lazysql",
            "ssh",
            "atuin",
            "tldr",
            "jq",
            "yq",
            "glow",
            "gh",
            "delta",
        ] {
            assert!(ids.contains(&required), "'{required}' katalogda yok");
        }
    }

    #[test]
    fn non_empty_categories_are_all_covered() {
        let entries = builtin_catalog().unwrap();
        for category in CATEGORIES {
            // Bu iki kategori gömülü katalogda boş olabilir:
            // "Özel Araçlar" yalnızca kullanıcı eklemeleri içindir.
            // Boş kategori arayüzde açık bir mesajla gösterilir.
            if *category == "Özel Araçlar" {
                continue;
            }
            assert!(
                entries.iter().any(|e| e.category == *category),
                "'{category}' boş"
            );
        }
    }

    #[test]
    fn alternatives_point_to_existing_tools() {
        let entries = builtin_catalog().unwrap();
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        for entry in &entries {
            if let Some(target) = &entry.alternative_to {
                assert!(
                    ids.contains(&target.as_str()),
                    "{} alternatifi '{}' bulunamadı",
                    entry.id,
                    target
                );
                assert_ne!(&entry.id, target, "{} kendine alternatif olamaz", entry.id);
            }
        }
    }

    #[test]
    fn command_help_records_have_text() {
        let entries = builtin_catalog().unwrap();
        for entry in &entries {
            if entry.launch_kind == LaunchKind::CommandHelp {
                assert!(
                    entry
                        .help_text
                        .as_deref()
                        .is_some_and(|t| !t.trim().is_empty()),
                    "{} için help_text boş",
                    entry.id
                );
            }
            if entry.launch_kind == LaunchKind::ShellIntegrationInfo {
                assert!(
                    entry
                        .info_text
                        .as_deref()
                        .is_some_and(|t| !t.trim().is_empty()),
                    "{} için info_text boş",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn broken_toml_error_names_the_source() {
        let err = parse_one("test.toml", "[[tools]\nid = \"x\"\n").unwrap_err();
        assert!(err.contains("test.toml"), "{err}");
        assert!(err.contains("TOML"), "{err}");
    }

    #[test]
    fn broken_toml_error_has_line_and_column() {
        let err = parse_one("bozuk.toml", "[[tools]\nid = \n").unwrap_err();
        assert!(err.contains("line"), "{err}");
    }

    #[test]
    fn invalid_record_error_names_the_source_and_id() {
        let text = "[[tools]]\nid = \"bad\"\ncategory = \"Olmayan\"\nexecutable = \"bad\"\n";
        let err = parse_one("x.toml", text).unwrap_err();
        assert!(err.contains("bad"), "{err}");
        assert!(err.contains("Olmayan"), "{err}");
        assert!(err.contains("x.toml"), "{err}");
    }

    #[test]
    fn empty_source_is_rejected() {
        let err = parse_one("bos.toml", "# boş\n").unwrap_err();
        assert!(err.contains("bos.toml"), "{err}");
    }

    #[test]
    fn user_favorite_is_merged_by_id() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![ToolEntry {
                id: "alpha".into(),
                favorite: true,
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let catalog = merge_with_user(builtin, &user).unwrap();
        let entry = catalog.get_by_id("alpha").unwrap();
        assert!(entry.favorite);
        assert_eq!(entry.name, "Alpha");
        assert_eq!(entry.category, "Sistem ve Kaynak Kullanımı");
        assert_eq!(entry.origin, EntryOrigin::Builtin);
    }

    #[test]
    fn user_can_add_own_tool() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![ToolEntry {
                id: "kendi".into(),
                name: "Kendi Aracım".into(),
                executable: "kendi-xyzz".into(),
                category: "Özel Araçlar".into(),
                description: "kendi".into(),
                launch_kind: LaunchKind::CustomCommand,
                args: vec!["--ornek".into()],
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let catalog = merge_with_user(builtin, &user).unwrap();
        let entry = catalog.get_by_id("kendi").unwrap();
        assert_eq!(entry.name, "Kendi Aracım");
        assert_eq!(entry.category, "Özel Araçlar");
        assert_eq!(entry.args, vec!["--ornek".to_string()]);
        assert_eq!(entry.launch_kind, LaunchKind::CustomCommand);
        assert_eq!(entry.origin, EntryOrigin::User);
    }

    #[test]
    fn entry_count_grows_with_user_only_tools() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let before = builtin.len();
        let user = UserConfig {
            tools: vec![
                ToolEntry {
                    id: "alpha".into(),
                    favorite: true,
                    ..ToolEntry::default()
                },
                ToolEntry {
                    id: "yeni".into(),
                    category: "Özel Araçlar".into(),
                    executable: "yeni-xyzz".into(),
                    ..ToolEntry::default()
                },
            ],
            ..UserConfig::default()
        };
        let catalog = merge_with_user(builtin, &user).unwrap();
        assert_eq!(catalog.entries().len(), before + 1);
    }

    #[test]
    fn empty_user_record_is_ignored() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![ToolEntry::default()],
            ..UserConfig::default()
        };
        let catalog = merge_with_user(builtin, &user).unwrap();
        assert_eq!(catalog.entries().len(), 1);
    }

    #[test]
    fn user_duplicate_new_id_is_rejected() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let tool = ToolEntry {
            id: "yeni".into(),
            category: "Özel Araçlar".into(),
            executable: "yeni-xyzz".into(),
            ..ToolEntry::default()
        };
        let mut dup = tool.clone();
        dup.name = "İkinci".into();
        let user = UserConfig {
            tools: vec![tool, dup],
            ..UserConfig::default()
        };
        let err = merge_with_user(builtin, &user).unwrap_err();
        assert!(err.contains("yeni"), "{err}");
    }

    #[test]
    fn user_override_keeps_other_fields() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![ToolEntry {
                id: "alpha".into(),
                description: "yeni açıklama".into(),
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let catalog = merge_with_user(builtin, &user).unwrap();
        let entry = catalog.get_by_id("alpha").unwrap();
        assert_eq!(entry.description, "yeni açıklama");
        assert_eq!(entry.name, "Alpha");
    }

    #[test]
    fn broken_user_override_is_rejected() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![ToolEntry {
                id: "alpha".into(),
                category: "Olmayan Kategori".into(),
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let err = merge_with_user(builtin, &user).unwrap_err();
        assert!(err.contains("alpha"), "{err}");
        assert!(err.contains("Olmayan Kategori"), "{err}");
    }

    #[test]
    fn invalid_launch_mode_in_builtin_override_is_rejected() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![ToolEntry {
                id: "alpha".into(),
                launch_mode: "termnal".into(),
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let err = merge_with_user(builtin, &user).unwrap_err();
        assert!(err.contains("alpha"), "{err}");
        assert!(err.contains("termnal"), "{err}");
        assert!(err.contains("terminal, background, detached"), "{err}");
    }

    #[test]
    fn missing_user_file_falls_back_to_builtin() {
        let path = std::env::temp_dir()
            .join("nova-yok-boyle-dosya")
            .join("config.toml");
        let catalog = load(Some(&path)).unwrap();
        assert!(catalog.get_by_id("btop").is_some());
        assert!(catalog.get_by_id("lazygit").is_some());
    }

    #[test]
    fn no_user_path_uses_builtin_only() {
        let catalog = load(None).unwrap();
        assert!(catalog.get_by_id("opencode").is_some());
    }

    #[test]
    fn broken_user_file_is_a_clear_error() {
        let dir = std::env::temp_dir().join(format!("nova-loader-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[[tools]\n").unwrap();
        let err = load(Some(&path)).unwrap_err();
        assert!(err.contains("config.toml"), "{err}");
        assert!(err.contains("TOML"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn user_file_merges_with_builtin_on_disk() {
        let dir = std::env::temp_dir().join(format!("nova-loader-merge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
[[tools]]
id = "btop"
favorite = true
"#,
        )
        .unwrap();
        let catalog = load(Some(&path)).unwrap();
        assert!(catalog.get_by_id("btop").unwrap().favorite);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn user_tool_without_id_is_skipped_only_when_fully_empty() {
        // id yok ama başka alanlar var: açık hata.
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![ToolEntry {
                name: "İsimsiz".into(),
                category: "Özel Araçlar".into(),
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let err = merge_with_user(builtin, &user).unwrap_err();
        assert!(err.contains("id"), "{err}");
    }

    #[test]
    fn user_executable_is_kept_apart_from_tool_name() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![ToolEntry {
                id: "yeni".into(),
                name: "Yeni Araç".into(),
                category: "Özel Araçlar".into(),
                executable: "yeni-komut".into(),
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let catalog = merge_with_user(builtin, &user).unwrap();
        let entry = catalog.get_by_id("yeni").unwrap();
        assert_eq!(entry.name, "Yeni Araç");
        assert_eq!(entry.executable, "yeni-komut");
        assert_eq!(entry.origin, EntryOrigin::User);
    }

    #[test]
    fn user_tool_with_executable_like_id_works() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![user_tool("yeni")],
            ..UserConfig::default()
        };
        let catalog = merge_with_user(builtin, &user).unwrap();
        let entry = catalog.get_by_id("yeni").unwrap();
        assert_eq!(entry.executable, "yeni");
    }

    #[test]
    fn empty_executable_falls_back_to_id() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![ToolEntry {
                id: "gizli-komut".into(),
                category: "Özel Araçlar".into(),
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let catalog = merge_with_user(builtin, &user).unwrap();
        let entry = catalog.get_by_id("gizli-komut").unwrap();
        assert_eq!(entry.executable, "gizli-komut");
        assert_eq!(entry.name, "gizli-komut");
    }

    #[test]
    fn path_like_executable_is_allowed() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        let user = UserConfig {
            tools: vec![ToolEntry {
                id: "yol".into(),
                category: "Özel Araçlar".into(),
                executable: "/bu/yol/kesinlikle/yok/xyz".into(),
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        // `/` içeren executable artık hata vermez; detection::resolve dosya
        // yoksa Missing durumunda bırakır, ancak giriş oluşturulur.
        let cat = merge_with_user(builtin, &user).unwrap();
        let entry = cat.get_by_id("yol").unwrap();
        assert_eq!(entry.executable, "/bu/yol/kesinlikle/yok/xyz");
        assert_eq!(entry.status, InstalledStatus::Missing);
    }

    #[test]
    fn favorite_overrides_and_launch_kind_are_merged() {
        let builtin = parse_one("mini.toml", MINI).unwrap();
        // Gömülü kayıt etkileşimli; kullanıcı onu özel komuta çevirmek
        // isterse bu, varsayılanla aynı olmadığı için uygulanır.
        let user = UserConfig {
            tools: vec![ToolEntry {
                id: "alpha".into(),
                launch_kind: LaunchKind::CustomCommand,
                args: vec!["--hizli".into()],
                favorite: true,
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let catalog = merge_with_user(builtin, &user).unwrap();
        let entry = catalog.get_by_id("alpha").unwrap();
        assert_eq!(entry.launch_kind, LaunchKind::CustomCommand);
        assert_eq!(entry.args, vec!["--hizli".to_string()]);
        assert!(entry.favorite);
        assert_eq!(entry.origin, EntryOrigin::Builtin);
    }

    #[test]
    fn default_launch_kind_does_not_erase_builtin_kind() {
        let text = r#"
[[tools]]
id = "beta"
category = "Sistem ve Kaynak Kullanımı"
executable = "beta-xyz"
launch_kind = "command_help"
help_text = "beta --yardim"
"#;
        let builtin = parse_one("mini.toml", text).unwrap();
        // Kullanıcı yalnızca favori işaretler; launch_kind varsayılanda
        // kalır ve gömülü `command_help` silinmemeli.
        let user = UserConfig {
            tools: vec![ToolEntry {
                id: "beta".into(),
                favorite: true,
                ..ToolEntry::default()
            }],
            ..UserConfig::default()
        };
        let catalog = merge_with_user(builtin, &user).unwrap();
        let entry = catalog.get_by_id("beta").unwrap();
        assert_eq!(entry.launch_kind, LaunchKind::CommandHelp);
        assert_eq!(entry.help_text.as_deref(), Some("beta --yardim"));
    }
}
