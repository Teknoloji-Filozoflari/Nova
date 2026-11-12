//! Nova TUI ekran görüntüsü yakalayıcı (docs/screenshots).
//!
//! Gerçek bir PTY açmaz: `ratatui::backend::TestBackend` ile `ui::draw`
//! çıktısını hücre hücre JSON’a döker. PNG üretimi için
//! `docs/screenshots/render_frames.py` kullanılır.
//!
//! Kullanım:
//!   cargo run --bin capture-screens -- docs/screenshots/frames
//!   python3 docs/screenshots/render_frames.py docs/screenshots/frames docs/screenshots

use nova::app::{App, PanelFocus};
use nova::catalog::{Catalog, CATEGORIES};
use nova::commands::CommandsBrowseMode;
use nova::detection::InstalledStatus;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let out = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("docs/screenshots/frames"));
    fs::create_dir_all(&out).expect("frames dizini oluşturulamadı");

    capture_main_list(&out);
    capture_store(&out);
    capture_settings(&out);
    capture_ai_actions(&out);
    capture_commands_start(&out);
    capture_commands_task_detail(&out);
    capture_commands_security_checklist(&out);
    capture_topic_guide_wide(&out);
    capture_topic_guide_learning(&out);
    capture_topic_guide_table_polish(&out);
    capture_topic_guide_table_rows(&out);
    capture_syntax_list(&out);
    capture_syntax_detail(&out);
    capture_quick_reference(&out);
    capture_quick_reference_card(&out);

    eprintln!("frames → {}", out.display());
}

fn base_app() -> App {
    // Ekran görüntüleri **gömülü (vendored) komut bankasından** üretilir:
    // kullanıcının disk önbelleği sürüme göre değişirse kareler kaymasın.
    let cache_dir = std::env::temp_dir().join("nova-capture-commands");
    std::env::set_var("NOVA_COMMANDS_DIR", &cache_dir);
    let entries = nova::catalog::loader::builtin_catalog().expect("gömülü katalog");
    let catalog = Catalog::from_entries(entries).expect("katalog");
    let mut app = App::new(catalog);
    // README görsellerinde dolu ana liste için gerçekçi kurulu set.
    mark_installed(
        &mut app,
        &[
            "btop",
            "fastfetch",
            "procs",
            "btm",
            "dust",
            "duf",
            "lazygit",
            "rg",
            "fd",
            "fzf",
            "eza",
            "bat",
            "yazi",
            "claude",
            "opencode",
            "codex",
            "minimax-code",
            "spf",
            "gdu",
            "bandwhich",
            "curl",
            "jq",
            "gh",
            "delta",
            "lazydocker",
        ],
    );
    // Mağaza durumunu da kısmen işaretle (liste rozetleri).
    for id in ["ripgrep", "fzf", "btop", "lazygit", "aider", "eza", "bat"] {
        app.store_status
            .insert(id.into(), InstalledStatus::Installed);
    }
    app
}

fn mark_installed(app: &mut App, ids: &[&str]) {
    for entry in app.catalog.entries_mut() {
        if ids.iter().any(|id| entry.id == *id) {
            entry.status = InstalledStatus::Installed;
            entry.resolved_path = Some(PathBuf::from(format!("/usr/bin/{}", entry.executable)));
        }
    }
}

fn set_category(app: &mut App, name: &str) {
    app.category_index = CATEGORIES
        .iter()
        .position(|c| *c == name)
        .unwrap_or_else(|| panic!("kategori yok: {name}"));
}

fn capture_main_list(out: &Path) {
    let mut app = base_app();
    set_category(&mut app, "Sistem ve Kaynak Kullanımı");
    app.focus = PanelFocus::Tool;
    app.tool_index = 0;
    app.status_message = "Nova — kurulu araçları başlat, Mağaza’dan yenilerini keşfet.".into();
    dump(&mut app, 120, 36, out.join("01-main-list.json"));
}

fn capture_store(out: &Path) {
    let mut app = base_app();
    app.select_store_tab();
    // Dosya ve Arama — ripgrep / fzf görünür.
    if let Some(idx) = nova::store::STORE_CATEGORIES
        .iter()
        .position(|c| *c == "Dosya ve Arama")
    {
        app.store_category_index = idx;
    }
    app.focus = PanelFocus::StoreApp;
    app.store_app_index = 0;
    app.status_message =
        "Mağaza: imzalı katalog + Nova-Store repo (main). Arch’ta pacman tercih edilir.".into();
    dump(&mut app, 140, 40, out.join("02-store.json"));
}

