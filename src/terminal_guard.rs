//! Terminalin eski durumuna dönmesini garanti eden koruyucu.
//!
//! Nova alt ekrana geçip ham moda girdikten sonra herhangi bir nedenle
//! (normal çıkış, `?` ile dönen hata, panik) sonlanırsa terminal bozuk
//! kalmamalıdır. Bu modül:
//!
//! - normal çıkış ve hata yolları için `Drop`,
//! - panik için bir kapanış hook'u (`panic hook`)
//!
//! kurar. Her ikisi de **aynı** [`restore_terminal`] işlemini çağırır; böylece
//! "geri yükleme" tek bir yerde tanımlıdır ve gerçek terminale dokunmadan
//! test edilebilir.

use crate::error::NovaError;
use std::io::Write;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

const RAW_MODE_ATTEMPTED: u8 = 1;
const ALTERNATE_SCREEN_ATTEMPTED: u8 = 2;

/// Terminal hazırlığında geri alınması gereken adımları paylaşır.
///
/// Panic hook'u ve RAII koruyucusu aynı işaretleri kullanır. Hook geri yüklemeyi
/// yaparsa işaretleri atomik olarak tüketir; yığın çözülürken çalışan `Drop`
/// aynı terminal adımlarını ikinci kez çalıştırmaz.
#[derive(Clone, Debug, Default)]
pub struct TerminalProgress(Arc<AtomicU8>);

impl TerminalProgress {
    fn mark_raw_mode_attempted(&self) {
        self.0.fetch_or(RAW_MODE_ATTEMPTED, Ordering::SeqCst);
    }

    fn mark_alternate_screen_attempted(&self) {
        self.0
            .fetch_or(ALTERNATE_SCREEN_ATTEMPTED, Ordering::SeqCst);
    }

    fn take(&self) -> u8 {
        self.0.swap(0, Ordering::SeqCst)
    }

    fn is_active(&self) -> bool {
        self.0.load(Ordering::SeqCst) != 0
    }
}

/// Terminalden yapılması gereken geri alma işlemleri.
///
/// Gerçek uygulamada crossterm kullanılır; testler bu trait'i taklit ederek
/// hem hata yolunu hem panik yolunu sınayabilir.
pub trait TerminalState {
    /// Ham modu kapatır.
    fn disable_raw_mode(&mut self) -> std::io::Result<()>;
    /// Alt ekrandan çıkar.
    fn leave_alternate_screen(&mut self) -> std::io::Result<()>;
}

/// Gerçek terminali `stdout` üzerinden geri yükler.
#[derive(Debug, Default)]
pub struct CrosstermState;

impl TerminalState for CrosstermState {
    fn disable_raw_mode(&mut self) -> std::io::Result<()> {
        crossterm::terminal::disable_raw_mode()
    }

    fn leave_alternate_screen(&mut self) -> std::io::Result<()> {
        crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)
    }
}

/// Terminali eski durumuna döndürür.
///
/// Hatalar bilinçli olarak yutulur: bu fonksiyon `Drop` içinden ve panik
/// hook'undan çağrılır; ikinci bir panik (veya çıkış sırasında anlamsız bir
/// hata) asıl hatayı gizlememelidir. İşaretlenmiş her adım denenir; biri
/// başarısız olsa bile sıradaki işaretli adım çalıştırılır.
pub fn restore_terminal<S: TerminalState>(state: &mut S, progress: &TerminalProgress) {
    let attempted = progress.take();
    if attempted & RAW_MODE_ATTEMPTED != 0 {
        let _ = state.disable_raw_mode();
    }
    if attempted & ALTERNATE_SCREEN_ATTEMPTED != 0 {
        let _ = state.leave_alternate_screen();
    }
}

/// Panik bilgisini tek satırlık, kullanıcıya yönelik bir metne çevirir.
///
/// Yığın izi burada üretilmez: varsayılan panik hook'u (kuruluysa) onu
/// kendisi ekler.
pub fn panic_report(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    let message = if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "bilinmeyen panik mesajı".to_string()
    };
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "konum bilinmiyor".to_string());

    format!(
        "Nova beklenmeyen bir hatayla kapandı ({location}): {message}\n\
         Terminal eski durumuna döndürülmeye çalışıldı. Bu mesajı bir sorun \
         bildirimine ekleyebilirsin."
    )
}

