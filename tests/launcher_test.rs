//! Launcher entegrasyon testleri.
//!
//! Proje kuralı: testler gerçek Konsole sürecini **zorunlu kılmaz**. Bu
//! yüzden burada komut oluşturma mantığı ve hata yolları sınanır; gerçek
//! terminal açma yalnızca elle yapılan ortam doğrulamasına bırakılır.

use nova::app::App;
use nova::catalog::{CatalogEntry, LaunchMode, WorkingDirectory};
use nova::detection::InstalledStatus;
use nova::error::NovaError;
use nova::launcher::{
    launch_with, select_adapter, Konsole, LaunchOutcome, LaunchRequest, ProcessSpawner,
    TerminalAdapter,
};
use std::cell::RefCell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// `PATH` gibi süreç geneli ortam değişkenlerini değiştiren testler
/// birbirini etkilemesin diye sırayla çalıştırılır.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct FakeSpawner {
    calls: RefCell<Vec<(PathBuf, Vec<String>)>>,
}

impl FakeSpawner {
    fn new() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
        }
    }
}

impl ProcessSpawner for FakeSpawner {
    fn spawn(&self, program: &Path, args: &[OsString]) -> Result<u32, NovaError> {
        self.calls.borrow_mut().push((
            program.to_path_buf(),
            args.iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect(),
        ));
        Ok(1)
    }
}

fn home() -> PathBuf {
    let h = std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("HOME gerekli");
    assert!(h.is_dir());
    h
}

fn entry_with(
    executable: &str,
    args: &[&str],
    wd: WorkingDirectory,
    mode: LaunchMode,
) -> CatalogEntry {
    CatalogEntry::new(
        "test-arac",
        "Test Aracı",
        "test",
        "Özel Araçlar",
        executable,
        args.iter().map(|s| s.to_string()).collect(),
        mode,
        wd,
        false,
    )
}

fn builtin_catalog() -> nova::catalog::Catalog {
    let entries = nova::catalog::loader::builtin_catalog().expect("gömülü katalog okunmalı");
    nova::catalog::Catalog::from_entries(entries).expect("katalog oluşturulamadı")
}

fn installed_or_skip(entry: &CatalogEntry) -> bool {
    entry.status == InstalledStatus::Installed
}

// ---------------------------------------------------------------------------
// İstek oluşturma (saf, yan etkisiz)
// ---------------------------------------------------------------------------

#[test]
fn missing_executable_never_produces_a_request_and_names_the_command() {
    let entry = entry_with(
        "nova-yok-boyle-komut-xyz",
        &[],
        WorkingDirectory::Home,
        LaunchMode::Terminal,
    );
    assert_eq!(entry.status, InstalledStatus::Missing);
    let err = LaunchRequest::from_entry(&entry, &home()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("nova-yok-boyle-komut-xyz"), "{msg}");
    assert!(msg.contains("kurulu değil"), "{msg}");
}

#[test]
fn invalid_directory_is_rejected_before_any_launch() {
    let entry = entry_with(
        "sh",
        &[],
        WorkingDirectory::Custom(PathBuf::from("/nova/yok/boyle/dizin")),
        LaunchMode::Terminal,
    );
    if !installed_or_skip(&entry) {
        return;
    }
    let err = LaunchRequest::from_entry(&entry, &home()).unwrap_err();
    assert!(err.to_string().contains("/nova/yok/boyle/dizin"), "{err}");
}

#[test]
fn request_carries_program_args_and_directory_separately() {
    let entry = entry_with(
        "sh",
        &["-c", "echo \"a b\"", "$HOME;rm -rf /"],
        WorkingDirectory::Home,
        LaunchMode::Terminal,
    );
    if !installed_or_skip(&entry) {
        return;
    }
    let req = LaunchRequest::from_entry(&entry, &home()).unwrap();
    assert!(req.program.is_absolute());
    assert_eq!(req.args.len(), 3);
    assert_eq!(req.args[1], OsString::from("echo \"a b\""));
    assert_eq!(req.args[2], OsString::from("$HOME;rm -rf /"));
    assert_eq!(
        req.working_directory,
        std::fs::canonicalize(home()).unwrap()
    );
}

#[test]
fn path_with_spaces_survives_directory_resolution() {
    let base = std::env::temp_dir().join("nova test dizini özel & $x");
    std::fs::create_dir_all(&base).unwrap();
    let entry = entry_with(
        "sh",
        &[],
        WorkingDirectory::Custom(base.clone()),
        LaunchMode::Terminal,
    );
    let result = LaunchRequest::from_entry(&entry, &home());
    let canonical = std::fs::canonicalize(&base);
    let _ = std::fs::remove_dir_all(&base);
    if !installed_or_skip(&entry) {
        return;
    }
    let req = result.expect("boşluklu dizin geçerli olmalı");
    assert_eq!(req.working_directory, canonical.unwrap());
}

