//! Komut Rehberi sekmesi: Nova-Commands bankası ≠ Mağaza ≠ PATH keşfi.

use nova::app::App;
use nova::catalog::Catalog;
use nova::commands::{CommandsSource, BANK_KIND, LOCAL_KIND};
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
fn commands_tab_renders_bank_surface_distinct_from_store() {
    let mut app = app();
    app.select_commands_tab();
    let text = all_text(&render(&app, 150, 42));

    assert!(text.contains("Komut Rehberi"), "sekme etiketi yok:\n{text}");
    assert!(
        text.contains("Kaynak") || text.contains("Bilgi Bankası"),
        "{text}"
    );
    assert!(
        text.contains("Sistemimde Bulunan"),
        "local kaynak etiketi yok:\n{text}"
    );
    assert!(text.contains("Bank Kategorileri") || text.contains("Bank Komutları"));
    // Mağaza panelleri Komut Rehberi'nde açılmaz.
    assert!(
        !text.contains("Mağaza Kategorileri"),
        "Mağaza paneli Komut Rehberi'ne sızmamalı"
    );
    assert_eq!(app.commands_bank.kind, BANK_KIND);
    assert_ne!(
        app.commands_search_index.kind, BANK_KIND,
        "search-index ayrı kind"
    );
}

#[test]
fn commands_tab_local_source_uses_local_kind_not_bank() {
    let mut app = app();
    app.select_commands_tab();
    app.commands_source = CommandsSource::Local;
    app.focus = nova::app::PanelFocus::CommandsList;
    let text = all_text(&render(&app, 150, 42));

    assert!(text.contains("Sistemimde Bulunan"), "{text}");
    assert!(text.contains("PATH") || text.contains("Keşfi"), "{text}");
    assert_eq!(app.commands_local.kind, LOCAL_KIND);
    assert_ne!(app.commands_local.kind, BANK_KIND);
    assert!(app.commands_local.stub);
}

#[test]
fn commands_detail_shows_examples_risk_and_packages() {
    let mut app = app();
    app.select_commands_tab();
    // Geniş: sağ detay paneli (banka: 2 ikincil sütun → ~168+ sütun gerekir).
    app.terminal_size = Some((180, 42));
    app.narrow_mode = false;
    // İlk kategoride örnekleri/paketi olan bir komut seç.
    let cmds = app.filtered_bank_commands();
    assert!(!cmds.is_empty());
    // Tercihen örnekleri olan kayda git.
    if let Some(idx) = cmds.iter().position(|c| !c.examples.is_empty()) {
        app.commands_entry_index = idx;
    }
    let text = all_text(&render(&app, 180, 42));
    assert!(
        text.contains("risk")
            || text.contains("Risk")
            || text.contains("güvenli")
            || text.contains("dikkat")
            || text.contains("tehlikeli"),
        "risk etiketi yok:\n{text}"
    );
    // Dual-concept footer.
    assert!(
        text.contains("nova-commands-bank") || text.contains("kürate") || text.contains("Mağaza"),
        "{text}"
    );
}

#[test]
fn commands_search_spans_categories_and_keeps_sources_separate() {
    let mut app = app();
    app.select_commands_tab();
    app.search_query = "ls".into();
    let bank_n = app.filtered_bank_commands().len();
    assert!(bank_n > 0, "bank araması boş olmamalı");
    // En azından arama sonuçları listelenir; Local ile birleştirilmez.
    app.commands_source = CommandsSource::Local;
    let _local_n = app.filtered_local_commands().len();
    // kind'lar karışmaz.
    assert_eq!(app.commands_bank.kind, BANK_KIND);
    assert_eq!(app.commands_local.kind, LOCAL_KIND);
}

#[test]
fn command_favorites_are_separate_from_tool_and_store() {
    let mut app = app();
    app.select_commands_tab();
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
    // Local kaynakta F bank favorisi eklemez.
    app.commands_source = CommandsSource::Local;
    let before = app.state.file.command_favorites.len();
    app.handle_key(f).unwrap();
    assert_eq!(app.state.file.command_favorites.len(), before);
}

#[test]
fn tab_order_is_guide_then_store_then_settings() {
    let mut app = app();
    // Tüm araç kategorilerini kurulu işaretle ki görünür olsun.
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
