//! **Hızlı referans** — geniş komut tablosu (tasarım-2).
//!
//! Üç parçadan oluşur:
//! 1. **Sekme şeridi** — gezinme modları yatay sekme gibi (aktif sekme altı çizili).
//! 2. **Filtre satırı** — bölüm / seviye / risk açılır listeleri + arama ("süz").
//! 3. **Tablo** — `KOMUT · NE YAPAR · ÖRNEK · SEVİYE · RİSK`.
//!
//! Kural: **uydurma veri yok**. Örnek sütunu katalogdaki ilk örnektir; örnek
//! çıktısı katalogda bulunmadığı için çıktı bloğu çizilmez.

use crate::app::App;
use crate::commands::{CommandEntry, CommandsBrowseMode, LearningLevel};
use crate::i18n::{commands_mode_label, Msg};
use crate::ui::syntax::highlight_prompt_line;
use crate::ui::{truncate, wrap_text, PaneWidths};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Sekme şeridi + filtre satırı + tablo başlığı için ayrılan satır sayısı.
pub const CHROME_ROWS: usize = 3;

/// Hızlı referans tablosunun ekran yerleşimi.
///
/// **Tek kaynak:** hem çizim (`ui::draw`) hem fare eşlemesi (`row_index_at_y`)
/// bu değerleri kullanır; ayrı hesaplanırsa tıklamalar satır kayar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableLayout {
    /// Tablonun ekrandaki ilk satırı.
    pub top: u16,
    /// Tablonun yüksekliği (satır).
    pub height: u16,
}

/// Dar düzende üstteki kaynak (mod) panelinin yüksekliği.
///
/// Çizim (`draw_commands_content`) ve fare eşlemesi aynı sayıyı kullanır.
pub fn source_panel_height(panes: &PaneWidths, content_height: u16) -> u16 {
    panes
        .secondary_height
        .min(content_height.saturating_sub(3))
        .max(3)
        .min(content_height)
}

/// Hızlı referans tablosunun yerleşimi.
///
/// Geniş terminalde tablo tüm içerik alanını kaplar; dar terminalde mod
/// panelinin altında başlar.
pub fn table_layout(
    content_top: u16,
    content_height: u16,
    panes: &PaneWidths,
    wide: bool,
) -> TableLayout {
    if wide {
        TableLayout {
            top: content_top,
            height: content_height,
        }
    } else {
        let src = source_panel_height(panes, content_height);
        TableLayout {
            top: content_top.saturating_add(src),
            height: content_height.saturating_sub(src),
        }
    }
}

/// Tablo sütun genişlikleri.
#[derive(Debug, Clone, Copy)]
struct Cols {
    cmd: usize,
    desc: usize,
    example: usize,
    level: usize,
    risk: usize,
}

/// Genişliğe göre sütun payı — **toplam genişlik panel genişliğini aşmaz**.
///
/// Sıra: önce açıklama sütunu daralır (örnek → komut), sonra rozet sütunları
/// (seviye → risk). Asgari değerler altında tablo okunmaz ama **taşmaz**.
fn compute_cols(width: usize) -> Cols {
    const MARKER: usize = 2; // seçim işareti
    const GAPS: usize = 8; // sütun arası boşluk (4 × 2)
    const BASE: usize = MARKER + GAPS;
    let mut level = 13usize;
    let mut risk = 13usize;
    let mut cmd = 16usize;
    let mut example = 24usize;

    let total =
        |cmd: usize, example: usize, level: usize, risk: usize| BASE + cmd + example + level + risk;

    // 1) Açıklama sütunu: en az 14 karakter hedeflenir; gerekirse örnek/komut daralır.
    while width.saturating_sub(total(cmd, example, level, risk)) < 14 && example > 10 {
        example -= 1;
    }
    while width.saturating_sub(total(cmd, example, level, risk)) < 8 && cmd > 8 {
        cmd -= 1;
    }
    // 2) Hâlâ sığmıyorsa rozet sütunları daralır.
    while total(cmd, example, level, risk) > width && level > 6 {
        level -= 1;
    }
    while total(cmd, example, level, risk) > width && risk > 6 {
        risk -= 1;
    }
    // 3) Son çare: örnek ve komut.
    while total(cmd, example, level, risk) > width && example > 6 {
        example -= 1;
    }
    while total(cmd, example, level, risk) > width && cmd > 6 {
        cmd -= 1;
    }
    let desc = width.saturating_sub(total(cmd, example, level, risk));
    Cols {
        cmd,
        desc,
        example,
        level,
        risk,
    }
}

