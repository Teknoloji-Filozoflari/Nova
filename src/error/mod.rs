//! Hata türleri ve raporlama modülü.
//! Nova uygulaması üzerinden gelen tüm hatalar bu modül üzerinden temsil edilir.

use std::fmt;

/// Nova uygulamasındaki tüm hata türleri.
#[derive(Debug)]
pub enum NovaError {
    /// Yapılandırma okuma/yazma hatası.
    Config(String),
    /// Katalog hatası.
    Catalog(String),
    /// TUI / terminal hatası.
    Terminal(String),
    /// Uygulama başlatma hatası.
    Launcher(String),
    /// Beklenmeyen iç hata.
    Internal(String),
}

impl fmt::Display for NovaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NovaError::Config(msg) => write!(f, "yapılandırma hatası: {msg}"),
            NovaError::Catalog(msg) => write!(f, "katalog hatası: {msg}"),
            NovaError::Terminal(msg) => write!(f, "terminal hatası: {msg}"),
            NovaError::Launcher(msg) => write!(f, "başlatma hatası: {msg}"),
            NovaError::Internal(msg) => write!(f, "iç hata: {msg}"),
        }
    }
}

impl std::error::Error for NovaError {}

impl From<std::io::Error> for NovaError {
    fn from(err: std::io::Error) -> Self {
        NovaError::Terminal(err.to_string())
    }
}

impl From<toml::de::Error> for NovaError {
    fn from(err: toml::de::Error) -> Self {
        NovaError::Config(err.to_string())
    }
}

impl From<toml::ser::Error> for NovaError {
    fn from(err: toml::ser::Error) -> Self {
        NovaError::Config(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_are_turkish() {
        let err = NovaError::Terminal("ekran bozuldu".into());
        assert_eq!(err.to_string(), "terminal hatası: ekran bozuldu");

        let err = NovaError::Launcher("arac bulunamadi".into());
        assert_eq!(err.to_string(), "başlatma hatası: arac bulunamadi");
    }

    #[test]
    fn error_implements_std_error() {
        fn assert_error<T: std::error::Error>() {}
        assert_error::<NovaError>();
    }
}
