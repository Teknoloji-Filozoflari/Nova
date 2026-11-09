//! Komut Rehberi sekmesi: görev odaklı IA · banka ≠ Mağaza ≠ PATH keşfi.

use nova::app::App;
use nova::catalog::Catalog;
use nova::commands::{CommandsBrowseMode, BANK_KIND, LOCAL_KIND};
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
        text.contains("Amaç") || text.contains("risk") || text.contains("Risk"),
        "amaç/risk yok:\n{text}"
    );
    assert!(
        text.contains("Güvenli örnek")
            || text.contains("Beklenen etki")
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