/// Mod etiketinden sıra numarasını soy (sekme şeridi sade): `"7 Hızlı referans"` → `"Hızlı referans"`.
fn tab_label(mode: CommandsBrowseMode, lang: crate::i18n::Language) -> String {
    let label = commands_mode_label(mode, lang);
    let trimmed = label.trim_start();
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return trimmed.to_string();
    }
    trimmed[digits..].trim_start().to_string()
}

/// Sekme şeridi — aktif sekme cyan + altı çizili.
///
/// Satır **panel genişliğini aşmaz**; sığmayan sekmeler çizilmez. Aktif sekme
/// daima görünür: şeride sığmıyorsa pencere aktif sekmeye kaydırılır ve sol
/// tarafta `‹` işareti çizilir (daha fazla sekme var).
fn tab_strip(app: &App, width: usize) -> Line<'static> {
    let pal = app.ui_theme().palette();
    let lang = app.ui_lang();
    const SEP: &str = " │ ";
    const SEP_LEN: usize = 3;
    const MORE: &str = "‹ ";
    const MORE_LEN: usize = 2;

    let modes = CommandsBrowseMode::ALL;
    let labels: Vec<String> = modes.iter().map(|m| tab_label(*m, lang)).collect();
    // Etiketler " Başlangıç " biçiminde çizilir.
    let item_w: Vec<usize> = labels.iter().map(|l| l.chars().count() + 2).collect();
    let active_idx = modes
        .iter()
        .position(|m| *m == app.commands_mode)
        .unwrap_or(0);

    // 1) Baştan doldur — sığan sekmeler.
    let mut start = 0usize;
    let mut end = 0usize;
    let mut used = 0usize;
    for (i, w) in item_w.iter().enumerate() {
        let add = if i == 0 { *w } else { SEP_LEN + w };
        if used + add > width {
            break;
        }
        used += add;
        end = i + 1;
    }

    let mut show_more = false;
    if active_idx >= end {
        start = active_idx;
        end = active_idx + 1;
        used = item_w[active_idx];
        while start > 0 {
            let add = SEP_LEN + item_w[start - 1];
            let extra = if start - 1 == 0 { 0 } else { MORE_LEN };
            if used + add + extra > width {
                break;
            }
            used += add;
            start -= 1;
        }
        show_more = start > 0 && used + MORE_LEN <= width;
        if show_more {
            used += MORE_LEN;
        }
        let mut i = active_idx + 1;
        while i < item_w.len() && used + SEP_LEN + item_w[i] <= width {
            used += SEP_LEN + item_w[i];
            end = i + 1;
            i += 1;
        }
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    if show_more {
        spans.push(Span::styled(
            MORE.to_string(),
            Style::default().fg(pal.muted),
        ));
    }
    for (i, label) in labels.iter().enumerate().take(end).skip(start) {
        let active = i == active_idx;
        let text = format!(" {label} ");
        if i > start {
            spans.push(Span::styled(
                SEP.to_string(),
                Style::default().fg(pal.border),
            ));
        }
        if active {
            spans.push(Span::styled(
                text,
                Style::default()
                    .fg(pal.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ));
        } else {
            spans.push(Span::styled(text, Style::default().fg(pal.muted)));
        }
    }
    Line::from(spans)
}

/// Filtre satırı — bölüm / seviye / risk + arama; sağda satır sayısı.
///
/// Sığmayan parçalar **atlanır** (satır panel genişliğini aşmaz).
fn filter_row(app: &App, width: usize, count: usize) -> Line<'static> {
    let pal = app.ui_theme().palette();
    let lang = app.ui_lang();
    let lang_id = lang.id();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;

    // Sırayla dene; sığmayan parça atlanır (süzgeç değeri kaybolmaz).
    let push_chip = |spans: &mut Vec<Span<'static>>, used: &mut usize, label: &str, value: &str| {
        let head = format!("{label}: ");
        let tail = format!("{value} ▾   ");
        let need = head.chars().count() + tail.chars().count();
        if *used + need > width {
            return;
        }
        spans.push(Span::styled(head, Style::default().fg(pal.muted)));
        spans.push(Span::styled(
            format!("{value} ▾"),
            Style::default().fg(pal.accent),
        ));
        spans.push(Span::styled("   ", Style::default().fg(pal.muted)));
        *used += need;
    };

    // Bölüm — `None` = tümü.
    let section = match &app.commands_quick_category {
        Some(cat) => lang.bank_category(cat).to_string(),
        None => lang.t(Msg::CommandsQuickFilterAll).to_string(),
    };
    push_chip(
        &mut spans,
        &mut used,
        lang.t(Msg::CommandsQuickFilterSection),
        &section,
    );

    // Seviye — `Başlangıç+` dahil.
    let level = app.commands_filters.learning.label(lang_id).to_string();
    push_chip(
        &mut spans,
        &mut used,
        lang.t(Msg::CommandsQuickFilterLevel),
        &level,
    );

    // Risk — `None` = tümü.
    let risk = match app.commands_filters.risk {
        Some(r) => r.label(lang_id).to_string(),
        None => lang.t(Msg::CommandsQuickFilterAll).to_string(),
    };
    push_chip(
        &mut spans,
        &mut used,
        lang.t(Msg::CommandsQuickFilterRisk),
        &risk,
    );

    // Arama ("süz").
    let query = if app.search_query.is_empty() {
        lang.t(Msg::CommandsQuickFilterEmpty).to_string()
    } else {
        app.search_query.clone()
    };
    let head = format!("{}: ", lang.t(Msg::CommandsQuickFilterQuery));
    if used + head.chars().count() + query.chars().count() <= width {
        spans.push(Span::styled(head.clone(), Style::default().fg(pal.muted)));
        let style = if app.search_query.is_empty() {
            Style::default().fg(pal.muted)
        } else {
            Style::default().fg(pal.search)
        };
        spans.push(Span::styled(query.clone(), style));
    }

    // Sağda: kaç komut listeleniyor.
    let right = lang
        .t(Msg::CommandsQuickCountShort)
        .replace("{n}", &count.to_string());
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let right_len = right.chars().count();
    if used + right_len + 2 <= width {
        spans.push(Span::styled(
            " ".repeat(width - used - right_len),
            Style::default().fg(pal.muted),
        ));
        spans.push(Span::styled(right, Style::default().fg(pal.muted)));
    }
    Line::from(spans)
}

