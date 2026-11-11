//! Komut Rehberi sekmesi: görev odaklı IA · banka ≠ Mağaza ≠ PATH keşfi.

use nova::app::App;
use nova::catalog::Catalog;
use nova::commands::{
    CommandsBrowseMode, LearningLevel, LearningLevelFilter, RiskLevel, BANK_KIND, LOCAL_KIND,
};
use ratatui::{backend::TestBackend, Terminal};

fn app() -> App {
    let entries = nova::catalog::loader::builtin_catalog().expect("gömülü katalog");
    App::new(Catalog::from_entries(entries).expect("katalog"))
}

fn render(app: &App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| nova::ui::draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn all_text(lines: &[String]) -> String {
    lines.join("\n")
}

#[test]
fn settings_lists_commands_bank_refresh_action() {
    let mut app = app();
    app.select_settings_tab();
    let text = all_text(&render(&app, 120, 28));
    assert!(
        text.contains("Komut bankası") || text.contains("Commands bank"),
        "Ayarlar Komut bankası eylemini göstermeli:\n{text}"
    );
    assert!(
        text.contains("Mağaza kataloğu") || text.contains("Store catalog"),
        "Mağaza kataloğu eylemi korunmalı:\n{text}"
    );
}

#[test]
fn commands_tab_opens_on_start_not_alpha_categories() {
    let mut app = app();
    app.select_commands_tab();
    assert_eq!(app.commands_mode, CommandsBrowseMode::Start);
    let text = all_text(&render(&app, 150, 42));

    assert!(text.contains("Komut Rehberi"), "sekme etiketi yok:\n{text}");
    assert!(
        text.contains("Başlangıç") || text.contains("Gezinme"),
        "görev odaklı başlangıç yok:\n{text}"
    );
    assert!(
        text.contains("Göreve göre") || text.contains("By task"),
        "görev modu yok:\n{text}"
    );
    assert!(
        text.contains("Sistemde bulunanlar") || text.contains("On this system"),
        "PATH modu yok:\n{text}"
    );
    // Mağaza panelleri Komut Rehberi'nde açılmaz.
    assert!(
        !text.contains("Mağaza Kategorileri"),
        "Mağaza paneli Komut Rehberi'ne sızmamalı"
    );
    assert_eq!(app.commands_bank.kind, BANK_KIND);
    assert!(
        !app.commands_bank.tasks.is_empty(),
        "katalog tasks yüklenmeli"
    );
    assert!(
        !app.commands_bank.profiles.is_empty(),
        "katalog profiles yüklenmeli"
    );
}

#[test]
fn commands_tab_wires_tasks_and_profiles() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::ByTask);
    assert!(app.commands_mode.needs_group_column());
    assert!(
        !app.filtered_tasks().is_empty(),
        "görev listesi boş olmamalı"
    );
    let text = all_text(&render(&app, 180, 42));
    assert!(
        text.contains("Görev") || text.contains("draft"),
        "görev yüzeyi yok:\n{text}"
    );

    app.set_commands_mode(CommandsBrowseMode::Context);
    assert!(!app.commands_bank.profiles.is_empty());
    // Çoklu seçim: Space/Enter profil ekler.
    app.focus = nova::app::PanelFocus::CommandsCategory;
    app.commands_category_index = 0;
    let id = app.commands_bank.profiles[0].id.clone();
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert!(app.commands_selected_profiles.contains(&id));
    assert!(
        app.commands_bank.profiles[0].verification.status
            == nova::commands::VerificationStatus::Draft
            || !app.commands_selected_profiles.is_empty()
    );
}

#[test]
fn commands_tab_local_source_uses_local_kind_not_bank() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::SystemPath);
    app.focus = nova::app::PanelFocus::CommandsList;
    let text = all_text(&render(&app, 150, 42));

    assert!(
        text.contains("Sistemde bulunanlar") || text.contains("PATH"),
        "{text}"
    );
    assert_eq!(app.commands_local.kind, LOCAL_KIND);
    assert_ne!(app.commands_local.kind, BANK_KIND);
    assert!(app.commands_local.stub);
    assert_eq!(app.commands_source(), nova::commands::CommandsSource::Local);
}

#[test]
fn commands_detail_shows_purpose_safe_example_risk_sudo() {
    let mut app = app();
    app.select_commands_tab();
    app.commands_filters.clear();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    // Geniş: sağ detay paneli (gruplu: commit sonrası Grup+Liste+Detay).
    app.terminal_size = Some((180, 42));
    app.narrow_mode = false;
    app.commit_commands_guide();
    let cmds = app.filtered_bank_commands();
    assert!(!cmds.is_empty());
    if let Some(idx) = cmds.iter().position(|c| !c.examples.is_empty()) {
        app.commands_entry_index = idx;
    }
    let text = all_text(&render(&app, 180, 42));
    assert!(
        text.contains("Ne zaman işe yarar")
            || text.contains("When it helps")
            || text.contains("Ne işe yarar")
            || text.contains("What is it for")
            || text.contains("Platform")
            || text.contains("risk")
            || text.contains("Risk")
            || text.contains("dikkat")
            || text.contains("güvenli")
            || text.contains("caution")
            || text.contains("safe"),
        "amaç/risk yok:\n{text}"
    );
    assert!(
        text.contains("Güvenli örnek")
            || text.contains("Beklenen etki")
            || text.contains("Örnekler")
            || text.contains("Examples")
            || text.contains("Bayrak")
            || text.contains("Flags")
            || text.contains("sudo")
            || text.contains("çalıştırılmaz")
            || text.contains("Yetki")
            || text.contains("Privilege"),
        "detay alanları yok:\n{text}"
    );
    assert!(
        text.contains("nova-commands-bank") || text.contains("kürate") || text.contains("Mağaza"),
        "{text}"
    );
}

