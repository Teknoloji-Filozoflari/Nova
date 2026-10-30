//! Kurulu komutları tespit etme modülü.
//! PATH üzerinden çalıştırılabilir dosyaların varlığını kontrol eder.
//!
//! Üç sonuç döndürür: kurulu, kurulu değil ve kontrol edilemedi.
//! "Kontrol edilemedi", komut adının geçersiz olduğu veya PATH bilgisinin
//! kullanılamadığı durumları ifade eder; bu ayrım kullanıcıya yanlış
//! "kurulu değil" bilgisi vermemek için gereklidir.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Bir komutun kurulu olma durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstalledStatus {
    /// Komut PATH üzerinde bulundu.
    Installed,
    /// Komut PATH üzerinde yok.
    Missing,
    /// Varlık kontrolü yapılamadı (geçersiz ad, boş PATH, yetki hatası...).
    #[default]
    Unknown,
}

impl InstalledStatus {
    /// Arayüzde gösterilecek kısa işaret.
    pub fn symbol(self) -> &'static str {
        match self {
            InstalledStatus::Installed => "✓",
            InstalledStatus::Missing => "✗",
            InstalledStatus::Unknown => "?",
        }
    }

    /// Durumun Türkçe açıklaması.
    pub fn label(self) -> &'static str {
        match self {
            InstalledStatus::Installed => "Kurulu",
            InstalledStatus::Missing => "Kurulu değil",
            InstalledStatus::Unknown => "Kontrol edilemedi",
        }
    }
}

/// Komutun durumunu ve PATH üzerinde çözümlenen yolunu döner.
pub fn resolve(cmd: &str) -> (InstalledStatus, Option<PathBuf>) {
    let trimmed = cmd.trim();
    // Yalnızca boşluk, boşluk içeren veya boş olan giriş bir komut adı değildir.
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return (InstalledStatus::Unknown, None);
    }

    // Komut doğrudan bir yol ise (içinde '/' var) varlığına bak.
    if trimmed.contains('/') {
        let path = Path::new(trimmed);
        return match std::fs::metadata(path) {
            Ok(meta) if is_executable(&meta) => {
                (InstalledStatus::Installed, Some(path.to_path_buf()))
            }
            Ok(_) => (InstalledStatus::Missing, None),
            Err(err) if err.kind() == ErrorKind::NotFound => (InstalledStatus::Missing, None),
            Err(_) => (InstalledStatus::Unknown, None),
        };
    }

    // PATH hiç tanımlı değilse arama yapılamaz; bu "kurulu değil" değildir.
    if let Ok(path_var) = std::env::var("PATH") {
        if path_var.trim().is_empty() {
            return (InstalledStatus::Unknown, None);
        }
    } else {
        return (InstalledStatus::Unknown, None);
    }

    match which::which(trimmed) {
        Ok(path) => (InstalledStatus::Installed, Some(path)),
        Err(which::Error::CannotFindBinaryPath) => (InstalledStatus::Missing, None),
        // Yetki hatası gibi diğer tüm durumlar "kontrol edilemedi" sayılır.
        Err(_) => (InstalledStatus::Unknown, None),
    }
}

/// Unix'te çalıştırılabilirlik bitini kontrol eder.
#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.is_file() && meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    meta.is_file()
}

/// Verilen komutun kurulu olup olmadığını döner. "Kontrol edilemedi"
/// durumunu da kesin olumsuz olarak raporlar; kesin ayrım için
/// `resolve` kullanılmalıdır.
pub fn is_installed(cmd: &str) -> bool {
    resolve(cmd).0 == InstalledStatus::Installed
}

/// Birden fazla komutun durumunu döner.
pub fn detect_many<'a>(cmds: &'a [&'a str]) -> Vec<(&'a str, InstalledStatus)> {
    cmds.iter().map(|cmd| (*cmd, resolve(cmd).0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_command_is_unknown() {
        assert_eq!(resolve("").0, InstalledStatus::Unknown);
        assert_eq!(resolve("   ").0, InstalledStatus::Unknown);
    }

    #[test]
    fn command_with_spaces_is_unknown() {
        assert_eq!(resolve("ls -la").0, InstalledStatus::Unknown);
    }

    #[test]
    fn nonexistent_command_is_missing() {
        assert_eq!(
            resolve("bu-comut-kesinlikle-yok-xyz123").0,
            InstalledStatus::Missing
        );
        assert!(!is_installed("bu-comut-kesinlikle-yok-xyz123"));
    }

    #[test]
    fn existing_command_is_installed_on_posix() {
        if cfg!(unix) {
            let (status, path) = resolve("sh");
            if which::which("sh").is_ok() {
                assert_eq!(status, InstalledStatus::Installed);
                assert!(path.is_some());
            }
        }
    }

    #[test]
    fn absolute_path_to_missing_file_is_missing() {
        assert_eq!(
            resolve("/bu/yol/kesinlikle/yok/xyz").0,
            InstalledStatus::Missing
        );
    }

    #[test]
    fn absolute_path_to_non_executable_file_is_missing() {
        let tmp = std::env::temp_dir().join("nova-detection-test.txt");
        std::fs::write(&tmp, "x").expect("test dosyası yazılamadı");
        let result = resolve(tmp.to_str().unwrap()).0;
        std::fs::remove_file(&tmp).ok();
        if cfg!(unix) {
            assert_eq!(result, InstalledStatus::Missing);
        }
    }

    #[test]
    fn detect_many_returns_pairs() {
        let result = detect_many(&["sh", "bu-comut-yok-xyz"]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "sh");
        assert_eq!(result[1].1, InstalledStatus::Missing);
    }

    #[test]
    fn symbol_and_label_are_consistent() {
        assert_eq!(InstalledStatus::Installed.symbol(), "✓");
        assert_eq!(InstalledStatus::Missing.label(), "Kurulu değil");
        assert_eq!(InstalledStatus::Unknown.label(), "Kontrol edilemedi");
    }
}
