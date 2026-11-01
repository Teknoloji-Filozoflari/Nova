//! Faz 4 — kişiselleştirme kabul testleri.
//!
//! Her test geçici bir dizinde çalışır; gerçek `~/.config/nova` ve
//! `~/.local/state/nova` dosyalarına dokunulmaz. Gerçek Konsole süreci
//! başlatılmaz: başlatıcılar test yerine geçen (fake) işlevlerdir.

use nova::app::{form, modal, App};
use nova::catalog::loader;
use nova::config::state::{load_state, save_state, AttemptStatus, StateFile};
use nova::config::{LaunchKind, ToolEntry};
use nova::detection::InstalledStatus;
use std::path::{Path, PathBuf};

fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::empty())
}

fn type_str(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.handle_key(key(crossterm::event::KeyCode::Char(ch)))
            .unwrap();
    }
}

fn render_text(app: &App, width: u16, height: u16) -> String {
    use ratatui::{backend::TestBackend, Terminal};
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| nova::ui::draw(frame, app)).unwrap();
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

/// Geçici bir dizin (test sonunda silinir).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("nova-test-{tag}-{nanos}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("geçici dizin oluşturulamadı");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Katalog + kalıcı durum dosyasıyla uygulama.
///
/// `state_path` verilirse durum dosyaya yazılır; verilmezse yalnızca bellekte
/// tutulur.
fn app_with_state(state_path: Option<PathBuf>, seed: bool) -> App {
    let entries = loader::builtin_catalog().expect("gömülü katalog okunmalı");
    let catalog = nova::catalog::Catalog::from_entries(entries).expect("katalog oluşturulamadı");
    let file = state_path
        .as_deref()
        .map(|p| load_state(p).state)
        .unwrap_or_default();
    let state = nova::app::state::AppState {
        path: state_path,
        file,
        active_project: None,
        restore_warnings: Vec::new(),
    };
    App::with_state(catalog, state, seed)
}

/// Başarılı (istek gönderildi) bir başlatma sonucu üretir.
///
/// `LaunchFn` normal bir işlev işaretçisidir (yakalama yapamaz); bu yüzden
/// testler ayrı sabit işlevler kullanır.
fn fake_ok(
    entry: &nova::catalog::CatalogEntry,
    _home: &Path,
    _opts: &nova::launcher::LaunchOptions,
) -> Result<nova::launcher::LaunchOutcome, nova::error::NovaError> {
    Ok(nova::launcher::LaunchOutcome {
        tool: entry.name.clone(),
        action: nova::launcher::LaunchAction::Program,
        terminal: "Konsole (taklit)",
        working_directory: PathBuf::from("/tmp"),
        emulator_pid: 4242,
    })
}

/// Başarısız bir başlatma: emülatör bulunamadı.
fn fake_missing_terminal(
    _entry: &nova::catalog::CatalogEntry,
    _home: &Path,
    _opts: &nova::launcher::LaunchOptions,
) -> Result<nova::launcher::LaunchOutcome, nova::error::NovaError> {
    Err(nova::error::NovaError::Launcher(
        "terminal bulunamadı".into(),
    ))
}

/// Belirli bir araca odaklanır.
fn focus_tool(app: &mut App, id: &str) {
    app.search_query.clear();
    app.favorite_only = false;
    app.focus = nova::app::PanelFocus::Tool;
    // Ana liste yalnızca kurulu araçları gösterir; testlerde hedefi kurulu yap.
    if let Some(entry) = app.catalog.entries_mut().iter_mut().find(|e| e.id == id) {
        entry.status = InstalledStatus::Installed;
    }
    // Aranan araç hangi kategorideyse o kategoriyi seç: `filtered_tools`
    // yalnızca seçili kategorinin araçlarını döner.
    if let Some(cat) = app
        .catalog
        .entries()
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.category.clone())
    {
        if let Some(pos) = nova::catalog::CATEGORIES.iter().position(|c| *c == cat) {
            app.category_index = pos;
        }
    }
    let tools = app.filtered_tools();
    let pos = tools
        .iter()
        .position(|e| e.id == id)
        .unwrap_or_else(|| panic!("araç bulunamadı: {id}"));
    app.tool_index = pos;
    // `select_tool` seçili girdiyi arama sonucundan okur; kategori de aynı
    // listede olduğundan indeks doğrudan kullanılır.
}