#[test]
fn commands_search_spans_categories_and_keeps_sources_separate() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    app.search_query = "ls".into();
    let bank_n = app.filtered_bank_commands().len();
    assert!(bank_n > 0, "bank araması boş olmamalı");
    app.set_commands_mode(CommandsBrowseMode::SystemPath);
    let _local_n = app.filtered_local_commands().len();
    assert_eq!(app.commands_bank.kind, BANK_KIND);
    assert_eq!(app.commands_local.kind, LOCAL_KIND);
}

#[test]
fn command_favorites_are_separate_from_tool_and_store() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    let id = {
        let cmds = app.filtered_bank_commands();
        assert!(!cmds.is_empty());
        cmds[0].id.clone()
    };
    assert!(!app.is_command_favorite(&id));
    app.commands_entry_index = 0;
    app.focus = nova::app::PanelFocus::CommandsList;
    let f = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('f'),
        crossterm::event::KeyModifiers::NONE,
    );
    app.handle_key(f).unwrap();
    assert!(app.is_command_favorite(&id));
    assert!(
        !app.state.file.favorites.contains(&id),
        "banka favorisi araç favorites setine yazılmamalı"
    );
    // PATH modunda F bank favorisi eklemez.
    app.set_commands_mode(CommandsBrowseMode::SystemPath);
    let before = app.state.file.command_favorites.len();
    app.handle_key(f).unwrap();
    assert_eq!(app.state.file.command_favorites.len(), before);
}

#[test]
fn tab_order_is_guide_then_store_then_settings() {
    let mut app = app();
    for e in app.catalog.entries_mut() {
        e.status = nova::detection::InstalledStatus::Installed;
    }
    app.focus = nova::app::PanelFocus::Category;
    let visible = app.visible_category_indices();
    app.category_index = *visible.last().unwrap();
    app.set_tab_index(visible.len());
    assert!(app.commands_tab);
    app.set_tab_index(visible.len() + 1);
    assert!(app.store_tab);
    assert!(!app.commands_tab);
    app.set_tab_index(visible.len() + 2);
    assert!(app.settings_tab);
}

#[test]
fn guide_keeps_max_three_content_panels_with_breadcrumb_esc() {
    let mut app = app();
    app.select_commands_tab();
    // Başlangıç: içerik hemen (liste odaklı), commit yok.
    assert_eq!(app.commands_mode, CommandsBrowseMode::Start);
    assert!(app.commands_mode_panel_visible());
    assert!(!app.commands_guide_committed);
    assert_eq!(app.pane_secondary_cols(), 1);

    app.set_commands_mode(CommandsBrowseMode::ByTask);
    assert!(app.commands_mode.needs_group_column());
    assert!(app.commands_mode_panel_visible());
    assert_eq!(app.pane_secondary_cols(), 2);
    // Yan detay Mod+Grup iken kapalı (max 3).
    app.terminal_size = Some((200, 42));
    app.narrow_mode = false;
    assert!(!app.commands_detail_side_active());

    app.commit_commands_guide();
    assert!(!app.commands_mode_panel_visible());
    assert_eq!(app.pane_secondary_cols(), 1);
    let crumb = app.commands_breadcrumb();
    assert!(
        crumb.contains("Göreve")
            || crumb.contains("task")
            || crumb.contains("Task")
            || !crumb.is_empty(),
        "breadcrumb boş: {crumb}"
    );

    // Esc: drill yoksa commit geri alınır.
    app.focus = nova::app::PanelFocus::CommandsList;
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert!(!app.commands_guide_committed);
    assert!(app.commands_mode_panel_visible());
    assert!(app.running);

    // PATH ayrı kalır.
    app.set_commands_mode(CommandsBrowseMode::SystemPath);
    assert_eq!(app.commands_source(), nova::commands::CommandsSource::Local);
    assert!(app.commands_mode_panel_visible());
}

#[test]
fn teachable_detail_shows_flags_and_os_fit() {
    let mut app = app();
    app.select_commands_tab();
    app.commands_filters.clear();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    app.terminal_size = Some((180, 48));
    app.narrow_mode = false;
    app.commit_commands_guide();
    let cmds = app.filtered_bank_commands();
    assert!(!cmds.is_empty());
    if let Some(idx) = cmds.iter().position(|c| !c.flags.is_empty()) {
        app.commands_entry_index = idx;
    }
    let text = all_text(&render(&app, 180, 48));
    assert!(
        text.contains("Ne zaman işe yarar")
            || text.contains("When it helps")
            || text.contains("Ne işe yarar")
            || text.contains("What is it for")
            || text.contains("Bayrak")
            || text.contains("Flags")
            || text.contains("Platform")
            || text.contains("risk")
            || text.contains("dikkat")
            || text.contains("uyumlu")
            || text.contains("Fits"),
        "öğretici detay yok:\n{text}"
    );
}

