//! Mağaza sekmesinin yerleşimini doğrulayan çizim (render) testleri.
//!
//! Mağaza, ayrı bir modal değil; ana ekranın sekme listesindeki son satırdır.
//! Yeterince geniş terminallerde ekran **dört sütuna** döner: sekme listesi,
//! mağaza kategorileri, uygulamalar ve sağ detay. Bu testler `TestBackend`
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
    // Uygulama listesi başlığı seçili kategoriye göre değişir.
    assert!(text.contains("AI ("), "uygulama listesi başlığı yok");
    // Geniş düzende detay sağ paneldedir (Borders::ALL başlığı).
    assert!(
        text.lines()
            .any(|l| l.contains("Mağaza Detay") && l.contains('─')),
        "sağ detay paneli kenarlıklı olmalı"
    );
}

#[test]
fn store_tab_is_above_settings_in_the_category_panel() {
    let mut app = app();
    app.select_store_tab();
    let lines = render(&app, 150, 42);
    let text = all_text(&lines);

    assert!(text.contains("Mağaza ("), "sol panelde Mağaza yok");
    assert!(text.contains("Ayarlar"), "sol panelde Ayarlar yok");
    app.select_settings_tab();
    let lines = render(&app, 150, 42);
    let text = all_text(&lines);
    assert!(
        text.contains("[Nova güncelle]") || text.contains("Nova güncelle"),
        "Ayarlar Nova güncelle düğmesi yok:\n{text}"
    );
    assert!(
        text.contains("[Mağaza kataloğu]") || text.contains("Mağaza kataloğu"),
        "Ayarlar Mağaza kataloğu düğmesi yok"
    );
    assert!(
        text.contains("[Özel araç ekle]") || text.contains("Özel araç"),
        "Ayarlar özel araç düğmesi yok"
    );
    assert!(text.contains("[Tema]"), "Ayarlar Tema düğmesi yok");
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

#[test]
fn store_detail_shows_trust_badge() {
    let mut app = app();
    app.select_store_tab();
    // AI kategorisindeki ilk kayıtlar pipx/npm → trust class C.
    let lines = render(&app, 150, 42);
    let text = all_text(&lines);
    assert!(text.contains("Güven:"), "detayda Güven satırı yok");
    assert!(
        text.contains("C —")
            || text.contains("D —")
            || text.contains("A —")
            || text.contains("B —"),
        "A/B/C/D güven sınıfı detayda görünmeli: {text}"
    );
    // Liste satırında eski [manual]/[hash] rozeti olmamalı.
    assert!(
        !text.contains("[manual]") && !text.contains("[hash]") && !text.contains("[hash+sig]"),
        "liste rozetleri kaldırılmış olmalı"
    );

    // Dosya ve Arama: ripgrep gibi download_archive (imzasız) → D + hash.
    let target = nova::store::STORE_CATEGORIES
        .iter()
        .position(|c| *c == "Dosya ve Arama")
        .expect("kategori var");
    app.store_category_index = target;
    app.store_app_index = 0;
    let lines = render(&app, 150, 42);
    let text = all_text(&lines);
    assert!(!text.contains("[hash]"), "liste satırında [hash] olmamalı");
    assert!(text.contains("Güven:"), "detayda Güven satırı yok");
    assert!(
        text.contains("D —") || text.contains("B —") || text.contains("(hash"),
        "arşiv güven sınıfı/bütünlük detayda görünmeli"
    );
}

#[test]
fn installed_tools_wide_layout_puts_detail_on_the_side_like_store() {
    let app = app();
    let lines = render(&app, 120, 36);
    let text = all_text(&lines);
    assert!(text.contains("Detay"), "kurulu detay yok:\n{text}");
    // Yan panel Borders::ALL: başlık satırında yatay çizgi.
    assert!(
        text.lines().any(|l| l.contains("Detay") && l.contains('─')),
        "kurulu yan detay kenarlıklı olmalı (Mağaza gibi):\n{text}"
    );
    assert!(
        nova::ui::tool_detail_side_layout(true, 120, false),
        "120 sütunda yan detay açılmalı"
    );
}

#[test]
fn job_progress_modal_renders_phase_bar_and_log_inside_nova() {
    let mut app = app();
    app.modal = nova::app::modal::Modal::JobProgress {
        kind: nova::progress::JobKind::NovaRepo,
        title: "Nova kaynak güncellemesi".into(),
        command: "cargo install --git … --force --locked".into(),
        phase: "Derleniyor".into(),
        status_line: "Derleniyor: nova".into(),
        log: vec![
            "$ cargo install --git …".into(),
            "   Compiling nova v0.1.0".into(),
        ],
        log_scroll: 0,
        spinner: 3,
        done: None,
    };
    let lines = render(&app, 100, 30);
    let text = all_text(&lines);
    assert!(text.contains("İlerleme"), "ilerleme modalı yok:\n{text}");
    assert!(text.contains("Derleniyor"), "faz yok:\n{text}");
    assert!(text.contains("Compiling nova"), "günlük yok:\n{text}");
    assert!(
        text.contains('█') || text.contains('░'),
        "ilerleme çubuğu yok:\n{text}"
    );
}