// ---------------------------------------------------------------------------
// Komut oluşturma (terminalden bağımsız)
// ---------------------------------------------------------------------------

#[test]
fn konsole_command_is_built_without_shell() {
    let req = LaunchRequest {
        program: PathBuf::from("/usr/bin/lazygit"),
        args: vec!["--path".into(), "/tmp/bir repo".into()],
        working_directory: PathBuf::from("/home/kullanici/git/depo"),
        stdin_data: None,
        stdin_file_path: None,
    };
    let args = Konsole.build_args(Path::new("/usr/bin/konsole"), &req);

    assert_eq!(
        args,
        vec![
            OsString::from("--new-tab"),
            OsString::from("--workdir"),
            OsString::from("/home/kullanici/git/depo"),
            OsString::from("-e"),
            OsString::from("/usr/bin/lazygit"),
            OsString::from("--path"),
            OsString::from("/tmp/bir repo"),
        ]
    );
    for forbidden in ["sh", "bash", "zsh", "-c"] {
        assert!(
            !args.iter().any(|a| a == forbidden),
            "kabuk yok: {forbidden}"
        );
    }
}

#[test]
fn adapter_is_extensible_via_trait() {
    // Gelecekteki emülatörler aynı trait'i uygular; launcher çekirdeği
    // değişmez.
    struct FakeKitty;
    impl TerminalAdapter for FakeKitty {
        fn name(&self) -> &'static str {
            "Kitty"
        }
        fn executable(&self) -> &'static str {
            "kitty"
        }
        fn build_args(&self, _e: &Path, r: &LaunchRequest) -> Vec<OsString> {
            let mut v = vec![
                OsString::from("--detach"),
                OsString::from("--directory"),
                r.working_directory.as_os_str().to_owned(),
            ];
            v.push(r.program.as_os_str().to_owned());
            v.extend(r.args.iter().cloned());
            v
        }
    }
    let req = LaunchRequest {
        program: PathBuf::from("/usr/bin/opencode"),
        args: vec![],
        working_directory: PathBuf::from("/home/kullanici/proje"),
        stdin_data: None,
        stdin_file_path: None,
    };
    let args: Vec<&dyn TerminalAdapter> = vec![&Konsole, &FakeKitty];
    assert_eq!(args[1].name(), "Kitty");
    let built = args[1].build_args(Path::new("/usr/bin/kitty"), &req);
    assert_eq!(built[0], OsString::from("--detach"));
    assert_ne!(
        built,
        args[0].build_args(Path::new("/usr/bin/konsole"), &req),
        "her emülatör kendi argümanlarını üretmeli"
    );
}

// ---------------------------------------------------------------------------
// Başlatma akışı ve TUI dayanıklılığı
// ---------------------------------------------------------------------------

/// `PATH`'i geçici olarak en kısa yola çevirir; dönüşünde eski `PATH`'i
/// geri yüklemek için guard döner.
///
/// Bu test **süreç ortamını** değiştirir; gerçek `konsole` bu makinede
/// kurulu olsa bile bulunamaz. Başka bir spawn denemesi yapılmaz.
struct PathOverride {
    previous: Option<OsString>,
}

impl PathOverride {
    fn empty() -> Self {
        let previous = std::env::var_os("PATH");
        // `which` yalnızca `PATH`'i tarar; boş bir değer hiçbir dosya
        // bulmaz. Boş dizin yolu ("" → geçerli dizin) tehlikeli olduğu
        // için tamamen boş bırakılmaz, var olmayan bir dizin verilir.
        std::env::set_var("PATH", "/nova/test-icin-yok-boyle-dizin");
        Self { previous }
    }
}

impl Drop for PathOverride {
    fn drop(&mut self) {
        match &self.previous {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
    }
}

#[test]
fn missing_konsole_gives_a_clear_error_and_no_process_is_spawned() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _path = PathOverride::empty();

    let err = select_adapter(None).err().expect("emülatör bulunmamalıydı");
    let msg = err.to_string();
    assert!(msg.contains("terminal emülatörü bulunamadı"), "{msg}");
    assert!(msg.contains("konsole"), "aranan emülatör adı yok: {msg}");