#[test]
fn narrow_80x24_keeps_critical_warning_and_selection_accessible() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::ServerSecurity);
    app.focus = nova::app::PanelFocus::CommandsList;
    app.terminal_size = Some((80, 24));
    app.narrow_mode = true;

    let steps = app.filtered_checklist_steps();
    assert!(steps.len() >= 2, "checklist en az 2 adım: {}", steps.len());
    // Son adıma in: dar listede kaydırma gerekir; seçim görünür kalmalı.
    app.commands_entry_index = steps.len() - 1;
    let selected_id = steps[app.commands_entry_index].id.clone();
    let selected_title = {
        let lang = app.ui_lang().id();
        steps[app.commands_entry_index].title.pick(lang).to_string()
    };
    assert!(
        selected_id.starts_with("security-check-"),
        "seçili adım id beklenirdi: {selected_id}"
    );

    let narrow = all_text(&render(&app, 80, 24));
    assert!(
        narrow.contains("çalıştırılmaz")
            || narrow.contains("NeverScan")
            || narrow.contains("never")
            || narrow.contains("Space")
            || narrow.contains("salt okunur")
            || narrow.contains("Read-only")
            || narrow.contains("tarama")
            || narrow.contains("scan"),
        "80x24 kritik güvenlik uyarısı erişilebilir olmalı:\n{narrow}"
    );
    // Seçili satır başlığı veya id listede / detayda kalsın (kırpılıp kaybolmasın).
    let title_hit = !selected_title.is_empty()
        && narrow.contains(selected_title.chars().take(12).collect::<String>().as_str());
    let id_hit = narrow.contains(&selected_id) || narrow.contains("security-check");
    assert!(
        title_hit || id_hit || narrow.contains("[ ]") || narrow.contains("[✓]"),
        "80x24 seçili checklist öğesi erişilebilir olmalı (id={selected_id}):\n{narrow}"
    );

    // Detay kaydırma içeriği sığmasa da çökmemeli; uyarı korunur.
    app.focus = nova::app::PanelFocus::CommandsDetail;
    app.commands_detail_scroll = 40;
    let scrolled = all_text(&render(&app, 80, 24));
    assert!(
        scrolled.contains("çalıştırılmaz")
            || scrolled.contains("never")
            || scrolled.contains("Space")
            || scrolled.contains("salt")
            || scrolled.contains("Read-only")
            || scrolled.contains("örnek")
            || scrolled.contains("example")
            || !scrolled.trim().is_empty(),
        "dar detay kaydırması panik/boş bırakmamalı:\n{scrolled}"
    );
}

#[test]
fn filters_each_facet_selects_expected_command_ids() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    app.commands_category_index = 0;
    let cat = app
        .commands_bank_categories()
        .first()
        .copied()
        .unwrap_or("files")
        .to_string();

    // İzole fixture’lar — her süzgeç kendi id’sini seçer.
    let fixtures = [
        serde_json::json!({
            "id": "facet-os-windows",
            "name": "facet-os-windows",
            "category": cat,
            "platforms": ["windows"],
            "risk": "safe",
            "learning_level": "beginner",
        }),
        serde_json::json!({
            "id": "facet-os-linux",
            "name": "facet-os-linux",
            "category": cat,
            "platforms": ["linux"],
            "risk": "safe",
            "learning_level": "beginner",
            "package_refs": [{"manager": "pacman", "package": "demo", "distributions": ["arch"]}],
            "shells": ["bash"],
        }),
        serde_json::json!({
            "id": "facet-distro-debian",
            "name": "facet-distro-debian",
            "category": cat,
            "platforms": ["linux"],
            "risk": "safe",
            "learning_level": "beginner",
            "package_refs": [{"manager": "apt", "package": "demo", "distributions": ["debian"]}],
        }),
        serde_json::json!({
            "id": "facet-shell-pwsh",
            "name": "facet-shell-pwsh",
            "category": cat,
            "platforms": ["windows"],
            "shells": ["powershell"],
            "risk": "safe",
            "learning_level": "beginner",
        }),
        serde_json::json!({
            "id": "facet-level-advanced-safe",
            "name": "facet-level-advanced-safe",
            "category": cat,
            "platforms": ["linux"],
            "risk": "safe",
            "learning_level": "advanced",
        }),
        serde_json::json!({
            "id": "facet-risk-dangerous",
            "name": "facet-risk-dangerous",
            "category": cat,
            "platforms": ["linux"],
            "risk": "dangerous",
            "learning_level": "beginner",
        }),
    ];
    for v in fixtures {
        app.commands_bank
            .commands
            .push(serde_json::from_value(v).expect("facet fixture"));
    }
    // Arama açıkken kategori kısıtı kalkar — facet’leri banka genelinde doğrula.
    app.search_query = "facet-".into();

    // OS
    app.commands_filters.os = Some("windows".into());
    let os_ids: Vec<_> = app
        .filtered_bank_commands()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        os_ids.contains(&"facet-os-windows"),
        "OS=windows facet-os-windows tutmalı: {os_ids:?}"
    );
    assert!(
        !os_ids.contains(&"facet-os-linux"),
        "OS=windows facet-os-linux elemmeli: {os_ids:?}"
    );
    app.commands_filters.clear();

    // Distro (Arch tercihi — debian fixture elemmeli)
    app.commands_filters.distro = Some("arch".into());
    let distro_ids: Vec<_> = app
        .filtered_bank_commands()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        distro_ids.contains(&"facet-os-linux"),
        "distro=arch pacman dağıtımlı facet-os-linux tutmalı: {distro_ids:?}"
    );
    assert!(
        !distro_ids.contains(&"facet-distro-debian"),
        "distro=arch facet-distro-debian elemmeli: {distro_ids:?}"
    );
    app.commands_filters.clear();

    // Shell
    app.commands_filters.shell = Some("powershell".into());
    let shell_ids: Vec<_> = app
        .filtered_bank_commands()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        shell_ids.contains(&"facet-shell-pwsh"),
        "shell=powershell facet-shell-pwsh tutmalı: {shell_ids:?}"
    );
    assert!(
        !shell_ids.contains(&"facet-os-linux"),
        "shell=powershell bash’li facet-os-linux elemmeli: {shell_ids:?}"
    );
    app.commands_filters.clear();

    // Profile — related_commands kesişimi
    let profile_id = "facet-profile-test";
    app.commands_bank.profiles.push(
        serde_json::from_value(serde_json::json!({
            "id": profile_id,
            "name": {"en": "Facet profile"},
            "summary": {"en": "test"},
            "related_commands": ["facet-os-linux"],
            "verification": {"status": "draft"},
        }))
        .expect("profile fixture"),
    );
    app.commands_filters.profile = Some(profile_id.into());
    let profile_ids: Vec<_> = app
        .filtered_bank_commands()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        profile_ids.contains(&"facet-os-linux"),
        "profil related_commands facet-os-linux tutmalı: {profile_ids:?}"
    );
    assert!(
        !profile_ids.contains(&"facet-os-windows"),
        "profil dışı facet-os-windows elemmeli: {profile_ids:?}"
    );
    app.commands_filters.clear();

    // learning_level (risk değil)
    app.commands_filters.learning = LearningLevelFilter::Advanced;
    let level_ids: Vec<_> = app
        .filtered_bank_commands()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        level_ids.contains(&"facet-level-advanced-safe"),
        "learning=advanced safe komutu tutmalı: {level_ids:?}"
    );
    assert!(
        !level_ids.contains(&"facet-risk-dangerous"),
        "learning=advanced beginner+dangerous’ı risk yüzünden advanced saymamalı: {level_ids:?}"
    );
    app.commands_filters.clear();

    // risk (learning değil)
    app.commands_filters.risk = Some(RiskLevel::Dangerous);
    let risk_ids: Vec<_> = app
        .filtered_bank_commands()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        risk_ids.contains(&"facet-risk-dangerous"),
        "risk=dangerous facet-risk-dangerous tutmalı: {risk_ids:?}"
    );
    assert!(
        !risk_ids.contains(&"facet-level-advanced-safe"),
        "risk=dangerous safe advanced’ı elemmeli: {risk_ids:?}"
    );
    app.commands_filters.clear();

    // Flag / alias araması PATH ile karışmaz
    app.search_query = "-h".into();
    let _ = app.filtered_bank_commands();
    app.set_commands_mode(CommandsBrowseMode::SystemPath);
    assert_eq!(app.commands_source(), nova::commands::CommandsSource::Local);
}

