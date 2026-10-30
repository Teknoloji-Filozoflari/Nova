//! Uygulama başlatıcı modülü.
//!
//! Katmanlar bilinçli olarak ayrıdır:
//!
//! 1. [`request::LaunchRequest`] — *ne* başlatılacak, *hangi argümanlarla* ve
//!    *hangi dizinde*. Yan etkisizdir; süreç başlatmaz.
//! 2. [`terminal::TerminalAdapter`] — isteği emülatörün argümanlarına çevirir.
//!    Yalnızca komut satırı üretir; test edilebilir.
//! 3. [`launch`] — süreci **ayrık** biçimde başlatır ve ne olduğunu dürüstçe
//!    raporlar.
//!
//! # "İstek oluşturuldu" ile "uygulama açıldı" farkı
//!
//! Nova bir isteği Konsole'ye iletebilir ve Konsole süreci başarıyla
//! `spawn` edilebilir; bu, aracın kendisinin çalıştığını **kanıtlamaz**.
//! Konsole arka planda ayrı bir pencerede işini görür, aracın çıktısı Nova'dan
//! gelmez. Bu yüzden [`LaunchOutcome`] yalnızca "yeni sekme istendi" der;
//! "uygulama sorunsuz açıldı" iddiasında bulunmaz.

pub mod request;
pub mod terminal;

/// Private command-line switch used by the child process that displays help
/// text and removes its temporary file after the pager exits.
pub const INTERNAL_PAGER_ARGUMENT: &str = "--nova-internal-page";

pub use request::{resolve_directory, DirectoryValidator, FsValidator, LaunchRequest};
pub use terminal::{select_adapter, Konsole, TerminalAdapter, TERMINALS};

use crate::catalog::{CatalogEntry, LaunchMode};
use crate::error::NovaError;
use std::path::Path;
use std::process::{Command, Stdio};

/// Nova'nın yeni sekmede ne gösterdiği.
///
/// Kullanıcıya "başlatıldı" demeden önce *neyin* istenildiğini ayırt etmek
/// için gereklidir: araç hiç çalıştırılmamış olabilir (yalnızca metin
/// gösterilmiştir).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchAction {
    /// Aracın kendisi çalıştırıldı.
    Program,
    /// Araç çalıştırılmadı; sabit kullanım metni gösterildi.
    CommandHelp,
    /// Araç çalıştırılmadı; kabuk entegrasyonu bilgisi gösterildi.
    IntegrationInfo,
}

/// Başlatma isteğinin sonucu.
///
/// Bu tür yalnızca Nova'nın **yapabildiği** kadarını anlatır.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOutcome {
    /// Aracın ekrandaki adı.
    pub tool: String,
    /// Yeni sekmede ne istendi.
    pub action: LaunchAction,
    /// Kullanılan terminal emülatörünün adı (ör. "Konsole").
    pub terminal: &'static str,
    /// Aracın çalışacağı çalışma dizini (tam yol, doğrulanmış).
    pub working_directory: std::path::PathBuf,
    /// Emülatör sürecinin işletim sistemi PID'i — yalnızca emülatörün.
    pub emulator_pid: u32,
}

impl LaunchOutcome {
    /// Kullanıcıya gösterilecek dürüst mesaj.
    ///
    /// "Açıldı" değil, "yeni sekmede … istendi" denir; metin görünümlerinde
    /// aracın çalıştırılmadığı açıkça söylenir.
    pub fn message(&self) -> String {
        let what = match self.action {
            LaunchAction::Program => "başlatılması",
            LaunchAction::CommandHelp => "kullanım metninin gösterilmesi",
            LaunchAction::IntegrationInfo => "kabuk entegrasyonu bilgisinin gösterilmesi",
        };
        format!(
            "'{}' için {} içinde yeni sekmede {what} istendi (dizin: {}) — \
             sekmenin açıldığını Nova doğrulayamaz, {} penceresine bak.",
            self.tool,
            self.terminal,
            self.working_directory.display(),
            self.terminal
        )
    }
}

