//! Faz 1-3 kullanıcı deneyimi testleri.
//!
//! Arama, kategori seçimi ve favori mantığının TUI bağlamında birlikte
//! doğru çalıştığını doğrular. Katalog gömülü TOML dosyasından yüklenir;
//! gerçek Konsole süreci başlatılmaz.

use nova::app::{App, PanelFocus};
use nova::catalog::{Catalog, LaunchMode, WorkingDirectory, CATEGORIES};
use nova::detection::InstalledStatus;

fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::empty())
}

fn category_index(name: &str) -> usize {
    CATEGORIES
        .iter()
        .position(|c| *c == name)
        .unwrap_or_else(|| panic!("bilinmeyen kategori: {name}"))
}

/// Uygulamayı gömülü (varsayılan) katalogla oluşturur.
///
/// Kullanıcının gerçek yapılandırma dosyası okunmaz: testler makineye
/// göre değişmemeli. Ana liste yalnızca kurulu araçları gösterdiği için
/// UX testlerinde tüm girdiler kurulu işaretlenir.
fn app_with_sample() -> App {
    let entries = nova::catalog::loader::builtin_catalog().expect("gömülü katalog okunmalı");
    let mut app = App::new(Catalog::from_entries(entries).expect("katalog oluşturulamadı"));
    for entry in app.catalog.entries_mut() {
        entry.status = InstalledStatus::Installed;
    }
    app
}

fn go_to_category(app: &mut App, name: &str) {
    app.category_index = category_index(name);
    app.tool_index = 0;
    app.focus = PanelFocus::Tool;
}

#[test]
fn all_required_categories_exist_in_order() {
    assert_eq!(
        CATEGORIES,
        &[
            "AI ve Kodlama",
            "Git ve Geliştirici Araçları",
            "Dosya Yönetimi ve Arama",
            "Sistem ve Kaynak Kullanımı",
            "Ağ ve Bağlantı",
            "Docker ve Sunucu",
            "Günlük Terminal Yardımcıları",
            "Özel Araçlar",
        ]
    );
}

#[test]
fn category_selection_changes_visible_tools() {
    let mut app = app_with_sample();

    go_to_category(&mut app, "Docker ve Sunucu");
    let docker_tools = app.filtered_tools();
    assert!(!docker_tools.is_empty());
    assert!(docker_tools
        .iter()
        .all(|e| e.category == "Docker ve Sunucu"));
    let docker_first = docker_tools.first().map(|e| e.id.clone());

    go_to_category(&mut app, "AI ve Kodlama");
    let ai_tools = app.filtered_tools();
    assert!(ai_tools.iter().all(|e| e.category == "AI ve Kodlama"));
    assert_ne!(docker_first, ai_tools.first().map(|e| e.id.clone()));
}

#[test]
fn every_category_has_at_least_one_tool_except_placeholder_ones() {
    // Bu kategori yalnızca kullanıcı araçları için boş tutulur. Boşken
    // arayüzün çökmediği aşağıdaki testte doğrulanır.
    const PLACEHOLDER: [&str; 1] = ["Özel Araçlar"];
    for (i, cat) in CATEGORIES.iter().enumerate() {
        let mut probe = app_with_sample();
        probe.category_index = i;
        if PLACEHOLDER.contains(cat) {
            assert!(
                probe.filtered_tools().is_empty(),
                "yer tutucu kategori boş olmalı: {cat}"
            );
            continue;
        }
        assert!(
            !probe.filtered_tools().is_empty(),
            "kategori boş olmamalı: {cat}"
        );
    }
}

#[test]
fn empty_category_does_not_break_navigation_or_selection() {
    let mut app = app_with_sample();
    go_to_category(&mut app, "Özel Araçlar");
    assert!(app.filtered_tools().is_empty());
    assert!(app.selected_tool().is_none());
    // Boş listede gezinme ve Enter çökmemeli.
    app.handle_key(key(crossterm::event::KeyCode::Down))
        .unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert!(app.running);
}

#[test]
fn arrow_keys_switch_panel() {
    let mut app = app_with_sample();
    assert_eq!(app.focus, PanelFocus::Category);
    app.handle_key(key(crossterm::event::KeyCode::Right))
        .unwrap();
    assert_eq!(app.focus, PanelFocus::Tool);
    app.handle_key(key(crossterm::event::KeyCode::Left))
        .unwrap();
    assert_eq!(app.focus, PanelFocus::Category);
}