#[test]
fn risk_filter_independent_of_learning_level() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    app.commands_category_index = 0;
    let cat = app
        .commands_bank_categories()
        .first()
        .copied()
        .unwrap_or("files")
        .to_string();

    // İzole fixture: safe-but-advanced vs dangerous-unspecified.
    app.commands_bank.commands.push(
        serde_json::from_value(serde_json::json!({
            "id": "test-safe-advanced",
            "name": "test-safe-advanced",
            "category": cat,
            "risk": "safe",
            "learning_level": "advanced",
        }))
        .expect("fixture"),
    );
    app.commands_bank.commands.push(
        serde_json::from_value(serde_json::json!({
            "id": "test-dangerous-unset",
            "name": "test-dangerous-unset",
            "category": cat,
            "risk": "dangerous",
            "learning_level": "unspecified",
        }))
        .expect("fixture"),
    );

    app.commands_filters.risk = Some(RiskLevel::Safe);
    let risk_ids: Vec<_> = app
        .filtered_bank_commands()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        risk_ids.contains(&"test-safe-advanced"),
        "risk=safe advanced komutu göstermeli: {risk_ids:?}"
    );
    assert!(
        !risk_ids.contains(&"test-dangerous-unset"),
        "risk=safe dangerous'ı elemmeli"
    );
    app.commands_filters.clear();

    app.commands_filters.learning = LearningLevelFilter::Advanced;
    let level_ids: Vec<_> = app
        .filtered_bank_commands()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        level_ids.contains(&"test-safe-advanced"),
        "safe-but-advanced advanced süzgecinde: {level_ids:?}"
    );
    assert!(
        !level_ids.contains(&"test-dangerous-unset"),
        "dangerous + belirtilmemiş advanced sayılmamalı: {level_ids:?}"
    );
}

#[test]
fn learning_level_filter_excludes_unspecified_honestly() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);

    let unspecified_n = app
        .commands_bank
        .commands
        .iter()
        .filter(|c| c.learning_level == LearningLevel::Unspecified)
        .count();
    assert!(
        unspecified_n > 0,
        "bankada unspecified learning_level beklenirdi"
    );
    assert!(
        app.commands_bank
            .commands
            .iter()
            .any(|c| c.learning_level == LearningLevel::Advanced && c.risk == RiskLevel::Safe),
        "safe-but-advanced (ör. systemd-analyze) bankada olmalı"
    );

    // Arama açıkken kategori kısıtı kalkar — seviye süzgecini banka genelinde doğrula.
    app.search_query = "e".into();

    app.commands_filters.learning = LearningLevelFilter::Beginner;
    let beginners = app.filtered_bank_commands();
    assert!(!beginners.is_empty());
    assert!(
        beginners
            .iter()
            .all(|c| c.learning_level == LearningLevel::Beginner),
        "beginner süzgeci yalnız açık beginner; risk ile uydurma yok"
    );

    app.commands_filters.learning = LearningLevelFilter::Intermediate;
    assert!(!app.filtered_bank_commands().is_empty());
    assert!(app
        .filtered_bank_commands()
        .iter()
        .all(|c| c.learning_level == LearningLevel::Intermediate));

    app.commands_filters.learning = LearningLevelFilter::Advanced;
    let advanced = app.filtered_bank_commands();
    assert!(!advanced.is_empty());
    assert!(advanced
        .iter()
        .all(|c| c.learning_level == LearningLevel::Advanced));
    assert!(
        advanced.iter().any(|c| c.risk == RiskLevel::Safe),
        "advanced süzgecinde safe komut da olmalı"
    );

    app.commands_filters.learning = LearningLevelFilter::Unspecified;
    let unset = app.filtered_bank_commands();
    assert!(!unset.is_empty());
    assert!(unset
        .iter()
        .all(|c| c.learning_level == LearningLevel::Unspecified));

    // UI: seviye + risk rozetleri birlikte.
    app.commands_filters.clear();
    app.search_query.clear();
    app.terminal_size = Some((160, 45));
    app.narrow_mode = false;
    let text = all_text(&render(&app, 160, 45));
    assert!(
        text.contains("belirtilmedi")
            || text.contains("unspecified")
            || text.contains("başlangıç")
            || text.contains("beginner")
            || text.contains("orta")
            || text.contains("ileri"),
        "öğrenme seviyesi rozeti görünmeli:\n{text}"
    );
}