/// `command_help` metnini ekranda tutacak sayfalayıcıyı bulur.
///
/// Girdi `handlers` listesi boşsa `[less, more]` varsayılır. `cat` bilinçli
/// olarak kullanılmaz: uzun metni "sayfalamak" için onu kullanmak metni
/// baştan sona akıtır ve terminalin geri kaydırma sınırına takılır.
///
/// Bulunan **ilk** programın çözümlenmiş yolu döner. Hiçbiri yoksa anlaşılır
/// bir hata döner; sessizce başka bir şeye düşülmez.
fn find_handler(entry: &CatalogEntry) -> Result<std::path::PathBuf, NovaError> {
    let candidates: Vec<String> = if entry.handlers.is_empty() {
        vec!["less".to_string(), "more".to_string()]
    } else {
        entry.handlers.clone()
    };
    for candidate in &candidates {
        let (status, path) = crate::detection::resolve(candidate);
        if status == crate::detection::InstalledStatus::Installed {
            if let Some(path) = path {
                return Ok(path);
            }
        }
    }
    Err(NovaError::Launcher(format!(
        "'{}' için kullanım metnini gösterecek sayfalayıcı bulunamadı \
         (denenen: {}). Metni tek seferde akıtmamak için `cat` kullanılmaz.",
        entry.name,
        candidates.join(", ")
    )))
}

/// Internal child mode: display one Nova help file and remove it afterward.
///
/// The path is constrained to a regular, private Nova help file in the
/// configured temporary directory so this hidden mode cannot remove arbitrary
/// user files when invoked directly.
pub fn run_internal_pager(pager: &Path, help_file: &Path) -> Result<(), NovaError> {
    run_internal_pager_with(pager, help_file, |pager, help_file| {
        Command::new(pager).arg(help_file).status()
    })
}

fn run_internal_pager_with<F>(pager: &Path, help_file: &Path, run_pager: F) -> Result<(), NovaError>
where
    F: FnOnce(&Path, &Path) -> std::io::Result<std::process::ExitStatus>,
{
    request::validate_private_help_file(help_file).map_err(|error| {
        NovaError::Launcher(format!(
            "dahili sayfalayıcıya geçersiz geçici dosya: {error}"
        ))
    })?;
    let pager_result = run_pager(pager, help_file);
    let cleanup_result = request::remove_private_help_file(help_file);

    let status = pager_result.map_err(|error| {
        NovaError::Launcher(format!(
            "kullanım metni sayfalayıcısı başlatılamadı: {error}"
        ))
    })?;
    cleanup_result.map_err(|error| {
        NovaError::Launcher(format!(
            "geçici kullanım metni dosyası silinemedi ('{}'): {error}",
            help_file.display()
        ))
    })?;
    if !status.success() {
        return Err(NovaError::Launcher(format!(
            "kullanım metni sayfalayıcısı başarısız oldu: {status}"
        )));
    }
    Ok(())
}

/// Süreç başlatmayı soyutlar. Testler gerçek bir terminal açmadan
/// aynı yolu sınayabilir.
pub trait ProcessSpawner {
    /// Verilen programı argümanlarla başlatır ve **bu çalıştırılabilir
    /// dosyanın** PID'ini döner.
    fn spawn(&self, program: &Path, args: &[std::ffi::OsString]) -> Result<u32, NovaError>;
}

/// Gerçek `std::process::Command` kullanan spawner.
///
/// Kabuk kullanılmaz; program ve argümanlar ayrı ayrı geçirilir.
#[derive(Debug, Clone, Copy, Default)]
pub struct CommandSpawner;

impl ProcessSpawner for CommandSpawner {
    fn spawn(&self, program: &Path, args: &[std::ffi::OsString]) -> Result<u32, NovaError> {
        let mut cmd = Command::new(program);
        cmd.args(args);
        // Konsole, çocuk süreç sonlanana kadar ön planda kalabilir ve
        // ebeveynin stdio'sunu tutabilir. TUI'nin bu yüzden donmaması için
        // standart akışlar ayrılır ve süreç beklenmez.
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd.spawn().map_err(|e| {
            NovaError::Launcher(format!(
                "'{}' çalıştırılamadı ({}): {e}",
                program.display(),
                e.kind()
            ))
        })?;

        // Çocuk süreç bilinçli olarak "zombileşmesin" diye beklenmez ve
        // `wait` edilmez; PID yalnızca raporlama içindir.
        let pid = child.id();
        drop(child);
        Ok(pid)
    }
}