fn capture_settings(out: &Path) {
    let mut app = base_app();
    app.select_settings_tab();
    app.settings_action_index = 0; // Güncelle (Nova)
    app.focus = PanelFocus::Tool; // Ayarlar listesi vurgulu
    app.status_message =
        "Ayarlar: Güncelle · Mağaza kataloğu · Komut bankası · Tema · Dil · Gelişmiş.".into();
    // Ayarlar alt detay şeridini sade tutmak için AI kategorisinden çık.
    set_category(&mut app, "Sistem ve Kaynak Kullanımı");
    dump(&mut app, 100, 32, out.join("03-settings.json"));
}

fn capture_ai_actions(out: &Path) {
    let mut app = base_app();
    set_category(&mut app, "AI ve Kodlama");
    // Claude Code seçili; Enter → Mağaza tarzı aksiyon modalı.
    if let Some(idx) = app
        .filtered_tools()
        .iter()
        .position(|e| e.id == "claude")
        .or_else(|| app.filtered_tools().iter().position(|e| e.has_ai_actions()))
    {
        app.tool_index = idx;
    }
    if let Some(entry) = app.selected_tool().cloned() {
        app.open_tool_actions_menu(&entry);
    }
    app.status_message = "Kurulu AI: Enter → aksiyon modalı (Aç / Devam / …). Esc kapatır.".into();
    dump(&mut app, 140, 36, out.join("04-ai-actions.json"));
}

/// Komut Rehberi · Başlangıç — Mod + Liste + yan Detay (max 3; TR tema).
fn capture_commands_start(out: &Path) {
    let mut app = base_app();
    app.select_commands_tab();
    assert_eq!(app.commands_mode, CommandsBrowseMode::Start);
    app.focus = PanelFocus::CommandsList;
    app.commands_entry_index = 0;
    app.status_message =
        "Komut Rehberi · Başlangıç: öğrenme sırası. PATH ≠ katalog. Çalıştırma yok.".into();
    // Yan detay için ≥140 sütun (tabs+mode+list_min+detail).
    dump(&mut app, 160, 40, out.join("05-commands-start.json"));
}

/// Komut Rehberi · Göreve göre drill + yan detay (commit sonrası: Grup+Liste+Detay).
///
/// Eski 4 sütunlu (Mod+Grup+Liste+Detay) düzeni bilinçli olarak yakalanmaz —
/// #62 yoğunluk kuralı: en fazla 3 içerik paneli.
fn capture_commands_task_detail(out: &Path) {
    let mut app = base_app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::ByTask);

    // disks / Disk alanını kontrol et — net görev + ilgili komutlar.
    let cats = app.commands_bank.task_categories();
    if let Some(idx) = cats.iter().position(|c| *c == "disks") {
        app.commands_category_index = idx;
    }
    let task_idx = app
        .filtered_tasks()
        .iter()
        .position(|t| t.id == "check-disk-space")
        .unwrap_or(0);
    app.commands_entry_index = task_idx;
    app.focus = PanelFocus::CommandsList;
    // Enter ile aynı yol: drill + commit (breadcrumb; mod paneli gizlenir).
    app.commands_task_drill = Some("check-disk-space".into());
    app.commands_entry_index = 0;
    app.commit_commands_guide();
    // Örnekli bir komut satırı seç (detay dolu görünsün).
    if let Some(idx) = app
        .filtered_bank_commands()
        .iter()
        .position(|c| !c.examples.is_empty())
    {
        app.commands_entry_index = idx;
    }
    app.status_message =
        "Göreve göre › Disk › Disk alanını kontrol et — ilgili komutlar; Enter çalıştırmaz.".into();
    dump(&mut app, 160, 42, out.join("06-commands-task-detail.json"));
}

/// Komut Rehberi · Sunucu güvenlik kontrolü (checklist; çalıştırmaz).
fn capture_commands_security_checklist(out: &Path) {
    let mut app = base_app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::ServerSecurity);
    app.focus = PanelFocus::CommandsList;
    app.commands_entry_index = 0;
    if let Some(step) = app.filtered_checklist_steps().first() {
        app.state
            .file
            .command_checklist
            .insert(step.state_key(), "ok".into());
    }
    app.status_message =
        "Sunucu güvenlik kontrolü — Space durum; tarama/çalıştırma yok; parola saklanmaz.".into();
    dump(
        &mut app,
        160,
        42,
        out.join("07-commands-security-checklist.json"),
    );
}

