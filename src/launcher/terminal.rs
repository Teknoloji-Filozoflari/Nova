//! Terminal emülatörü adapterleri.
//!
//! Nova'nın kendisi bir TUI'dir; bir aracı "yeni sekmede" açmak için dışarıdan
//! bir terminal emülatörüne ihtiyaç duyar. Farklı emülatörlerin argüman
//! biçimleri birbirinden çok farklı olduğu için araya küçük bir adapter
//! katmanı konur.
//!
//! Tasarım sınırı bilinçlidir: **plugin sistemi yoktur.** Yeni bir emülatör
//! eklemek, `TerminalAdapter` trait'ini uygulayan bir struct yazmak ve
//! `TERMINALS` listesine bir satır eklemekten ibarettir.
//!
//! Otomatik seçim sırası (`preferred` boşken):
//! 1. `$TERMINAL` ortam değişkeni (bilinen bir emülatör adına çözülürse)
//! 2. [`TERMINALS`] listesi: xdg-terminal-exec, kitty, alacritty, wezterm,
//!    gnome-terminal, konsole, foot, xterm, tmux

use crate::error::NovaError;
use crate::launcher::request::LaunchRequest;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Bir terminal emülatörü adapteri.
///
/// Adapterlerin tek işi **komut satırı üretmek**tir; süreç başlatmazlar.
/// Bu sayede argüman sırası ve biçimi gerçek bir terminal açmadan test
/// edilebilir.
pub trait TerminalAdapter {
    /// Kullanıcıya gösterilecek ad (ör. "Konsole").
    fn name(&self) -> &'static str;

    /// Emülatörün çalıştırılabilir dosya adı (PATH üzerinde aranır).
    fn executable(&self) -> &'static str;

    /// Yeni bir sekmede isteği çalıştıracak argümanları üretir.
    ///
    /// `emulator` emülatörün bulunan tam yoludur; yalnızca bilgi amaçlıdır,
    /// argümanlara konmaz.
    fn build_args(&self, emulator: &Path, request: &LaunchRequest) -> Vec<OsString>;
}

/// Program + argümanlar (+ isteğe bağlı stdin dosya yolu) ekler.
fn push_command(args: &mut Vec<OsString>, request: &LaunchRequest) {
    args.push(request.program.as_os_str().to_owned());
    args.extend(request.args.iter().cloned());
    if let Some(path) = request.stdin_path() {
        args.push(path.as_os_str().to_owned());
    }
}

/// KDE Konsole adapteri.
///
/// Doğrulanmış davranış (Konsole 26.08.2):
/// - `--workdir <dir>` yeni sekmenin çalışma dizinini belirler.
/// - `--new-tab` sekme ister; pencere yoksa Konsole sekme değil pencere açar
///   (yine de Nova'dan bağımsız yeni bir Konsole penceresi olur).
/// - `-e <cmd>` **tüm izleyen argümanları yakalar**, bu yüzden son seçenek
///   olmak zorundadır. Konsole yardım metni bunu açıkça söyler.
#[derive(Debug, Clone, Copy, Default)]
pub struct Konsole;

impl TerminalAdapter for Konsole {
    fn name(&self) -> &'static str {
        "Konsole"
    }

    fn executable(&self) -> &'static str {
        "konsole"
    }

    fn build_args(&self, _emulator: &Path, request: &LaunchRequest) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            "--new-tab".into(),
            "--workdir".into(),
            request.working_directory.as_os_str().to_owned(),
            // Bu noktadan sonraki her şey `-e` tarafından yutulur.
            "-e".into(),
        ];
        push_command(&mut args, request);
        args
    }
}

/// Freedesktop `xdg-terminal-exec` (tercih edilen terminal başlatıcısı).
#[derive(Debug, Clone, Copy, Default)]
pub struct XdgTerminalExec;

impl TerminalAdapter for XdgTerminalExec {
    fn name(&self) -> &'static str {
        "xdg-terminal-exec"
    }

    fn executable(&self) -> &'static str {
        "xdg-terminal-exec"
    }

    fn build_args(&self, _emulator: &Path, request: &LaunchRequest) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            format!("--dir={}", request.working_directory.display()).into(),
            "--".into(),
        ];
        push_command(&mut args, request);
        args
    }
}