/// Bir çalıştırma biçiminin şu an desteklenip desteklenmediğini kontrol eder.
fn ensure_supported_mode(entry: &CatalogEntry) -> Result<(), NovaError> {
    match entry.launch_mode {
        LaunchMode::Terminal => Ok(()),
        LaunchMode::Background | LaunchMode::Detached => Err(NovaError::Launcher(format!(
            "'{}' için çalıştırma biçimi bu sürümde desteklenmiyor \
             (yalnızca yeni terminal sekmesi destekleniyor).",
            entry.name
        ))),
    }
}

/// Bir katalog girdisinden **süreç başlatmadan** yeni sekmeye geçirilecek
/// programı ve argümanları üretir (araç argümanları ayrı ayrı, kopyalanır).
fn entry_program_and_args(
    entry: &CatalogEntry,
    program: std::path::PathBuf,
) -> (std::path::PathBuf, Vec<std::ffi::OsString>) {
    (
        program,
        entry.args.iter().map(std::ffi::OsString::from).collect(),
    )
}

/// Bir sekmeye özgü başlatma seçenekleri.
///
/// Nova'nın kalıcı durumundan (seçili proje dizini, varsayılan çalışma
/// dizini, tercih edilen terminal) gelen, **doğrulanmamış** tercihler.
/// Doğrulama `launch_with_options` içinde yapılır.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    /// Seçili proje dizini; `working_directory = "project"` girdileri buraya
    /// taşınır.
    pub project_dir: Option<std::path::PathBuf>,
    /// `Current`/`Home` yerine kullanılacak varsayılan çalışma dizini.
    pub default_working_dir: Option<std::path::PathBuf>,
    /// Desteklenen emülatörler arasında tercih (ör. `"konsole"`).
    pub preferred_terminal: Option<String>,
}

/// Bir katalog girdisini yeni bir terminal sekmesinde başlatır.
///
/// Adımlar ve her birinin anlamı:
///
/// 1. Çalıştırma biçimi destekleniyor mu? (hayır → istek yok)
/// 2. `launch_kind` uygun mu?
///    - `interactive` / `custom_command`: araç kurulu mu, çalışma dizini
///      geçerli mi? → [`LaunchRequest`] (**süreç başlatılmadan** doğrulanır).
///    - `command_help`: araç **çalıştırılmaz**; TOML'dan gelen sabit metin
///      bulunan bir sayfalayıcıya beslenir.
///    - `shell_integration_info`: araç **çalıştırılmaz**; bilgi metni
///      gösterilir. Araç kurulu olmasa bile çalışabilir.
/// 3. Terminal emülatörü bulundu mu? (yok → anlaşılır hata)
/// 4. Emülatör süreci başlatıldı mı? → [`LaunchOutcome`] ("istendi").
///
/// `S` enjekte edilebilir spawner'dır; testler gerçek Konsole açmaz.
pub fn launch_with<S: ProcessSpawner>(
    entry: &CatalogEntry,
    home: &Path,
    spawner: &S,
) -> Result<LaunchOutcome, NovaError> {
    launch_with_options(entry, home, &LaunchOptions::default(), spawner)
}