#[test]
fn down_key_navigates_tools_within_category() {
    let mut app = app_with_sample();
    go_to_category(&mut app, "Sistem ve Kaynak Kullanımı");
    let count = app.filtered_tools().len();
    assert!(count >= 2);

    assert_eq!(app.tool_index, 0);
    app.handle_key(key(crossterm::event::KeyCode::Down))
        .unwrap();
    assert_eq!(app.tool_index, 1);
    // Son öğede durur.
    for _ in 0..5 {
        app.handle_key(key(crossterm::event::KeyCode::Down))
            .unwrap();
    }
    assert_eq!(app.tool_index, count - 1);
    for _ in 0..10 {
        app.handle_key(key(crossterm::event::KeyCode::Up)).unwrap();
    }
    assert_eq!(app.tool_index, 0);
}

#[test]
fn search_matches_every_field_case_insensitively() {
    let mut app = app_with_sample();
    // Arama kategoriden bağımsız çalışsın: kategori panelindeyken de sonuç bulur.
    let cases = [
        ("OpenCode", "opencode"),               // ad
        ("yazi", "yazi"),                       // ad / komut
        ("bandwhich", "bandwhich"),             // komut
        ("AĞ VE BAĞLANTI", "curl"),             // kategori (büyük harf)
        ("hız testi", "cloudflare-speed-test"), // açıklama
    ];
    for (query, expected_id) in cases {
        app.search_query = query.into();
        // Kategori filtresi uygulandığı için sonucu katalog genelinde doğrula.
        let global: Vec<_> = app
            .catalog
            .entries()
            .iter()
            .filter(|e| app.matches_search(e))
            .map(|e| e.id.as_str())
            .collect();
        assert!(
            global.contains(&expected_id),
            "'{query}' sorgusu {expected_id} aracını bulmalı; bulunanlar: {global:?}"
        );
    }
}

#[test]
fn search_within_category_filters_current_list() {
    let mut app = app_with_sample();
    go_to_category(&mut app, "Sistem ve Kaynak Kullanımı");
    // Arama kategoriye bağlı değildir; "btop" ayrıca `btm`nin açıklamasında
    // ve alternatif bilgisinde geçtiği için iki sonuç döner.
    app.search_query = "btop".into();
    let tools = app.filtered_tools();
    let ids: Vec<&str> = tools.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["btop", "btm"], "kategori filtresi aramada uygulanmaz");

    app.search_query = "bulunmayan-sorgu-xyz".into();
    assert!(app.filtered_tools().is_empty());
    assert!(app.selected_tool().is_none());
}

#[test]
fn typing_in_search_updates_results_and_esc_clears() {
    let mut app = app_with_sample();
    app.handle_key(key(crossterm::event::KeyCode::Char('/')))
        .unwrap();
    assert!(app.search_active);
    assert_eq!(app.focus, PanelFocus::Tool);

    for c in "fastfetch".chars() {
        app.handle_key(key(crossterm::event::KeyCode::Char(c)))
            .unwrap();
    }
    assert_eq!(app.search_query, "fastfetch");
    assert_eq!(app.filtered_tools().len(), 1);
    assert_eq!(app.filtered_tools()[0].id, "fastfetch");

    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    assert!(!app.search_active);
    assert!(app.search_query.is_empty());
    assert!(app.running, "Esc aramadan çıkmalı, uygulamayı kapatmamalı");
}

#[test]
fn esc_outside_search_quits() {
    let mut app = app_with_sample();
    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    assert!(!app.running);
}

#[test]
fn backspace_shortens_search_query() {
    let mut app = app_with_sample();
    app.handle_key(key(crossterm::event::KeyCode::Char('/')))
        .unwrap();
    for c in "btopx".chars() {
        app.handle_key(key(crossterm::event::KeyCode::Char(c)))
            .unwrap();
    }
    assert!(app.filtered_tools().is_empty());
    app.handle_key(key(crossterm::event::KeyCode::Backspace))
        .unwrap();
    assert_eq!(app.search_query, "btop");
    assert_eq!(app.filtered_tools()[0].id, "btop");
}

#[test]
fn favorite_toggle_changes_state_and_filter() {
    let mut app = app_with_sample();
    go_to_category(&mut app, "Docker ve Sunucu");
    let id = app.selected_tool().unwrap().id.clone();
    assert!(!app.catalog.get_by_id(&id).unwrap().favorite);

    app.handle_key(key(crossterm::event::KeyCode::Char('F')))
        .unwrap();
    assert!(app.catalog.get_by_id(&id).unwrap().favorite);
    assert!(app.status_message.contains("favorilere eklendi"));

    // Favori filtresi açıkken araç listede görünür.
    app.handle_key(key(crossterm::event::KeyCode::Char('c')))
        .unwrap();
    assert!(app.favorite_only);
    assert!(app.filtered_tools().iter().any(|e| e.id == id));

    // Tekrar F ile çıkarılınca favori filtresinden düşer.
    app.handle_key(key(crossterm::event::KeyCode::Char('f')))
        .unwrap();
    assert!(!app.catalog.get_by_id(&id).unwrap().favorite);
    assert!(!app.filtered_tools().iter().any(|e| e.id == id));
    assert!(app.status_message.contains("favorilerden çıkarıldı"));
}