/// Kitty adapteri.
#[derive(Debug, Clone, Copy, Default)]
pub struct Kitty;

impl TerminalAdapter for Kitty {
    fn name(&self) -> &'static str {
        "Kitty"
    }

    fn executable(&self) -> &'static str {
        "kitty"
    }

    fn build_args(&self, _emulator: &Path, request: &LaunchRequest) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            "--detach".into(),
            "--directory".into(),
            request.working_directory.as_os_str().to_owned(),
        ];
        push_command(&mut args, request);
        args
    }
}

/// Alacritty adapteri.
#[derive(Debug, Clone, Copy, Default)]
pub struct Alacritty;

impl TerminalAdapter for Alacritty {
    fn name(&self) -> &'static str {
        "Alacritty"
    }

    fn executable(&self) -> &'static str {
        "alacritty"
    }

    fn build_args(&self, _emulator: &Path, request: &LaunchRequest) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            "--working-directory".into(),
            request.working_directory.as_os_str().to_owned(),
            "-e".into(),
        ];
        push_command(&mut args, request);
        args
    }
}

/// WezTerm adapteri.
#[derive(Debug, Clone, Copy, Default)]
pub struct Wezterm;

impl TerminalAdapter for Wezterm {
    fn name(&self) -> &'static str {
        "WezTerm"
    }

    fn executable(&self) -> &'static str {
        "wezterm"
    }

    fn build_args(&self, _emulator: &Path, request: &LaunchRequest) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            "start".into(),
            "--cwd".into(),
            request.working_directory.as_os_str().to_owned(),
            "--".into(),
        ];
        push_command(&mut args, request);
        args
    }
}

/// GNOME Terminal adapteri.
#[derive(Debug, Clone, Copy, Default)]
pub struct GnomeTerminal;

impl TerminalAdapter for GnomeTerminal {
    fn name(&self) -> &'static str {
        "GNOME Terminal"
    }

    fn executable(&self) -> &'static str {
        "gnome-terminal"
    }

    fn build_args(&self, _emulator: &Path, request: &LaunchRequest) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            format!(
                "--working-directory={}",
                request.working_directory.display()
            )
            .into(),
            "--".into(),
        ];
        push_command(&mut args, request);
        args
    }
}

/// Foot (Wayland) adapteri.
#[derive(Debug, Clone, Copy, Default)]
pub struct Foot;

impl TerminalAdapter for Foot {
    fn name(&self) -> &'static str {
        "Foot"
    }

    fn executable(&self) -> &'static str {
        "foot"
    }

    fn build_args(&self, _emulator: &Path, request: &LaunchRequest) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            "-D".into(),
            request.working_directory.as_os_str().to_owned(),
        ];
        push_command(&mut args, request);
        args
    }
}

/// XTerm adapteri.
///
/// XTerm'in güvenilir bir çalışma dizini seçeneği yoktur; başlatıcı süreç
/// `current_dir` ile emülatörü doğru dizinde açar (bkz. `CommandSpawner`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Xterm;

impl TerminalAdapter for Xterm {
    fn name(&self) -> &'static str {
        "XTerm"
    }

    fn executable(&self) -> &'static str {
        "xterm"
    }

    fn build_args(&self, _emulator: &Path, request: &LaunchRequest) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec!["-e".into()];
        push_command(&mut args, request);
        args
    }
}

/// tmux adapteri (son çare; çalışan bir tmux sunucusu bekler).
#[derive(Debug, Clone, Copy, Default)]
pub struct Tmux;

impl TerminalAdapter for Tmux {
    fn name(&self) -> &'static str {
        "tmux"
    }

    fn executable(&self) -> &'static str {
        "tmux"
    }

    fn build_args(&self, _emulator: &Path, request: &LaunchRequest) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            "new-window".into(),
            "-c".into(),
            request.working_directory.as_os_str().to_owned(),
            "--".into(),
        ];
        push_command(&mut args, request);
        args
    }
}

/// Bilinen emülatörler, otomatik tercih sırasına göre.
///
/// `$TERMINAL` bu listeden önce ayrıca denenir (bkz. [`select_adapter`]).
pub const TERMINALS: &[&dyn TerminalAdapter] = &[
    &XdgTerminalExec,
    &Kitty,
    &Alacritty,
    &Wezterm,
    &GnomeTerminal,
    &Konsole,
    &Foot,
    &Xterm,
    &Tmux,
];