/// [`launch_with`] gibi çalışır, ancak sekmeye özgü tercihleri uygular.
///
/// - `options.project_dir` verilmişse `project` çalışma dizini girdileri
///   o dizine taşınır.
/// - `options.default_working_dir` verilmişse `current`/`home` yerine
///   kullanılır; geçersizse **açık hata** döner (sessiz düşüş yok).
/// - `options.preferred_terminal` verilmişse desteklenmeyen ad açık hata
///   döner; desteklenen ama sistemde bulunmayan ad da açık hatadır.
pub fn launch_with_options<S: ProcessSpawner>(
    entry: &CatalogEntry,
    home: &Path,
    options: &LaunchOptions,
    spawner: &S,
) -> Result<LaunchOutcome, NovaError> {
    ensure_supported_mode(entry)?;

    if entry.launch_kind == crate::config::LaunchKind::ShellIntegrationInfo {
        let text = entry.info_text.clone().unwrap_or_else(|| {
            format!(
                "'{}' bu görünümden başlatılmaz; kabuk entegrasyonu gerektirir.",
                entry.name
            )
        });
        return launch_text(
            entry,
            home,
            options,
            spawner,
            LaunchAction::IntegrationInfo,
            &text,
        );
    }

    if entry.launch_kind == crate::config::LaunchKind::CommandHelp {
        let text = entry.help_text.clone().ok_or_else(|| {
            NovaError::Launcher(format!(
                "'{}' için sabit kullanım metni tanımlı değil.",
                entry.name
            ))
        })?;
        return launch_text(
            entry,
            home,
            options,
            spawner,
            LaunchAction::CommandHelp,
            &text,
        );
    }

    // Adım 2: istek. Hem araç kurulu değilse hem de çalışma dizini
    // geçersizse burada, hiçbir süreç başlamadan hata döner.
    let mut request =
        LaunchRequest::from_entry_with_project(entry, home, options.project_dir.as_deref())?;
    request.working_directory =
        apply_working_directory(entry, home, options, request.working_directory)?;

    // Gömülü katalogdaki `args` yalnızca `custom_command` için geçirilir:
    // etkileşimli araçlara argümansız başlatmak güvenli varsayılandır.
    if entry.launch_kind == crate::config::LaunchKind::CustomCommand {
        let (program, args) = entry_program_and_args(entry, request.program);
        request.program = program;
        request.args = args;
    }

    // Adım 3: emülatör.
    let (emulator_path, adapter) = select_adapter(options.preferred_terminal.as_deref())?;

    // Adım 4: yalnızca emülatörün argümanları üretilir; aracın kendi
    // argümanları `-e` sonrasına olduğu gibi geçer.
    let args = adapter.build_args(&emulator_path, &request);
    let emulator_pid = spawner.spawn(&emulator_path, &args)?;

    Ok(LaunchOutcome {
        tool: entry.name.clone(),
        action: LaunchAction::Program,
        terminal: adapter.name(),
        working_directory: request.working_directory,
        emulator_pid,
    })
}

/// Çalışma dizini tercihini (`proje` / `varsayılan`) uygular.
///
/// Dizinler doğrulanır; hiçbir dizin oluşturulmaz.
fn apply_working_directory(
    entry: &CatalogEntry,
    home: &Path,
    options: &LaunchOptions,
    resolved: std::path::PathBuf,
) -> Result<std::path::PathBuf, NovaError> {
    use crate::catalog::WorkingDirectory;

    let mut directory = resolved;
    // Proje dizini girdisi ama seçili proje yok: girdinin kendi dizinini
    // kullanır (gömülü katalogda `Project` yok; yalnızca kullanıcı
    // `working_dir_path` verdiyse buradadır).
    if let (WorkingDirectory::Project(_), Some(project)) =
        (&entry.working_directory, &options.project_dir)
    {
        directory = resolve_directory(
            &WorkingDirectory::Project(project.clone()),
            home,
            &FsValidator,
            &entry.name,
        )?;
    }

    if let (WorkingDirectory::Current | WorkingDirectory::Home, Some(default)) =
        (&entry.working_directory, &options.default_working_dir)
    {
        directory = resolve_directory(
            &WorkingDirectory::Custom(default.clone()),
            home,
            &FsValidator,
            &entry.name,
        )
        .map_err(|e| {
            NovaError::Launcher(format!(
                "{e} (ayarlardaki varsayılan çalışma dizini: '{}')",
                default.display()
            ))
        })?;
    }
    Ok(directory)
}