// ---------------------------------------------------------------------------
// Favoriler kalıcılığı
// ---------------------------------------------------------------------------

#[test]
fn favorites_survive_restart_and_catalog_updates() {
    let tmp = TempDir::new("fav");
    let path = tmp.path().join("state.toml");

    let mut first = app_with_state(Some(path.clone()), true);
    focus_tool(&mut first, "curl");
    first
        .handle_key(key(crossterm::event::KeyCode::Char('f')))
        .unwrap();
    assert!(
        first.state.file.favorites.contains("curl"),
        "favori durum dosyasına yazılmalı"
    );
    assert!(path.exists(), "ilk favori state dosyasını oluşturmalı");

    // Yeniden başlat: dosya okunur, gömülü tohumlar uygulanmaz.
    let second = app_with_state(Some(path.clone()), false);
    assert!(second.state.file.favorites.contains("curl"));
    let entry = second
        .catalog
        .entries()
        .iter()
        .find(|e| e.id == "curl")
        .expect("curl katalogda");
    assert!(entry.favorite, "katalog girdisi favori işaretli olmalı");

    // Katalog güncellense de (bilinmeyen bir kimlik) favori korunur.
    let mut file: StateFile = load_state(&path).state;
    file.favorites.insert("gelecekte-eklenecek-arac".into());
    save_state(&path, &file).unwrap();
    let third = app_with_state(Some(path.clone()), false);
    assert!(third.state.file.favorites.contains("curl"));
    assert!(third
        .state
        .file
        .favorites
        .contains("gelecekte-eklenecek-arac"));
}

#[test]
fn builtin_favorites_seed_only_when_state_file_is_absent() {
    let tmp = TempDir::new("seed");
    let path = tmp.path().join("state.toml");
    // Gömülü katalogda `favorite = true` yok: tohumlama hiçbir şey uydurmaz.
    let seeded = app_with_state(Some(path.clone()), true);
    let seeded_count = seeded
        .catalog
        .entries()
        .iter()
        .filter(|e| e.favorite)
        .count();
    assert_eq!(seeded_count, 0, "uydurma favori tohumlanmamalı");

    // Elle eklenmiş bir tohum yalnızca ilk çalıştırmada uygulanır.
    let mut file = StateFile::default();
    file.favorites.insert("curl".into());
    save_state(&path, &file).unwrap();
    let from_file = app_with_state(Some(path.clone()), false);
    assert!(from_file.state.file.favorites.contains("curl"));

    // Boş ama var olan bir durum dosyası "ilk çalıştırma" sayılmaz.
    save_state(&path, &StateFile::default()).unwrap();
    let mut later = app_with_state(Some(path.clone()), false);
    assert_eq!(
        later.state.file.favorites.len(),
        0,
        "kullanıcı tüm favorileri kaldırdıysa yeniden tohumlanmamalı"
    );
    later.persist_state().unwrap();
    let on_disk = load_state(&path).state;
    assert!(on_disk.favorites.is_empty());
}

// ---------------------------------------------------------------------------
// Geçmiş
// ---------------------------------------------------------------------------

#[test]
fn recents_record_attempts_and_distinguish_rejected_from_sent_requests() {
    // Terminal isteği reddedilse bile kullanıcının denemesi geçmişe yazılır.
    let tmp = TempDir::new("recent-attempt");
    let state_path = tmp.path().join("state.toml");
    let mut failed = app_with_state(Some(state_path.clone()), false);
    failed.set_launcher(fake_missing_terminal);
    focus_tool(&mut failed, "curl");
    failed
        .handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert_eq!(failed.state.file.recents.len(), 1);
    assert_eq!(failed.state.file.recents[0].status, AttemptStatus::Rejected);
    assert!(failed.status_message.contains("terminal bulunamadı"));
    assert_eq!(
        load_state(&state_path).state.recents[0].status,
        AttemptStatus::Rejected,
        "reddedilen deneme kalıcı geçmişe yazılmalı"
    );

    // Kurulu olmayan araç ana listeden düşer (Mağaza’da kalır).
    let mut missing = app_with_state(None, false);
    focus_tool(&mut missing, "curl");
    let entry = missing
        .catalog
        .entries_mut()
        .iter_mut()
        .find(|entry| entry.id == "curl")
        .unwrap();
    entry.status = InstalledStatus::Missing;
    assert!(
        !missing.filtered_tools().iter().any(|e| e.id == "curl"),
        "kurulu olmayan curl ana listede görünmemeli"
    );

    // Launcher isteği kabul edince bu durum kaydedilir; harici uygulama
    // çalışıyor iddiası değildir.
    let mut ok = app_with_state(None, false);
    ok.set_launcher(fake_ok);
    focus_tool(&mut ok, "curl");
    ok.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert_eq!(ok.state.file.recents.len(), 1);
    assert_eq!(ok.state.file.recents[0].id, "curl");
    assert_eq!(ok.state.file.recents[0].status, AttemptStatus::RequestSent);
    assert!(ok.status_message.contains("istendi"));
}