/// Terminal temelleri — geniş yan detay: Alt konular + komut tablosu (ellipsis yok).
fn capture_topic_guide_wide(out: &Path) {
    let mut app = base_app();
    select_topic_guide(&mut app, "terminal-basics");
    widen_commands_detail(&mut app);
    app.focus = PanelFocus::CommandsDetail;
    // Öğrenme sırası uzun — Alt konular + komut tablosunu çerçeveye al.
    let wrap_w = 100usize;
    let concept = app
        .commands_bank
        .concepts
        .iter()
        .find(|c| c.id == "terminal-basics")
        .expect("terminal-basics");
    let lines = nova::ui::guide_detail::topic_guide_lines(&app, concept, wrap_w);
    let scroll = lines
        .iter()
        .position(|l| {
            let t: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            t.trim() == "Alt konular:" || t.trim() == "Subtopics:"
        })
        .unwrap_or(0) as u16;
    app.commands_detail_scroll = scroll.saturating_sub(1);
    app.status_message =
        "Konu Rehberi · Terminal temelleri — geniş detay; Alt konular + komut tablosu okunur."
            .into();
    dump(&mut app, 180, 46, out.join("preview-topic-guide-wide.json"));
}

/// Borular konusu — KOMUT TABLOSU renkli pill + ayırıcı + öğrenme snippet.
fn capture_topic_guide_table_polish(out: &Path) {
    let mut app = base_app();
    select_topic_guide(&mut app, "shell-pipes-redirection");
    widen_commands_detail(&mut app);
    app.focus = PanelFocus::CommandsDetail;
    let wrap_w = 72usize;
    let concept = app
        .commands_bank
        .concepts
        .iter()
        .find(|c| c.id == "shell-pipes-redirection")
        .expect("shell-pipes-redirection");
    let lines = nova::ui::guide_detail::topic_guide_lines(&app, concept, wrap_w);
    // Ortadan başla: birkaç tablo satırı + öğrenme snippet’leri birlikte görünsün.
    let table_at = lines
        .iter()
        .position(|l| {
            let t: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            let t = t.trim();
            t.starts_with("KOMUT TABLOSU")
                || t.starts_with("Komut tablosu")
                || t == "Command table:"
        })
        .unwrap_or(0);
    let learn_at = lines
        .iter()
        .position(|l| {
            let t: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            let t = t.trim();
            t == "ÖĞRENME:" || t == "Learning:"
        })
        .unwrap_or(table_at);
    // Tablo sonuna yakın + öğrenme (örnek `cut … | sort`).
    app.commands_detail_scroll = learn_at.saturating_sub(10) as u16;
    let _ = table_at;
    app.status_message =
        "Konu Rehberi · Borular — komut tablosu cyan/risk pill/ayırıcı; öğrenme snippet renkli."
            .into();
    // Tablo + öğrenme ``snippet`` aynı karede.
    dump(&mut app, 160, 54, out.join("topic-guide-table-polish.json"));
}

/// KOMUT TABLOSU satırları — cyan ad · risk pill · ince ayırıcı (yakın plan).
fn capture_topic_guide_table_rows(out: &Path) {
    let mut app = base_app();
    select_topic_guide(&mut app, "shell-pipes-redirection");
    widen_commands_detail(&mut app);
    app.focus = PanelFocus::CommandsDetail;
    let wrap_w = 72usize;
    let concept = app
        .commands_bank
        .concepts
        .iter()
        .find(|c| c.id == "shell-pipes-redirection")
        .expect("shell-pipes-redirection");
    let lines = nova::ui::guide_detail::topic_guide_lines(&app, concept, wrap_w);
    let scroll = lines
        .iter()
        .position(|l| {
            let t: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            let t = t.trim();
            t.starts_with("KOMUT TABLOSU")
                || t.starts_with("Komut tablosu")
                || t == "Command table:"
        })
        .unwrap_or(0) as u16;
    app.commands_detail_scroll = scroll.saturating_sub(1);
    app.status_message =
        "Konu Rehberi · KOMUT TABLOSU — cyan ad · risk pill · ince ayırıcı.".into();
    dump(&mut app, 160, 40, out.join("topic-guide-table-rows.json"));
}