/// Araç çalıştırmadan, sabit bir metni yeni sekmede gösterir.
///
/// Konsole, başlattığı programın stdin'ini Nova'dan alamadığı için metin özel
/// geçici dosyaya yazılır. Yeni sekmede Nova'nın dahili pager modu dosya
/// yolunu sayfalayıcıya argüman olarak verir ve pager kapandıktan sonra dosyayı
/// siler. Sayfalayıcı yoksa istek hiç oluşturulmaz.
fn launch_text<S: ProcessSpawner>(
    entry: &CatalogEntry,
    home: &Path,
    options: &LaunchOptions,
    spawner: &S,
    action: LaunchAction,
    text: &str,
) -> Result<LaunchOutcome, NovaError> {
    let handler = find_handler(entry)?;
    // Seçili proje varsa girdinin kendi (bayatlamış olabilecek) dizini hiç
    // çözümlenmez; bkz. `LaunchRequest::from_entry_with_project`.
    let selection = match (&entry.working_directory, &options.project_dir) {
        (crate::catalog::WorkingDirectory::Project(_), Some(p)) => {
            crate::catalog::WorkingDirectory::Project(p.clone())
        }
        _ => entry.working_directory.clone(),
    };
    let mut request = LaunchRequest {
        program: handler.clone(),
        args: Vec::new(),
        working_directory: apply_working_directory(
            entry,
            home,
            options,
            selection.resolve(home, &entry.name)?,
        )?,
        stdin_data: None,
        stdin_file_path: None,
    };
    request.set_stdin(text.as_bytes().to_vec());
    let helper_executable = std::env::current_exe().map_err(|error| {
        NovaError::Launcher(format!(
            "Nova'nın çalıştırılabilir yolu bulunamadı: {error}"
        ))
    })?;
    let (emulator_path, adapter) = select_adapter(options.preferred_terminal.as_deref())?;
    // Metin önce diske yazılır: sekmedeki sayfalayıcı açılırken hazır olmalı.
    let help_file = request
        .write_stdin_file()?
        .ok_or_else(|| NovaError::Launcher("kullanım metni dosyası oluşturulamadı".into()))?;
    // Nova'nın küçük dahili modu pager bittikten sonra dosyayı siler. Böylece
    // Konsole'nin asenkron açılışına zaman tanırken kalıcı artık bırakmayız.
    request.program = helper_executable;
    request.args = vec![
        std::ffi::OsString::from(INTERNAL_PAGER_ARGUMENT),
        handler.into_os_string(),
        help_file.as_os_str().to_owned(),
    ];
    request.stdin_data = None;
    request.stdin_file_path = None;
    let args = adapter.build_args(&emulator_path, &request);
    let emulator_pid = match spawner.spawn(&emulator_path, &args) {
        Ok(pid) => pid,
        Err(error) => {
            let _ = request::remove_private_help_file(&help_file);
            return Err(error);
        }
    };
    Ok(LaunchOutcome {
        tool: entry.name.clone(),
        action,
        terminal: adapter.name(),
        working_directory: request.working_directory,
        emulator_pid,
    })
}

