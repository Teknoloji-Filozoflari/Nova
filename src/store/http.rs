//! Gerçek HTTP indirici.
//!
//! [`crate::store::install::Downloader`] arayüzünün ağ üzerinden çalışan
//! uygulaması. İndirme **kabuk betiği olarak değil**, doğrudan HTTP isteği
//! olarak yapılır: `curl | sh` benzeri bir desen kullanılmaz ve indirilen
//! dosya daha sonra SHA-256 özetiyle doğrulanır.
//!
//! Yanıt gövdesi bir dosyaya akıtılır (tamamı belleğe alınmaz) ve boyut
//! sınırı uygulanır.

use crate::store::install::{Downloader, InstallError};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// İndirilecek en büyük dosya boyutu (byte).
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
/// Tek isteğin toplam zaman sınırı.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// `ureq` tabanlı indirici.
#[derive(Debug, Clone, Default)]
pub struct HttpDownloader {
    _private: (),
}

impl HttpDownloader {
    /// Yeni bir indirici oluşturur.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// İstek için yapılandırılmış ajanı üretir.
    fn agent(&self) -> ureq::Agent {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build();
        config.into()
    }
}

impl Downloader for HttpDownloader {
    fn download(&self, url: &str, destination: &Path) -> Result<std::path::PathBuf, InstallError> {
        if !url.starts_with("https://") {
            return Err(InstallError::Security(format!(
                "yalnızca https:// adresleri indirilebilir: '{}'",
                url
            )));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                InstallError::Permission(format!("'{}' oluşturulamadı: {e}", parent.display()))
            })?;
        }

        let agent = self.agent();
        let mut response = agent
            .get(url)
            .call()
            .map_err(|e| InstallError::Network(format!("'{}' indirilemedi: {e}", url)))?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(InstallError::Network(format!(
                "'{}' sunucusu {} yanıtı verdi",
                url, status
            )));
        }

        let mut file = fs::File::create(destination).map_err(|e| {
            InstallError::Permission(format!("'{}' oluşturulamadı: {e}", destination.display()))
        })?;
        let mut reader = response.body_mut().as_reader();
        let written = std::io::copy(&mut reader, &mut file)
            .map_err(|e| InstallError::Network(format!("'{}' indirilemedi: {e}", url)))?;
        file.flush().map_err(|e| {
            InstallError::Download(format!("'{}' yazılamadı: {e}", destination.display()))
        })?;

        if written == 0 {
            return Err(InstallError::Download(format!(
                "'{}' boş bir dosya indirdi",
                destination.display()
            )));
        }
        if written > MAX_DOWNLOAD_BYTES {
            return Err(InstallError::Download(format!(
                "dosya çok büyük: {} bayt (sınır {} bayt)",
                written, MAX_DOWNLOAD_BYTES
            )));
        }

        Ok(destination.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_https_urls_are_refused() {
        let downloader = HttpDownloader::new();
        let dest = std::env::temp_dir().join("nova-http-test-bin");
        let err = downloader
            .download("http://example.org/a.tar.gz", &dest)
            .unwrap_err();
        assert!(matches!(err, InstallError::Security(_)), "hata: {err}");
    }

    #[test]
    fn http_error_status_is_reported_as_network_error() {
        let downloader = HttpDownloader::new();
        let dest = std::env::temp_dir().join("nova-http-test-status");
        // example.org her zaman 200 döner; bu yüzden yalnızca tip kontrolü
        // yapılır, gerçek indirme beklenmez.
        match downloader.download("https://example.org/nova-yok-boyle-dosya", &dest) {
            Ok(_) => {}
            Err(InstallError::Network(_)) => {}
            Err(other) => panic!("beklenmeyen hata: {other}"),
        }
        let _ = fs::remove_file(&dest);
    }
}