/// Desteklenen emülatörlerin çalıştırılabilir adlarını virgülle birleştirir.
pub fn supported_executables_list() -> String {
    TERMINALS
        .iter()
        .map(|a| a.executable())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Verilen adın (görünen ad veya executable) bilinen bir adapter olup
/// olmadığını kontrol eder; eşleşirse adapter döner.
pub fn find_adapter_by_name(name: &str) -> Option<&'static dyn TerminalAdapter> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    TERMINALS
        .iter()
        .copied()
        .find(|a| a.executable().eq_ignore_ascii_case(name) || a.name().eq_ignore_ascii_case(name))
}

/// `$TERMINAL` ortam değişkenini bilinen bir adaptere çözümlemeyi dener.
///
/// Değer absolute yol veya PATH'teki bir komut olabilir. Basename bilinen
/// bir emülatörle eşleşmezse `None` döner (sessizce yanlış `-e` uydurulmaz).
fn resolve_terminal_env() -> Option<(PathBuf, &'static dyn TerminalAdapter)> {
    let raw = std::env::var_os("TERMINAL")?;
    if raw.is_empty() {
        return None;
    }
    let candidate = PathBuf::from(&raw);
    let basename = candidate.file_name()?.to_str()?;
    let adapter = find_adapter_by_name(basename)?;

    // Kullanıcının verdiği yolu tercih et; yoksa adapter executable'ını ara.
    let resolved = if candidate.is_absolute() {
        if candidate.is_file() {
            candidate
        } else {
            return None;
        }
    } else {
        which::which(&raw)
            .or_else(|_| which::which(adapter.executable()))
            .ok()?
    };
    Some((resolved, adapter))
}