#[test]
fn server_security_checklist_persists_local_state_only() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::ServerSecurity);
    let steps = app.filtered_checklist_steps();
    assert!(
        !steps.is_empty(),
        "sunucu güvenlik checklist adımları boş olmamalı"
    );
    assert!(
        steps.iter().any(|s| s.id.starts_with("security-check-")),
        "Commands security-check-* görevleri checklist'te olmalı; ids={:?}",
        steps.iter().map(|s| s.id.as_str()).collect::<Vec<_>>()
    );
    app.commands_entry_index = 0;
    app.focus = nova::app::PanelFocus::CommandsList;
    let key = steps[0].state_key_for(&app.checklist_target);
    assert!(
        key.contains("::local::") || key.contains(&format!("::{}::", app.checklist_target)),
        "hedef-kapsamlı anahtar beklenirdi: {key}"
    );
    assert!(!app.state.file.command_checklist.contains_key(&key));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert_eq!(
        app.state
            .file
            .command_checklist
            .get(&key)
            .map(String::as_str),
        Some("ok")
    );
    // Parola / host saklanmaz — yalnız durum anahtarları.
    for v in app.state.file.command_checklist.values() {
        assert!(matches!(v.as_str(), "ok" | "review" | "na" | "unchecked"));
    }
    let text = all_text(&render(&app, 120, 40));
    assert!(
        text.contains("güvenlik")
            || text.contains("Security")
            || text.contains("Kontrol")
            || text.contains("Checklist")
            || text.contains("[✓]")
            || text.contains("[ ]"),
        "checklist yüzeyi yok:\n{text}"
    );
    // Enter çalıştırmaz
    let before_modal = app.modal.is_open();
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert_eq!(app.modal.is_open(), before_modal);
}

#[test]
fn checklist_progress_isolated_across_targets_and_migrates_legacy() {
    use nova::app::state::AppState;
    use nova::commands::{
        checklist_progress_key, migrate_checklist_progress, CHECKLIST_DEFAULT_TARGET,
        SERVER_SECURITY_GUIDE_ID,
    };

    let entries = nova::catalog::loader::builtin_catalog().expect("katalog");
    let catalog = Catalog::from_entries(entries).expect("katalog");

    // Eski 2 parçalı + çıplak kayıtlar → App yüklemede local hedefine taşınır.
    let mut state = AppState::in_memory();
    state.file.command_checklist.insert(
        format!("{SERVER_SECURITY_GUIDE_ID}::security-check-ssh"),
        "ok".into(),
    );
    state
        .file
        .command_checklist
        .insert("security-check-firewall-ports".into(), "review".into());
    let mut app = App::with_state(catalog, state, false);
    // Varsayılan hedef “Bu cihaz” / local — eski kayıtlar buraya taşınır.
    assert_eq!(app.checklist_target, CHECKLIST_DEFAULT_TARGET);
    assert!(
        app.state
            .checklist_targets()
            .iter()
            .any(|t| t.id == CHECKLIST_DEFAULT_TARGET),
        "varsayılan hedef kaydı yok"
    );
    assert_eq!(app.checklist_target_display_name(), "Bu cihaz");
    let ssh_key = checklist_progress_key(CHECKLIST_DEFAULT_TARGET, "security-check-ssh");
    let fw_key = checklist_progress_key(CHECKLIST_DEFAULT_TARGET, "security-check-firewall-ports");
    assert_eq!(
        app.state
            .file
            .command_checklist
            .get(&ssh_key)
            .map(String::as_str),
        Some("ok"),
        "eski guide::step kaydı local hedefine taşınmalı"
    );
    assert_eq!(
        app.state
            .file
            .command_checklist
            .get(&fw_key)
            .map(String::as_str),
        Some("review"),
        "çıplak adım kaydı local hedefine taşınmalı"
    );
    assert!(
        !app.state
            .file
            .command_checklist
            .contains_key(&format!("{SERVER_SECURITY_GUIDE_ID}::security-check-ssh")),
        "eski 2 parçalı anahtar kalkmalı"
    );

    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::ServerSecurity);
    let steps = app.filtered_checklist_steps();
    let ssh_idx = steps
        .iter()
        .position(|s| s.id == "security-check-ssh")
        .expect("ssh step");
    let other_idx = steps
        .iter()
        .position(|s| s.id != "security-check-ssh")
        .expect("another step");

    // Aynı hedefte adımlar birbirine sızmaz.
    assert_eq!(app.checklist_step_state(&steps[ssh_idx]).as_str(), "ok");
    assert_eq!(
        app.checklist_step_state(&steps[other_idx]).as_str(),
        if steps[other_idx].id == "security-check-firewall-ports" {
            "review"
        } else {
            "unchecked"
        }
    );

    // Farklı hedef: local ok → prod unchecked (yalıtım).
    app.checklist_target = "prod-host".into();
    assert_eq!(
        app.checklist_step_state(&steps[ssh_idx]).as_str(),
        "unchecked",
        "prod-host local ilerlemesini görmemeli"
    );
    app.commands_entry_index = ssh_idx;
    app.focus = nova::app::PanelFocus::CommandsList;
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    let prod_key = steps[ssh_idx].state_key_for("prod-host");
    assert_eq!(
        app.state
            .file
            .command_checklist
            .get(&prod_key)
            .map(String::as_str),
        Some("ok")
    );
    assert_eq!(
        app.state
            .file
            .command_checklist
            .get(&ssh_key)
            .map(String::as_str),
        Some("ok"),
        "prod işaretleme local kaydı bozmamalı"
    );

    // migrate idempotent
    let before = app.state.file.command_checklist.clone();
    assert_eq!(
        migrate_checklist_progress(&mut app.state.file.command_checklist),
        0
    );
    assert_eq!(app.state.file.command_checklist, before);
}

