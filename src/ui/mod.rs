//! Kullanıcı arayüzü modülü.
//! Ratatui tabanlı terminal arayüzünün çizim işlevlerini içerir.
//!
//! Düzen terminal boyutuna göre üç kademede uyarlanır:
//! - **Geniş (≥ 70 sütun):** kategori paneli solda, araç listesi sağda.
//! - **Orta (45–69 sütun):** tek sütun; kategori başlık satırında gösterilir,
//!   araç listesi ve detay alanı görünür.
//! - **Dar (< 45 sütun):** yalnızca araç listesi; detay ve büyük yardım
//!   paneli gizlenir, tek satırlık kısa yardım gösterilir.
//!
//! Hiçbir metin bayt indeksiyle kesilmez; `truncate` karakter sayarak çalıştığı
//! için Türkçe karakterlerde panik oluşmaz.

pub mod modal;
pub mod theme;

pub use theme::{ThemePalette, UiTheme};

use crate::app::{App, PanelFocus};
use crate::catalog::{short_category_name, CATEGORIES};
use crate::detection::InstalledStatus;
use crate::store::{SETTINGS_TAB_LABEL, STORE_CATEGORIES, STORE_TAB_LABEL};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

/// Uygulamanın etkin tema paleti.
fn palette(app: &App) -> ThemePalette {
    app.ui_theme().palette()
}

/// Bu genişliğin altında iki panel yan yana sığmaz.
pub const WIDE_MIN_WIDTH: u16 = 70;
/// Bu genişliğin altında detay alanı gizlenir.
pub const MEDIUM_MIN_WIDTH: u16 = 45;
/// Bu yüksekliğin altında yardım paneli tek satıra iner.
pub const TALL_MIN_HEIGHT: u16 = 14;
/// Bu genişliğin altında uzun yardım ipuçları sığmaz.
const TIGHT_HINT_WIDTH: u16 = 52;
/// Yardım açıkken araç listesine bırakılan en az satır.
pub const CONTENT_MIN_HEIGHT: u16 = 5;
/// Yardım panelinin tam tuş tablosu için ayırabileceği en fazla satır.
pub const HELP_HELP_MAX_HEIGHT: u16 = 10;
/// Sol panelin (sekme listesi) genişliği.
pub const CATEGORY_PANEL_WIDTH: u16 = 26;
/// Mağaza kategori panelinin genişliği (üç sütunlu mağaza görünümünde).
pub const STORE_CATEGORY_PANEL_WIDTH: u16 = 26;
/// Araç detay panelinin satır sayısı (üst kenarlık dâhil).
pub const DETAIL_ROWS: u16 = 8;
/// Mağaza detay panelinin satır sayısı (üst kenarlık dâhil).
///
/// Mağaza detayı araç detayından daha fazla bilgi taşır (sürüm, canlı takip,
/// kurulum yöntemi); bu yüzden biraz daha yüksektir.
pub const STORE_DETAIL_ROWS: u16 = 11;

/// Fare tıklama testi için hesaplanan düzen alanları. `ui::draw` ile aynı
/// eşikleri kullandığı iki kod yolu senkron kalır.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClickAreas {
    /// İçerik alanının ilk satırı (başlık + arama satırından sonra).
    pub content_top: u16,
    /// İçerik alanının yüksekliği (satır).
    pub content_height: u16,
    /// Geniş mod (iki panel) mı?
    pub wide: bool,
    /// Kategori panelinin bitiş sütunu (hariç); dar modda tüm genişlik.
    pub category_end_x: u16,
    /// Mağaza kategori panelinin bitiş sütunu (hariç). Yalnızca mağaza
    /// sekmesinde ve geniş modda anlamlıdır.
    pub store_category_end_x: u16,
    /// Mağaza kategori panelinin bittiği satır (hariç). Yalnızca mağaza
    /// sekmesinde ve dar modda anlamlıdır (kategoriler üstte).
    pub store_category_end_y: u16,
}

/// Terminal boyutuna göre fare tıklama alanlarını hesaplar.
pub fn click_areas(size: (u16, u16), show_help: bool, store_tab: bool) -> ClickAreas {
    let (w, h) = size;
    let wide = w >= WIDE_MIN_WIDTH;
    let show_detail = w >= MEDIUM_MIN_WIDTH && h >= 10;
    let tall = h >= TALL_MIN_HEIGHT;
    let detail_h = if show_detail {
        if store_tab {
            STORE_DETAIL_ROWS.min(h.saturating_sub(CONTENT_MIN_HEIGHT + 3))
        } else {
            DETAIL_ROWS
        }
    } else {
        0
    };
    let help_height = if show_help {
        let reserved = 1 + 1 + CONTENT_MIN_HEIGHT + if show_detail { 2 } else { 0 } + 1;
        let reserved = if store_tab {
            1 + 1 + CONTENT_MIN_HEIGHT + detail_h + 1
        } else {
            reserved
        };
        h.saturating_sub(reserved).clamp(1, HELP_HELP_MAX_HEIGHT)
    } else if tall {
        3
    } else {
        1
    };
    let content_top = 2; // başlık(1) + arama(1)
    let content_bottom = h.saturating_sub(detail_h + 1 + help_height);
    let content_height = content_bottom.saturating_sub(content_top);
    let category_end_x = if wide { CATEGORY_PANEL_WIDTH } else { w };
    // Dar modda mağaza kategorileri içerik alanının üstünde durur.
    let store_category_rows =
        (STORE_CATEGORIES.len() as u16 + 2).min(content_height.saturating_sub(3));
    ClickAreas {
        content_top,
        content_height,
        wide,
        category_end_x,
        store_category_end_x: CATEGORY_PANEL_WIDTH + STORE_CATEGORY_PANEL_WIDTH,
        store_category_end_y: content_top + store_category_rows,
    }
}

