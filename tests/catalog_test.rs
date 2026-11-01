//! Faz 3 katalog testleri.
//!
//! Kataloğun gömülü TOML dosyasından yüklendiğini, bozuk yapılandırmanın
//! anlaşılır bir hataya çevrildiğini ve kullanıcı dosyasının gömülü katalogla
//! kimliğe göre birleştiğini sınar. Katalog UI'dan bağımsızdır: bu testler
//! arayüz çizmez, yalnızca veri modelini kullanır.

use nova::catalog::loader;
use nova::catalog::{catalog_entry_from_tool, Catalog, CatalogEntry, CATEGORIES};
use nova::config::{LaunchKind, ToolEntry, UserConfig};
use nova::detection::InstalledStatus;

/// Gömülü katalogun çözümlenmiş hâli.
fn builtin_entries() -> Vec<CatalogEntry> {
    loader::builtin_catalog().expect("gömülü katalog okunmalı")
}

fn builtin_catalog() -> Catalog {
    Catalog::from_entries(builtin_entries()).expect("katalog oluşturulamadı")
}

/// Kimliği verilen yeni bir kullanıcı aracı.
fn user_tool(id: &str, executable: &str) -> ToolEntry {
    ToolEntry {
        id: id.into(),
        name: format!("{id} adı"),
        description: "kullanıcı test aracı".into(),
        category: "Özel Araçlar".into(),
        executable: executable.into(),
        ..ToolEntry::default()
    }
}

fn catalog_with_user(tools: Vec<ToolEntry>) -> Catalog {
    loader::merge_with_user(
        builtin_entries(),
        &UserConfig {
            tools,
            ..UserConfig::default()
        },
    )
    .expect("birleştirme başarısız")
}

// ---------------------------------------------------------------------------
// Gömülü katalog: içerik ve kapsam
// ---------------------------------------------------------------------------

#[test]
fn builtin_catalog_loads_with_entries() {
    let cat = builtin_catalog();
    assert!(!cat.entries().is_empty());
}

#[test]
fn every_entry_has_a_known_category() {
    for entry in builtin_entries() {
        assert!(
            CATEGORIES.contains(&entry.category.as_str()),
            "tanımsız kategori: {} ({})",
            entry.category,
            entry.id
        );
    }
}

#[test]
fn entry_ids_are_unique() {
    let mut ids: Vec<String> = builtin_entries().into_iter().map(|e| e.id).collect();
    let len = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), len, "katalog id'leri benzersiz olmalı");
}

#[test]
fn required_tools_are_present_and_zoxide_httpie_are_absent() {
    let cat = builtin_catalog();
    let required = [
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
    ];
    for id in required {
        assert!(cat.get_by_id(id).is_some(), "eksik araç: {id}");
    }
    for id in ["zoxide", "httpie", "http"] {
        assert!(cat.get_by_id(id).is_none(), "katalogda olmamalı: {id}");
        assert!(
            cat.entries()
                .iter()
                .all(|e| e.executable != id && !e.name.eq_ignore_ascii_case(id)),
            "katalogda {id} geçmemeli"
        );
    }
}

#[test]
fn required_categories_hold_their_tools() {
    let cat = builtin_catalog();
    let expected: [(&str, &[&str]); 7] = [
        ("AI ve Kodlama", &["opencode", "codex", "claude"]),
        ("Git ve Geliştirici Araçları", &["lazygit"]),
        (
            "Sistem ve Kaynak Kullanımı",
            &["btop", "fastfetch", "procs", "btm", "dust", "duf"],
        ),
        (
            "Dosya Yönetimi ve Arama",
            &["spf", "yazi", "gdu", "fzf", "rg", "fd", "bat", "eza"],
        ),
        (
            "Ağ ve Bağlantı",
            &["bandwhich", "trip", "cloudflare-speed-test", "mtr", "curl"],
        ),
        ("Docker ve Sunucu", &["lazydocker", "lazysql", "ssh"]),
        (
            "Günlük Terminal Yardımcıları",
            &["atuin", "tldr", "jq", "yq", "glow", "gh", "delta"],
        ),
    ];
    for (category, ids) in expected {
        let in_category: Vec<&str> = cat
            .by_category(category)
            .into_iter()
            .map(|e| e.id.as_str())
            .collect();
        for id in ids {
            assert!(
                in_category.contains(id),
                "'{id}' '{category}' kategorisinde değil; bulunanlar: {in_category:?}"
            );
        }
    }
}