#[test]
fn favorite_filter_and_search_combine() {
    let mut app = app_with_sample();
    // Gömülü katalogda hiç favori yok; burada favori yapılan araç aranır.
    // Favori filtresi açılmadan önce seçim yapılır, yoksa liste boşalır.
    go_to_category(&mut app, "Docker ve Sunucu");
    let id = app.selected_tool().unwrap().id.clone();
    app.handle_key(key(crossterm::event::KeyCode::Char('f')))
        .unwrap();
    app.favorite_only = true;

    app.search_query = id.clone();
    let tools = app.filtered_tools();
    assert!(tools.iter().all(|e| e.favorite));
    assert!(tools.iter().any(|e| e.id == id));

    app.search_query = "curl".into(); // curl favori değil
    assert!(app.filtered_tools().iter().all(|e| e.favorite));
    assert!(app.filtered_tools().is_empty());
}

#[test]
fn favorites_survive_category_switch_and_are_in_memory() {
    let mut app = app_with_sample();
    // İlk ağ aracı favori yapılır (sabit bir kimliğe bağlı kalınmaz).
    go_to_category(&mut app, "Ağ ve Bağlantı");
    let curl_id = app.selected_tool().unwrap().id.clone();
    app.handle_key(key(crossterm::event::KeyCode::Char('f')))
        .unwrap();
    assert!(app.catalog.get_by_id(&curl_id).unwrap().favorite);

    go_to_category(&mut app, "AI ve Kodlama");
    assert!(app.catalog.get_by_id(&curl_id).unwrap().favorite);
}

#[test]
fn enter_on_installed_tool_requests_tab_without_opening_konsole() {
    // Gerçek Konsole süreci başlatılmaz: başlatıcı sahte bir fonksiyonla
    // değiştirilir, ama `handle_key` -> `select_tool` yolu aynen sınanır.
    fn requested(
        entry: &nova::catalog::CatalogEntry,
        _home: &std::path::Path,
        _opts: &nova::launcher::LaunchOptions,
    ) -> Result<nova::launcher::LaunchOutcome, nova::error::NovaError> {
        Ok(nova::launcher::LaunchOutcome {
            tool: entry.name.clone(),
            action: nova::launcher::LaunchAction::Program,
            terminal: "Konsole",
            working_directory: std::path::PathBuf::from("/home/kullanici"),
            emulator_pid: 1,
        })
    }

    let mut app = app_with_sample();
    app.set_launcher(requested);
    let entry = CatalogEntryInstalledProbe::find(&app);
    let Some((category, _id)) = entry else {
        eprintln!("kurulu araç bulunamadı, test atlanıyor");
        return;
    };
    go_to_category(&mut app, &category);
    let idx = app
        .filtered_tools()
        .iter()
        .position(|e| e.status == InstalledStatus::Installed)
        .expect("kurulu araç seçici listede olmalı");
    app.tool_index = idx;
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert!(app.running, "Enter Nova'yı kapatmamalı");
    assert!(
        app.status_message
            .contains("yeni sekmede başlatılması istendi"),
        "beklenmeyen mesaj: {}",
        app.status_message
    );
    assert!(
        !app.status_message.contains("başarıyla açıldı"),
        "açıldığı iddia edilmemeli: {}",
        app.status_message
    );
}

#[test]
fn enter_on_missing_tool_is_refused() {
    let entry = nova::catalog::CatalogEntry::new(
        "yok",
        "Yok",
        "Kurulu olmayan",
        "Özel Araçlar",
        "nova-faz1-kurulu-degil-xyz",
        vec![],
        LaunchMode::Terminal,
        WorkingDirectory::Current,
        false,
    );
    assert_eq!(entry.status, InstalledStatus::Missing);

    let catalog = Catalog::from_entries(vec![entry]).expect("katalog oluşturulamadı");
    let mut app = App::new(catalog);
    // Kurulu olmayan araç ana listede gizlenir; launcher hiç çağrılmaz.
    app.set_launcher(|_, _, _| panic!("kurulu olmayan araç için başlatma denenmemeli"));
    go_to_category(&mut app, "Özel Araçlar");
    assert!(
        app.filtered_tools().is_empty(),
        "kurulu olmayan araç ana listede görünmemeli"
    );
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert!(
        app.status_message.contains("Başlatılacak araç yok")
            || app.status_message.contains("kurulu değil"),
        "beklenmeyen mesaj: {}",
        app.status_message
    );
}