    // Tam başlatma akışı da spawna kadar gelmemeli.
    let entry = entry_with("sh", &[], WorkingDirectory::Home, LaunchMode::Terminal);
    if entry.status != InstalledStatus::Missing {
        let spawner = FakeSpawner::new();
        let err = launch_with(&entry, &home(), &spawner).unwrap_err();
        assert!(err.to_string().contains("emülatörü bulunamadı"), "{err}");
        assert!(
            spawner.calls.borrow().is_empty(),
            "emülatör yokken hiçbir süreç başlatılmamalı"
        );
    }
}

#[test]
fn preferred_konsole_that_is_not_installed_is_not_replaced_silently() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _path = PathOverride::empty();

    let err = select_adapter(Some("konsole"))
        .err()
        .expect("kurulu olmayan tercih reddedilmeliydi");
    let msg = err.to_string();
    assert!(msg.contains("bulunamadı"), "{msg}");
    assert!(msg.contains("tercih"), "tercih bağlamı yok: {msg}");
}

#[test]
fn unsupported_preferred_terminal_names_supported_ones() {
    let err = select_adapter(Some("kitty"))
        .err()
        .expect("desteklenmeyen tercih reddedilmeliydi");
    let msg = err.to_string();
    assert!(msg.contains("kitty"), "{msg}");
    assert!(msg.contains("desteklenmiyor"), "{msg}");
    assert!(
        msg.contains("konsole"),
        "desteklenenler listelenmeli: {msg}"
    );
}

#[test]
fn app_survives_launcher_errors_and_shows_message() {
    fn failing_launcher(
        _e: &CatalogEntry,
        _h: &Path,
        _o: &nova::launcher::LaunchOptions,
    ) -> Result<LaunchOutcome, NovaError> {
        Err(NovaError::Launcher(
            "Desteklenen bir terminal emülatörü bulunamadı (aranan: konsole)".into(),
        ))
    }

    let mut app = App::new(builtin_catalog());
    for entry in app.catalog.entries_mut() {
        entry.status = InstalledStatus::Installed;
    }
    app.set_launcher(failing_launcher);
    app.focus = nova::app::PanelFocus::Tool;
    app.handle_key(k('j')).unwrap();
    let before = app.tool_index;
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::empty(),
    ))
    .unwrap();

    assert!(app.running, "hata Nova'yı kapatmamalı");
    assert_eq!(app.tool_index, before);
    assert!(app.status_message.contains("terminal emülatörü bulunamadı"));
}

#[test]
fn app_reports_requested_not_opened() {
    fn ok_launcher(
        _e: &CatalogEntry,
        _h: &Path,
        _o: &nova::launcher::LaunchOptions,
    ) -> Result<LaunchOutcome, NovaError> {
        Ok(LaunchOutcome {
            tool: "Test".into(),
            action: nova::launcher::LaunchAction::Program,
            terminal: "Konsole",
            working_directory: PathBuf::from("/home/kullanici"),
            emulator_pid: 7,
        })
    }

    let mut app = App::new(builtin_catalog());
    for entry in app.catalog.entries_mut() {
        entry.status = InstalledStatus::Installed;
    }
    app.set_launcher(ok_launcher);
    app.focus = nova::app::PanelFocus::Tool;
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::empty(),
    ))
    .unwrap();

    assert!(app.running);
    assert!(
        app.status_message.contains("istendi"),
        "{}",
        app.status_message
    );
    assert!(
        app.status_message.contains("doğrulayamaz"),
        "açıldığı iddia edilmemeli: {}",
        app.status_message
    );
}

#[test]
fn launcher_reports_spawn_call_as_request_not_confirmation() {
    // `PATH`i boşaltan testlerle aynı kilit: emülatör araması da `PATH`e
    // bağlıdır, aksi hâlde bu test rastgele "konsole bulunamadı" alır.
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let entry = entry_with("sh", &[""], WorkingDirectory::Home, LaunchMode::Terminal);
    if !installed_or_skip(&entry) || nova::launcher::select_adapter(None).is_err() {
        return;
    }
    let spawner = FakeSpawner::new();
    let outcome = launch_with(&entry, &home(), &spawner).unwrap();
    assert_eq!(spawner.calls.borrow().len(), 1);
    let (program, args) = &spawner.calls.borrow()[0];
    assert!(program
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("konsole"));
    assert!(args.contains(&"--new-tab".to_string()));
    assert_eq!(outcome.emulator_pid, 1);
    assert!(outcome.message().contains("istendi"));
}

fn k(c: char) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(c),
        crossterm::event::KeyModifiers::empty(),
    )
}