/// Tablo başlığı — `KOMUT · NE YAPAR · ÖRNEK · SEVİYE · RİSK`.
fn header_row(app: &App, cols: Cols) -> Line<'static> {
    let pal = app.ui_theme().palette();
    let lang = app.ui_lang();
    let head = Style::default().fg(pal.muted).add_modifier(Modifier::BOLD);
    let mut spans: Vec<Span<'static>> = vec![Span::styled("  ".to_string(), head)];
    let push = |spans: &mut Vec<Span<'static>>, label: &str, width: usize| {
        // Etiket sütun genişliğine kırpılır — başlık satırı paneli taşırmaz.
        spans.push(Span::styled(
            format!("{:<width$}", truncate(label, width), width = width),
            head,
        ));
        spans.push(Span::styled("  ".to_string(), head));
    };
    push(&mut spans, lang.t(Msg::CommandsQuickColCommand), cols.cmd);
    push(&mut spans, lang.t(Msg::CommandsQuickColWhat), cols.desc);
    push(
        &mut spans,
        lang.t(Msg::CommandsQuickColExample),
        cols.example,
    );
    push(&mut spans, lang.t(Msg::CommandsQuickColLevel), cols.level);
    push(
        &mut spans,
        lang.t(Msg::CommandsQuickColRisk),
        cols.risk.saturating_sub(2),
    );
    Line::from(spans)
}