/// Gerçek spawner ile başlatır (varsayılan seçenekler).
pub fn launch(entry: &CatalogEntry, home: &Path) -> Result<LaunchOutcome, NovaError> {
    launch_with(entry, home, &CommandSpawner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::WorkingDirectory;
    use std::cell::RefCell;
    use std::ffi::OsString;
    use std::path::PathBuf;

    /// Gerçek süreç başlatmayan spawner: çağrıları kaydeder.
    struct RecordingSpawner {
        calls: RefCell<Vec<(PathBuf, Vec<OsString>)>>,
        pid: u32,
        fail: bool,
    }

    impl RecordingSpawner {
        fn ok() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                pid: 4242,
                fail: false,
            }
        }
        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::ok()
            }
        }
    }

    impl ProcessSpawner for RecordingSpawner {
        fn spawn(&self, program: &Path, args: &[OsString]) -> Result<u32, NovaError> {
            self.calls
                .borrow_mut()
                .push((program.to_path_buf(), args.to_vec()));
            if self.fail {
                return Err(NovaError::Launcher("emülatör başlatılamadı".into()));
            }
            Ok(self.pid)
        }
    }

    fn home() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("testler için HOME gerekli");
        assert!(home.is_dir());
        home
    }

    fn entry(executable: &str, args: &[&str], wd: WorkingDirectory) -> CatalogEntry {
        CatalogEntry::new(
            "test",
            "Test Aracı",
            "test",
            "Test",
            executable,
            args.iter().map(|s| s.to_string()).collect(),
            LaunchMode::Terminal,
            wd,
            false,
        )
    }

    #[test]
    fn missing_executable_is_refused_before_spawning() {
        let spawner = RecordingSpawner::ok();
        let e = entry("nova-yok-boyle-bir-komut-xyz", &[], WorkingDirectory::Home);
        let err = launch_with(&e, &home(), &spawner).expect_err("hata bekleniyordu");
        let msg = err.to_string();
        assert!(msg.contains("nova-yok-boyle-bir-komut-xyz"), "{msg}");
        assert!(spawner.calls.borrow().is_empty(), "spawn çağrılmamalı");
    }

    #[cfg(unix)]
    #[test]
    fn internal_pager_removes_its_private_help_file_after_exit() {
        use std::os::unix::process::ExitStatusExt;

        let mut request = LaunchRequest {
            program: PathBuf::from("/usr/bin/less"),
            args: Vec::new(),
            working_directory: std::env::temp_dir(),
            stdin_data: None,
            stdin_file_path: None,
        };
        request.set_stdin(b"temporary help text".to_vec());
        let path = request.write_stdin_file().unwrap().unwrap();

        let result = run_internal_pager_with(Path::new("unused"), &path, |_, help_file| {
            assert_eq!(std::fs::read(help_file).unwrap(), b"temporary help text");
            Ok(std::process::ExitStatus::from_raw(0))
        });

        assert!(result.is_ok());
        assert!(
            !path.exists(),
            "yardım dosyası pager kapandıktan sonra silinmeli"
        );
    }

    #[test]
    fn invalid_working_directory_is_refused_before_spawning() {
        let spawner = RecordingSpawner::ok();
        let e = entry(
            "sh",
            &[],
            WorkingDirectory::Custom(PathBuf::from("/nova/yok/boyle/dizin")),
        );
        if e.status != crate::detection::InstalledStatus::Installed {
            return;
        }
        let err = launch_with(&e, &home(), &spawner).expect_err("hata bekleniyordu");
        assert!(err.to_string().contains("/nova/yok/boyle/dizin"), "{err}");
        assert!(spawner.calls.borrow().is_empty());
    }

    #[test]
    fn successful_spawn_reports_requested_not_confirmed() {
        let spawner = RecordingSpawner::ok();
        let e = entry("sh", &["a b", "$HOME"], WorkingDirectory::Home);
        if e.status != crate::detection::InstalledStatus::Installed {
            return;
        }
        if select_adapter(None).is_err() {
            return; // bu ortamda terminal yok
        }
        let outcome = launch_with(&e, &home(), &spawner).unwrap();
        assert_eq!(outcome.tool, "Test Aracı");
        assert_eq!(outcome.emulator_pid, 4242);
        let msg = outcome.message();
        assert!(msg.contains("istendi"), "{msg}");
        assert!(
            !msg.contains("açıldı (") && !msg.contains("başarıyla açıldı"),
            "uygulamanın açıldığı iddia edilmemeli: {msg}"
        );
        // Argümanlar ayrı ve bozulmadan geçmeli.
        let calls = spawner.calls.borrow();
        let (program, args) = &calls[0];
        assert!(program
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("konsole"));
        let tail: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(&tail[tail.len() - 2..], &["a b", "$HOME"]);
    }

    #[test]
    fn spawn_failure_is_a_launcher_error() {
        let spawner = RecordingSpawner::failing();
        let e = entry("sh", &[], WorkingDirectory::Home);
        if e.status != crate::detection::InstalledStatus::Installed || select_adapter(None).is_err()
        {
            return;
        }
        let err = launch_with(&e, &home(), &spawner).expect_err("hata bekleniyordu");
        assert!(matches!(err, NovaError::Launcher(_)));
    }

    #[test]
    fn non_terminal_launch_mode_is_refused() {
        let spawner = RecordingSpawner::ok();
        let mut e = entry("sh", &[], WorkingDirectory::Home);
        e.launch_mode = LaunchMode::Background;
        let err = launch_with(&e, &home(), &spawner).expect_err("hata bekleniyordu");
        assert!(err.to_string().contains("desteklenmiyor"), "{err}");
        assert!(spawner.calls.borrow().is_empty());
    }
}