// Düzen eşikleri birbirini mantıksal olarak izlemeli; ihlal derleme hatasıdır.
const _: () = assert!(WIDE_MIN_WIDTH > MEDIUM_MIN_WIDTH);
const _: () = assert!(TIGHT_HINT_WIDTH < WIDE_MIN_WIDTH);
const _: () = assert!(TIGHT_HINT_WIDTH > 20);

/// Metni en fazla `max` karakter olacak şekilde keser (UTF-8 güvenli).
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// Durum satırındaki mesajın rengi.
///
/// (Genel API: entegrasyon testleri renk ayrımını doğrudan sınar.)
///
/// Nova'nın ürettiği mesajlar sabit biçimdedir: başarı `✓ ` ile, hata
/// `Hata: ` ile başlar. Diğer her şey (nötr bilgi, onay istemi) sarıdır.
pub fn message_color(message: &str) -> Color {
    if message.starts_with('✓') {
        Color::Green
    } else if message.starts_with("Hata:") {
        Color::Red
    } else {
        Color::Yellow
    }
}

/// Mesajın simgesi (varsa).
fn message_symbol(message: &str) -> Option<&'static str> {
    if message.starts_with('✓') {
        Some("✓")
    } else if message.starts_with("Hata:") {
        Some("✗")
    } else {
        None
    }
}

/// Kurulu durumuna göre renk.
fn status_color(status: InstalledStatus) -> Color {
    match status {
        InstalledStatus::Installed => Color::Green,
        InstalledStatus::Missing => Color::Red,
        InstalledStatus::Unknown => Color::Yellow,
    }
}

/// TUI ekranını çizer.
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Açık temada yüzey boyanır; aksi hâlde kategori etiketleri (siyah metin)
    // koyu emülatör arka planında kaybolur.
    let pal = palette(app);
    if let Some(bg) = pal.surface {
        f.render_widget(Clear, area);
        f.render_widget(
            Block::default().style(Style::default().bg(bg).fg(pal.text)),
            area,
        );
    }

    let wide = area.width >= WIDE_MIN_WIDTH;
    let show_detail = area.width >= MEDIUM_MIN_WIDTH && area.height >= 10;
    let tall = area.height >= TALL_MIN_HEIGHT;

    // Detay panelinin yüksekliği. Mağaza detayı araç detayından daha fazla
    // bilgi taşır (sürüm, canlı takip, kurulum yöntemi); ekrana
    // sığmayacak kadar büyürse içerik alanı korunacak şekilde kırpılır.
    let detail_h = if show_detail {
        if app.store_tab {
            STORE_DETAIL_ROWS.min(area.height.saturating_sub(CONTENT_MIN_HEIGHT + 3))
        } else {
            DETAIL_ROWS
        }
    } else {
        0
    };

    // Yardım açıkken panel tuş tablosuna, dosya yollarına ve etkin çalışma
    // dizinine yer açar. Yükseklik ekranla birlikte büyür ama araç listesine
    // en az `CONTENT_MIN_HEIGHT` satır bırakır; küçük terminallerde ise
    // satırlar kırpılır (bkz. `draw_help`).
    //
    // Araç sekmesinde detay panelinin yüksekliği yardımın büyümesine izin
    // verilen alandan düşülmez (mevcut davranış korunur); mağaza sekmesinde
    // detay daha yüksek olduğu için gerçekten düşülür.
    let help_height = if app.show_help {
        let reserved = 1 + 1 + CONTENT_MIN_HEIGHT + if show_detail { 2 } else { 0 } + 1;
        let reserved = if app.store_tab {
            1 + 1 + CONTENT_MIN_HEIGHT + detail_h + 1
        } else {
            reserved
        };
        area.height
            .saturating_sub(reserved)
            .clamp(1, HELP_HELP_MAX_HEIGHT)
    } else if tall {
        3
    } else {
        1
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),           // başlık
            Constraint::Length(1),           // arama satırı
            Constraint::Min(3),              // içerik
            Constraint::Length(detail_h),    // detay
            Constraint::Length(1),           // durum
            Constraint::Length(help_height), // yardım
        ])
        .split(area);

    draw_header(f, app, chunks[0]);
    draw_search(f, app, chunks[1]);
    draw_content(f, app, chunks[2], wide);
    if show_detail {
        draw_detail(f, app, chunks[3]);
    }
    draw_status(f, app, chunks[4]);
    draw_help(f, app, chunks[5], tall);

    // Modal en son çizilir: altındaki içeriği örter, ana ekran tuşları da
    // modal açıkken işlenmez (bkz. `App::handle_key`).
    modal::draw(f, app);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let accent = palette(app).accent;
    let title = Paragraph::new(Line::from(Span::styled(
        "Nova",
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center);
    f.render_widget(title, area);
}

fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let (text, style) = if app.search_active || !app.search_query.is_empty() {
        (
            format!("/ {}", app.search_query),
            Style::default().fg(Color::Yellow),
        )
    } else if area.width < TIGHT_HINT_WIDTH {
        (
            "/ ara  c favori  ?".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (
            "/ ara   c favoriler   ? yardım".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    };
    let text = truncate(&text, area.width.saturating_sub(1) as usize);
    f.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
}

fn draw_content(f: &mut Frame, app: &App, area: Rect, wide: bool) {
    if app.store_tab {
        draw_store_content(f, app, area, wide);
    } else if app.settings_tab {
        if wide {
            if app.theme_picker_open {
                let panels = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(CATEGORY_PANEL_WIDTH),
                        Constraint::Length(STORE_CATEGORY_PANEL_WIDTH),
                        Constraint::Min(20),
                    ])
                    .split(area);
                draw_categories(f, app, panels[0]);
                draw_settings_panel(f, app, panels[1]);
                draw_theme_picker(f, app, panels[2]);
            } else {
                let panels = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(CATEGORY_PANEL_WIDTH),
                        Constraint::Min(20),
                    ])
                    .split(area);
                draw_categories(f, app, panels[0]);
                draw_settings_panel(f, app, panels[1]);
            }
        } else if app.theme_picker_open {
            let half = (area.height / 2).max(3);
            let panels = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(half), Constraint::Min(3)])
                .split(area);
            draw_settings_panel(f, app, panels[0]);
            draw_theme_picker(f, app, panels[1]);
        } else {
            draw_settings_panel(f, app, area);
        }
    } else if wide {
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(CATEGORY_PANEL_WIDTH),
                Constraint::Min(20),
            ])
            .split(area);
        draw_categories(f, app, panels[0]);
        draw_tools(f, app, panels[1]);
    } else {
        // Tek sütun: kategori seçimi başlıkta özetlenir.
        draw_tools(f, app, area);
    }
}

