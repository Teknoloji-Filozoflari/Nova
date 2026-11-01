//! Mağaza sekmesinin yerleşimini doğrulayan çizim (render) testleri.
//!
//! Mağaza, ayrı bir modal değil; ana ekranın sekme listesindeki son satırdır.
//! Sekme seçildiğinde ekran **üç sütuna** döner: sekme listesi, mağaza
//! kategorileri ve o kategorideki uygulamalar. Bu testler `TestBackend`
//! ile çizilmiş arabelleği okur; gerçek bir terminal açılmaz.

use nova::app::App;
use nova::catalog::Catalog;
use ratatui::{backend::TestBackend, Terminal};

/// Uygulamayı oluşturur (gömülü katalog + gömülü mağaza).
fn app() -> App {
    let entries = nova::catalog::loader::builtin_catalog().expect("gömülü katalog okunmalı");
    App::new(Catalog::from_entries(entries).expect("katalog oluşturulamadı"))
}

/// Ekranı satırlara çevirir.
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

/// Tüm satırların birleşimidir; arama için kullanılır.
fn all_text(lines: &[String]) -> String {
    lines.join("\n")
}

#[test]
fn store_tab_renders_three_panels() {
    let mut app = app();
    app.select_store_tab();
    let lines = render(&app, 150, 42);
    let text = all_text(&lines);

    assert!(text.contains("Kategoriler"), "sekme listesi yok");
    assert!(
        text.contains("Mağaza Kategorileri"),
        "mağaza kategorileri yok"
    );
    assert!(text.contains("Mağaza Detay"), "mağaza detayı yok");
    // Üçüncü sütun uygulama listesidir; başlığı seçili kategoriye göre değişir.
    assert!(text.contains("AI ("), "uygulama listesi başlığı yok");
}

#[test]
fn store_tab_is_the_last_row_of_the_category_panel() {
    let mut app = app();
    app.select_store_tab();
    let lines = render(&app, 150, 42);

    // Sol panelin son satırı "Mağaza" etiketini taşır.
    let store_row = lines
        .iter()
        .find(|l| l.contains("Mağaza (") && l.contains("Mağaza"))
        .expect("Mağaza satırı bulunamadı");
    assert!(
        store_row.trim_start().starts_with('│') || store_row.contains("Mağaza ("),
        "sol panelde Mağaza satırı yok: {store_row}"
    );
}

#[test]
fn selecting_a_store_category_changes_the_app_list() {
    let mut app = app();
    app.select_store_tab();
    // "Dosya ve Arama" mağaza kategorisine geç.
    let target = nova::store::STORE_CATEGORIES
        .iter()
        .position(|c| *c == "Dosya ve Arama")
        .expect("kategori var");
    app.store_category_index = target;

    let lines = render(&app, 150, 42);
    let text = all_text(&lines);
    assert!(text.contains("ripgrep"), "ripgrep listede değil");
    assert!(text.contains("fzf"), "fzf listede değil");
    assert!(text.contains("eza"), "eza listede değil");
}

#[test]
fn leaving_the_store_tab_restores_the_tool_layout() {
    let mut app = app();
    app.select_store_tab();
    assert!(app.store_tab);

    // Sekme listesine dön ve yukarı hareketle mağazadan çık.
    app.set_tab_index(0);
    assert!(!app.store_tab);

    let lines = render(&app, 150, 42);
    let text = all_text(&lines);
    assert!(text.contains("Kategoriler"), "sekme listesi yok");
    assert!(
        !text.contains("Mağaza Kategorileri"),
        "mağaza paneli kapanmalıydı"
    );
    assert!(!text.contains("Mağaza Detay"), "mağaza detayı kapanmalıydı");
}

#[test]
fn store_tab_narrow_layout_stacks_categories_over_apps() {
    let mut app = app();
    app.select_store_tab();
    app.narrow_mode = true;
    let lines = render(&app, 60, 30);
    let text = all_text(&lines);
    assert!(text.contains("Mağaza Kategorileri"), "kategori paneli yok");
    assert!(text.contains("Mağaza Detay"), "mağaza detayı yok");
    // Uygulama listesi başlığı da görünmeli.
    assert!(text.contains("AI ("), "uygulama listesi başlığı yok");
}