#[test]
fn launch_kinds_are_appropriate() {
    let cat = builtin_catalog();
    // TUI araçları doğrudan başlatılır.
    for id in [
        "opencode",
        "codex",
        "claude",
        "lazygit",
        "btop",
        "yazi",
        "duf",
        "fzf",
        "lazydocker",
        "glow",
    ] {
        assert_eq!(
            cat.get_by_id(id).unwrap().launch_kind,
            LaunchKind::Interactive,
            "{id} etkileşimli olmalı"
        );
    }
    // Kabuk entegrasyonu gereken araçlar yalnızca bilgi gösterir.
    for id in ["atuin", "delta"] {
        assert_eq!(
            cat.get_by_id(id).unwrap().launch_kind,
            LaunchKind::ShellIntegrationInfo,
            "{id} bilgi görünümü olmalı"
        );
    }
    // Kullanım metni sabittir ve TOML'dan gelir; araç çalıştırılmaz.
    for id in ["rg", "fd", "trip", "mtr", "curl", "ssh", "tldr", "jq", "yq"] {
        let entry = cat.get_by_id(id).unwrap();
        assert_eq!(entry.launch_kind, LaunchKind::CommandHelp, "{id}");
        assert!(
            entry.help_text.as_deref().is_some_and(|t| !t.is_empty()),
            "{id} için kullanım metni yok"
        );
    }
    // Cloudflare testi kullanıcı tarafından değiştirilebilir bir komuttur.
    assert_eq!(
        cat.get_by_id("cloudflare-speed-test").unwrap().launch_kind,
        LaunchKind::CustomCommand
    );
    // gdu ve btm alternatif olduklarını belirtir.
    assert_eq!(
        cat.get_by_id("gdu").unwrap().alternative_to.as_deref(),
        Some("duf")
    );
    assert_eq!(
        cat.get_by_id("btm").unwrap().alternative_to.as_deref(),
        Some("btop")
    );
}

#[test]
fn builtin_tools_use_the_requested_executable_names() {
    let cat = builtin_catalog();
    for (id, executable) in [
        ("opencode", "opencode"),
        ("codex", "codex"),
        ("claude", "claude"),
        ("lazygit", "lazygit"),
        ("btop", "btop"),
        ("fastfetch", "fastfetch"),
        ("procs", "procs"),
        ("btm", "btm"),
        ("dust", "dust"),
        ("duf", "duf"),
        ("spf", "spf"),
        ("yazi", "yazi"),
        ("gdu", "gdu"),
        ("fzf", "fzf"),
        ("rg", "rg"),
        ("fd", "fd"),
        ("bat", "bat"),
        ("eza", "eza"),
        ("bandwhich", "bandwhich"),
        ("trip", "trip"),
        ("mtr", "mtr"),
        ("curl", "curl"),
        ("lazydocker", "lazydocker"),
        ("lazysql", "lazysql"),
        ("ssh", "ssh"),
        ("atuin", "atuin"),
        ("tldr", "tldr"),
        ("jq", "jq"),
        ("yq", "yq"),
        ("glow", "glow"),
        ("gh", "gh"),
        ("delta", "delta"),
    ] {
        assert_eq!(cat.get_by_id(id).unwrap().executable, executable, "{id}");
    }
}

#[test]
fn cloudflare_default_command_reports_measured_speed_without_printing_payload() {
    let entry = builtin_catalog()
        .get_by_id("cloudflare-speed-test")
        .unwrap()
        .clone();
    assert_eq!(entry.executable, "curl");
    assert!(entry
        .args
        .windows(2)
        .any(|pair| pair == ["-o", "/dev/null"]));
    assert!(entry
        .args
        .windows(2)
        .any(|pair| pair[0] == "-w" && pair[1].contains("%{speed_download}")));
    assert!(entry.args.iter().any(|arg| arg == "--max-time"));
}

#[test]
fn descriptions_are_short_and_present() {
    for entry in builtin_entries() {
        let len = entry.description.chars().count();
        assert!(
            (10..=160).contains(&len),
            "'{}' açıklaması {len} karakter: {}",
            entry.id,
            entry.description
        );
    }
}

// ---------------------------------------------------------------------------
// Bozuk yapılandırma anlaşılır hata verir
// ---------------------------------------------------------------------------