/// Komut satırı — renkli sütunlar + satır-içi rozetler.
fn command_row(app: &App, cmd: &CommandEntry, cols: Cols, selected: bool) -> Line<'static> {
    let pal = app.ui_theme().palette();
    let lang = app.ui_lang();
    let lang_id = lang.id();
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Seçim işareti — listenin geri kalanıyla aynı dil (▸ / boşluk).
    spans.push(Span::styled(
        if selected { "▸ " } else { "  " },
        Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
    ));

    // KOMUT — cyan kalın.
    spans.push(Span::styled(
        format!(
            "{:<width$}",
            truncate(&cmd.name, cols.cmd),
            width = cols.cmd
        ),
        Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        "  ".to_string(),
        Style::default().fg(pal.muted),
    ));

    // NE YAPAR — özet metni (satır-içi sarma yok; sütun genişliği sabit).
    let summary = cmd.summary.pick(lang_id);
    spans.push(Span::styled(
        format!(
            "{:<width$}",
            truncate(summary, cols.desc),
            width = cols.desc
        ),
        Style::default().fg(pal.text),
    ));
    spans.push(Span::styled(
        "  ".to_string(),
        Style::default().fg(pal.muted),
    ));

    // ÖRNEK — katalogdaki ilk örnek (`$` istemli, renkli).
    let example = cmd
        .examples
        .first()
        .map(|ex| ex.command.as_str())
        .unwrap_or("");
    if example.is_empty() {
        spans.push(Span::styled(
            " ".repeat(cols.example),
            Style::default().fg(pal.muted),
        ));
    } else {
        let short = truncate(example, cols.example.saturating_sub(2));
        let colored = highlight_prompt_line(&short, pal);
        let used: usize = colored.iter().map(|s| s.content.chars().count()).sum();
        for s in colored {
            spans.push(Span::styled(s.content, s.style));
        }
        if used < cols.example {
            spans.push(Span::styled(
                " ".repeat(cols.example - used),
                Style::default().fg(pal.muted),
            ));
        }
    }
    spans.push(Span::styled(
        "  ".to_string(),
        Style::default().fg(pal.muted),
    ));

    // SEVİYE — öğrenme seviyesi rozeti.
    let level = cmd.learning_level;
    let level_label = if level.is_specified() {
        level.label(lang_id)
    } else {
        "—"
    };
    let level_text = format!("[{level_label}]");
    spans.push(Span::styled(
        format!(
            "{:<width$}",
            truncate(&level_text, cols.level),
            width = cols.level
        ),
        Style::default()
            .fg(pal.level_color(level))
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        "  ".to_string(),
        Style::default().fg(pal.muted),
    ));

    // RİSK — risk rozeti.
    let risk_text = format!("[{}]", cmd.risk.label(lang_id));
    spans.push(Span::styled(
        format!(
            "{:<width$}",
            truncate(&risk_text, cols.risk),
            width = cols.risk
        ),
        Style::default()
            .fg(pal.risk_color(cmd.risk))
            .add_modifier(Modifier::BOLD),
    ));

    Line::from(spans)
}

/// Görünür pencerenin başlangıcı — seçili satır ortada kalacak şekilde.
pub fn window_start(selected: usize, total: usize, visible: usize) -> usize {
    if visible == 0 || total == 0 {
        return 0;
    }
    selected
        .min(total - 1)
        .saturating_sub(visible / 2)
        .min(total.saturating_sub(visible))
}

/// Hızlı referans tablosunda tıklanan satırın **mutlak** indeksi.
///
/// Tablo kenarlıksız ve kendi penceresini kaydırır; `list_index_at_y` yerine
/// bu eşleme kullanılır. `table_top` / `table_height` çizimle aynı olmalıdır
/// ([`table_layout`]).
pub fn row_index_at_y(
    y: u16,
    table_top: u16,
    table_height: u16,
    selected: usize,
    len: usize,
) -> Option<usize> {
    let chrome = CHROME_ROWS as u16;
    if table_height <= chrome || len == 0 {
        return None;
    }
    let list_top = table_top.saturating_add(chrome);
    let visible = table_height.saturating_sub(chrome) as usize;
    if y < list_top {
        return None;
    }
    let visual = (y - list_top) as usize;
    if visual >= visible {
        return None;
    }
    let idx = window_start(selected, len, visible) + visual;
    (idx < len).then_some(idx)
}

/// Tablo sütunlarının asgari genişliği — altında tek sütunlu liste çizilir.
pub const MIN_TABLE_WIDTH: usize = 34;

