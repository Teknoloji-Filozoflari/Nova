//! Mağaza ağ güvenlik politikası.
//!
//! İndirme ve API istekleri ortak kurallara uyar:
//! - yalnızca HTTPS (`https_only`)
//! - sınırlı yönlendirme sayısı
//! - yanıt gövdesi boyut üst sınırı

use std::time::Duration;

/// İndirilecek en büyük dosya boyutu (byte).
pub const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
/// Metin (checksum / imza / API) yanıtları için üst sınır.
pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
/// İzlenecek en fazla HTTPS yönlendirmesi.
pub const MAX_REDIRECTS: u32 = 3;
/// Tek isteğin toplam zaman sınırı.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Mağaza istekleri için sertleştirilmiş `ureq` ajanı.
pub fn secure_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .https_only(true)
        .max_redirects(MAX_REDIRECTS)
        .max_redirects_will_error(true)
        .build()
        .into()
}

/// Adresin `https://` ile başladığını doğrular.
pub fn require_https(url: &str) -> Result<(), String> {
    if url.starts_with("https://") {
        Ok(())
    } else {
        Err(format!(
            "yalnızca https:// adresleri kullanılabilir: '{url}'"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_https_accepts_only_https() {
        assert!(require_https("https://example.org/a").is_ok());
        assert!(require_https("http://example.org/a").is_err());
        assert!(require_https("ftp://example.org/a").is_err());
    }

    #[test]
    fn redirect_and_size_caps_are_finite() {
        const {
            assert!(MAX_REDIRECTS > 0 && MAX_REDIRECTS <= 10);
            assert!(MAX_DOWNLOAD_BYTES > 0);
            assert!(MAX_TEXT_BYTES > 0 && MAX_TEXT_BYTES < MAX_DOWNLOAD_BYTES);
        }
    }
}