#[test]
fn recents_are_newest_first_deduped_and_limited() {
    let mut st = nova::app::state::AppState::in_memory();
    st.set_recent_limit(3);
    st.record_attempt("a", "A", 10, AttemptStatus::RequestSent);
    st.record_attempt("b", "B", 20, AttemptStatus::Rejected);
    st.record_attempt("c", "C", 30, AttemptStatus::RequestSent);
    st.record_attempt("a", "A", 40, AttemptStatus::RequestSent);
    let ids: Vec<&str> = st.file.recents.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "c", "b"], "en yeni önce, tekrarsız, sınırlı");

    st.record_attempt("d", "D", 50, AttemptStatus::Rejected);
    let ids: Vec<&str> = st.file.recents.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["d", "a", "c"]);

    assert_eq!(st.clear_recents(), 3);
    assert!(st.file.recents.is_empty());
}

#[test]
fn recent_limit_default_and_hard_cap() {
    let mut st = nova::app::state::AppState::in_memory();
    assert_eq!(st.recent_limit(), nova::config::state::RECENT_LIMIT_DEFAULT);
    st.set_recent_limit(10_000);
    assert_eq!(st.recent_limit(), nova::config::state::RECENT_LIMIT_MAX);
    st.set_recent_limit(0);
    assert_eq!(st.recent_limit(), nova::config::state::RECENT_LIMIT_DEFAULT);
}

#[test]
fn history_modal_clear_requires_confirmation() {
    let mut app = app_with_state(None, false);
    app.state.record_attempt(
        "curl",
        "curl",
        nova::app::now_secs(),
        AttemptStatus::RequestSent,
    );
    app.handle_key(key(crossterm::event::KeyCode::Char('r')))
        .unwrap();
    assert_eq!(app.modal, modal::Modal::History);

    // X onay ister; Esc geçmişi silmez.
    app.handle_key(key(crossterm::event::KeyCode::Char('x')))
        .unwrap();
    assert!(matches!(
        app.modal,
        modal::Modal::Confirm {
            action: modal::ConfirmAction::ClearRecents,
            ..
        }
    ));
    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    assert_eq!(app.state.file.recents.len(), 1, "Esc silmemeli");

    app.handle_key(key(crossterm::event::KeyCode::Char('r')))
        .unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Char('x')))
        .unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert!(app.state.file.recents.is_empty(), "onay silmeli");
}

// ---------------------------------------------------------------------------
// Özel araç ekleme + doğrulama + silme
// ---------------------------------------------------------------------------