/// Mağaza sekmesinin içerik alanı.
///
/// Geniş terminalde **üç sütun**: sekme listesi (araç kategorileri +
/// mağaza sekmesi), mağaza kategorileri ve o kategorideki uygulamalar.
/// Dar terminalde yatay üç sütun sığmaz; mağaza kategorileri üste,
/// uygulamalar alta yerleşir.
fn draw_store_content(f: &mut Frame, app: &App, area: Rect, wide: bool) {
    if wide {
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(CATEGORY_PANEL_WIDTH),
                Constraint::Length(STORE_CATEGORY_PANEL_WIDTH),
                Constraint::Min(20),
            ])
            .split(area);
        draw_categories(f, app, panels[0]);
        draw_store_categories(f, app, panels[1]);
        draw_store_apps(f, app, panels[2]);
    } else {
        // Uygulama listesine en az üç satır bırakılır.
        let cat_height = (STORE_CATEGORIES.len() as u16 + 2)
            .min(area.height.saturating_sub(3))
            .max(1);
        let panels = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(cat_height), Constraint::Min(3)])
            .split(area);
        draw_store_categories(f, app, panels[0]);
        draw_store_apps(f, app, panels[1]);
    }
}

fn draw_categories(f: &mut Frame, app: &App, area: Rect) {
    let pal = palette(app);
    // Yalnızca kurulu araç içeren kategoriler gösterilir (boşlar gizlenir).
    let mut items: Vec<ListItem> = app
        .visible_category_indices()
        .into_iter()
        .map(|i| {
            let cat = CATEGORIES[i];
            let count = app
                .catalog
                .by_category(cat)
                .into_iter()
                .filter(|e| {
                    e.status == InstalledStatus::Installed
                        && app.matches_search(e)
                        && (!app.favorite_only || e.favorite)
                })
                .count();
            let selected = i == app.category_index && !app.store_tab && !app.settings_tab;
            let focused = selected && app.focus == PanelFocus::Category;
            let style = if focused {
                Style::default()
                    .fg(pal.accent_on)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(pal.accent)
            } else if count == 0 {
                Style::default().fg(pal.muted)
            } else {
                Style::default().fg(pal.text)
            };
            let prefix = if selected { "▶ " } else { "  " };
            let label = truncate(
                &format!("{} ({count})", short_category_name(cat).unwrap_or(cat)),
                area.width.saturating_sub(6) as usize,
            );
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(label, style),
            ]))
        })
        .collect();

    // Mağaza, ardından Ayarlar.
    let store_count = app.store.entries().len();
    let store_selected = app.store_tab;
    let store_focused = store_selected && app.focus == PanelFocus::Category;
    let store_style = if store_focused {
        Style::default()
            .fg(pal.accent_on)
            .bg(pal.accent)
            .add_modifier(Modifier::BOLD)
    } else if store_selected {
        Style::default().fg(pal.accent)
    } else if store_count == 0 {
        Style::default().fg(pal.muted)
    } else {
        Style::default().fg(pal.text)
    };
    let store_label = truncate(
        &format!("{STORE_TAB_LABEL} ({store_count})"),
        area.width.saturating_sub(6) as usize,
    );
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            if store_selected { "▶ " } else { "  " },
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(store_label, store_style),
    ])));

    let settings_selected = app.settings_tab;
    let settings_focused = settings_selected && app.focus == PanelFocus::Category;
    let settings_style = if settings_focused {
        Style::default()
            .fg(pal.accent_on)
            .bg(pal.accent)
            .add_modifier(Modifier::BOLD)
    } else if settings_selected {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.text)
    };
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            if settings_selected { "▶ " } else { "  " },
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate(SETTINGS_TAB_LABEL, area.width.saturating_sub(6) as usize),
            settings_style,
        ),
    ])));

    let border = if app.focus == PanelFocus::Category {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    // Satır stilleri zaten seçimi gösteriyor; List’in varsayılan REVERSED
    // vurgusu açık temada kategori adlarını siler — kapatılır.
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(" Kategoriler "),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.selected_tab_index()));
    f.render_stateful_widget(list, area, &mut state);
}

/// Ayarlar sekmesi: Güncelle + Tema düğmeleri.
fn draw_settings_panel(f: &mut Frame, app: &App, area: Rect) {
    let pal = palette(app);
    let theme = app.ui_theme();
    let theme_hint = if app.theme_picker_open {
        format!("Şu an: {} — sağ panelden seç", theme.label())
    } else {
        format!(
            "Şu an: {} ({}) — Enter ile seçenekler",
            theme.label(),
            theme.id()
        )
    };
    let actions = [
        (
            "Güncelle",
            "GitHub Releases ikilisi — tema için cargo rebuild gerekir",
        ),
        ("Tema", theme_hint.as_str()),
    ];
    let items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, (title, hint))| {
            let selected = i == app.settings_action_index;
            let focused = selected && app.focus == PanelFocus::Tool;
            let style = if focused {
                Style::default()
                    .fg(pal.accent_on)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(pal.accent)
            } else {
                Style::default().fg(pal.text)
            };
            let prefix = if selected { "▶ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(format!("[{title}]  "), style),
                Span::styled((*hint).to_string(), Style::default().fg(pal.muted)),
            ]))
        })
        .collect();
    let border = if app.focus == PanelFocus::Tool {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(" Ayarlar "),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.settings_action_index));
    f.render_stateful_widget(list, area, &mut state);
}

