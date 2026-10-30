//! Liste satırlarının panel sınırları içinde kaldığını doğrulayan
//! çizim (render) testleri.
//!
//! Gerçek uygulamada `btop`/`Claude Code` gibi uzun adlar durum ve açıklama
//! sütunlarıyla çakışıyor, satır metni panel kenarlığının üzerine biniyordu.
//! Bu testler çizilmiş arabelleği okuyup her satırın gerçekten panel içinde
//! bittiğini sınar; `TestBackend` kullanıldığı için gerçek bir terminal
//! açılmaz.

use nova::app::App;
use nova::catalog::Catalog;
use ratatui::{backend::TestBackend, Terminal};

/// Uzun adlı ve uzun açıklamalı bir katalog: taşma hatasını en kolay
/// tetikleyen örnekler.
fn app_with_long_names() -> App {
    let mut entries = nova::catalog::loader::builtin_catalog().expect("gömülü katalog okunmalı");
    // Gömülü katalogdaki "Claude Code" kaydını uzun adla değiştir.
    for entry in entries.iter_mut() {
        if entry.name == "Claude Code" {
            entry.name = "Claude Code Supervisor Üst Düzey Ajan Araç Kutusu".into();
        }
    }
    App::new(Catalog::from_entries(entries).expect("katalog oluşturulamadı"))
}

fn app_with_custom_long_name() -> App {
    let mut app = app_with_long_names();
    app.catalog
        .insert_entry(nova::config::ToolEntry {
            id: "uzun-ad".into(),
            name: "Çok Uzun Bir Özel Araç Adı Deneme Amaçlı Buraya Yazıldı".into(),
            description: "Uzun açıklama: bu metin panelin sağ kenarını kesinlikle aşmamalı.".into(),
            category: "Özel Araçlar".into(),
            executable: "man".into(),
            ..nova::config::ToolEntry::default()
        })
        .expect("test aracı eklenmeli");
    // "Özel Araçlar" kategorisine geç ki uzun ad ekranda görünsün.
    app.category_index = nova::catalog::CATEGORIES
        .iter()
        .position(|c| *c == "Özel Araçlar")
        .expect("kategori var");
    app
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

/// Araç panelinin sağ kenarlık sütununu bulur (ilk satırdaki en sağdaki `┐`).
fn tools_panel_right_edge(screen: &[String]) -> Option<usize> {
    screen
        .iter()
        .find_map(|row| row.rfind('┐').filter(|_| row.contains(" ─")))
}

#[test]
fn tool_rows_never_overrun_the_panel_at_any_width() {
    for app in [app_with_long_names(), app_with_custom_long_name()] {
        for width in [40u16, 45, 52, 69, 70, 80, 100, 120, 160] {
            let screen = render(&app, width, 30);
            let Some(edge) = tools_panel_right_edge(&screen) else {
                // Dar düzende araç paneli görünmüyorsa taşma da olmaz.
                continue;
            };
            for (y, row) in screen.iter().enumerate() {
                // Kenarlığın sağında metin varsa satır paneldan taşmıştır.
                let tail: String = row
                    .chars()
                    .skip(edge + 1)
                    .collect::<String>()
                    .trim_end()
                    .to_string();
                // Detay/yardım satırları ve durum satırı paneldan bağımsızdır;
                // yalnızca araç satırları (favori yıldızıyla başlayanlar) sınanır.
                let is_tool_row = row.contains('☆') || row.contains('★');
                if is_tool_row {
                    assert!(
                        tail.is_empty(),
                        "genişlik {width}, satır {y}: panel kenarı aşıldı\n{}",
                        screen_join(&screen)
                    );
                }
            }
        }
    }
}

fn screen_join(screen: &[String]) -> String {
    screen.join("\n")
}

#[test]
fn tool_row_widths_fit_within_the_available_space() {
    // İçerik, kenarlık dâhil verilen her genişlikte sığmalı.
    // Sabit ön ek (vurgu + favori + durum) tek başına 6 sütun tutar; bu
    // eşik altında ratatui satırı kırpar, taşma olmaz. Kenarlıklar düşünce
    // en az 6 sütun kalan durumlar gerçekten anlamlı olanlardır.
    let fixed = nova::ui::TOOL_ROW_FIXED_WIDTH;
    for width in 2..=200u16 {
        let inner = width.saturating_sub(2) as usize;
        if inner < fixed {
            continue;
        }
        let (name, desc) = nova::ui::tool_row_widths(inner, 24);
        let sep = if desc > 0 { 3 } else { 0 };
        let used = fixed + name + sep + desc;
        assert!(
            used <= inner,
            "genişlik {width}: kullanılan {used} > içerik {inner} (ad {name}, açıklama {desc})"
        );
    }
}

#[test]
fn long_name_is_shortened_but_description_keeps_room() {
    let (name, desc) = nova::ui::tool_row_widths(70, 24);
    assert!(name <= 24, "ad sınırsız: {name}");
    assert!(desc >= 8, "açıklamaya yer kalmadı: {desc}");
}
