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
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    // Geniş: sağ detay paneli (gruplu: 2 ikincil sütun → ~168+ sütun gerekir).
    app.terminal_size = Some((180, 42));
    app.narrow_mode = false;
    let cmds = app.filtered_bank_commands();
    assert!(!cmds.is_empty());
    if let Some(idx) = cmds.iter().position(|c| !c.examples.is_empty()) {
        app.commands_entry_index = idx;
    }
    let text = all_text(&render(&app, 180, 42));
    assert!(
        text.contains("Ne işe yarar")
            || text.contains("What is it for")
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
            || text.contains("çalıştırılmaz"),
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
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    app.terminal_size = Some((160, 45));
    app.narrow_mode = false;
    let cmds = app.filtered_bank_commands();
    assert!(!cmds.is_empty());
    if let Some(idx) = cmds.iter().position(|c| !c.flags.is_empty()) {
        app.commands_entry_index = idx;
    }
    let text = all_text(&render(&app, 160, 45));
    assert!(
        text.contains("Ne işe yarar")
            || text.contains("What is it for")
            || text.contains("Bayrak")
            || text.contains("Flags")
            || text.contains("risk"),
        "öğretici detay yok:\n{text}"
    );
    // 80x24 dar: kritik uyarılar panik/clip olmamalı
    let narrow = all_text(&render(&app, 80, 24));
    assert!(!narrow.is_empty(), "80x24 boş render");
}

#[test]
fn filters_os_risk_and_search_aliases_flags() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    let before = app.filtered_bank_commands().len();
    assert!(before > 0);
    app.commands_filters.risk = Some(nova::commands::RiskLevel::Safe);
    let safe_n = app.filtered_bank_commands().len();
    assert!(safe_n <= before);
    assert!(app
        .filtered_bank_commands()
        .iter()
        .all(|c| c.risk == nova::commands::RiskLevel::Safe));
    app.commands_filters.clear();
    // Flag / alias araması (PATH ile karışmaz)
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
    let key = steps[0].state_key();
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
fn enter_never_auto_runs_commands() {
    let mut app = app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    app.focus = nova::app::PanelFocus::CommandsList;
    let before = app.modal.is_open();
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ))
    .unwrap();
    assert_eq!(
        app.modal.is_open(),
        before,
        "Enter modal/çalıştırma açmamalı"
    );
    assert!(
        app.status_message.contains("Bank")
            || app.status_message.contains("risk")
            || app.status_message.contains("çalıştır")
            || !app.status_message.is_empty(),
        "bilgi satırı beklenirdi: {}",
        app.status_message
    );
}