/// Tema seçici yan paneli (Ayarlar → Tema).
fn draw_theme_picker(f: &mut Frame, app: &App, area: Rect) {
    let pal = palette(app);
    let current = app.ui_theme();
    let items: Vec<ListItem> = UiTheme::all()
        .iter()
        .enumerate()
        .map(|(i, theme)| {
            let selected = i == app.theme_picker_index;
            let focused = selected && app.focus == PanelFocus::ThemePicker;
            let style = if focused {
                Style::default()
                    .fg(pal.accent_on)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(pal.accent)
            } else {
                Style::default().fg(pal.text)
            };
            let mark = if *theme == current { "● " } else { "○ " };
            let prefix = if selected { "▶ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(mark, Style::default().fg(pal.accent)),
                Span::styled(format!("{} ({})", theme.label(), theme.id()), style),
            ]))
        })
        .collect();
    let border = if app.focus == PanelFocus::ThemePicker {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(" Tema seç "),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.theme_picker_index));
    f.render_stateful_widget(list, area, &mut state);
}

/// Mağaza kategorileri panelini çizer.
fn draw_store_categories(f: &mut Frame, app: &App, area: Rect) {
    let pal = palette(app);
    let items: Vec<ListItem> = STORE_CATEGORIES
        .iter()
        .enumerate()
        .map(|(i, cat)| {
            let count = app.store.by_category(cat).len();
            let selected = i == app.store_category_index;
            let focused = selected && app.focus == PanelFocus::StoreCategory;
            let style = if focused {
                Style::default()
                    .fg(pal.accent_on)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(pal.accent)
            } else if count == 0 {
                Style::default().fg(pal.muted)
            } else {
                Style::default().fg(pal.text)
            };
            let prefix = if selected { "▶ " } else { "  " };
            // Panelin iç genişliği (kenarlıklar düşülmüş) ve "▶ " öneki
            // hesaba katılır; kategori adı ile sayı birlikte sığmalıdır.
            let label = truncate(
                &format!("{cat} ({count})"),
                area.width.saturating_sub(4) as usize,
            );
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(label, style),
            ]))
        })
        .collect();

    let border = if app.focus == PanelFocus::StoreCategory {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(" Mağaza Kategorileri "),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.store_category_index));
    f.render_stateful_widget(list, area, &mut state);
}