#[test]
fn question_mark_toggles_help_and_esc_closes_help_first() {
    let mut app = app_with_sample();
    app.handle_key(key(crossterm::event::KeyCode::Char('?')))
        .unwrap();
    assert!(app.show_help);
    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    assert!(!app.show_help);
    assert!(app.running, "yardım kapanmalı, uygulama kapanmamalı");
}

#[test]
fn help_esc_then_esc_quits() {
    let mut app = app_with_sample();
    app.handle_key(key(crossterm::event::KeyCode::Char('?')))
        .unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    assert!(!app.running);
}

/// Kurulu bir araç ve kategorisini bulmak için küçük yardımcı.
struct CatalogEntryInstalledProbe;

impl CatalogEntryInstalledProbe {
    fn find(app: &App) -> Option<(String, String)> {
        app.catalog
            .entries()
            .iter()
            .find(|e| e.status == InstalledStatus::Installed)
            .map(|e| (e.category.clone(), e.id.clone()))
    }
}

/// Yardım ekranının metnini verilen terminal boyutunda çizip düz metin döndürür.
fn render_help_text(width: u16, height: u16) -> String {
    use ratatui::{backend::TestBackend, Terminal};

    let mut app = app_with_sample();
    app.handle_key(key(crossterm::event::KeyCode::Char('?')))
        .unwrap();
    assert!(app.show_help);

    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| nova::ui::draw(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..height {
        for x in 0..width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

#[test]
fn help_screen_shows_full_key_table_and_paths_when_open() {
    // Faz 4'te tuş tablosu bir satır uzadı; sabit yükseklik onu kırpıyordu.
    let text = render_help_text(120, 40);
    for expected in [
        "F Favori",
        "N Yeni özel araç",
        "D Aracı sil",
        "R Son kullanılanlar",
        "P Proje dizinleri",
        "S Ayarlar",
        "Q Çıkış",
        "config: ",
        "state: ",
        "çalışma dizini: ",
    ] {
        assert!(
            text.contains(expected),
            "yardım ekranında eksik: {expected:?}\n{text}"
        );
    }
}

#[test]
fn help_screen_renders_without_panic_on_small_terminals() {
    // Yükseklik hesabı küçük ekranlarda taşmamalı ya da panik üretmemeli.
    for (w, h) in [(40, 8), (40, 10), (40, 12), (40, 14), (60, 20), (30, 12)] {
        let text = render_help_text(w, h);
        assert!(text.contains("Nova"), "{w}x{h} ekranı boş çizildi");
    }
}

#[test]
fn ai_tool_actions_sit_in_options_panel_with_side_detail() {
    // Geniş Kurulu: sekme|liste|Detay yan yana; Aksiyonlar altta dikey
    // seçenek listesi (Ayarlar / özel araç ekle düzeni: ▶ [etiket]).
    use ratatui::{backend::TestBackend, Terminal};

    let mut app = app_with_sample();
    go_to_category(&mut app, "AI ve Kodlama");
    let idx = app
        .filtered_tools()
        .iter()
        .position(|e| e.id == "codex")
        .expect("codex AI listesinde olmalı");
    app.tool_index = idx;
    let action_count = app.selected_tool().map(|e| e.actions.len()).unwrap_or(0);
    assert!(action_count >= 2, "codex en az iki aksiyon taşımalı");

    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|f| nova::ui::draw(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let mut lines = Vec::new();
    for y in 0..36u16 {
        let mut row = String::new();
        for x in 0..120u16 {
            row.push_str(buffer[(x, y)].symbol());
        }
        lines.push(row);
    }
    let text = lines.join("\n");
    assert!(text.contains("Detay"), "detay paneli yok:\n{text}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Aksiyonlar") && l.contains('─')),
        "aksiyonlar dikey panel başlığı olmalı:\n{text}"
    );
    let actions_y = lines
        .iter()
        .position(|l| l.contains("Aksiyonlar"))
        .expect("Aksiyonlar satırı");
    let detail_y = lines
        .iter()
        .position(|l| l.contains("Detay"))
        .expect("Detay satırı");
    assert!(
        detail_y < actions_y,
        "yan Detay içerik satırında, Aksiyonlar altında olmalı (detail={detail_y} actions={actions_y}):\n{text}"
    );
    let panel_body: String = lines
        .iter()
        .skip(actions_y)
        .take(nova::ui::actions_panel_rows(action_count) as usize)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        panel_body.contains("[Aç]")
            || panel_body.contains("[Devam")
            || panel_body.contains("[Yardım]"),
        "aksiyon etiketleri Ayarlar stili [köşeli] satırlar olmalı:\n{panel_body}"
    );
    let areas = nova::ui::click_areas_ex(nova::ui::ClickAreasInput {
        size: (120, 36),
        show_help: false,
        store_tab: false,
        commands_tab: false,
        tool_detail_side: true,
        actions_count: action_count,
        panes: nova::ui::PaneWidths::default(),
        secondary_cols: 1,
    });
    assert!(areas.tool_detail_side);
    assert_eq!(
        areas.actions_height,
        nova::ui::actions_panel_rows(action_count)
    );
    assert!(areas.tools_end_x < 120);
    assert_eq!(nova::ui::TOOL_DETAIL_PANEL_WIDTH, 72);
    assert_eq!(
        nova::ui::STORE_DETAIL_PANEL_WIDTH,
        nova::ui::TOOL_DETAIL_PANEL_WIDTH
    );
}

#[test]
fn ai_tool_actions_are_visible_without_tab_and_focus_moves_with_arrows() {
    // Aksiyonlar AI aracı seçilince dikey seçenek panelinde görünür;
    // Right odak taşır, ↑↓ seçer, Left araç listesine döner.
    use std::sync::Mutex;
    static LAST: Mutex<Option<(String, Vec<String>, nova::config::LaunchKind)>> = Mutex::new(None);

    fn capture(
        entry: &nova::catalog::CatalogEntry,
        _home: &std::path::Path,
        _opts: &nova::launcher::LaunchOptions,
    ) -> Result<nova::launcher::LaunchOutcome, nova::error::NovaError> {
        *LAST.lock().unwrap() = Some((entry.id.clone(), entry.args.clone(), entry.launch_kind));
        Ok(nova::launcher::LaunchOutcome {
            tool: entry.name.clone(),
            action: nova::launcher::LaunchAction::Program,
            terminal: "Konsole (taklit)",
            working_directory: std::path::PathBuf::from("/tmp"),
            emulator_pid: 7,
        })
    }

    let mut app = app_with_sample();
    app.set_launcher(capture);
    go_to_category(&mut app, "AI ve Kodlama");
    // Codex'e git (aksiyon tohumu var).
    let idx = app
        .filtered_tools()
        .iter()
        .position(|e| e.id == "codex")
        .expect("codex AI listesinde olmalı");
    app.tool_index = idx;
    assert!(
        app.selected_tool_has_ai_actions(),
        "seçili AI aracı aksiyon taşımalı"
    );
    assert_eq!(app.focus, PanelFocus::Tool);

    // Enter listede → birincil açma (interactive, args yok).
    *LAST.lock().unwrap() = None;
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    let primary = LAST.lock().unwrap().clone().expect("birincil açma");
    assert_eq!(primary.0, "codex");
    assert!(primary.1.is_empty());
    assert_eq!(primary.2, nova::config::LaunchKind::Interactive);

    // Right → aksiyon paneli; ↓ resume'a, Enter → custom_command + resume.
    app.handle_key(key(crossterm::event::KeyCode::Right))
        .unwrap();
    assert_eq!(app.focus, PanelFocus::ToolActions);
    assert_eq!(app.tool_action_index, 0);
    app.handle_key(key(crossterm::event::KeyCode::Down))
        .unwrap();
    assert_eq!(app.tool_action_index, 1); // resume
    *LAST.lock().unwrap() = None;
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    let resumed = LAST.lock().unwrap().clone().expect("resume aksiyonu");
    assert_eq!(resumed.0, "codex");
    assert_eq!(resumed.1, vec!["resume".to_string()]);
    assert_eq!(resumed.2, nova::config::LaunchKind::CustomCommand);

    // Left her zaman araç listesine döner (dikey listede ←→ seçmez).
    app.handle_key(key(crossterm::event::KeyCode::Left))
        .unwrap();
    assert_eq!(app.focus, PanelFocus::Tool);
}

#[test]
fn non_ai_tool_has_no_actions_focus_in_chain() {
    let mut app = app_with_sample();
    go_to_category(&mut app, "Sistem ve Kaynak Kullanımı");
    assert!(!app.selected_tool_has_ai_actions());
    assert_eq!(app.focus, PanelFocus::Tool);
    app.handle_key(key(crossterm::event::KeyCode::Right))
        .unwrap();
    // Aksiyon paneli zincirde yok; sağda kalır (Tool).
    assert_eq!(app.focus, PanelFocus::Tool);
}
