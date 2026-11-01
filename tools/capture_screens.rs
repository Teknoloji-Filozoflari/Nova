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

    eprintln!("frames → {}", out.display());
}

fn base_app() -> App {
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
    dump(&app, 120, 36, out.join("01-main-list.json"));
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
    dump(&app, 140, 40, out.join("02-store.json"));
}

fn capture_settings(out: &Path) {
    let mut app = base_app();
    app.select_settings_tab();
    app.settings_action_index = 0; // Güncelle (Nova)
    app.focus = PanelFocus::Tool; // Ayarlar listesi vurgulu
    app.status_message = "Ayarlar: Güncelle (Releases + repo main) · Tema · Dil · Gelişmiş.".into();
    // Ayarlar alt detay şeridini sade tutmak için AI kategorisinden çık.
    set_category(&mut app, "Sistem ve Kaynak Kullanımı");
    dump(&app, 100, 32, out.join("03-settings.json"));
}

fn capture_ai_actions(out: &Path) {
    let mut app = base_app();
    set_category(&mut app, "AI ve Kodlama");
    // Claude Code seçili; aksiyon paneli yan yana (genişlik ≥ 110).
    if let Some(idx) = app
        .filtered_tools()
        .iter()
        .position(|e| e.id == "claude")
        .or_else(|| app.filtered_tools().iter().position(|e| e.has_ai_actions()))
    {
        app.tool_index = idx;
    }
    app.focus = PanelFocus::ToolActions;
    app.tool_action_index = 0;
    app.status_message = "AI araçları: detay yanında Aç / Devam et / Çıkış / Yardım.".into();
    dump(&app, 140, 36, out.join("04-ai-actions.json"));
}

fn dump(app: &App, width: u16, height: u16, path: PathBuf) {
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
        Color::Cyan => [64, 200, 210, 255],
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