/// Mağaza uygulama listesi panelini çizer.
fn draw_store_apps(f: &mut Frame, app: &App, area: Rect) {
    let apps = app.filtered_store_apps();
    let inner_width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = apps
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let selected = i == app.store_app_index;
            let focused = selected && app.focus == PanelFocus::StoreApp;
            let style = if focused {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            let prefix = if selected { "▶ " } else { "  " };

            // Durum işareti: yeni sürüm / PM güncellemesi varsa ↑, kuruluysa ✓.
            let has_update = app.store_has_pending_update(&entry.id);
            let (marker, marker_style) = if has_update {
                ("↑ ", Style::default().fg(Color::Yellow))
            } else if app.store_status(&entry.id) == crate::detection::InstalledStatus::Installed {
                ("✓ ", Style::default().fg(Color::Green))
            } else {
                ("· ", Style::default().fg(Color::DarkGray))
            };

            let name = truncate(&entry.name, inner_width.saturating_sub(6));
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(marker, marker_style),
                Span::styled(name, style),
            ]))
        })
        .collect();

    let title = if !app.search_query.is_empty() {
        format!(
            " 🔍 '{}' ({}) ",
            truncate(&app.search_query, 20),
            apps.len()
        )
    } else {
        format!(
            " {} ({}) ",
            truncate(app.selected_store_category(), 20),
            apps.len()
        )
    };

    let border = if app.focus == PanelFocus::StoreApp {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(title),
    );
    let mut state = ListState::default();
    if !apps.is_empty() {
        state.select(Some(app.store_app_index.min(apps.len() - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);
}

/// Liste satırında ad sütununa ayrılan azami genişlik.
///
/// Tek bir uzun ad (ör. "Claude Code") açıklamayı ekranın dışına itmesin
/// diye ad kısaltılır; açıklamaya her zaman yer kalır.
const NAME_MAX_WIDTH: usize = 24;
/// Ad ile açıklama arasındaki ayırıcı.
const NAME_DESC_SEP: &str = " — ";
/// Açıklamaya bırakılan en az genişlik; ad bu sınıra kadar kısalır.
const DESC_MIN_WIDTH: usize = 8;

/// Bir araç satırındaki `(_)`: ad ve açıklama dışındaki sütun sayısı
/// (vurgu işareti 2 + favori 1 + boşluk 1 + durum 1 + boşluk 1).
/// Testlerin toplamı yeniden hesaplarken kullandığı doğrulama değeridir.
pub const TOOL_ROW_FIXED_WIDTH: usize = 6;

/// Ad ve açıklamaya ayrılan sütun sayılarını hesaplar.
///
/// `inner`, satırın kullanabildiği sütun sayısıdır (kenarlıklar hariç).
/// Toplam (`TOOL_ROW_FIXED_WIDTH` + ad + ayırıcı + açıklama) her zaman
/// `inner`i aşmaz. `max_width` ad için üst sınırdır.
pub fn tool_row_widths(inner: usize, max_width: usize) -> (usize, usize) {
    let sep = NAME_DESC_SEP.chars().count();
    // Sabit ön ek (vurgu + favori + durum) için yer yoksa çizilecek bir ad
    // da yoktur; ratatui satırı kırpar, taşma olmaz.
    if inner < TOOL_ROW_FIXED_WIDTH {
        return (0, 0);
    }
    let free = inner - TOOL_ROW_FIXED_WIDTH;
    // Ad + ayırıcı için bile yer yoksa yalnızca ad yazılır (açıklama 0).
    if free <= sep {
        return (free.min(max_width), 0);
    }
    let name_width = NAME_MAX_WIDTH
        .min(max_width)
        .min(free.saturating_sub(sep + DESC_MIN_WIDTH))
        .max(1);
    let desc_width = free.saturating_sub(name_width + sep);
    (name_width, desc_width)
}

fn draw_tools(f: &mut Frame, app: &App, area: Rect) {
    let tools = app.filtered_tools();
    // Kenarlıklar (2 sütun) satır içeriğine dahil değildir.
    let inner_width = area.width.saturating_sub(2) as usize;
    let (name_width, desc_width) = tool_row_widths(inner_width, NAME_MAX_WIDTH);

    let items: Vec<ListItem> = tools
        .iter()
        .map(|entry| {
            let fav = if entry.favorite { "★" } else { "☆" };
            let name = truncate(&entry.name, name_width);
            let desc = truncate(&entry.description, desc_width);
            let mut spans = vec![
                Span::styled(fav, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(
                    entry.status.symbol(),
                    Style::default().fg(status_color(entry.status)),
                ),
                Span::raw(" "),
                Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
            ];
            // Yer kalmadıysa ayırıcı ve açıklama hiç yazılmaz; satır asla
            // panel dışına taşmaz.
            if desc_width > 0 {
                spans.push(Span::styled(
                    format!("{NAME_DESC_SEP}{desc}"),
                    Style::default().fg(Color::Gray),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut state = ListState::default();
    if !tools.is_empty() {
        state.select(Some(app.tool_index.min(tools.len() - 1)));
    }

    let category_label = app.selected_category().unwrap_or("");
    let title = if !app.search_query.is_empty() {
        format!(
            " 🔍 '{}' ({}) ",
            truncate(&app.search_query, 20),
            tools.len()
        )
    } else if app.favorite_only {
        format!(" ★ Favoriler ({}) ", tools.len())
    } else {
        format!(
            " {} ({}) ",
            truncate(category_label, area.width.saturating_sub(26) as usize),
            tools.len()
        )
    };

    let border = if app.focus == PanelFocus::Tool {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(truncate(&title, area.width.saturating_sub(2) as usize));

    if tools.is_empty() {
        let empty = if app.favorite_only {
            "Bu görünümde favori araç yok.  F ile favorilere ekleyebilirsin."
        } else if !app.search_query.is_empty() {
            "Aramayla eşleşen araç yok.  Esc ile aramayı temizle."
        } else {
            "Bu kategoride araç yok. Özel araç eklemek için ? yardımındaki yapılandırma yoluna bak."
        };
        let para = Paragraph::new(Line::from(Span::styled(
            empty,
            Style::default().fg(Color::DarkGray),
        )))
        .wrap(Wrap { trim: true })
        .block(block);
        f.render_widget(para, area);
        return;
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let lines = if app.store_tab {
        store_detail_lines(app, area)
    } else if let Some(entry) = app.selected_tool() {
        let mut lines: Vec<Line> = Vec::new();
        let inner_w = area.width.saturating_sub(4) as usize; // kenarlık + boşluk

        // 1. Ad + kategori (bold).
        lines.push(Line::from(vec![Span::styled(
            format!("{} · {}", entry.name, entry.category),
            Style::default().add_modifier(Modifier::BOLD),
        )]));

        // 2. Kısa ve anlaşılır açıklama (gray).
        lines.push(Line::from(Span::styled(
            truncate(&entry.description, inner_w),
            Style::default().fg(Color::Gray),
        )));

        // 3. Kurulum durumu (renkli) + çözümlenen yol veya executable.
        let status_line = match &entry.resolved_path {
            Some(path) => format!("{} Kurulu ({})", entry.status.symbol(), path.display()),
            None => format!(
                "{} {}: {}",
                entry.status.symbol(),
                entry.status.label(),
                entry.executable
            ),
        };
        lines.push(Line::from(Span::styled(
            truncate(&status_line, inner_w),
            Style::default().fg(status_color(entry.status)),
        )));

        // 4. Çalıştırılacak komut (executable + ayrı argümanlar).
        let cmd = if entry.args.is_empty() {
            entry.executable.clone()
        } else {
            format!("{} {}", entry.executable, entry.args.join(" "))
        };
        lines.push(Line::from(vec![
            Span::styled("Komut: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                truncate(&cmd, inner_w.saturating_sub(7)),
                Style::default().fg(Color::White),
            ),
        ]));

        // 5. Kullanım örnekleri (katalog metaverisi, doğrulanmış).
        if !entry.examples.is_empty() {
            lines.push(Line::from(Span::styled(
                "Kullanım örnekleri:",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )));
            for example in &entry.examples {
                lines.push(Line::from(vec![
                    Span::styled("  • ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        truncate(example, inner_w.saturating_sub(4)),
                        Style::default().fg(Color::White),
                    ),
                ]));
            }
        }

        // 6. Kabuk entegrasyonu / ek kurulum bilgisi (varsa).
        if let Some(info) = &entry.setup_info {
            lines.push(Line::from(Span::styled(
                truncate(info, inner_w),
                Style::default().fg(Color::Yellow),
            )));
        }

        lines
    } else {
        vec![Line::from(Span::styled(
            "Henüz araç seçilmedi.",
            Style::default().fg(Color::DarkGray),
        ))]
    };
    let detail = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::TOP)
            .title(if app.store_tab {
                " Mağaza Detay "
            } else {
                " Detay "
            }),
    );
    f.render_widget(detail, area);
}

/// Mağaza uygulamasının detay satırlarını üretir.
///
/// Sıralama bilinçlidir: en sık bakılan bilgiler (sürüm, kurulum durumu,
/// canlı takip) üstte durur; dar terminalde alt satırlar kırpılsa bile
/// kullanıcı gerekli olanı görür.
fn store_detail_lines<'a>(app: &'a App, area: Rect) -> Vec<Line<'a>> {
    let Some(entry) = app.selected_store_app() else {
        return vec![Line::from(Span::styled(
            "Mağazada seçili uygulama yok.",
            Style::default().fg(Color::DarkGray),
        ))];
    };

    let inner_w = area.width.saturating_sub(4) as usize;
    let status = app.store_status(&entry.id);
    let has_update = app.store_has_pending_update(&entry.id);
    let pm_update = app.store_pm_update_for(&entry.id);
    let resolved_plan = if entry.install_method.is_package_manager() {
        crate::store::install::resolve_entry_install_plan(entry)
    } else {
        None
    };

    let mut lines: Vec<Line> = Vec::new();

    // 1. Ad + kategori (bold).
    lines.push(Line::from(Span::styled(
        format!("{} · {}", entry.name, entry.category),
        Style::default().add_modifier(Modifier::BOLD),
    )));

    // 2. Açıklama (gray).
    lines.push(Line::from(Span::styled(
        truncate(&entry.description, inner_w),
        Style::default().fg(Color::Gray),
    )));

    // 3. Sürüm.
    lines.push(Line::from(vec![
        Span::styled("Sürüm: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            entry.version.as_deref().unwrap_or("belirtilmemiş"),
            Style::default().fg(Color::White),
        ),
    ]));

    // 4. Kurulum durumu (önbellekten; her karede `which` çağrısı yapılmaz).
    lines.push(Line::from(vec![
        Span::styled("Durum: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} {}", status.symbol(), status.label()),
            Style::default().fg(status_color(status)),
        ),
    ]));

    // 5. Canlı sürüm takibi / PM güncelleme.
    let follow = if let Some(pm) = pm_update {
        format!(
            "PM güncelleme ({}) → {}",
            pm.method.short_label(),
            pm.rendered()
        )
    } else {
        match app.store_update_for(&entry.id) {
            Some(release) => {
                let current = entry.version.clone().unwrap_or_default();
                if crate::store::manifest::is_newer(&release.version, &current) {
                    format!("yeni sürüm {} → {}", current, release.version)
                } else {
                    format!("güncel ({})", release.version)
                }
            }
            None => match app.store_check_errors.get(&entry.id) {
                Some(_) => "denetlenemedi".to_string(),
                None => match entry.release {
                    Some(_) => "denetlenmedi (U)".to_string(),
                    None if entry.install_method.is_package_manager() => {
                        "PM: denetlenmedi (U)".to_string()
                    }
                    None => "takip edilmiyor".to_string(),
                },
            },
        }
    };
    lines.push(Line::from(vec![
        Span::styled("Canlı takip: ", Style::default().fg(Color::DarkGray)),
        Span::styled(follow, Style::default().fg(Color::Cyan)),
    ]));

    // 6. Kurulum yöntemi (host’a göre seçilmiş PM gösterilir).
    let display_method = resolved_plan
        .as_ref()
        .map(|p| p.method)
        .unwrap_or(entry.install_method);
    let method_line = if entry.install_method.is_installable() {
        let mut label = format!(
            "{} · {}",
            if status == crate::detection::InstalledStatus::Installed {
                "kurulu"
            } else {
                "kurulabilir"
            },
            display_method.short_label()
        );
        if display_method != entry.install_method {
            label.push_str(&format!(
                " (katalog: {})",
                entry.install_method.short_label()
            ));
        }
        label
    } else {
        entry.install_method.label().to_string()
    };
    lines.push(Line::from(vec![
        Span::styled("Kurulum: ", Style::default().fg(Color::DarkGray)),
        Span::styled(method_line, Style::default().fg(Color::Yellow)),
    ]));
    if entry.install_method.is_package_manager() {
        let cmd = resolved_plan
            .as_ref()
            .map(|p| p.rendered())
            .or_else(|| {
                entry
                    .install_command
                    .as_ref()
                    .map(|a| crate::store::format_install_command(a))
            })
            .unwrap_or_default();
        if !cmd.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Komut: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    truncate(&cmd, inner_w.saturating_sub(8)),
                    Style::default().fg(Color::White),
                ),
            ]));
        }
    }

    // 7. Kaynak ve lisans.
    lines.push(Line::from(vec![
        Span::styled("Kaynak: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate(&entry.source, inner_w.saturating_sub(10)),
            Style::default().fg(Color::White),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Lisans: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate(&entry.license, inner_w.saturating_sub(10)),
            Style::default().fg(Color::White),
        ),
    ]));

    // 8. Çalıştırılabilir dosya.
    lines.push(Line::from(vec![
        Span::styled("Executable: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate(&entry.executable, inner_w.saturating_sub(12)),
            Style::default().fg(Color::White),
        ),
    ]));

    // 9. Kullanıcıya ne yapılacağını söyleyen satır (tuiui: Enter: Install).
    let installed = status == crate::detection::InstalledStatus::Installed;
    lines.push(Line::from(Span::styled(
        if has_update {
            "Enter: güncelle   U: sürümleri denetle"
        } else if installed {
            "Enter: kaldır / güncelleme denetle   U: sürümleri denetle"
        } else if entry.install_method.is_package_manager() {
            "Enter: Install   U: sürümleri denetle"
        } else if entry.install_method.is_archive() {
            "Enter: Install (arşiv)   U: sürümleri denetle"
        } else {
            "Enter: resmî talimat   U: sürümleri denetle"
        },
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )));

    lines
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let tight = area.width < TIGHT_HINT_WIDTH;

    // Anlık geri bildirim (favori eklendi, Enter sonucu, reddedilen başlatma)
    // her zaman önceliklidir; aksi hâlde kullanıcı eyleminin sonucunu göremez.
    let line = if !app.status_message.is_empty() {
        let color = message_color(&app.status_message);
        let mut spans = Vec::new();
        // Başarı ve hata yalnızca renkle değil simgeyle de ayrılır: renk
        // körlüğü olan kullanıcı da sonucu ayırt edebilmelidir.
        if let Some(symbol) = message_symbol(&app.status_message) {
            spans.push(Span::styled(
                format!("{symbol} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        }
        spans.push(Span::styled(
            truncate(&app.status_message, area.width as usize),
            Style::default().fg(color),
        ));
        Line::from(spans)
    } else if app.store_tab {
        // Mağazada seçili uygulamanın durumu; araç durumuyla karışmaz.
        match app.selected_store_app() {
            Some(entry) => {
                let status = app.store_status(&entry.id);
                let color = status_color(status);
                let mut spans = vec![
                    Span::styled(
                        status.symbol().to_string() + " ",
                        Style::default().fg(color),
                    ),
                    Span::styled(format!("{}:", status.label()), Style::default().fg(color)),
                ];
                if !tight {
                    spans.push(Span::raw(format!(" {}   ", entry.name)));
                    spans.push(Span::styled(
                        entry.executable.clone(),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                Line::from(spans)
            }
            None => Line::from(Span::styled(
                "Mağaza kategorisinde uygulama yok.",
                Style::default().fg(Color::DarkGray),
            )),
        }
    } else if let Some(entry) = app.selected_tool() {
        let color = status_color(entry.status);
        let mut spans = vec![
            Span::styled(
                entry.status.symbol().to_string() + " ",
                Style::default().fg(color),
            ),
            Span::styled(
                format!("{}:", entry.status.label()),
                Style::default().fg(color),
            ),
        ];
        // Dar terminalde aracın adı ve uzun yol sığmaz; kesilmiş bir yol
        // göstermek yanıltıcı olacağı için yalnızca durum yazılır.
        if !tight {
            spans.push(Span::raw(format!(" {}   ", entry.name)));
            if let Some(path) = &entry.resolved_path {
                spans.push(Span::styled(
                    format!("({})", path.display()),
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                spans.push(Span::styled(
                    entry.executable.clone(),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }
        Line::from(spans)
    } else {
        Line::from(Span::styled(
            "↑↓ gezin   ←→ panel   / ara   ? yardım",
            Style::default().fg(Color::DarkGray),
        ))
    };
    f.render_widget(Paragraph::new(line), area);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect, tall: bool) {
    let tight = area.width < TIGHT_HINT_WIDTH;
    // Kısa yardım satırı: dar terminalde tek satır, gezinme sırasında ipucu.
    // Çok dar terminallerde yalnızca en gerekli tuşlar söylenir; uzun metni
    // tek satıra sıkıştırmak okunaksız olur.
    let short = if app.search_active {
        if tight {
            "yaz · Esc çık"
        } else {
            "yaz: ara   Enter/Esc: aramadan çık"
        }
    } else if app.store_tab {
        match app.focus {
            PanelFocus::Category => {
                if tight {
                    "↑↓ · / · ?"
                } else {
                    "↑↓ sekme   →/l mağaza   / ara   ? yardım"
                }
            }
            PanelFocus::StoreCategory => {
                if tight {
                    "↑↓ · / · ?"
                } else {
                    "↑↓ kategori   →/l uygulamalar   / ara   ? yardım"
                }
            }
            _ => {
                if tight {
                    "↑↓ · Enter · ?"
                } else {
                    "↑↓ uygulama   ←/h kategori   Enter Install   U denetle   ? yardım"
                }
            }
        }
    } else if app.narrow_mode && tight {
        "↑↓ · / · ?"
    } else if app.narrow_mode {
        "↑↓ araç   / ara   Enter   F favori   ? yardım"
    } else if app.focus == PanelFocus::Category {
        "↑↓ kategori   →/l araçlar   / ara   ? yardım"
    } else {
        "↑↓ araç   ←/h kategori   Enter başlat   F favori   ? yardım"
    };

    let narrow = area.width < WIDE_MIN_WIDTH;
    if !tall && !app.show_help {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                short,
                Style::default().fg(Color::DarkGray),
            ))),
            area,
        );
        return;
    }

    // Yardım paneli terminal yüksekliğine göre kısalır; satırlar alana sığmazsa
    // taşmak yerine kırpılır.
    let full = help_full_lines(app);
    let compact = vec![help_line(&[
        ("↑↓", "Gezin"),
        ("←→", "Panel"),
        ("/", "Ara"),
        ("Enter", "Başlat"),
        ("F", "Favori"),
        ("N", "Yeni araç"),
        ("R", "Geçmiş"),
        ("P", "Proje"),
        ("M", "Mağaza"),
        ("?", "Yardım"),
    ])];

    let mut lines = if app.show_help && narrow {
        // Dar terminalde yardım açıkken tam tablo sığmaz ve yükseklik de
        // arttıkça ilk satırlarda kalır; en sık kullanılan tuşlar öne alınır
        // ki kırpma anlamlı bir şeyi kessin.
        help_full_lines_narrow(app)
    } else if app.show_help {
        full
    } else {
        compact
    };

    // Satır sarması yerine sığdığı kadarını göster: ayrılmış yükseklik sabit.
    // Açık yardım ekranı tek satıra indirilmez; kullanıcı tam tabloyu istedi.
    if tight && !app.show_help {
        lines = vec![short_line(short)];
    }
    let capacity = area.height.saturating_sub(1) as usize;
    lines.truncate(capacity.max(1));

    let block = Block::default().borders(Borders::TOP).title(" Klavye ");
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left);
    f.render_widget(para, area);
}

/// Yardım ekranının tam tuş tablosu.
///
/// Tablo, `App::handle_key`'deki gerçek davranışı yansıtır: burada listelenen
/// her tuş uygulamada çalışır, uygulamadaki her ana ekran tuşu burada
/// listelenir. Dosya yolları da burada gösterilir; kullanıcı yapılandırmayı
/// aramadan bulabilir.
fn help_full_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        help_line(&[
            ("↑↓/jk", "Gezin"),
            ("←→/hl", "Panel"),
            ("Tab", "Panel değiştir"),
            ("/", "Ara"),
        ]),
        help_line(&[
            ("Enter", "Başlat / bilgi"),
            ("Esc", "Aramayı kapat"),
            ("?", "Yardımı kapat"),
        ]),
        help_line(&[("F", "Favori"), ("C", "Yalnızca favoriler")]),
        help_line(&[
            ("N", "Yeni özel araç"),
            ("D", "Aracı sil/gizle"),
            ("R", "Son kullanılanlar"),
            ("P", "Proje dizinleri"),
            ("S", "Ayarlar"),
        ]),
        help_line(&[
            ("M", "Mağaza sekmesi"),
            ("U", "Güncellemeyi denetle"),
            ("O", "Dosya seçici"),
            ("A", "Varsayılan AI"),
        ]),
        help_line(&[("Q", "Çıkış"), ("Ctrl+C", "Çıkış")]),
    ];
    // Mağaza sekmesinin kendi tuşları ayrı satırda anlatılır; sekme
    // görünümü farklı bir gezinme düzeni sunduğu için tek satırlık tablo
    // yetersiz kalır.
    if app.store_tab {
        lines.push(help_line(&[("Mağaza", "↑↓ · Enter Install / güncelle")]));
        lines.push(help_line(&[("U", "Mağazada sürüm denetle")]));
    }
    let (config_line, state_line, workdir_line) = help_info_lines(app);
    lines.push(config_line);
    lines.push(state_line);
    lines.push(workdir_line);
    lines
}

/// Dar terminalde yardım: tuşlar tek sütun, önce en sık kullanılanlar.
///
/// Yükseklik sınırlı olduğu için kırpma kaçınılmaz; bu sıralama kırpılan
/// kısmın en az kritik tuşlar olmasını sağlar.
fn help_full_lines_narrow(app: &App) -> Vec<Line<'static>> {
    let entries: [(&'static str, &'static str); 17] = [
        ("↑↓", "Gezin"),
        ("←→", "Panel"),
        ("/", "Ara"),
        ("Enter", "Başlat"),
        ("?", "Yardım"),
        ("F", "Favori"),
        ("P", "Proje dizini"),
        ("S", "Ayarlar"),
        ("R", "Son kullanılanlar"),
        ("N", "Yeni araç"),
        ("D", "Aracı sil"),
        ("M", "Mağaza"),
        ("U", "Güncellemeyi denetle"),
        ("O", "Dosya seçici"),
        ("A", "Varsayılan AI"),
        ("C", "Yalnızca favoriler"),
        ("Q", "Çıkış"),
    ];
    let mut lines: Vec<Line<'static>> = entries
        .iter()
        .map(|(key, desc)| help_line(&[(*key, *desc)]))
        .collect();
    let (config_line, state_line, workdir_line) = help_info_lines(app);
    lines.push(config_line);
    lines.push(state_line);
    lines.push(workdir_line);
    lines
}

/// Yardım ekranının bilgi satırları: dosya yolları ve etkin çalışma dizini.
///
/// Uzun yollar karakter sayılarak yalnızca görsel sığdırma için kesilir; bilgi
/// kaybı yalnızca ekrandadır, uygulama tam yolu kullanmaya devam eder.
fn help_info_lines(app: &App) -> (Line<'static>, Line<'static>, Line<'static>) {
    // Dinamik satırlar: yollar ve etkin yol. Uzun yollar karakter sayılarak
    // yalnızca görsel sığdırma için kesilir; bilgi kaybı yalnızca ekrandadır.
    let mut info = Vec::new();
    for (key, value) in [
        ("config", &app.config_path_display),
        ("state", &app.state_path_display),
    ] {
        info.push(Line::from(vec![
            Span::styled(format!("{key}: "), Style::default().fg(Color::DarkGray)),
            Span::styled(value.clone(), Style::default().fg(Color::Gray)),
        ]));
    }
    let workdir = match (
        &app.state.active_project,
        &app.state.prefs().default_working_dir,
    ) {
        (Some(p), _) => format!("{} (etkin proje)", p.display()),
        (None, Some(d)) => format!("{d} (varsayılan)"),
        (None, None) => app.home_dir.display().to_string(),
    };
    info.push(Line::from(vec![
        Span::styled("çalışma dizini: ", Style::default().fg(Color::DarkGray)),
        Span::styled(workdir, Style::default().fg(Color::Gray)),
    ]));
    let mut it = info.into_iter();
    (
        it.next().expect("config satırı"),
        it.next().expect("state satırı"),
        it.next().expect("çalışma dizini satırı"),
    )
}

/// Düz metinden tek satırlık yardım satırı üretir.
fn short_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(Color::DarkGray),
    ))
}

/// `(tuş, açıklama)` çiftlerinden tek bir yardım satırı üretir.
fn help_line(items: &[(&'static str, &'static str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (key, desc)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(
            (*key).to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(" {desc}")));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_text() {
        assert_eq!(truncate("htop", 10), "htop");
    }

    #[test]
    fn truncate_is_utf8_safe() {
        // Eskiden `&text[..35]` bayt indeksiyle kesildiği için panik oluşuyordu.
        let text = "Geliştirici araçları için çok uzun bir açıklama metni";
        let cut = truncate(text, 10);
        assert_eq!(cut.chars().count(), 10);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn truncate_does_not_panic_on_turkish_chars() {
        let text = "İşlem görüntüleyici ş ç ğ ü ö İ";
        let _ = truncate(text, 1);
        let _ = truncate(text, 0);
    }

    #[test]
    fn status_colors_differ_and_cover_all_states() {
        assert_eq!(
            status_color(InstalledStatus::Installed),
            Color::Green,
            "kurulu yeşil olmalı"
        );
        assert_ne!(
            status_color(InstalledStatus::Missing),
            status_color(InstalledStatus::Unknown),
            "kurulu değil ile kontrol edilemedi ayırt edilmeli"
        );
    }

    #[test]
    fn truncate_respects_budget_for_every_width() {
        // 1..=60 sütun için üretilen başlık hiçbir zaman alanı aşmamalı.
        let sample = "Günlük Terminal Yardımcıları ◆ ★ ✓ çok uzun bir başlık";
        for width in 1..=60u16 {
            let budget = width.saturating_sub(2) as usize;
            assert!(
                truncate(sample, budget).chars().count() <= budget.max(1),
                "genişlik {width} için metin alanı aşıyor"
            );
        }
    }
}