#[test]
fn checklist_targets_ui_switch_preserves_independent_progress() {
    use nova::app::modal::Modal;
    use nova::commands::CHECKLIST_DEFAULT_TARGET;

    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::ServerSecurity);
    let steps = app.filtered_checklist_steps();
    assert!(!steps.is_empty());
    let ssh_idx = steps
        .iter()
        .position(|s| s.id == "security-check-ssh")
        .expect("ssh step");
    app.commands_entry_index = ssh_idx;
    app.focus = nova::app::PanelFocus::CommandsList;

    // local → ok
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    let local_key = steps[ssh_idx].state_key_for(CHECKLIST_DEFAULT_TARGET);
    assert_eq!(
        app.state
            .file
            .command_checklist
            .get(&local_key)
            .map(String::as_str),
        Some("ok")
    );

    // t → hedef seçici; A → ad gir; Enter → ekle; Enter → etkinleştir
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('t'),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert!(matches!(app.modal, Modal::ChecklistTargetPicker { .. }));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert!(matches!(app.modal, Modal::ChecklistTargetAdd { .. }));
    for c in "Staging".chars() {
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::NONE,
        ))
        .unwrap();
    }
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert!(matches!(app.modal, Modal::ChecklistTargetPicker { .. }));
    // Seçili satır yeni hedef; Enter ile etkinleştir.
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert_eq!(app.modal, Modal::None);
    assert_eq!(app.checklist_target, "staging");
    assert_eq!(app.checklist_target_display_name(), "Staging");
    assert_eq!(
        app.state.file.prefs.checklist_target.as_deref(),
        Some("staging")
    );

    // Aynı adım staging’de unchecked; Space → ok (local’den bağımsız).
    assert_eq!(
        app.checklist_step_state(&steps[ssh_idx]).as_str(),
        "unchecked",
        "staging local ilerlemesini görmemeli"
    );
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    let staging_key = steps[ssh_idx].state_key_for("staging");
    assert_eq!(
        app.state
            .file
            .command_checklist
            .get(&staging_key)
            .map(String::as_str),
        Some("ok")
    );
    assert_eq!(
        app.state
            .file
            .command_checklist
            .get(&local_key)
            .map(String::as_str),
        Some("ok"),
        "staging işareti local kaydı bozmamalı"
    );

    // Geri local’e geç — önceki ok korunur.
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('t'),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    // local ilk satır
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('k'),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert_eq!(app.checklist_target, CHECKLIST_DEFAULT_TARGET);
    assert_eq!(app.checklist_step_state(&steps[ssh_idx]).as_str(), "ok");

    // Sırlar saklanmaz — yalnızca id + ad + durum.
    for t in app.state.checklist_targets() {
        assert!(!t.id.contains('@'));
        assert!(!t.name.contains("BEGIN PRIVATE"));
    }
    for v in app.state.file.command_checklist.values() {
        assert!(matches!(v.as_str(), "ok" | "review" | "na" | "unchecked"));
    }

    let text = all_text(&render(&app, 120, 40));
    assert!(
        text.contains("Bu cihaz") || text.contains("This device"),
        "varsayılan hedef yüzeyi yok:\n{text}"
    );
}

