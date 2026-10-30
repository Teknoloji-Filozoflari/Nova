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
            request.program.as_os_str().to_owned(),
        ];
        args.extend(request.args.iter().cloned());
        // Konsole, `-e` ile başlattığı programın stdin'ini Nova'dan alamaz.
        // Sabit metin (kullanım bilgisi) özel izinli geçici dosyaya yazılır
        // ve sayfalayıcıya **dosya adı** olarak verilir; kabuk veya
        // yönlendirme kullanılmaz. Çağıran dosyayı önceden oluşturmalıdır.
        if let Some(path) = request.stdin_path() {
            args.push(path.as_os_str().to_owned());
        }
        args
    }
}

/// Bilinen emülatörler, tercih sırasına göre.
pub const TERMINALS: &[&dyn TerminalAdapter] = &[&Konsole];

/// Sistemde bulunan terminal adapterini ve yolunu döner.
///
/// `preferred` verilmişse yalnızca o adaptör aranır:
/// - desteklenmeyen bir ad → açık hata ("bu sürümde desteklenmiyor"),
/// - desteklenen ama kurulu olmayan ad → açık hata (sessizce başka
///   emülatöre düşmek, kullanıcının tercihini yok saymak olurdu).
pub fn select_adapter(
    preferred: Option<&str>,
) -> Result<(PathBuf, &'static dyn TerminalAdapter), NovaError> {
    if let Some(name) = preferred.map(str::trim).filter(|n| !n.is_empty()) {
        let adapter = TERMINALS
            .iter()
            .find(|a| {
                a.executable().eq_ignore_ascii_case(name) || a.name().eq_ignore_ascii_case(name)
            })
            .ok_or_else(|| {
                NovaError::Launcher(format!(
                    "Tercih edilen terminal '{name}' bu sürümde desteklenmiyor \
                     (desteklenen: {}).",
                    TERMINALS
                        .iter()
                        .map(|a| a.executable())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
        return which::which(adapter.executable())
            .map(|p| (p, *adapter))
            .map_err(|_| {
                NovaError::Launcher(format!(
                    "Tercih edilen terminal '{}' sistemde bulunamadı; ayarlardan \
                     tercihi değiştir veya kur.",
                    adapter.executable()
                ))
            });
    }

    let mut tried = Vec::new();
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
        assert_eq!(TERMINALS.len(), 1);
    }
}