/// Aynı konu — Öğrenme bölümüne kaydırılmış (paragraf boşlukları).
fn capture_topic_guide_learning(out: &Path) {
    let mut app = base_app();
    select_topic_guide(&mut app, "terminal-basics");
    widen_commands_detail(&mut app);
    app.focus = PanelFocus::CommandsDetail;
    let wrap_w = 100usize;
    let concept = app
        .commands_bank
        .concepts
        .iter()
        .find(|c| c.id == "terminal-basics")
        .expect("terminal-basics");
    let lines = nova::ui::guide_detail::topic_guide_lines(&app, concept, wrap_w);
    // "Öğrenme:" gövde başlığı — "Öğrenme sırası" ile karışmasın.
    let scroll = lines
        .iter()
        .position(|l| {
            let t: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            let t = t.trim();
            t == "Öğrenme:" || t == "Learning:"
        })
        .unwrap_or(0) as u16;
    app.commands_detail_scroll = scroll;
    app.status_message =
        "Konu Rehberi · Öğrenme — paragraflı düzen; sıkışık duvar metin değil.".into();
    dump(
        &mut app,
        180,
        46,
        out.join("preview-topic-guide-learning.json"),
    );
}

fn select_topic_guide(app: &mut App, concept_id: &str) {
    app.select_commands_tab();
    assert_eq!(app.commands_mode, CommandsBrowseMode::Start);
    app.commands_filters.clear();
    let idx = app
        .filtered_concepts()
        .iter()
        .position(|c| c.id == concept_id)
        .unwrap_or_else(|| panic!("konu yok: {concept_id}"));
    app.commands_entry_index = idx;
    app.focus = PanelFocus::CommandsList;
}

fn widen_commands_detail(app: &mut App) {
    use nova::config::state::UiScreen;
    let sizes = app.state.file.prefs.pane_sizes.get_mut(UiScreen::Commands);
    sizes.detail_width = Some(nova::ui::DETAIL_PANEL_WIDTH_MAX);
    sizes.list = Some(22);
    sizes.secondary = Some(16);
    sizes.tabs = Some(14);
}

/// Tüm komutlar listesi — ÖRNEK sütunu sözdizimi renkleri (geniş liste).
fn capture_syntax_list(out: &Path) {
    use nova::config::state::UiScreen;
    let mut app = base_app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    app.commit_commands_guide();
    // Dosya kategorisi — ls/cd/grep örnekleri.
    let cats = app.commands_bank_categories();
    if let Some(idx) = cats
        .iter()
        .position(|c| *c == "files" || *c == "filesystem")
    {
        app.commands_category_index = idx;
    }
    let sizes = app.state.file.prefs.pane_sizes.get_mut(UiScreen::Commands);
    sizes.list = Some(56);
    sizes.secondary = Some(18);
    sizes.tabs = Some(16);
    sizes.detail_width = Some(40);
    app.focus = PanelFocus::CommandsList;
    // Seçimi alta al ki üst satırlar renkli ÖRNEK göstersin.
    if app.filtered_bank_commands().len() > 3 {
        app.commands_entry_index = 3;
    }
    app.status_message =
        "Komut Rehberi · Tüm komutlar — ÖRNEK sözdizimi: $ muted · cmd cyan · flag sarı · path yeşil."
            .into();
    dump(&mut app, 160, 40, out.join("syntax-highlight-list.json"));
}

/// ls detay — SÖZDİZİMİ + ÖRNEKLER renkli.
fn capture_syntax_detail(out: &Path) {
    use nova::config::state::UiScreen;
    let mut app = base_app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::AllCommands);
    app.commit_commands_guide();
    let cats = app.commands_bank_categories();
    if let Some(idx) = cats.iter().position(|c| {
        app.commands_bank
            .by_category(c)
            .iter()
            .any(|cmd| cmd.name == "ls")
    }) {
        app.commands_category_index = idx;
    }
    if let Some(idx) = app
        .filtered_bank_commands()
        .iter()
        .position(|c| c.name == "ls")
    {
        app.commands_entry_index = idx;
    }
    let sizes = app.state.file.prefs.pane_sizes.get_mut(UiScreen::Commands);
    sizes.detail_width = Some(nova::ui::DETAIL_PANEL_WIDTH_MAX);
    sizes.list = Some(18);
    sizes.secondary = Some(16);
    sizes.tabs = Some(14);
    app.focus = PanelFocus::CommandsDetail;
    app.commands_detail_scroll = 0;
    app.status_message =
        "Komut Rehberi · ls — SÖZDİZİMİ / ÖRNEKLER sözdizimi renklendirme; Enter çalıştırmaz."
            .into();
    dump(&mut app, 170, 44, out.join("syntax-highlight-detail.json"));
}