#[test]
fn checklist_platform_filter_match_and_no_match() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::ServerSecurity);
    let steps = app.filtered_checklist_steps();
    let fw = steps
        .iter()
        .find(|s| s.id == "security-check-firewall-ports")
        .expect("firewall checklist step from vendored bank");
    assert!(
        fw.examples.len() >= 2,
        "vendored firewall task must keep multi-platform examples, got {}",
        fw.examples.len()
    );

    // Match: Windows must not surface linux ufw/ss.
    app.commands_filters.os = Some("windows".into());
    let win = nova::commands::select_checklist_examples(&fw.examples, &app.commands_filters);
    assert!(!win.is_empty(), "windows firewall example expected");
    assert!(
        win.iter()
            .all(|e| { !e.example.command.contains("ufw") && !e.example.command.contains("ss -") }),
        "windows selection leaked linux cmds: {:?}",
        win.iter()
            .map(|e| e.example.command.as_str())
            .collect::<Vec<_>>()
    );

    // Distro: debian seçilince Arch/pacman yarışını kaybetmeli.
    let updates = steps
        .iter()
        .find(|s| s.id == "security-check-updates")
        .expect("updates checklist step");
    app.commands_filters.os = Some("linux".into());
    app.commands_filters.distro = Some("debian".into());
    let deb = nova::commands::select_checklist_examples(&updates.examples, &app.commands_filters);
    assert!(
        deb.iter().any(|e| e.example.command.contains("apt")),
        "debian distro apt örneği beklenirdi: {:?}",
        deb.iter()
            .map(|e| e.example.command.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        deb.iter().all(|e| {
            !e.example.command.contains("pacman") && !e.example.command.contains("checkupdates")
        }),
        "debian süzgeci arch/pacman göstermemeli: {:?}",
        deb.iter()
            .map(|e| e.example.command.as_str())
            .collect::<Vec<_>>()
    );

    // Shell: powershell → windows net örnekleri.
    app.commands_filters.clear();
    app.commands_filters.shell = Some("powershell".into());
    let pwsh = nova::commands::select_checklist_examples(&fw.examples, &app.commands_filters);
    assert!(
        !pwsh.is_empty()
            && pwsh.iter().all(|e| {
                e.example
                    .shell
                    .as_deref()
                    .is_some_and(|s| s.to_ascii_lowercase().contains("powershell"))
                    || e.shells
                        .iter()
                        .any(|s| s.to_ascii_lowercase().contains("powershell"))
            }),
        "powershell shell süzgeci eşleşmeli: {:?}",
        pwsh.iter()
            .map(|e| (e.example.command.as_str(), e.shells.as_slice()))
            .collect::<Vec<_>>()
    );

    // No-match: docker has no BSD impl (jails ≠ Docker) → empty, no silent substitute.
    // (Windows now has docker examples in the combined #69 catalog.)
    let docker = steps
        .iter()
        .find(|s| s.id == "security-check-docker")
        .expect("docker checklist step");
    app.commands_filters.clear();
    app.commands_filters.os = Some("bsd".into());
    let empty = nova::commands::select_checklist_examples(&docker.examples, &app.commands_filters);
    assert!(
        empty.is_empty(),
        "bsd must not inherit linux/windows docker example: {:?}",
        empty
            .iter()
            .map(|e| e.example.command.as_str())
            .collect::<Vec<_>>()
    );

    // UI surfaces the no-example copy when filter has no match.
    if let Some(idx) = steps.iter().position(|s| s.id == "security-check-docker") {
        app.commands_entry_index = idx;
    }
    let text = all_text(&render(&app, 140, 42));
    assert!(
        text.contains("No example for this platform")
            || text.contains("Bu platform")
            || text.contains("örnek yok"),
        "checklist should tell user there is no platform example:\n{text}"
    );
}

#[test]
fn enter_and_nav_keys_never_auto_execute_guide_or_checklist() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    app.focus = nova::app::PanelFocus::CommandsList;
    let before_modal = app.modal.is_open();
    assert!(app.install_job.is_none());
    assert!(app.pending_tty_pm.is_none());
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert_eq!(
        app.modal.is_open(),
        before_modal,
        "Enter modal/çalıştırma açmamalı"
    );
    assert!(
        app.install_job.is_none() && app.pending_tty_pm.is_none(),
        "Enter arka plan / TTY işi başlatmamalı"
    );
    assert!(
        app.status_message.contains("Bank")
            && (app.status_message.contains("risk")
                || app.status_message.contains("doğrulama")
                || app.status_message.contains("verification")),
        "Enter bilgi satırı Bank+risk/doğrulama içermeli (boş fallback yok): {}",
        app.status_message
    );
    assert!(
        !app.status_message
            .to_ascii_lowercase()
            .contains("çalıştırılıyor")
            && !app.status_message.to_ascii_lowercase().contains("running"),
        "Enter çalıştırma mesajı vermemeli: {}",
        app.status_message
    );

    // Gezinme tuşları da çalıştırmaz.
    for code in [
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyCode::Up,
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyCode::Left,
        crossterm::event::KeyCode::Tab,
    ] {
        let modal_before = app.modal.is_open();
        app.handle_key(crossterm::event::KeyEvent::new(
            code,
            crossterm::event::KeyModifiers::NONE,
        ))
        .unwrap();
        assert_eq!(app.modal.is_open(), modal_before, "{code:?} modal açmamalı");
        assert!(
            app.install_job.is_none() && app.pending_tty_pm.is_none(),
            "{code:?} iş başlatmamalı"
        );
    }

    // Checklist: Enter / oklar durum değiştirmez ve çalıştırmaz.
    app.set_commands_mode(CommandsBrowseMode::ServerSecurity);
    app.focus = nova::app::PanelFocus::CommandsList;
    app.commands_entry_index = 0;
    let steps = app.filtered_checklist_steps();
    assert!(!steps.is_empty());
    let key = steps[0].state_key_for(&app.checklist_target);
    assert!(!app.state.file.command_checklist.contains_key(&key));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert!(
        !app.state.file.command_checklist.contains_key(&key),
        "Enter checklist durumunu değiştirmemeli"
    );
    assert!(
        app.status_message.contains("çalıştırılmaz")
            || app.status_message.contains("never")
            || app.status_message.contains("tarama")
            || app.status_message.contains("scan")
            || app.status_message.contains("Space"),
        "checklist Enter bilgi satırı çalıştırmama uyarısı içermeli: {}",
        app.status_message
    );
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert!(
        app.state.file.command_checklist.is_empty()
            || !app.state.file.command_checklist.contains_key(&key),
        "Down checklist işaretlememeli"
    );
    assert!(app.install_job.is_none() && app.pending_tty_pm.is_none());
    assert!(!app.modal.is_open());
}

