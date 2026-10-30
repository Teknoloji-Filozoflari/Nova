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

use crate::app::{App, PanelFocus};
use crate::catalog::CATEGORIES;
use crate::detection::InstalledStatus;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

/// Bu genişliğin altında iki panel yan yana sığmaz.
pub const WIDE_MIN_WIDTH: u16 = 70;
/// Bu genişliğin altında detay alanı gizlenir.
const MEDIUM_MIN_WIDTH: u16 = 45;
/// Bu yüksekliğin altında yardım paneli tek satıra iner.
const TALL_MIN_HEIGHT: u16 = 14;
/// Bu genişliğin altında uzun yardım ipuçları sığmaz.
const TIGHT_HINT_WIDTH: u16 = 52;
/// Yardım açıkken araç listesine bırakılan en az satır.
const CONTENT_MIN_HEIGHT: u16 = 5;
/// Yardım panelinin tam tuş tablosu için ayırabileceği en fazla satır.
const HELP_HELP_MAX_HEIGHT: u16 = 10;

// Düzen eşikleri birbirini mantıksal olarak izlemeli; ihlal derleme hatasıdır.
const _: () = assert!(WIDE_MIN_WIDTH > MEDIUM_MIN_WIDTH);
const _: () = assert!(TIGHT_HINT_WIDTH < WIDE_MIN_WIDTH);
const _: () = assert!(TIGHT_HINT_WIDTH > 20);

/// Metni en fazla `max` karakter olacak şekilde keser (UTF-8 güvenli).
fn truncate(text: &str, max: usize) -> String {
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

    let wide = area.width >= WIDE_MIN_WIDTH;
    let show_detail = area.width >= MEDIUM_MIN_WIDTH && area.height >= 10;
    let tall = area.height >= TALL_MIN_HEIGHT;

    // Yardım açıkken panel tuş tablosuna, dosya yollarına ve etkin çalışma
    // dizinine yer açar. Yükseklik ekranla birlikte büyür ama araç listesine
    // en az `CONTENT_MIN_HEIGHT` satır bırakır; küçük terminallerde ise
    // satırlar kırpılır (bkz. `draw_help`).
    let help_height = if app.show_help {
        let reserved = 1 + 1 + CONTENT_MIN_HEIGHT + if show_detail { 2 } else { 0 } + 1;
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
            Constraint::Length(1),                               // başlık
            Constraint::Length(1),                               // arama satırı
            Constraint::Min(3),                                  // içerik
            Constraint::Length(if show_detail { 2 } else { 0 }), // detay
            Constraint::Length(1),                               // durum
            Constraint::Length(help_height),                     // yardım
        ])
        .split(area);

    draw_header(f, chunks[0]);
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

fn draw_header(f: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(Span::styled(
        "Nova",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
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
    if wide {
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(26), Constraint::Min(20)])
            .split(area);
        draw_categories(f, app, panels[0]);
        draw_tools(f, app, panels[1]);
    } else {
        // Tek sütun: kategori seçimi başlıkta özetlenir.
        draw_tools(f, app, area);
    }
}

fn draw_categories(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = CATEGORIES
        .iter()
        .enumerate()
        .map(|(i, cat)| {
            let count = app
                .catalog
                .by_category(cat)
                .into_iter()
                .filter(|e| app.matches_search(e) && (!app.favorite_only || e.favorite))
                .count();
            let selected = i == app.category_index;
            let focused = selected && app.focus == PanelFocus::Category;
            let style = if focused {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(Color::Cyan)
            } else if count == 0 {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            let prefix = if selected { "▶ " } else { "  " };
            let label = truncate(cat, area.width.saturating_sub(6) as usize);
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(label, style),
            ]))
        })
        .collect();

    let border = if app.focus == PanelFocus::Category {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(" Kategoriler "),
    );
    let mut state = ListState::default().with_selected(Some(app.category_index));
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
    let lines = if let Some(entry) = app.selected_tool() {
        let mut spans = vec![
            Span::styled(
                format!("{} · {}", entry.name, entry.category),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(entry.description.clone(), Style::default().fg(Color::Gray)),
        ];
        // Başlatma türü Enter'a basmadan önce görünür olmalı: bazı kayıtlar
        // aracı çalıştırmaz, sabit bir metin gösterir.
        if let Some(kind) = entry.launch_kind.mark() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("[{kind}]"),
                Style::default().fg(Color::Yellow),
            ));
        }
        // Bir alternatifse, asıl araçla aynı olmadığı belirtilir.
        if let Some(target) = &entry.alternative_to {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("[alternatif: {target}]"),
                Style::default().fg(Color::DarkGray),
            ));
        }
        // Çalışma dizini kısa etiketi: araç özelinde dizin tanımlıysa
        // kullanıcı Enter'a basmadan önce nerede açılacağını görür.
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("[{}]", entry.working_directory.label()),
            Style::default().fg(Color::DarkGray),
        ));
        let mut lines = vec![Line::from(spans)];
        // Etkileşimli araçların açıklaması zaten yukarıda; diğer türler
        // için Enter'ın ne yapacağını/kabuk entegrasyonu gerekip
        // gerekmediğini açıklayan ikinci satır.
        if let Some(hint) = entry.launch_hint() {
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            )));
        }
        lines
    } else {
        vec![Line::from(Span::styled(
            "Henüz araç seçilmedi.",
            Style::default().fg(Color::DarkGray),
        ))]
    };
    let detail = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::TOP).title(" Detay "));
    f.render_widget(detail, area);
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
        help_line(&[("Q", "Çıkış"), ("Ctrl+C", "Çıkış")]),
    ];
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
    let entries: [(&'static str, &'static str); 13] = [
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