/// Hızlı referans — geniş tablo + filtre satırı (tasarım-2).
fn capture_quick_reference(out: &Path) {
    use nova::config::state::UiScreen;
    let mut app = base_app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::QuickReference);
    app.commands_quick_category = Some("system".into());
    app.search_query = "system".into();
    let sizes = app.state.file.prefs.pane_sizes.get_mut(UiScreen::Commands);
    sizes.tabs = Some(18);
    sizes.secondary = Some(20);
    app.focus = PanelFocus::CommandsList;
    app.commands_entry_index = 0;
    // Durum satırı bağlam cümlesini göstersin (tasarım-2 alt şeridi).
    app.status_message.clear();
    dump(&mut app, 170, 34, out.join("commands-quick-reference.json"));
}

/// Hızlı referans — kart açık (alt şerit): tasarım-1 kart düzeni.
fn capture_quick_reference_card(out: &Path) {
    use nova::config::state::UiScreen;
    let mut app = base_app();
    app.select_commands_tab();
    app.set_commands_mode(CommandsBrowseMode::QuickReference);
    let sizes = app.state.file.prefs.pane_sizes.get_mut(UiScreen::Commands);
    sizes.tabs = Some(18);
    sizes.secondary = Some(20);
    app.focus = PanelFocus::CommandsList;
    if let Some(idx) = app
        .filtered_bank_commands()
        .iter()
        .position(|c| c.name == "ls")
    {
        app.commands_entry_index = idx;
    }
    // Kart şeridi açık (Enter ile açılır) — tasarım-1 kartı.
    app.commands_quick_card = true;
    app.focus = PanelFocus::CommandsDetail;
    dump(
        &mut app,
        170,
        46,
        out.join("commands-quick-reference-card.json"),
    );
}

fn dump(app: &mut App, width: u16, height: u16, path: PathBuf) {
    app.terminal_size = Some((width, height));
    app.narrow_mode = width < nova::ui::WIDE_MIN_WIDTH;

    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("TestBackend");
    terminal.draw(|f| nova::ui::draw(f, app)).expect("draw");
    let buffer = terminal.backend().buffer();

    let mut cells = Vec::with_capacity((width as usize) * (height as usize));
    for y in 0..height {
        for x in 0..width {
            let cell = &buffer[(x, y)];
            let (fg, bg) = (color_rgba(cell.fg), color_rgba(cell.bg));
            cells.push(serde_json::json!({
                "s": cell.symbol(),
                "fg": fg,
                "bg": bg,
            }));
        }
    }

    let payload = serde_json::json!({
        "width": width,
        "height": height,
        "cells": cells,
        "note": "Nova TestBackend frame — authentic ui::draw output",
    });

    let mut file = fs::File::create(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    file.write_all(serde_json::to_string(&payload).unwrap().as_bytes())
        .expect("yazılamadı");
    eprintln!("wrote {}", path.display());
}

fn color_rgba(c: Color) -> [u8; 4] {
    match c {
        Color::Reset => [0, 0, 0, 0],
        Color::Black => [12, 14, 18, 255],
        Color::Red => [224, 78, 78, 255],
        Color::Green => [72, 180, 110, 255],
        Color::Yellow => [230, 190, 70, 255],
        Color::Blue => [70, 130, 220, 255],
        Color::Magenta => [200, 90, 180, 255],
        Color::Cyan => [0, 229, 255, 255],
        Color::Gray => [160, 168, 180, 255],
        Color::DarkGray => [90, 98, 110, 255],
        Color::LightRed => [255, 120, 120, 255],
        Color::LightGreen => [120, 220, 150, 255],
        Color::LightYellow => [255, 230, 120, 255],
        Color::LightBlue => [120, 180, 255, 255],
        Color::LightMagenta => [240, 140, 220, 255],
        Color::LightCyan => [120, 230, 240, 255],
        Color::White => [236, 240, 245, 255],
        Color::Rgb(r, g, b) => [r, g, b, 255],
        Color::Indexed(i) => indexed_rgba(i),
    }
}

fn indexed_rgba(i: u8) -> [u8; 4] {
    // Basit 256-renk yaklaşık; Nova temaları çoğunlukla named/Rgb kullanır.
    match i {
        0 => [12, 14, 18, 255],
        1 => [224, 78, 78, 255],
        2 => [72, 180, 110, 255],
        3 => [230, 190, 70, 255],
        4 => [70, 130, 220, 255],
        5 => [200, 90, 180, 255],
        6 => [64, 200, 210, 255],
        7 => [200, 205, 210, 255],
        8 => [90, 98, 110, 255],
        9..=15 => color_rgba(match i {
            9 => Color::LightRed,
            10 => Color::LightGreen,
            11 => Color::LightYellow,
            12 => Color::LightBlue,
            13 => Color::LightMagenta,
            14 => Color::LightCyan,
            _ => Color::White,
        }),
        _ => [180, 180, 180, 255],
    }
}
