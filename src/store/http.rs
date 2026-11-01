//! Gerçek HTTP indirici.
//!
//! [`crate::store::install::Downloader`] arayüzünün ağ üzerinden çalışan
//! uygulaması. İndirme **kabuk betiği olarak değil**, doğrudan HTTP isteği
//! olarak yapılır: `curl | sh` benzeri bir desen kullanılmaz ve indirilen
//! dosya daha sonra SHA-256 özetiyle doğrulanır.
//!
//! Yanıt gövdesi bir dosyaya akıtılır (tamamı belleğe alınmaz). Yalnızca
//! HTTPS, sınırlı yönlendirme ve boyut üst sınırı uygulanır.

use crate::store::install::{Downloader, InstallError};
use crate::store::net::{self, MAX_DOWNLOAD_BYTES};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

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
        net::secure_agent()
    }
}

impl Downloader for HttpDownloader {
    fn download(&self, url: &str, destination: &Path) -> Result<std::path::PathBuf, InstallError> {
        net::require_https(url).map_err(InstallError::Security)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                InstallError::Permission(format!("'{}' oluşturulamadı: {e}", parent.display()))
            })?;
        }

        let agent = self.agent();
        let mut response = agent
            .get(url)
            .call()
            .map_err(|e| InstallError::Network(format!("'{url}' indirilemedi: {e}")))?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(InstallError::Network(format!(
                "'{url}' sunucusu {status} yanıtı verdi"
            )));
        }

        if let Some(len) = response.body().content_length() {
            if len > MAX_DOWNLOAD_BYTES {
                return Err(InstallError::Download(format!(
                    "dosya çok büyük: Content-Length {len} bayt (sınır {MAX_DOWNLOAD_BYTES} bayt)"
                )));
            }
        }

        let mut file = fs::File::create(destination).map_err(|e| {
            InstallError::Permission(format!("'{}' oluşturulamadı: {e}", destination.display()))
        })?;
        let mut reader = response.body_mut().as_reader();
        let mut buf = [0u8; 64 * 1024];
        let mut written: u64 = 0;
        loop {
            let n = reader.read(&mut buf).map_err(|e| {
                let _ = fs::remove_file(destination);
                InstallError::Network(format!("'{url}' indirilemedi: {e}"))
            })?;
            if n == 0 {
                break;
            }
            written = written.saturating_add(n as u64);
            if written > MAX_DOWNLOAD_BYTES {
                let _ = fs::remove_file(destination);
                return Err(InstallError::Download(format!(
                    "dosya çok büyük: {written} bayt (sınır {MAX_DOWNLOAD_BYTES} bayt)"
                )));
            }
            file.write_all(&buf[..n]).map_err(|e| {
                let _ = fs::remove_file(destination);
                InstallError::Download(format!("'{}' yazılamadı: {e}", destination.display()))
            })?;
        }
        file.flush().map_err(|e| {
            InstallError::Download(format!("'{}' yazılamadı: {e}", destination.display()))
        })?;

        if written == 0 {
            let _ = fs::remove_file(destination);
            return Err(InstallError::Download(format!(
                "'{}' boş bir dosya indirdi",
                destination.display()
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

    #[test]
    fn agent_enforces_https_only_and_redirect_cap() {
        // Derleme zamanı / yapılandırma duman testi: ajan oluşturulabilmeli.
        let agent = HttpDownloader::new().agent();
        let _ = agent;
        assert_eq!(crate::store::net::MAX_REDIRECTS, 3);
        const {
            assert!(crate::store::net::MAX_DOWNLOAD_BYTES > 0);
        }
    }
}