#[test]
fn broken_toml_names_file_and_line() {
    let dir = std::env::temp_dir().join(format!("nova-catalog-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bozuk.toml");
    std::fs::write(&path, "[[tools]\nid = \"x\"\n").unwrap();
    let err = loader::load(Some(&path)).unwrap_err();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(err.contains("bozuk.toml"), "dosya adı yok: {err}");
    assert!(err.contains("geçerli TOML değil"), "{err}");
    assert!(err.contains("satır") || err.contains("line"), "{err}");
}

#[test]
fn unknown_category_is_rejected_with_valid_list() {
    let mut tool = user_tool("tuhaf", "tuhaf-xyz");
    tool.category = "Uydurma Kategori".into();
    let err = catalog_entry_from_tool(&tool).unwrap_err();
    assert!(err.contains("tuhaf"), "{err}");
    assert!(err.contains("AI ve Kodlama"), "geçerli liste yok: {err}");
}

#[test]
fn path_like_executable_is_allowed() {
    let tool = user_tool("yol", "/bu/yol/kesinlikle/yok/xyz");
    // `/` içeren executable artık hata vermez; detection::resolve dosya
    // yoksa Missing durumunda bırakır, ancak giriş oluşturulur.
    let entry = catalog_entry_from_tool(&tool).unwrap();
    assert_eq!(entry.executable, "/bu/yol/kesinlikle/yok/xyz");
}

#[test]
fn empty_catalog_source_is_rejected() {
    let dir = std::env::temp_dir().join(format!("nova-catalog-bos-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bos.toml");
    // Ayrıştırılabilen ama hiç geçerli araç bırakmayan kaynak: doğrulama
    // her kaydı reddettiği için birleştirme katalogsuz kalır.
    std::fs::write(&path, "[[tools]]\nid = \"x\"\n").unwrap();
    let err = loader::merge_with_user(
        Vec::new(),
        &UserConfig {
            tools: vec![ToolEntry::default()],
            ..UserConfig::default()
        },
    )
    .unwrap_err();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(err.contains("araç") || err.contains("katalog"), "{err}");
}

#[test]
fn user_file_without_tools_is_not_an_error() {
    let dir = std::env::temp_dir().join(format!("nova-catalog-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, "[ui]\ntheme = \"dark\"\n").unwrap();
    let builtin = builtin_entries();
    let expected = builtin.len();
    let cat = loader::load_with_builtin(builtin, Some(&path)).expect("hata olmamalı");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(cat.entries().len(), expected);
}

// ---------------------------------------------------------------------------
// Kullanıcı dosyası: güncelleme kullanıcı verisini korur
// ---------------------------------------------------------------------------

#[test]
fn user_file_favorite_and_custom_tool_survive() {
    let cat = catalog_with_user(vec![
        ToolEntry {
            id: "btop".into(),
            favorite: true,
            ..ToolEntry::default()
        },
        user_tool("kendi-aracim", "kendi-aracim-xyz"),
    ]);
    assert!(cat.get_by_id("btop").unwrap().favorite);
    let own = cat.get_by_id("kendi-aracim").expect("kullanıcı aracı yok");
    assert_eq!(own.category, "Özel Araçlar");
    assert_eq!(
        own.origin,
        nova::config::EntryOrigin::User,
        "köken kullanıcı olmalı"
    );
    // Gömülü araçlar hâlâ yerinde.
    assert!(cat.get_by_id("opencode").is_some());
    assert_eq!(
        cat.get_by_id("opencode").unwrap().origin,
        nova::config::EntryOrigin::Builtin
    );
}

#[test]
fn user_config_can_change_cloudflare_executable_and_arguments() {
    let dir = std::env::temp_dir().join(format!("nova-cloudflare-config-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        r#"
[[tools]]
id = "cloudflare-speed-test"
executable = "nova-test-speed-tool"
args = ["--bytes", "1000000"]
"#,
    )
    .unwrap();

    let catalog = loader::load(Some(&path)).expect("özelleştirme yüklenmeli");
    let entry = catalog.get_by_id("cloudflare-speed-test").unwrap();
    assert_eq!(entry.executable, "nova-test-speed-tool");
    assert_eq!(entry.args, ["--bytes", "1000000"]);
    assert_eq!(entry.launch_kind, LaunchKind::CustomCommand);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn user_sees_own_tool_even_when_missing_and_it_stays_visible() {
    let cat = catalog_with_user(vec![user_tool(
        "kesin-yok-arac",
        "nova-kesin-yok-boyle-komut-xyz",
    )]);
    let entry = cat.get_by_id("kesin-yok-arac").unwrap();
    assert_eq!(entry.status, InstalledStatus::Missing);
    assert!(cat.missing().iter().any(|e| e.id == "kesin-yok-arac"));
    assert!(cat
        .by_category("Özel Araçlar")
        .iter()
        .any(|e| e.id == "kesin-yok-arac"));
}

#[test]
fn duplicate_user_id_without_category_is_rejected() {
    let err = loader::merge_with_user(
        builtin_entries(),
        &UserConfig {
            tools: vec![user_tool("ayni", "a"), user_tool("ayni", "b")],
            ..UserConfig::default()
        },
    )
    .unwrap_err();
    assert!(err.contains("ayni"), "{err}");
}

// ---------------------------------------------------------------------------
// Arama ve durum tespiti
// ---------------------------------------------------------------------------

#[test]
fn search_filters_by_name_executable_category_and_description() {
    let cat = builtin_catalog();

    assert!(cat.search("OpenCode").iter().any(|e| e.id == "opencode")); // ad
    assert!(cat.search("bandwhich").iter().any(|e| e.id == "bandwhich")); // komut
    assert!(cat.search("Docker ve Sunucu").iter().any(|e| e.id == "ssh")); // kategori
    assert!(cat
        .search("hız testi")
        .iter()
        .any(|e| e.id == "cloudflare-speed-test")); // açıklama

    assert!(cat.search("").is_empty(), "boş sorgu sonuç döndürmez");
    assert!(cat.search("bulunmayan-sorgu-xyz").is_empty());
}

#[test]
fn search_is_case_insensitive_for_turkish_text() {
    let cat = builtin_catalog();
    let lower = cat.search("sistem ve kaynak");
    let upper = cat.search("SİSTEM VE KAYNAK");
    let plain = cat.search("Sistem Ve Kaynak");
    assert!(!lower.is_empty());
    assert_eq!(lower.len(), plain.len());
    assert_eq!(lower.len(), upper.len());
}

#[test]
fn status_reflects_detection_without_running_the_tool() {
    let cat = catalog_with_user(vec![user_tool(
        "yok-arac",
        "nova-kesin-yok-boyle-komut-xyz",
    )]);
    let missing = cat.get_by_id("yok-arac").unwrap();
    assert_eq!(missing.status, InstalledStatus::Missing);
    assert!(missing.resolved_path.is_none());
    // Hiçbir giriş "Kontrol edilemedi" olmamalı: PATH var ve komut adları
    // geçerli (yol içermiyor).
    assert!(cat.unknown().is_empty(), "{:?}", cat.unknown());
}

#[test]
fn by_category_filters_correctly() {
    let cat = builtin_catalog();
    let files = cat.by_category("Dosya Yönetimi ve Arama");
    assert!(files
        .iter()
        .all(|e| e.category == "Dosya Yönetimi ve Arama"));
    assert!(files.iter().any(|e| e.id == "yazi"));
}

#[test]
fn toggle_favorite_changes_state() {
    let mut cat = builtin_catalog();
    assert!(!cat.get_by_id("btop").unwrap().favorite);

    assert!(cat.toggle_favorite("btop"));
    assert!(cat.get_by_id("btop").unwrap().favorite);
    assert!(cat.favorites().iter().any(|e| e.id == "btop"));

    assert!(!cat.toggle_favorite("btop"));
    assert!(!cat.get_by_id("btop").unwrap().favorite);
    assert!(!cat.toggle_favorite("yok"));
}

#[test]
fn empty_executable_resolves_to_unknown() {
    let entry = CatalogEntry::new(
        "bos",
        "Boş",
        "Boş komut",
        "Özel Araçlar",
        "",
        vec![],
        nova::catalog::LaunchMode::Terminal,
        nova::catalog::WorkingDirectory::Current,
        false,
    );
    assert_eq!(entry.status, InstalledStatus::Unknown);
    assert_eq!(entry.status.label(), "Kontrol edilemedi");
}

// ---------------------------------------------------------------------------
// Katalog UI'dan bağımsız
// ---------------------------------------------------------------------------

/// Kayıt sırası korunmalı: kullanıcı aracı gömülü listenin sonuna eklenir.
#[test]
fn builtin_entries_come_first_then_user_tools() {
    let cat = catalog_with_user(vec![user_tool("en-son", "en-son-xyz")]);
    let ids: Vec<&str> = cat.entries().iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids.last(), Some(&"en-son"));
    assert_eq!(ids.len(), builtin_entries().len() + 1);
}

#[test]
fn launch_hint_explains_non_interactive_kinds() {
    let cat = builtin_catalog();
    // Etkileşimli araç için ekstra satır yok: açıklama yeterli.
    assert!(cat.get_by_id("btop").unwrap().launch_hint().is_none());
    let help = cat.get_by_id("rg").unwrap().launch_hint().unwrap();
    assert!(help.contains("araç çalıştırılmaz"), "{help}");
    let info = cat.get_by_id("atuin").unwrap().launch_hint().unwrap();
    assert!(info.contains("kabuk entegrasyonu"), "{info}");
}

// ---------------------------------------------------------------------------
// Kısa kategori adları ile arama
// ---------------------------------------------------------------------------

#[test]
fn search_matches_short_category_name() {
    let cat = builtin_catalog();
    let ai_by_short: Vec<&str> = cat.search("AI").iter().map(|e| e.id.as_str()).collect();
    let ai_by_full: Vec<&str> = cat
        .search("AI ve Kodlama")
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    // Kısa ve tam kategori adı aynı AI & Kodlama aracılarını döndürmeli.
    assert_eq!(ai_by_short, ai_by_full);
    assert!(ai_by_short.contains(&"opencode"));
    assert!(ai_by_short.contains(&"codex"));
    assert!(ai_by_short.contains(&"claude"));

    let git_by_short: Vec<&str> = cat.search("Git").iter().map(|e| e.id.as_str()).collect();
    assert!(git_by_short.contains(&"lazygit"));
}

#[test]
fn search_is_case_insensitive_for_short_names() {
    let cat = builtin_catalog();
    let lower = cat
        .search("ai")
        .iter()
        .map(|e| e.id.as_str())
        .collect::<Vec<_>>();
    let upper = cat
        .search("AI")
        .iter()
        .map(|e| e.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(lower, upper);
    assert!(!lower.is_empty());
}

// ---------------------------------------------------------------------------
// Katalog metaverisi (örnekler ve kurulum bilgisi)
// ---------------------------------------------------------------------------

#[test]
fn catalog_entries_have_examples_and_setup_info() {
    let cat = builtin_catalog();

    // opencode: örnekler ve kurulum bilgisi var.
    let opencode = cat.get_by_id("opencode").unwrap();
    assert!(!opencode.examples.is_empty());
    assert!(opencode.examples.contains(&"opencode".to_string()));
    assert!(opencode.examples.contains(&"opencode --help".to_string()));
    assert!(opencode.setup_info.is_some());
    assert!(opencode
        .setup_info
        .as_deref()
        .unwrap()
        .contains("kimlik doğrulama"));

    // rg: örnekler var.
    let rg = cat.get_by_id("rg").unwrap();
    assert!(!rg.examples.is_empty());
    assert!(rg.examples.contains(&"rg <desen>".to_string()));
    assert!(rg.examples.contains(&"rg -i <desen>".to_string()));

    // atuin: setup_info var (kabuk entegrasyonu).
    let atuin = cat.get_by_id("atuin").unwrap();
    assert!(atuin.setup_info.is_some());
    assert!(atuin
        .setup_info
        .as_deref()
        .unwrap()
        .contains("Kabuk entegrasyonu"));

    // delta: setup_info var.
    let delta = cat.get_by_id("delta").unwrap();
    assert!(delta.setup_info.is_some());
    assert!(delta
        .setup_info
        .as_deref()
        .unwrap()
        .contains("pager = delta"));

    // Biraz örnekli olmayan bir araç (btop): examples boş olabilir ama setup_info yok.
    let btop = cat.get_by_id("btop").unwrap();
    assert!(btop.examples.contains(&"btop".to_string()));
}

#[test]
fn catalog_entry_examples_are_verified_not_invented() {
    let cat = builtin_catalog();
    // Tüm örnekler katalog metaverisi olarak tutulur; Nova bunları çalıştırmaz.
    for entry in cat.entries() {
        for example in &entry.examples {
            // Örnek boş olmamalı, kabuk karakteri içerememeli.
            assert!(!example.is_empty(), "boş örnek: {}", entry.id);
        }
    }
}