#[test]
fn custom_tool_form_validates_reviews_and_saves() {
    let tmp = TempDir::new("custom");
    let path = tmp.path().join("state.toml");
    let mut app = app_with_state(Some(path.clone()), false);

    app.handle_key(key(crossterm::event::KeyCode::Char('n')))
        .unwrap();
    // Boş form Enter ile reddedilir ve modal formda kalır.
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert!(matches!(app.modal, modal::Modal::ToolForm(_)));
    assert!(
        app.status_message.starts_with("Hata:"),
        "eksik alan açık hata olmalı"
    );

    // Ad
    type_str(&mut app, "Deneme Aracı");
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap();
    // Açıklama
    type_str(&mut app, "test aracı");
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap();
    // Kategori (Özel Araçlar varsayılan)
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap();
    // Program
    type_str(&mut app, "duf");
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap();
    // Argümanlar
    type_str(&mut app, "--only local");
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();

    // Kaydetmeden önce gözden geçirme adımı.
    match &app.modal {
        modal::Modal::ToolReview { valid, .. } => {
            assert_eq!(valid.executable, "duf");
            assert_eq!(valid.args, vec!["--only", "local"]);
            assert_eq!(valid.category, "Özel Araçlar");
        }
        other => panic!("gözden geçirme bekleniyordu, bulunan: {other:?}"),
    }
    let review_text = render_text(&app, 100, 30);
    assert!(review_text.contains("kullanıcı yetkilerinle çalışır"));
    assert!(review_text.contains("özel komut"));

    // Esc geri döner, veriler korunur.
    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    assert!(matches!(app.modal, modal::Modal::ToolForm(_)));

    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert_eq!(app.modal, modal::Modal::None);
    assert!(
        app.status_message.starts_with('✓'),
        "başarı açık işaretlenmeli"
    );

    // Katalogda ve dosyada var; yeniden başlatınca da yüklenir.
    assert!(app.catalog.entries().iter().any(|e| e.id == "deneme-araci"));
    let reopened = app_with_state(Some(path.clone()), false);
    // Gömülü katalogda da 'duf' var; özel aracı kimliğiyle bul.
    let entry = reopened
        .catalog
        .entries()
        .iter()
        .find(|e| e.id == "deneme-araci")
        .expect("kaydedilen araç yeniden yüklenmeli");
    // Argümanlar ve çalışma dizini biçimi korunur (yeniden yüklemede
    // `interactive`'e düşmez).
    let stored = reopened
        .state
        .file
        .custom_tools
        .iter()
        .find(|t| t.id == "deneme-araci")
        .expect("araç durum dosyasında");
    assert_eq!(stored.launch_kind, LaunchKind::CustomCommand);
    assert_eq!(stored.args, vec!["--only", "local"]);
    // Çalışma dizini yazılmadıysa ev dizini kullanılır.
    assert_eq!(stored.working_directory, "home");

    assert_eq!(entry.args, vec!["--only", "local"]);
    assert!(!entry.favorite);

    // Silme onayı ister; Esc silmez, Enter siler ve kalıcıdır.
    let id = entry.id.clone();
    app.search_query.clear();
    focus_tool(&mut app, &id);
    app.handle_key(key(crossterm::event::KeyCode::Char('d')))
        .unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    assert!(app.catalog.entries().iter().any(|e| e.id == id));
    app.handle_key(key(crossterm::event::KeyCode::Char('d')))
        .unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert!(!app.catalog.entries().iter().any(|e| e.id == id));
    let after = app_with_state(Some(path), false);
    assert!(!after.catalog.entries().iter().any(|e| e.id == id));
}

#[test]
fn custom_tool_form_launch_type_can_be_selected_and_saved() {
    let tmp = TempDir::new("custom-kind");
    let mut app = app_with_state(Some(tmp.path().join("state.toml")), false);
    app.handle_key(key(crossterm::event::KeyCode::Char('n')))
        .unwrap();
    type_str(&mut app, "Etkileşimli Deneme");
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap();
    type_str(&mut app, "Argümansız TUI aracı");
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap(); // kategori
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap(); // executable
    type_str(&mut app, "duf");
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap(); // argümanlar
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap(); // dizin
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap(); // tür
    app.handle_key(key(crossterm::event::KeyCode::Left))
        .unwrap(); // interactive
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();

    match &app.modal {
        modal::Modal::ToolReview { valid, .. } => {
            assert_eq!(valid.launch_kind, LaunchKind::Interactive);
        }
        other => panic!("tür seçimi sonrası inceleme bekleniyordu: {other:?}"),
    }
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    let entry = app
        .catalog
        .entries()
        .iter()
        .find(|entry| entry.id == "etkilesimli-deneme")
        .unwrap();
    assert_eq!(entry.launch_kind, LaunchKind::Interactive);
}