/// Hızlı referans görünümünün satırları (sekme şeridi + filtre + tablo).
///
/// `width` / `height` panel boyutu; `height` azalınca tablo kaydırılır.
pub fn quick_reference_lines(app: &App, width: usize, height: usize) -> Vec<Line<'static>> {
    let pal = app.ui_theme().palette();
    let mut lines: Vec<Line<'static>> = Vec::new();
    if height == 0 {
        return lines;
    }
    lines.push(tab_strip(app, width));
    if height < 2 {
        return lines;
    }
    let cmds = app.filtered_bank_commands();
    lines.push(filter_row(app, width, cmds.len()));
    if height < CHROME_ROWS {
        return lines;
    }

    let cols = compute_cols(width);
    if width < MIN_TABLE_WIDTH {
        // Çok dar panel: tablo sığmaz — tek sütun (ad + özet) çizilir.
        lines.push(Line::from(Span::styled(
            app.ui_lang().t(Msg::CommandsQuickColCommand).to_string(),
            Style::default().fg(pal.muted).add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(header_row(app, cols));
    }

    // Görünür pencere — seçili satır ortada kalacak şekilde kaydırılır.
    let visible = height.saturating_sub(CHROME_ROWS);
    if visible == 0 {
        return lines;
    }
    let selected = app.commands_entry_index.min(cmds.len().saturating_sub(1));
    let start = window_start(selected, cmds.len(), visible);
    for (i, cmd) in cmds.iter().enumerate().skip(start).take(visible) {
        if width < MIN_TABLE_WIDTH {
            lines.push(simple_row(app, cmd, width, i == selected));
        } else {
            lines.push(command_row(app, cmd, cols, i == selected));
        }
    }
    if cmds.is_empty() {
        lines.push(Line::from(Span::styled(
            app.ui_lang().t(Msg::CommandsFilterNoMatch).to_string(),
            Style::default().fg(pal.warn),
        )));
    }
    lines
}

/// Çok dar panelde tek satırlık komut satırı — ad + özet.
fn simple_row(app: &App, cmd: &CommandEntry, width: usize, selected: bool) -> Line<'static> {
    let pal = app.ui_theme().palette();
    let lang_id = app.ui_lang().id();
    let name = truncate(&cmd.name, width.saturating_sub(5).max(4));
    let rest = width.saturating_sub(name.chars().count() + 5);
    let summary = truncate(cmd.summary.pick(lang_id), rest);
    Line::from(vec![
        Span::styled(
            if selected { "▸ " } else { "  " },
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            name,
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", Style::default().fg(pal.muted)),
        Span::styled(summary, Style::default().fg(pal.text)),
    ])
}

/// Durum satırı metni — tasarımdaki bağlam cümlesi.
pub fn quick_reference_status(app: &App) -> String {
    let lang = app.ui_lang();
    let total = app.commands_bank.commands.len();
    let count = app.filtered_bank_commands().len();
    lang.t(Msg::CommandsQuickCount)
        .replace("{n}", &count.to_string())
        .replace("{total}", &total.to_string())
}

/// Seviye süzgeci etiketi (durum satırı rozetleri için).
pub fn level_filter_label(app: &App) -> String {
    app.commands_filters
        .learning
        .label(app.ui_lang().id())
        .to_string()
}

/// Seviye süzgeci gerçekten sınırlı mı?
pub fn level_filter_active(app: &App) -> bool {
    app.commands_filters.learning != crate::commands::LearningLevelFilter::All
}

/// Öğrenme seviyesi süzgeci için boşsa tüm komutlar (test yardımcısı).
pub fn command_matches_level_filter(cmd: &CommandEntry, level: LearningLevel) -> bool {
    match level {
        LearningLevel::Beginner | LearningLevel::Intermediate | LearningLevel::Advanced => {
            cmd.learning_level == level
        }
        LearningLevel::Unspecified => cmd.learning_level == LearningLevel::Unspecified,
    }
}

/// Sütun genişliklerini test için dışa açar.
pub fn column_widths(width: usize) -> (usize, usize, usize, usize, usize) {
    let c = compute_cols(width);
    (c.cmd, c.desc, c.example, c.level, c.risk)
}

/// Metin sarmalama yardımcısı (dışa açık testler için).
pub fn wrap_for_test(text: &str, width: usize) -> Vec<String> {
    wrap_text(text, width)
}
