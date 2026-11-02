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
fn commands_tab_renders_bank_surface_distinct_from_store() {
    let mut app = app();
    app.select_commands_tab();
    let text = all_text(&render(&app, 150, 42));

    assert!(text.contains("Komut Rehberi"), "sekme etiketi yok:\n{text}");
    assert!(text.contains("Bilgi Bankası"), "bank kaynağı yok:\n{text}");
    assert!(
        text.contains("Sistemimde Bulunan"),
        "local kaynak etiketi yok:\n{text}"
    );
    assert!(text.contains("Bank Kategorileri") || text.contains("Komutlar"));
    // Mağaza panelleri Komut Rehberi'nde açılmaz.
    assert!(
        !text.contains("Mağaza Kategorileri"),
        "Mağaza paneli Komut Rehberi'ne sızmamalı"
    );
    assert_eq!(app.commands_bank.kind, BANK_KIND);
}

#[test]
fn commands_tab_local_source_uses_local_kind_not_bank() {
    let mut app = app();
    app.select_commands_tab();
    app.commands_source = CommandsSource::Local;
    app.focus = nova::app::PanelFocus::CommandsList;
    let text = all_text(&render(&app, 150, 42));

    assert!(text.contains("Sistemimde Bulunan"), "{text}");
    assert_eq!(app.commands_local.kind, LOCAL_KIND);
    assert_ne!(app.commands_local.kind, BANK_KIND);
    assert!(app.commands_local.stub);
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