/// Sistemde bulunan terminal adapterini ve yolunu döner.
///
/// `preferred` verilmişse yalnızca o adaptör aranır:
/// - desteklenmeyen bir ad → açık hata ("bu sürümde desteklenmiyor"),
/// - desteklenen ama kurulu olmayan ad → açık hata (sessizce başka
///   emülatöre düşmek, kullanıcının tercihini yok saymak olurdu).
///
/// `preferred` boşsa sıra: `$TERMINAL` (biliniyorsa) → [`TERMINALS`].
pub fn select_adapter(
    preferred: Option<&str>,
) -> Result<(PathBuf, &'static dyn TerminalAdapter), NovaError> {
    if let Some(name) = preferred.map(str::trim).filter(|n| !n.is_empty()) {
        let adapter = find_adapter_by_name(name).ok_or_else(|| {
            NovaError::Launcher(format!(
                "Tercih edilen terminal '{name}' bu sürümde desteklenmiyor \
                 (desteklenen: {}).",
                supported_executables_list()
            ))
        })?;
        return which::which(adapter.executable())
            .map(|p| (p, adapter))
            .map_err(|_| {
                NovaError::Launcher(format!(
                    "Tercih edilen terminal '{}' sistemde bulunamadı; ayarlardan \
                     tercihi değiştir veya kur.",
                    adapter.executable()
                ))
            });
    }

    let mut tried = Vec::new();

    if let Some((path, adapter)) = resolve_terminal_env() {
        return Ok((path, adapter));
    }
    if std::env::var_os("TERMINAL").is_some_and(|v| !v.is_empty()) {
        tried.push("$TERMINAL");
    }

    for adapter in TERMINALS {
        tried.push(adapter.executable());
        if let Ok(path) = which::which(adapter.executable()) {
            return Ok((path, *adapter));
        }
    }
    Err(NovaError::Launcher(format!(
        "Desteklenen bir terminal emülatörü bulunamadı (aranan: {}). \
         Yeni sekmede başlatma için bunlardan birini kur.",
        tried.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_request() -> LaunchRequest {
        LaunchRequest {
            program: PathBuf::from("/usr/bin/lazygit"),
            args: vec!["--path".into(), "/tmp/repo".into()],
            working_directory: PathBuf::from("/home/kullanici/git/depo"),
            stdin_data: None,
            stdin_file_path: None,
        }
    }

    #[test]
    fn konsole_uses_new_tab_and_workdir() {
        let args = Konsole.build_args(Path::new("/usr/bin/konsole"), &sample_request());
        assert_eq!(args[0], OsString::from("--new-tab"));
        assert_eq!(args[1], OsString::from("--workdir"));
        assert_eq!(args[2], OsString::from("/home/kullanici/git/depo"));
    }

    #[test]
    fn konsole_dash_e_is_last_option_then_program() {
        // Konsole `-e` seçeneği tüm izleyen argümanları yakalar; bu yüzden
        // `-e` son seçenek olmak zorundadır.
        let args = Konsole.build_args(Path::new("/usr/bin/konsole"), &sample_request());
        let e_index = args.iter().position(|a| a == "-e").expect("-e bulunmalı");
        assert_eq!(
            e_index,
            args.len() - 1 - (1 + sample_request().args.len()),
            "-e'den sonra yalnızca program ve argümanları gelmeli"
        );
        assert_eq!(args[e_index + 1], OsString::from("/usr/bin/lazygit"));
        for arg in &args[e_index + 1..] {
            assert!(
                !arg.to_string_lossy().starts_with('-') || *arg == "--path",
                "-e'den sonra terminal seçeneği görünümlü argüman yalnızca programın kendisi olabilir: {arg:?}"
            );
        }
    }

    #[test]
    fn konsole_passes_program_args_verbatim() {
        let req = LaunchRequest {
            program: PathBuf::from("/usr/bin/printf"),
            args: vec!["a b".into(), "$HOME".into(), ";rm -rf /".into(), "".into()],
            working_directory: PathBuf::from("/tmp"),
            stdin_data: None,
            stdin_file_path: None,
        };
        let args = Konsole.build_args(Path::new("/usr/bin/konsole"), &req);
        let tail: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            &tail[tail.len() - 4..],
            &["a b", "$HOME", ";rm -rf /", ""],
            "argümanlar değiştirilmeden, ayrı ayrı geçmeli"
        );
    }

    #[test]
    fn argument_with_leading_dash_is_not_mistaken_for_terminal_option() {
        // `-e`'den sonraki her şey programın argümanıdır; Konsole bunları
        // kendi seçeneği sanmaz.
        let req = LaunchRequest {
            program: PathBuf::from("/usr/bin/tool"),
            args: vec!["--workdir".into(), "--new-tab".into()],
            working_directory: PathBuf::from("/tmp"),
            stdin_data: None,
            stdin_file_path: None,
        };
        let args = Konsole.build_args(Path::new("/usr/bin/konsole"), &req);
        let count_workdir = args.iter().filter(|a| *a == "--workdir").count();
        assert_eq!(
            count_workdir, 2,
            "biri Konsole seçeneği, biri araç argümanı"
        );
    }

    #[test]
    fn no_shell_is_inserted_into_konsole_args() {
        let args = Konsole.build_args(Path::new("/usr/bin/konsole"), &sample_request());
        for forbidden in ["sh", "bash", "-c", "/bin/sh", "/bin/bash"] {
            assert!(
                !args.iter().any(|a| a == forbidden),
                "komut satırında kabuk olmamalı: {forbidden}"
            );
        }
    }

    #[test]
    fn stdin_data_is_passed_as_a_file_not_as_text() {
        let mut req = sample_request();
        req.set_stdin(b"uzun bir kullanim metni".to_vec());
        let path = req.write_stdin_file().unwrap().expect("dosya yazılmalı");
        assert_eq!(std::fs::read(&path).unwrap(), b"uzun bir kullanim metni");

        let args = Konsole.build_args(Path::new("/usr/bin/konsole"), &req);
        let last = args.last().unwrap().to_string_lossy().into_owned();
        assert_eq!(
            last,
            path.to_string_lossy(),
            "dosya yolu son argüman olmalı"
        );
        assert!(
            !args
                .iter()
                .any(|a| a.to_string_lossy().contains("kullanim metni")),
            "metin komut satırına konmamalı"
        );
        let _ = crate::launcher::request::remove_private_help_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn stdin_file_is_private_and_unique() {
        use std::os::unix::fs::PermissionsExt;

        let mut first = sample_request();
        first.set_stdin(b"first".to_vec());
        let first_path = first.write_stdin_file().unwrap().unwrap();
        let mode = std::fs::metadata(&first_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode & 0o077, 0, "yardım dosyası başkalarına kapalı olmalı");
        let directory_mode = std::fs::metadata(first_path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            directory_mode & 0o077,
            0,
            "yardım klasörü başkalarına kapalı olmalı"
        );

        let mut second = sample_request();
        second.set_stdin(b"second".to_vec());
        let second_path = second.write_stdin_file().unwrap().unwrap();
        assert_ne!(first_path, second_path, "her istek ayrı dosya kullanmalı");

        let _ = crate::launcher::request::remove_private_help_file(&first_path);
        let _ = crate::launcher::request::remove_private_help_file(&second_path);
    }

    #[test]
    fn konsole_reports_its_names() {
        assert_eq!(Konsole.name(), "Konsole");
        assert_eq!(Konsole.executable(), "konsole");
    }

    #[test]
    fn terminals_list_prefers_documented_order() {
        let names: Vec<&str> = TERMINALS.iter().map(|a| a.executable()).collect();
        assert_eq!(
            names,
            vec![
                "xdg-terminal-exec",
                "kitty",
                "alacritty",
                "wezterm",
                "gnome-terminal",
                "konsole",
                "foot",
                "xterm",
                "tmux",
            ]
        );
    }

    #[test]
    fn kitty_uses_directory_and_no_shell() {
        let args = Kitty.build_args(Path::new("/usr/bin/kitty"), &sample_request());
        assert_eq!(
            args,
            vec![
                OsString::from("--detach"),
                OsString::from("--directory"),
                OsString::from("/home/kullanici/git/depo"),
                OsString::from("/usr/bin/lazygit"),
                OsString::from("--path"),
                OsString::from("/tmp/repo"),
            ]
        );
        assert!(!args.iter().any(|a| a == "-c" || a == "sh"));
    }

    #[test]
    fn alacritty_wezterm_gnome_foot_xterm_tmux_arg_shapes() {
        let req = sample_request();
        let alacritty = Alacritty.build_args(Path::new("/usr/bin/alacritty"), &req);
        assert_eq!(alacritty[0], OsString::from("--working-directory"));
        assert_eq!(alacritty[2], OsString::from("-e"));

        let wez = Wezterm.build_args(Path::new("/usr/bin/wezterm"), &req);
        assert_eq!(wez[0], OsString::from("start"));
        assert_eq!(wez[1], OsString::from("--cwd"));
        assert_eq!(wez[3], OsString::from("--"));

        let gnome = GnomeTerminal.build_args(Path::new("/usr/bin/gnome-terminal"), &req);
        assert!(gnome[0]
            .to_string_lossy()
            .starts_with("--working-directory="));
        assert_eq!(gnome[1], OsString::from("--"));

        let foot = Foot.build_args(Path::new("/usr/bin/foot"), &req);
        assert_eq!(foot[0], OsString::from("-D"));

        let xterm = Xterm.build_args(Path::new("/usr/bin/xterm"), &req);
        assert_eq!(xterm[0], OsString::from("-e"));
        assert_eq!(xterm[1], OsString::from("/usr/bin/lazygit"));

        let tmux = Tmux.build_args(Path::new("/usr/bin/tmux"), &req);
        assert_eq!(tmux[0], OsString::from("new-window"));
        assert_eq!(tmux[1], OsString::from("-c"));

        let xdg = XdgTerminalExec.build_args(Path::new("/usr/bin/xdg-terminal-exec"), &req);
        assert!(xdg[0].to_string_lossy().starts_with("--dir="));
        assert_eq!(xdg[1], OsString::from("--"));
    }

    #[test]
    fn find_adapter_accepts_name_or_executable() {
        assert_eq!(
            find_adapter_by_name("Kitty").map(|a| a.executable()),
            Some("kitty")
        );
        assert_eq!(
            find_adapter_by_name("gnome-terminal").map(|a| a.name()),
            Some("GNOME Terminal")
        );
        assert!(find_adapter_by_name("st").is_none());
    }

    #[test]
    fn unknown_env_terminal_is_ignored_without_matching_adapter() {
        // resolve_terminal_env yalnızca bilinen basename kabul eder.
        assert!(find_adapter_by_name("urxvt").is_none());
        assert!(find_adapter_by_name("st").is_none());
    }
}
