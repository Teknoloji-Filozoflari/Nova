//! Nova — TUI terminal uygulama başlatıcısı.
//! Giriş noktası. Terminali kurar, uygulamayı çalıştırır ve çıkışta
//! terminalin eski durumuna döndüğünden emin olur.
//!
//! Terminalin geri yüklenmesi (normal çıkış, hata ve panik yolları) ile
//! ilgili tüm mantık [`nova::terminal_guard`] içindedir; bu dosya yalnızca
//! onu sıraya koyar.

use crossterm::event::EnableMouseCapture;
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};
use crossterm::ExecutableCommand;
use nova::app::App;
use nova::error::NovaError;
use nova::terminal_guard::{self, CrosstermState, TerminalGuard};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::stdout;
use std::path::Path;

/// Gerçek terminali ham moda alıp alt ekrana geçer.
///
/// Her adım, denenmeden önce guard'a bildirilir. Böylece hazırlık bir yerde
/// başarısız olursa veya panik oluşursa yalnızca etkilenmiş adımlar geri alınır.
fn enter_terminal(guard: &mut TerminalGuard<CrosstermState>) -> Result<(), NovaError> {
    guard.mark_raw_mode_attempted();
    enable_raw_mode().map_err(terminal_guard::raw_mode_error)?;
    guard.mark_alternate_screen_attempted();
    stdout()
        .execute(EnterAlternateScreen)
        .map_err(terminal_guard::raw_mode_error)?;
    guard.mark_mouse_capture_attempted();
    stdout()
        .execute(EnableMouseCapture)
        .map_err(terminal_guard::raw_mode_error)?;
    Ok(())
}

/// Kataloğu ve kullanıcı durumunu okuyup uygulamayı çalıştırır.
fn run() -> Result<(), NovaError> {
    // Kataloğu **terminali hazırlamadan önce** yükle: bozuk bir
    // yapılandırma hatası normal ekranda okunabilir olmalı, alt ekranda
    // kaybolmamalı. Yükleme sırasında hiçbir araç çalıştırılmaz.
    let catalog = nova::catalog::loader::load_default().map_err(|e| {
        let config_hint = nova::config::user_config_path()
            .map(|path| {
                format!(
                    "'{}' dosyasını düzeltip Nova'yı yeniden başlat.",
                    path.display()
                )
            })
            .unwrap_or_else(|| {
                "$HOME bulunamadığı için kullanıcı yapılandırma dosyası belirlenemedi.".to_string()
            });
        NovaError::Config(format!("Araç kataloğu yüklenemedi: {e}\n{config_hint}"))
    })?;

    // Kullanıcı durumunu (favoriler, geçmiş, proje dizinleri) **terminali
    // hazırlamadan önce** oku: bozuk bir durum dosyası yedeklensin ve uyarı
    // normal ekranda görülsün, alt ekranda kaybolmasın.
    let state_path = nova::config::state::state_path();
    let loaded = match state_path.as_deref() {
        Some(path) => nova::config::state::load_state(path),
        None => nova::config::state::StateLoad {
            state: Default::default(),
            warning: Some(
                "Ev dizini ($HOME) bulunamadı; favoriler ve geçmiş bu oturumda \
                 yalnızca bellekte tutulacak."
                    .into(),
            ),
            existed: false,
        },
    };
    let mut app_state = nova::app::state::AppState {
        path: state_path,
        file: loaded.state,
        active_project: None,
        restore_warnings: Vec::new(),
    };
    // `config.toml`'daki gelişmiş ayarlar (`[ui] recent_limit`).
    let state_warning = loaded.warning;
    let mut prefs_warning = None;
    // `config.toml`'da tanımlı araçların kimlikleri: bu araçlar Nova'da
    // silinirse dosya yeniden yazılmaz, yalnızca gizlenir.
    let mut config_tool_ids: Vec<String> = Vec::new();
    if let Some(cfg_path) = nova::config::user_config_path() {
        if let Ok(cfg) = nova::config::load_user_config(&cfg_path) {
            config_tool_ids = cfg.tools.iter().map(|t| t.id.clone()).collect();
            if let Some(limit) = cfg.ui.recent_limit {
                let before = app_state.file.prefs.recent_limit_value;
                app_state.set_recent_limit(limit);
                if before.is_none() {
                    prefs_warning = Some(format!("Geçmiş sınırı {} olarak ayarlandı.", limit));
                }
            }
        }
    }
    // Tercihleri (varsayılan çalışma dizini) yükleme öncesi doğrula: sessizce
    // ev dizinine düşmek yerine açık bir uyarı gösterilir.
    if let Some(dir) = app_state.file.prefs.default_working_dir.clone() {
        let path = std::path::PathBuf::from(&dir);
        if !path.is_dir() {
            app_state.file.prefs.default_working_dir = None;
            prefs_warning = Some(format!(
                "Ayarlardaki varsayılan çalışma dizini '{}' bulunamadı; bu sefer ev dizini kullanılacak.",
                path.display()
            ));
        }
    }

    // Terminali hazırla (ekranı koruyan bir guard ile).
    let mut guard = TerminalGuard::new(CrosstermState);
    terminal_guard::install_panic_hook(guard.progress());
    enter_terminal(&mut guard)?;

    // Ratatui terminalini oluştur.
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    // Uygulamayı çalıştır. Durum dosyası yoksa gömülü favoriler tohumlanır.
    let mut app = App::with_state(catalog, app_state, !loaded.existed);
    app.persist_first_run();
    app.set_config_tool_ids(config_tool_ids);
    let messages: Vec<String> = [state_warning, prefs_warning]
        .into_iter()
        .flatten()
        .chain(app.state.restore_warnings.iter().cloned())
        .collect();
    if !messages.is_empty() {
        app.status_message = messages.join(" ");
    }
    // Başlangıç terminal boyutuna göre dar/geniş modu ayarla.
    if let Ok(size) = terminal.size() {
        app.terminal_size = Some((size.width, size.height));
        app.narrow_mode = size.width < nova::ui::WIDE_MIN_WIDTH;
        if app.narrow_mode {
            app.focus = nova::app::PanelFocus::Tool;
        }
    }
    app.run(&mut terminal)?;

    // Self-update sonrası: önce terminali geri yükle, sonra yeni ikiliyi exec et.
    // Guard Drop / elle restore ham moda bozulmuş bir oturum bırakmaz; exec
    // başarısızsa süreç ayakta kalır ve hata normal ekranda basılır.
    let restart = app.pending_self_restart.take();
    drop(terminal);
    guard.restore();
    if let Some(binary) = restart {
        if let Err(err) = nova::update::restart_into(&binary) {
            eprintln!("Nova: {err}");
            return Err(NovaError::Internal(err));
        }
    }

    Ok(())
}

/// Ana giriş noktası.
fn main() -> Result<(), NovaError> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref()
        == Some(std::ffi::OsStr::new(
            nova::launcher::INTERNAL_PAGER_ARGUMENT,
        ))
    {
        let pager = args.next().ok_or_else(|| {
            NovaError::Launcher("dahili sayfalayıcı programı belirtilmedi".into())
        })?;
        let help_file = args
            .next()
            .ok_or_else(|| NovaError::Launcher("dahili sayfalayıcı dosyası belirtilmedi".into()))?;
        if args.next().is_some() {
            return Err(NovaError::Launcher(
                "dahili sayfalayıcıya fazla argüman verildi".into(),
            ));
        }
        return nova::launcher::run_internal_pager(Path::new(&pager), Path::new(&help_file));
    }
    run()
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_module_compiles() {
        // Bu test sadece modülün derlendiğini doğrular.
    }
}
