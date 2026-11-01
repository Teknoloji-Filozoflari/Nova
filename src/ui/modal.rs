//! Modalların çizimi: onay, özel araç formu/gözden geçirme, proje seçici,
//! proje ekleme ve geçmiş.
//!
//! Modal, mevcut ekranın üstüne ortalanmış bir kutuda çizilir. Hiçbir yerde
//! sabit piksel genişliği varsayılmaz: kutu terminale göre küçülür, uzun
//! metinler karakter sayılarak kesilir (Türkçe karakterlerde panik olmaz).

use super::truncate;
use crate::app::form::{Field, ToolForm, FIELD_ORDER};
use crate::app::modal::{Modal, UpdateStatus};
use crate::app::state::relative_time;
use crate::app::App;
use crate::file_picker::FilePickerState;
use crate::update::{GitHubRelease, UpdateCheck, Version};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListState;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

/// Modallar için en fazla kutu genişliği.
const MODAL_MAX_WIDTH: u16 = 76;
/// Kutunun ekran kenarına bırakacağı en az boşluk (her yanda).
const MODAL_MARGIN: u16 = 2;

/// Modal kutusunun dikdörtgeni: `satır sayısı`na göre ortalanmış.
fn modal_area(area: Rect, width: u16, lines: u16) -> Rect {
    let max_w = area.width.saturating_sub(MODAL_MARGIN * 2).max(1);
    let w = width.min(MODAL_MAX_WIDTH).min(max_w);
    // Kutu başlık + içerik + kenarlık ister; terminal çok küçükse ekran
    // sınırı geçerlidir (taşmak yerine kırpılır).
    let h = (lines + 2).min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

/// Açık modalı çizer; modal yoksa hiçbir şey yapmaz.
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    match &app.modal {
        Modal::None => {}
        Modal::Confirm { question, .. } => draw_confirm(f, area, question),
        Modal::ToolForm(form) => draw_form(f, area, form),
        Modal::ToolReview { valid, .. } => draw_review(f, area, valid),
        Modal::ProjectPicker { index } => draw_picker(f, area, app, *index),
        Modal::ProjectAdd { input, error } => draw_project_add(f, area, input, error.as_deref()),
        Modal::History => draw_history(f, area, app),
        Modal::Settings {
            preferred_terminal,
            default_working_dir,
            theme,
            default_ai_tool,
            default_image_app,
            default_video_app,
            focus,
            error,
        } => draw_settings(
            f,
            area,
            app,
            preferred_terminal,
            default_working_dir,
            theme,
            default_ai_tool,
            default_image_app,
            default_video_app,
            *focus,
            error.as_deref(),
        ),
        Modal::FilePicker { state } => draw_file_picker(f, area, state),
        Modal::UpdateCheck {
            status,
            check,
            error,
        } => draw_update_check(f, area, status, check.as_ref(), error.as_deref()),
        Modal::UpdateConfirm {
            current,
            latest,
            release,
        } => draw_update_confirm(f, area, current, latest, release),
        Modal::StoreInstalledMenu { name, index, .. } => {
            draw_store_installed_menu(f, area, name, *index)
        }
    }
}