#[test]
fn form_rejects_program_with_space_and_bad_working_dir() {
    let mut f = form::ToolForm {
        name: "X".into(),
        executable: "/usr/bin/duf".into(),
        ..form::ToolForm::default()
    };
    let err = f.validate().unwrap_err();
    assert!(err.iter().any(|e| e.contains("ad olmalı")), "{err:?}");
    // Hata alan bazında da işaretlenir: program alanı 4. sırada.
    assert!(f.errors[3].is_some());

    let mut f = form::ToolForm {
        name: "X".into(),
        executable: "duf".into(),
        working_dir: "  ".into(),
        ..form::ToolForm::default()
    };
    assert!(f.validate().is_ok(), "boş çalışma dizini ev dizini demek");
}

#[test]
fn builtin_tool_cannot_be_deleted() {
    let mut app = app_with_state(None, false);
    focus_tool(&mut app, "curl");
    app.handle_key(key(crossterm::event::KeyCode::Char('d')))
        .unwrap();
    assert_eq!(app.modal, modal::Modal::None, "onay istenmemeli");
    assert!(app.status_message.starts_with("Hata:"));
    assert!(app.catalog.entries().iter().any(|e| e.id == "curl"));
}

#[test]
fn config_tool_is_hidden_not_rewritten() {
    let tmp = TempDir::new("cfg");
    let path = tmp.path().join("state.toml");
    let mut app = app_with_state(Some(path.clone()), false);
    app.set_config_tool_ids(std::iter::once("curl".to_string()));
    // `curl` gömülü bir araç; config kimliği bildirilse bile gömülü araç
    // silinemez (gömülü kontrolü config kontrolünden önce gelir).
    focus_tool(&mut app, "curl");
    app.handle_key(key(crossterm::event::KeyCode::Char('d')))
        .unwrap();
    assert!(app.status_message.starts_with("Hata:"));
}

// ---------------------------------------------------------------------------
// Proje dizinleri
// ---------------------------------------------------------------------------

#[test]
fn project_dir_selection_reaches_launch_options() {
    let tmp = TempDir::new("proj");
    let path = tmp.path().join("state.toml");
    let project = tmp.path().join("proje");
    std::fs::create_dir_all(&project).unwrap();

    let mut app = app_with_state(Some(path.clone()), false);
    app.state
        .add_project_dir(project.to_str().unwrap())
        .unwrap();
    // Proje listesi kalıcıdır (yeniden başlatmada da durur).
    app.persist_state().unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Char('p')))
        .unwrap();
    assert!(matches!(app.modal, modal::Modal::ProjectPicker { .. }));
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    let opts = app.launch_options();
    assert_eq!(opts.project_dir.as_deref(), Some(project.as_path()));

    // Proje listesi kalıcıdır; etkin seçim oturumluktur.
    let reopened = app_with_state(Some(path), false);
    assert_eq!(reopened.state.project_dirs().len(), 1);
    assert!(reopened.state.active_project.is_none());
}

#[test]
fn settings_save_default_workdir_and_supported_terminal() {
    let tmp = TempDir::new("settings");
    let state_path = tmp.path().join("state.toml");
    let project = tmp.path().join("varsayilan-proje");
    std::fs::create_dir(&project).unwrap();
    let mut app = app_with_state(Some(state_path.clone()), false);

    app.handle_key(key(crossterm::event::KeyCode::Char('s')))
        .unwrap();
    assert!(app.settings_tab, "S Ayarlar sekmesini açmalı");
    app.open_advanced_settings_modal();
    assert!(matches!(app.modal, modal::Modal::Settings { .. }));
    let settings_text = render_text(&app, 120, 30);
    assert!(settings_text.contains("Ayarlar"));
    assert!(settings_text.contains("Katalog:"));
    assert!(settings_text.contains("Kullanıcı ayarları:"));
    for (width, height) in [(40, 8), (40, 12), (60, 16), (120, 30)] {
        let _ = render_text(&app, width, height);
    }
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap();
    type_str(&mut app, project.to_str().unwrap());
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();

    assert_eq!(app.modal, modal::Modal::None);
    assert_eq!(
        app.state.file.prefs.default_working_dir.as_deref(),
        Some(project.to_str().unwrap())
    );
    assert_eq!(
        app.state.file.prefs.preferred_terminal.as_deref(),
        Some("konsole")
    );
    assert_eq!(
        app.launch_options().default_working_dir.as_deref(),
        Some(project.as_path())
    );

    let reopened = app_with_state(Some(state_path), false);
    assert_eq!(
        reopened.state.file.prefs.default_working_dir.as_deref(),
        Some(project.to_str().unwrap())
    );
}