#[test]
fn start_topic_detail_shows_related_commands_tasks_learning() {
    let mut app = app();
    app.select_commands_tab();
    app.commands_filters.clear();
    assert!(
        app.commands_list_shows_concepts(),
        "Başlangıç konu listesi göstermeli"
    );
    let concepts = app.filtered_concepts();
    assert!(!concepts.is_empty(), "konu listesi boş olmamalı");
    // related_commands dolu bir konu seç
    if let Some(idx) = concepts.iter().position(|c| !c.related_commands.is_empty()) {
        app.commands_entry_index = idx;
    }
    app.terminal_size = Some((140, 40));
    app.narrow_mode = false;
    let text = all_text(&render(&app, 140, 40));
    assert!(
        text.contains("Konular") || text.contains("Topics") || text.contains("Öğrenme"),
        "konu listesi/öğrenme yok:\n{text}"
    );
    assert!(
        text.contains("İlgili komut")
            || text.contains("Related command")
            || text.contains("Öğrenme")
            || text.contains("Learning"),
        "konu detayında related/learning yok:\n{text}"
    );
    // Enter çalıştırmaz
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert!(app.install_job.is_none() && app.pending_tty_pm.is_none());
    assert!(!app.modal.is_open());
    assert!(
        app.status_message.contains("çalıştırılmaz")
            || app.status_message.contains("Bank")
            || app.status_message.contains("Konu"),
        "konu Enter bilgi satırı: {}",
        app.status_message
    );
}

#[test]
fn host_filter_default_is_editable_and_clearable() {
    let mut app = app();
    app.select_commands_tab();
    // Güvenilir host’ta OS (ve Arch’ta distro) tohumlanır.
    let seeded_os = app.commands_filters.os.clone();
    assert!(
        app.commands_host_filter_seeded,
        "host süzgeci bir kez tohumlanmalı"
    );
    if seeded_os.is_some() {
        assert!(
            app.status_message.contains("os:")
                || app.status_message.contains("Host")
                || app.status_message.contains("süzgeç"),
            "host varsayılanı durum satırında: {}",
            app.status_message
        );
    }
    // Düzenlenebilir: OS döngüsü (`o`)
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('o'),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    // Temizle — yeniden dayatma yok
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('x'),
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert!(app.commands_filters.os.is_none());
    assert!(app.commands_filters.distro.is_none());
    app.select_commands_tab();
    assert!(
        app.commands_filters.os.is_none(),
        "x sonrası host süzgeci yeniden dayatılmamalı"
    );
}

#[test]
fn bank_filter_no_match_does_not_substitute_other_os() {
    let mut app = app();
    app.select_commands_tab();
    app.commands_filters.clear();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    app.commands_category_index = 0;
    // Eşleşmeyen OS — boş liste, ikame yok
    app.commands_filters.os = Some("nonexistent-os-xyz".into());
    let cmds = app.filtered_bank_commands();
    assert!(
        cmds.is_empty(),
        "no-match OS boş dönmeli, ikame yok: {:?}",
        cmds.iter().map(|c| c.id.as_str()).collect::<Vec<_>>()
    );
    app.terminal_size = Some((140, 36));
    let text = all_text(&render(&app, 140, 36));
    assert!(
        text.contains("eşleşme yok")
            || text.contains("No match")
            || text.contains("ikame")
            || text.contains("substitut"),
        "no-match UI mesajı yok:\n{text}"
    );
}

#[test]
fn bank_detail_examples_respect_os_filter_no_cross_os_surface() {
    let mut app = app();
    app.select_commands_tab();
    app.commands_filters.clear();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    let cat = app
        .commands_bank_categories()
        .first()
        .copied()
        .unwrap_or("files")
        .to_string();
    app.commands_bank.commands.push(
        serde_json::from_value(serde_json::json!({
            "id": "nv122-ex-filter",
            "name": "nv122-ex-filter",
            "category": cat,
            "platforms": ["linux", "windows"],
            "risk": "safe",
            "examples": [
                {"command": "ss -tuln", "platforms": ["linux"], "risk": "safe"},
                {"command": "Get-NetTCPConnection", "platforms": ["windows"], "shell": "powershell", "risk": "safe"}
            ]
        }))
        .unwrap(),
    );
    app.search_query = "nv122-ex-filter".into();
    app.commands_filters.os = Some("windows".into());
    app.commit_commands_guide();
    app.commands_entry_index = 0;
    app.terminal_size = Some((160, 48));
    app.narrow_mode = false;
    let text = all_text(&render(&app, 160, 48));
    assert!(
        text.contains("Get-NetTCPConnection"),
        "windows süzgeci windows örneğini göstermeli:\n{text}"
    );
    assert!(
        !text.contains("ss -tuln"),
        "windows süzgeci linux örneğini detayda yüzeylememeli:\n{text}"
    );
}

#[test]
fn distro_filter_no_arch_collapse_on_debian_only_package() {
    let mut app = app();
    app.select_commands_tab();
    app.commands_filters.clear();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    let cat = app
        .commands_bank_categories()
        .first()
        .copied()
        .unwrap_or("files")
        .to_string();
    app.commands_bank.commands.push(
        serde_json::from_value(serde_json::json!({
            "id": "nv122-debian-only",
            "name": "nv122-debian-only",
            "category": cat,
            "platforms": ["linux"],
            "risk": "safe",
            "packages": {"debian": "demo"},
        }))
        .unwrap(),
    );
    app.commands_bank.commands.push(
        serde_json::from_value(serde_json::json!({
            "id": "nv122-arch-only",
            "name": "nv122-arch-only",
            "category": cat,
            "platforms": ["linux"],
            "risk": "safe",
            "packages": {"arch": "demo"},
        }))
        .unwrap(),
    );
    app.search_query = "nv122-".into();
    app.commands_filters.distro = Some("arch".into());
    let ids: Vec<_> = app
        .filtered_bank_commands()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        ids.contains(&"nv122-arch-only"),
        "arch distro arch-only tutmalı: {ids:?}"
    );
    assert!(
        !ids.contains(&"nv122-debian-only"),
        "arch distro debian-only’yi Arch’a çökertmemeli: {ids:?}"
    );
    app.commands_filters.distro = Some("debian".into());
    let ids: Vec<_> = app
        .filtered_bank_commands()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert!(ids.contains(&"nv122-debian-only"));
    assert!(!ids.contains(&"nv122-arch-only"));
}