fn draw_store_installed_menu(f: &mut Frame, screen: Rect, name: &str, index: usize) {
    let width = 56.min(screen.width.saturating_sub(MODAL_MARGIN * 2));
    let options = ["Kaldır / uninstall", "Güncelleme denetle (U)"];
    let mut lines = vec![
        Line::from(Span::styled(
            format!("'{name}' kurulu — ne yapılsın?"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for (i, label) in options.iter().enumerate() {
        let selected = i == index;
        lines.push(Line::from(Span::styled(
            format!("{} {label}", if selected { "▶" } else { " " }),
            if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑↓ seç   Enter onayla   Esc vazgeç",
        Style::default().fg(Color::DarkGray),
    )));
    let area = modal_area(screen, width, lines.len() as u16);
    let inner = frame(f, area, "Kurulu uygulama");
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// Güncelleme onay ekranını çizer.
fn draw_update_confirm(
    f: &mut Frame,
    area: Rect,
    current: &Version,
    latest: &Version,
    release: &GitHubRelease,
) {
    let inner_w = area.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        "Güncelleme onayı",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Mevcut: ", Style::default().fg(Color::DarkGray)),
        Span::styled(current.to_string(), Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Yeni: ", Style::default().fg(Color::DarkGray)),
        Span::styled(latest.to_string(), Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(Span::styled(
        format!("Yayın notları: {}", truncate(&release.body, inner_w)),
        Style::default().fg(Color::Gray),
    )));
    lines.push(Line::from(vec![
        Span::styled("Kaynak: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate(&release.html_url, inner_w.saturating_sub(10)),
            Style::default().fg(Color::Blue),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter: kurulum onayı   Esc: kapat",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));

    let list = List::new(lines.into_iter().map(ListItem::new).collect::<Vec<_>>()).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Güncelleme onayı "),
    );
    f.render_widget(list, area);
}

/// Güncelleme denetim ekranını çizer.
fn draw_update_check(
    f: &mut Frame,
    area: Rect,
    status: &UpdateStatus,
    check: Option<&UpdateCheck>,
    error: Option<&str>,
) {
    let inner_w = area.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    match status {
        UpdateStatus::Idle => {
            lines.push(Line::from(Span::styled(
                "Güncellemeleri denetlemek için Enter'a basın.",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            if let Some(current) = current_version() {
                lines.push(Line::from(vec![
                    Span::styled("Mevcut sürüm: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(current.to_string(), Style::default().fg(Color::White)),
                ]));
            }
        }
        UpdateStatus::Checking => {
            lines.push(Line::from(Span::styled(
                "Güncellemeler denetleniyor...",
                Style::default().fg(Color::Yellow),
            )));
        }
        UpdateStatus::Done => match check {
            Some(UpdateCheck::UpToDate { current }) => {
                lines.push(Line::from(Span::styled(
                    "Güncelleme yok; sürüm zaten güncel.",
                    Style::default().fg(Color::Green),
                )));
                lines.push(Line::from(vec![
                    Span::styled("Sürüm: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(current.to_string(), Style::default().fg(Color::White)),
                ]));
            }
            Some(UpdateCheck::UpdateAvailable {
                current,
                latest,
                release,
            }) => {
                lines.push(Line::from(Span::styled(
                    "Güncelleme mevcut!",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(vec![
                    Span::styled("Mevcut: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(current.to_string(), Style::default().fg(Color::White)),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("Yeni: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(latest.to_string(), Style::default().fg(Color::White)),
                ]));
                lines.push(Line::from(Span::styled(
                    format!("Yayın notları: {}", truncate(&release.body, inner_w)),
                    Style::default().fg(Color::Gray),
                )));
                lines.push(Line::from(vec![
                    Span::styled("Kaynak: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        truncate(&release.html_url, inner_w.saturating_sub(10)),
                        Style::default().fg(Color::Blue),
                    ),
                ]));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Enter: kurulum onayı   Esc: kapat",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            None => {
                lines.push(Line::from(Span::styled(
                    "Denetim sonucu yok.",
                    Style::default().fg(Color::Yellow),
                )));
            }
            Some(UpdateCheck::Error(msg)) => {
                lines.push(Line::from(Span::styled(
                    format!("Denetim hatası: {msg}"),
                    Style::default().fg(Color::Red),
                )));
            }
        },
    }

    if let Some(err) = error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Hata: {err}"),
            Style::default().fg(Color::Red),
        )));
    }

    let list = List::new(lines.into_iter().map(ListItem::new).collect::<Vec<_>>()).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Güncellemeleri denetle "),
    );
    f.render_widget(list, area);
}

/// Mevcut sürümü `CARGO_PKG_VERSION`'dan döner.
fn current_version() -> Option<Version> {
    Version::parse(env!("CARGO_PKG_VERSION")).ok()
}

/// Dosya seçici ekranını çizer: mevcut dizin, dosya/dizin listesi ve ipuç çubuğu.
fn draw_file_picker(f: &mut Frame, area: Rect, state: &FilePickerState) {
    let inner_w = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = state
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let selected = i == state.selected;
            let focused = selected;
            let style = if focused {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else if entry.is_dir {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            let prefix = if selected { "▶ " } else { "  " };
            let marker = if entry.is_dir { "📁 " } else { "📄 " };
            let name = truncate(
                &format!("{marker}{}", entry.name),
                inner_w.saturating_sub(4),
            );
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(name, style),
            ]))
        })
        .collect();

    let title = format!(
        " {} ({}) ",
        truncate(
            &state.current_dir.to_string_lossy(),
            area.width.saturating_sub(10) as usize,
        ),
        state.entries.len()
    );

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    let mut state_list = ListState::default();
    if !state.entries.is_empty() {
        state_list.select(Some(state.selected));
    }
    f.render_stateful_widget(list, area, &mut state_list);

    if let Some(err) = &state.error {
        let err_area = Rect::new(area.x, area.y.saturating_sub(1), area.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("Hata: {err}"),
                Style::default().fg(Color::Red),
            ))),
            err_area,
        );
    }

    let hint = "↑↓: gezin   ←/h: yukarı   →/l: gir   Enter: seç   Esc: kapat";
    let hint_area = Rect::new(area.x, area.y, area.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        ))),
        hint_area,
    );
}

/// Kutuyu temizleyip kenarlıkla çizer.
fn frame(f: &mut Frame, area: Rect, title: &str) -> Rect {
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);
    inner
}

/// Sarmalanmış metnin kaplayacağı satır sayısı.
fn wrapped_lines(text: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    let mut lines = 0u16;
    for raw in text.lines() {
        let n = raw.chars().count();
        lines += n.max(1).div_ceil(width) as u16;
    }
    lines.max(1)
}

fn draw_confirm(f: &mut Frame, screen: Rect, question: &str) {
    let width = 56.min(screen.width.saturating_sub(MODAL_MARGIN * 2));
    let inner_w = width.saturating_sub(2);
    let hint = "Enter: onayla   Esc: vazgeç";
    let lines = wrapped_lines(question, inner_w) + 2;
    let area = modal_area(screen, width, lines);
    let inner = frame(f, area, "Onay gerekli");
    let mut text = vec![Line::from(Span::styled(
        question.to_string(),
        Style::default().fg(Color::Yellow),
    ))];
    text.push(Line::from(""));
    text.push(Line::from(Span::styled(
        hint,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
}

fn draw_form(f: &mut Frame, screen: Rect, form: &ToolForm) {
    let width = 62.min(screen.width.saturating_sub(MODAL_MARGIN * 2));
    let inner_w = width.saturating_sub(2);
    let mut lines: Vec<Line> = Vec::new();
    for (i, field) in FIELD_ORDER.iter().enumerate() {
        let focused = i == form.focus;
        let label_style = if focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let marker = if focused { "> " } else { "  " };
        let value = match field {
            Field::Category => format!("‹ {} ›", form.category()),
            Field::LaunchKind => format!("‹ {} ›", form.launch_kind.label()),
            _ => {
                let text = match field {
                    Field::Name => &form.name,
                    Field::Description => &form.description,
                    Field::Executable => &form.executable,
                    Field::Args => &form.args,
                    Field::WorkingDir => &form.working_dir,
                    Field::Category | Field::LaunchKind => unreachable!(),
                };
                if text.is_empty() && !focused {
                    "(boş)".to_string()
                } else {
                    text.clone()
                }
            }
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker}{}: ", field.label()), label_style),
            Span::styled(
                truncate(
                    &value,
                    inner_w.saturating_sub(field.label().chars().count() as u16 + 4) as usize,
                ),
                if focused {
                    Style::default().fg(Color::White)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ),
        ]));
        if let Some(err) = &form.errors[i] {
            lines.push(Line::from(Span::styled(
                format!("   ✗ {}", truncate(err, inner_w.saturating_sub(6) as usize)),
                Style::default().fg(Color::Red),
            )));
        }
    }
    lines.push(Line::from(""));
    let hint = if form.focused_field().is_picker() {
        "←→: seç   Tab/↑↓: alan   Enter: denetle   Esc: iptal"
    } else {
        "Tab/↑↓: alan   Enter: denetle   Esc: iptal"
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "Kabuk yok: argümanlar ayrı ve olduğu gibi aktarılır.",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "Program senin yetkilerinle çalışır; Nova güvenliğini doğrulamaz.",
        Style::default().fg(Color::DarkGray),
    )));
    let area = modal_area(screen, width, lines.len() as u16);
    let inner = frame(f, area, "Yeni özel araç");
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_review(f: &mut Frame, screen: Rect, valid: &crate::app::form::ValidTool) {
    let width = 64.min(screen.width.saturating_sub(MODAL_MARGIN * 2));
    let inner_w = width.saturating_sub(2);
    let mut lines = Vec::new();
    for (key, value) in crate::app::form::review_lines(valid) {
        lines.push(Line::from(vec![
            Span::styled(format!("{key}: "), Style::default().fg(Color::Gray)),
            Span::styled(
                truncate(
                    &value,
                    inner_w.saturating_sub(key.chars().count() as u16 + 3) as usize,
                ),
                Style::default().fg(Color::White),
            ),
        ]));
    }
    lines.push(Line::from(Span::styled(
        "Kaydetme state.toml'a yazılır.",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "Nova programı kurmaz veya güvenliğini doğrulamaz.",
        Style::default().fg(Color::Yellow),
    )));
    lines.push(Line::from(Span::styled(
        "Kabuk yok; argümanlar ayrı aktarılır.",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "Program senin kullanıcı yetkilerinle çalışır.",
        Style::default().fg(Color::Yellow),
    )));
    lines.push(Line::from(Span::styled(
        "Enter: kaydet   Esc: forma dön",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    let area = modal_area(screen, width, lines.len() as u16);
    let inner = frame(f, area, "Gözden geçir");
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_picker(f: &mut Frame, screen: Rect, app: &App, index: usize) {
    let width = 60.min(screen.width.saturating_sub(MODAL_MARGIN * 2));
    let inner_w = width.saturating_sub(2);
    let dirs = app.state.project_dirs();
    let mut lines = Vec::new();
    if dirs.is_empty() {
        lines.push(Line::from(Span::styled(
            "Kayıtlı proje dizini yok. 'A' ile ekleyebilirsin.",
            Style::default().fg(Color::Yellow),
        )));
    }
    for (i, dir) in dirs.iter().enumerate() {
        let active = app.state.active_project.as_deref() == Some(dir.as_path());
        let exists = dir.exists();
        let marker = if i == index { "> " } else { "  " };
        let mark = if active { "● " } else { "  " };
        let missing = if exists { "" } else { "  (dizin yok)" };
        let style = if !exists {
            Style::default().fg(Color::Red)
        } else if i == index {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        lines.push(Line::from(Span::styled(
            format!(
                "{marker}{mark}{}{missing}",
                truncate(
                    &dir.display().to_string(),
                    inner_w.saturating_sub(6) as usize
                )
            ),
            style,
        )));
    }
    if let Some(active) = &app.state.active_project {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "Etkin: {} (yalnızca bu oturum)",
                truncate(
                    &active.display().to_string(),
                    inner_w.saturating_sub(18) as usize
                )
            ),
            Style::default().fg(Color::Green),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑↓: seç   Enter: etkin yap   A: ekle   D: listeden çıkar   Esc: kapat",
        Style::default().fg(Color::DarkGray),
    )));
    let area = modal_area(screen, width, lines.len() as u16);
    let inner = frame(f, area, "Proje dizinleri");
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_project_add(f: &mut Frame, screen: Rect, input: &str, error: Option<&str>) {
    let width = 58.min(screen.width.saturating_sub(MODAL_MARGIN * 2));
    let inner_w = width.saturating_sub(2);
    let mut lines = vec![Line::from(vec![
        Span::styled("Yol: ", Style::default().fg(Color::Gray)),
        Span::styled(input.to_string(), Style::default().fg(Color::White)),
        Span::styled("▏", Style::default().fg(Color::Cyan)),
    ])];
    if let Some(err) = error {
        lines.push(Line::from(Span::styled(
            format!("✗ {}", truncate(err, inner_w.saturating_sub(4) as usize)),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Girilen dizin var olmalı; Nova dosya sistemi oluşturmaz.",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "Enter: ekle   Esc: geri",
        Style::default().fg(Color::DarkGray),
    )));
    let area = modal_area(screen, width, lines.len() as u16);
    let inner = frame(f, area, "Proje dizini ekle");
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_history(f: &mut Frame, screen: Rect, app: &App) {
    let width = 64.min(screen.width.saturating_sub(MODAL_MARGIN * 2));
    let recents = &app.state.file.recents;
    let mut lines = Vec::new();
    if recents.is_empty() {
        lines.push(Line::from(Span::styled(
            "Henüz başlatma denemesi yok.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "İstek sonucu gösterilir; aracın sağlıklı çalıştığı doğrulanmaz.",
            Style::default().fg(Color::DarkGray),
        )));
    }
    // Kutu içine sığacak kadar kayıt göster (başlık + ipucu + boş satır).
    let capacity = screen.height.saturating_sub(8) as usize;
    for entry in recents.iter().take(capacity.max(1)) {
        lines.push(Line::from(vec![
            Span::styled("• ", Style::default().fg(Color::Cyan)),
            Span::styled(entry.name.clone(), Style::default().fg(Color::White)),
            Span::styled(
                format!(
                    "  · {}  · {}",
                    match entry.status {
                        crate::config::state::AttemptStatus::RequestSent => "istek gönderildi",
                        crate::config::state::AttemptStatus::Rejected => "reddedildi",
                    },
                    relative_time(entry.at, crate::app::now_secs())
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    if recents.len() > capacity.max(1) {
        lines.push(Line::from(Span::styled(
            format!("… {} kayıt daha", recents.len() - capacity.max(1)),
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "X: geçmişi temizle (onay ister)   Esc: kapat",
        Style::default().fg(Color::DarkGray),
    )));
    let area = modal_area(screen, width, lines.len() as u16);
    let inner = frame(f, area, "Son kullanılanlar");
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

#[allow(clippy::too_many_arguments)]
fn draw_settings(
    f: &mut Frame,
    screen: Rect,
    app: &App,
    preferred_terminal: &str,
    default_working_dir: &str,
    theme: &str,
    default_ai_tool: &str,
    default_image_app: &str,
    default_video_app: &str,
    focus: usize,
    error: Option<&str>,
) {
    let width = 68.min(screen.width.saturating_sub(MODAL_MARGIN * 2));
    let inner_w = width.saturating_sub(2) as usize;
    let mut lines = Vec::new();
    for (index, (label, value)) in [
        ("Varsayılan terminal", preferred_terminal),
        (
            "Varsayılan çalışma dizini",
            if default_working_dir.is_empty() {
                "(boş: ev dizini)"
            } else {
                default_working_dir
            },
        ),
        (
            "Tema",
            if theme.is_empty() {
                "(varsayılan)"
            } else {
                theme
            },
        ),
        (
            "Varsayılan AI aracı",
            if default_ai_tool.is_empty() {
                "(seçilmedi)"
            } else {
                default_ai_tool
            },
        ),
        (
            "Varsayılan resim uygulaması",
            if default_image_app.is_empty() {
                "(seçilmedi)"
            } else {
                default_image_app
            },
        ),
        (
            "Varsayılan video uygulaması",
            if default_video_app.is_empty() {
                "(seçilmedi)"
            } else {
                default_video_app
            },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let active = index == focus;
        let marker = if active { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{marker}{label}: "),
                if active {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                },
            ),
            Span::styled(
                truncate(value, inner_w.saturating_sub(label.chars().count() + 4)),
                Style::default().fg(Color::White),
            ),
        ]));
    }
    lines.push(Line::from(Span::styled(
        "Şu an yalnızca Konsole desteklenir; boş terminal alanı varsayılanı kullanır.",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(vec![
        Span::styled("Katalog: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate(&app.config_path_display, inner_w.saturating_sub(9)),
            Style::default().fg(Color::Gray),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Kullanıcı ayarları: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate(&app.state_path_display, inner_w.saturating_sub(19)),
            Style::default().fg(Color::Gray),
        ),
    ]));
    if let Some(error) = error {
        lines.push(Line::from(Span::styled(
            format!("Hata: {}", truncate(error, inner_w.saturating_sub(7))),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(Line::from(Span::styled(
        "Tab/↑↓: alan   Enter: kaydet   Esc: vazgeç",
        Style::default().fg(Color::Cyan),
    )));
    let area = modal_area(screen, width, lines.len() as u16);
    let inner = frame(f, area, "Ayarlar");
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// Modalların küçük terminallerde genişlik sınırına uyduğunu doğrular.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_area_never_exceeds_screen() {
        for w in 1..=120u16 {
            for h in 1..=40u16 {
                let screen = Rect::new(0, 0, w, h);
                let area = modal_area(screen, 76, 30);
                assert!(area.width <= w, "genişlik taştı: {w}x{h}");
                assert!(area.height <= h, "yükseklik taştı: {w}x{h}");
                assert!(area.right() <= w && area.bottom() <= h);
            }
        }
    }

    #[test]
    fn wrapped_lines_counts_long_text() {
        assert_eq!(wrapped_lines("abc", 10), 1);
        assert_eq!(wrapped_lines("a\nb", 10), 2);
        assert_eq!(wrapped_lines(&"x".repeat(25), 10), 3);
    }
}