#[test]
fn settings_refuse_missing_workdir_and_unsupported_terminal_without_changes() {
    let mut app = app_with_state(None, false);
    app.open_advanced_settings_modal();
    app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap();
    let missing = std::env::temp_dir().join(format!(
        "nova-settings-missing-{}-{}",
        std::process::id(),
        nova::app::now_secs()
    ));
    type_str(&mut app, missing.to_str().unwrap());
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert!(matches!(app.modal, modal::Modal::Settings { .. }));
    assert!(app.status_message.contains("okunamadı"));
    assert!(!missing.exists(), "ayar kaydetme dizin oluşturmamalı");
    assert!(app.state.file.prefs.default_working_dir.is_none());

    app.handle_key(key(crossterm::event::KeyCode::Up)).unwrap();
    for _ in 0.."konsole".chars().count() {
        app.handle_key(key(crossterm::event::KeyCode::Backspace))
            .unwrap();
    }
    type_str(&mut app, "kitty");
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert!(app.status_message.contains("yalnızca Konsole"));
    assert!(app.state.file.prefs.preferred_terminal.is_none());
}

#[test]
fn missing_project_dir_asks_before_creating_anything() {
    let tmp = TempDir::new("projmiss");
    let missing = tmp.path().join("yok").join("proje");
    let mut st = nova::app::state::AppState::in_memory();
    st.add_project_dir(missing.to_str().unwrap()).unwrap();
    st.set_active_project(Some(missing.clone()));

    let mut entry = nova::catalog::catalog_entry_from_tool(&ToolEntry {
        id: "proj-tool".into(),
        name: "Projeli".into(),
        category: "Özel Araçlar".into(),
        executable: "duf".into(),
        working_directory: "project".into(),
        // Proje seçilmezse kullanılacak varsayılan dizi.
        working_dir_path: Some(missing.to_string_lossy().to_string()),
        ..ToolEntry::default()
    })
    .unwrap();
    entry.status = InstalledStatus::Installed;
    let mut app = App::with_state(
        nova::catalog::Catalog::from_entries(vec![entry]).unwrap(),
        st,
        false,
    );
    // Gerçek başlatıcı gibi davran: eksik proje dizini hata döner.
    app.set_launcher(|entry, _home, options| {
        if let nova::catalog::WorkingDirectory::Project(_) = entry.working_directory {
            if let Some(dir) = &options.project_dir {
                if !dir.exists() {
                    return Err(nova::error::NovaError::Launcher(format!(
                        "Çalışma dizini bulunamadı: {}",
                        dir.display()
                    )));
                }
            }
        }
        Ok(nova::launcher::LaunchOutcome {
            tool: entry.name.clone(),
            action: nova::launcher::LaunchAction::Program,
            terminal: "Konsole (taklit)",
            working_directory: PathBuf::from("/tmp"),
            emulator_pid: 4242,
        })
    });
    // Tek araç "Özel Araçlar" kategorisinde.
    app.category_index = nova::catalog::CATEGORIES
        .iter()
        .position(|c| *c == "Özel Araçlar")
        .unwrap();
    app.focus = nova::app::PanelFocus::Tool;
    app.handle_key(key(crossterm::event::KeyCode::Enter))
        .unwrap();
    assert!(!missing.exists(), "onay olmadan dizin oluşturulmamalı");
    match &app.modal {
        modal::Modal::Confirm {
            action: modal::ConfirmAction::CreateDir { path, .. },
            ..
        } => assert_eq!(path, &missing),
        other => panic!("dizin onayı bekleniyordu: {other:?}"),
    }
    // Esc: hiçbir şey oluşturulmaz.
    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    assert!(!missing.exists());
}

// ---------------------------------------------------------------------------
// Bozuk durum dosyası
// ---------------------------------------------------------------------------