/// Panik anında yapılacak işin **sırası**.
///
/// 1. Terminal geri yüklenir (aksi hâlde mesaj alt ekranda kaybolur),
/// 2. açık bir rapor yazılır,
/// 3. önceki hook (varsa) mesajıyla birlikte çağrılır; Nova onu kaldırmaz.
///
/// Bu fonksiyon `PanicHookInfo` ile ilgilenmez; bu yüzden testlerde değer
/// verilerek doğrudan sınanabilir (`PanicHookInfo` testte üretilemez).
pub fn run_panic_sequence(
    restore: &mut impl FnMut(),
    write: &mut impl FnMut(),
    previous: Option<&mut dyn FnMut()>,
) {
    restore();
    write();
    if let Some(previous) = previous {
        previous();
    }
}

/// Panik hook'larında kullanılan closure türleri.
type HookFn = Box<dyn FnMut(&std::panic::PanicHookInfo<'_>) + Send>;
/// Panik anında terminali geri yükleyen closure.
type RestoreFn = Box<dyn FnMut() + Send>;

/// Panik hook'u kurulumunun tek somut adımı.
///
/// `std::panic::set_hook` süreç genelini değiştirdiği için hook'u kuran
/// fonksiyon ile **davranış** ([`run_panic_sequence`]) ayrı tutulur.
pub fn make_panic_hook(
    restore: RestoreFn,
    write: HookFn,
    previous: Option<HookFn>,
) -> Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static> {
    // `set_hook` `Fn + Sync` ister; iç durum bir kilitle korunarak `Fn`
    // elde edilir. Kilit zehirlenirse (panik sırasında) içeriğe yine
    // erişilir: geri yükleme tam bu anda en çok gereken şeydir.
    let state = std::sync::Mutex::new((restore, write, previous));
    Box::new(move |info| {
        let mut state = match state.lock() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (restore, write, previous) = &mut *state;
        // Sıra bilinçli olarak run_panic_sequence ile aynıdır; burada
        // closure'lar farklı türde olduğundan adımlar açıkça yazılır
        // (bkz. run_panic_sequence: geri yükle, yaz, önceki hook).
        restore();
        write(info);
        if let Some(previous) = previous.as_mut() {
            previous(info);
        }
    })
}

/// Panik hook'unu süreç genelinde kurar.
///
/// Zaten kurulmuş bir hook varsa (ör. test koşucusu) **kaldırılmaz**,
/// korunur ve nova'nın geri yüklemesi ondan önce çalışır.
pub fn install_panic_hook(progress: TerminalProgress) {
    static INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if INSTALLED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    let hook = make_panic_hook(
        Box::new(move || restore_terminal(&mut CrosstermState, &progress)),
        Box::new(|info| write_panic_report(&panic_report(info))),
        Some(previous),
    );
    std::panic::set_hook(hook);
}

/// Panik raporunu `stderr`'e yazar. Alt ekrandan çıkıldığı için görünür.
fn write_panic_report(report: &str) {
    let mut stderr = std::io::stderr();
    let _ = writeln!(stderr, "{report}");
    let _ = stderr.flush();
}

/// Terminali hazırlayan RAII koruyucu.
///
/// `state` dışarıdan verilir: gerçek uygulama [`CrosstermState`] verir,
/// testler taklit verir. Yalnızca hazırlanmaya başlanmış terminal adımları
/// geri alınır.
pub struct TerminalGuard<S: TerminalState> {
    state: S,
    progress: TerminalProgress,
}

impl<S: TerminalState> TerminalGuard<S> {
    /// Yeni bir koruyucu oluşturur.
    pub fn new(state: S) -> Self {
        Self {
            state,
            progress: TerminalProgress::default(),
        }
    }

    /// Raw mode hazırlığına başlanmadan hemen önce çağrılır.
    pub fn mark_raw_mode_attempted(&self) {
        self.progress.mark_raw_mode_attempted();
    }

    /// Alt ekran geçişi denenmeden hemen önce çağrılır.
    pub fn mark_alternate_screen_attempted(&self) {
        self.progress.mark_alternate_screen_attempted();
    }

    /// Panic hook'un aynı hazırlık durumunu izlemesini sağlar.
    pub fn progress(&self) -> TerminalProgress {
        self.progress.clone()
    }

    /// Geri alınması gereken bir terminal adımı var mı?
    pub fn is_active(&self) -> bool {
        self.progress.is_active()
    }

    /// Elle geri yükleme; `Drop` de aynı işi yapar, ikinci kez yapılmaz.
    pub fn restore(&mut self) {
        restore_terminal(&mut self.state, &self.progress);
    }
}

impl<S: TerminalState> Drop for TerminalGuard<S> {
    fn drop(&mut self) {
        self.restore();
    }
}

/// Ham moda geçiş başarısız olduğunda dönen açık hata.
pub fn raw_mode_error(err: std::io::Error) -> NovaError {
    NovaError::Terminal(format!(
        "terminal ham moda alınamadı; Nova bir terminalde çalıştırılmalı: {err}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Panik hook'ları `Send` olmak zorundadır; testler paylaşılan bir
    /// sayaç için `Arc<Mutex<..>>` kullanır.
    type Shared = std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>;

    struct HookFakeState {
        log: Shared,
    }

    impl TerminalState for HookFakeState {
        fn disable_raw_mode(&mut self) -> std::io::Result<()> {
            self.log.lock().unwrap().push("raw-off");
            Ok(())
        }

        fn leave_alternate_screen(&mut self) -> std::io::Result<()> {
            self.log.lock().unwrap().push("alt-off");
            Ok(())
        }
    }

    /// Çağrıları kaydeden taklit terminal.
    struct FakeState {
        log: Rc<RefCell<Vec<&'static str>>>,
        fail_raw: bool,
        fail_alt: bool,
    }

    impl FakeState {
        fn new() -> (Self, Rc<RefCell<Vec<&'static str>>>) {
            let log = Rc::new(RefCell::new(Vec::new()));
            (
                Self {
                    log: log.clone(),
                    fail_raw: false,
                    fail_alt: false,
                },
                log,
            )
        }
    }

    impl TerminalState for FakeState {
        fn disable_raw_mode(&mut self) -> std::io::Result<()> {
            self.log.borrow_mut().push("raw-off");
            if self.fail_raw {
                return Err(std::io::Error::other("raw"));
            }
            Ok(())
        }

        fn leave_alternate_screen(&mut self) -> std::io::Result<()> {
            self.log.borrow_mut().push("alt-off");
            if self.fail_alt {
                return Err(std::io::Error::other("alt"));
            }
            Ok(())
        }
    }

    #[test]
    fn drop_restores_terminal_exactly_once() {
        let (state, log) = FakeState::new();
        {
            let guard = TerminalGuard::new(state);
            guard.mark_raw_mode_attempted();
            guard.mark_alternate_screen_attempted();
        }
        assert_eq!(*log.borrow(), vec!["raw-off", "alt-off"]);
    }

    #[test]
    fn inactive_guard_does_not_touch_the_terminal() {
        let (state, log) = FakeState::new();
        drop(TerminalGuard::new(state));
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn failed_raw_mode_attempt_does_not_leave_an_unentered_alternate_screen() {
        let (state, log) = FakeState::new();
        let guard = TerminalGuard::new(state);
        guard.mark_raw_mode_attempted();
        drop(guard);
        assert_eq!(*log.borrow(), vec!["raw-off"]);
    }

    #[test]
    fn explicit_restore_then_drop_does_not_restore_twice() {
        let (state, log) = FakeState::new();
        let mut guard = TerminalGuard::new(state);
        guard.mark_raw_mode_attempted();
        guard.mark_alternate_screen_attempted();
        guard.restore();
        assert!(!guard.is_active());
        drop(guard);
        assert_eq!(*log.borrow(), vec!["raw-off", "alt-off"]);
    }

    #[test]
    fn failing_step_does_not_skip_the_other_and_never_panics() {
        let (mut state, log) = FakeState::new();
        state.fail_raw = true;
        state.fail_alt = true;
        let progress = TerminalProgress::default();
        progress.mark_raw_mode_attempted();
        progress.mark_alternate_screen_attempted();
        restore_terminal(&mut state, &progress);
        assert_eq!(*log.borrow(), vec!["raw-off", "alt-off"]);

        let (mut state, log) = FakeState::new();
        state.fail_raw = true;
        let progress = TerminalProgress::default();
        progress.mark_raw_mode_attempted();
        progress.mark_alternate_screen_attempted();
        restore_terminal(&mut state, &progress);
        assert_eq!(*log.borrow(), vec!["raw-off", "alt-off"]);
    }

    #[test]
    fn panic_sequence_restores_then_writes_then_calls_previous_hook() {
        let order: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let (a, b, c) = (order.clone(), order.clone(), order.clone());
        run_panic_sequence(
            &mut || a.borrow_mut().push("restore"),
            &mut || b.borrow_mut().push("write"),
            Some(&mut || c.borrow_mut().push("previous")),
        );
        assert_eq!(*order.borrow(), vec!["restore", "write", "previous"]);
    }

    #[test]
    fn panic_sequence_without_previous_hook_still_restores() {
        let order: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let (a, b) = (order.clone(), order.clone());
        run_panic_sequence(
            &mut || a.borrow_mut().push("restore"),
            &mut || b.borrow_mut().push("write"),
            None,
        );
        assert_eq!(*order.borrow(), vec!["restore", "write"]);
    }

    #[test]
    fn panic_hook_through_a_real_panic_restores_and_reports() {
        // Gerçek bir panik ile hook'u çalıştır: `PanicHookInfo` yalnızca
        // böyle üretilebilir. Hook süreç geneline kurulur ve sonra
        // kaldırılır; bu testin kendi hook'u daima bir sonraki hook olarak
        // zincirlenir, varsayılan rapor geri getirilir.
        let order: Shared = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let guard = TerminalGuard::new(HookFakeState { log: order.clone() });
        let hook_progress = guard.progress();
        let (restore_log, write_log) = (order.clone(), order.clone());
        std::panic::set_hook(make_panic_hook(
            Box::new(move || {
                restore_terminal(
                    &mut HookFakeState {
                        log: restore_log.clone(),
                    },
                    &hook_progress,
                )
            }),
            Box::new(move |info| {
                let report = panic_report(info);
                assert!(report.contains("kapanış testi"), "{report}");
                write_log.lock().unwrap().push("write");
            }),
            None,
        ));
        let panicked_before_setup =
            std::panic::catch_unwind(|| panic!("kapanış testi, terminal kurulmadan önce"));
        assert!(panicked_before_setup.is_err());
        assert_eq!(*order.lock().unwrap(), vec!["write"]);

        guard.mark_raw_mode_attempted();
        guard.mark_alternate_screen_attempted();
        let panicked_during_session =
            std::panic::catch_unwind(|| panic!("kapanış testi, terminal etkin"));
        let _ = std::panic::take_hook();
        drop(guard);
        assert!(panicked_during_session.is_err());
        assert_eq!(
            *order.lock().unwrap(),
            vec!["write", "raw-off", "alt-off", "write"]
        );
    }

    // Not: "hook içinde ikinci panik" senaryosu süreç içinde test edilemez;
    // Rust panik sırasında panik atıldığında süreci abort eder. Bu yüzden
    // `make_panic_hook` içindeki zehirli-kilit yolu yalnızca kodda bir
    // güvenlik ağı olarak kalır (panik hook'ları kendi closure'larında panik
    // atmamalıdır).

    #[test]
    fn raw_mode_error_is_a_terminal_error_with_advice() {
        let err = raw_mode_error(std::io::Error::other("No such device or address"));
        assert!(matches!(err, NovaError::Terminal(_)));
        let msg = err.to_string();
        assert!(msg.contains("terminal ham moda alınamadı"), "{msg}");
        assert!(msg.contains("No such device or address"), "{msg}");
    }
}