#[test]
fn broken_state_file_is_backed_up_not_lost() {
    let tmp = TempDir::new("bozuk");
    let path = tmp.path().join("state.toml");
    std::fs::write(&path, "favorites = [ bu bozuk toml").unwrap();

    let loaded = load_state(&path);
    assert!(loaded.warning.is_some(), "uyarı verilmeli");
    assert!(loaded.state.favorites.is_empty());

    // Bozuk içerik silinmez; yedek dosyada durur.
    let backups: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("state.toml.bozuk-")
        })
        .collect();
    assert_eq!(backups.len(), 1, "tek yedek alınmalı");
    let content = std::fs::read_to_string(backups[0].path()).unwrap();
    assert!(content.contains("bu bozuk toml"), "içerik korunmalı");

    // Yeni durum yazılabilir; yedek hâlâ yerinde.
    let mut file = loaded.state;
    file.favorites.insert("curl".into());
    save_state(&path, &file).unwrap();
    let reread = load_state(&path);
    assert!(reread.warning.is_none());
    assert!(reread.state.favorites.contains("curl"));
    assert!(backups[0].path().exists());
}

#[test]
fn atomic_save_leaves_no_temp_file() {
    let tmp = TempDir::new("atomic");
    let path = tmp.path().join("state.toml");
    let mut file = StateFile::default();
    file.favorites.insert("curl".into());
    save_state(&path, &file).unwrap();
    let names: Vec<String> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["state.toml".to_string()],
        "geçici dosya kalmamalı"
    );
}

// ---------------------------------------------------------------------------
// Klavye ve modallar
// ---------------------------------------------------------------------------

#[test]
fn modal_blocks_main_screen_shortcuts() {
    let mut app = app_with_state(None, false);
    app.handle_key(key(crossterm::event::KeyCode::Char('n')))
        .unwrap();
    assert!(matches!(app.modal, modal::Modal::ToolForm(_)));
    // `q`, `d`, `c` gibi ana ekran tuşları formda metin olur / hiçbir şey
    // yapmaz; uygulama kapanmaz.
    app.handle_key(key(crossterm::event::KeyCode::Char('q')))
        .unwrap();
    assert!(app.running, "modal açıkken Q uygulamayı kapatmamalı");
    // Form açık kalır; 'q' ad alanına yazılır (başka bir ekrana geçilmez).
    match &app.modal {
        modal::Modal::ToolForm(f) => assert_eq!(f.name, "q"),
        other => panic!("form bekleniyordu: {other:?}"),
    }
    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    assert_eq!(app.modal, modal::Modal::None);

    // Onay ekranında bilinmeyen tuş onaylamaz.
    app.handle_key(key(crossterm::event::KeyCode::Char('r')))
        .unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Char('x')))
        .unwrap();
    app.handle_key(key(crossterm::event::KeyCode::Char('y')))
        .unwrap();
    assert!(matches!(app.modal, modal::Modal::Confirm { .. }));
}

#[test]
fn help_lists_every_main_screen_shortcut() {
    let mut app = app_with_state(None, false);
    app.handle_key(key(crossterm::event::KeyCode::Char('?')))
        .unwrap();
    assert!(app.show_help);
    // Yardım açıkken ? kapatır, Esc kapatır.
    app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
    assert!(!app.show_help);
    assert!(app.running, "Esc yardımı kapatır, uygulamayı kapatmaz");
}

#[test]
fn status_message_colors_distinguish_success_error_and_info() {
    assert_eq!(
        nova::ui::message_color("✓ kaydedildi"),
        ratatui::style::Color::Green
    );
    assert_eq!(
        nova::ui::message_color("Hata: olmadı"),
        ratatui::style::Color::Red
    );
    assert_eq!(
        nova::ui::message_color("bilgi"),
        ratatui::style::Color::Yellow
    );
}

#[test]
fn relative_time_labels() {
    let now = 1_000_000;
    assert_eq!(nova::app::state::relative_time(now, now - 5), "az önce");
    assert_eq!(nova::app::state::relative_time(now, now - 120), "2 dk önce");
    assert_eq!(
        nova::app::state::relative_time(now, now - 7200),
        "2 sa önce"
    );
    assert_eq!(
        nova::app::state::relative_time(now, now - 2 * 86_400),
        "2 gün önce"
    );
}
