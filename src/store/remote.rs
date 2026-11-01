//! İmzalı uzak Nova-Store kataloğu.
//!
//! Güvenlik modeli ([`docs/nova-store-architecture`](../../../docs) ile uyumlu):
//!
//! 1. Uzak katalog **yalnızca** gömülü minisign genel anahtarıyla imza
//!    doğrulanırsa kabul edilir.
//! 2. Doğrulama başarısız / ağ hatası / 404 → son güvenilen yerel kopya
//!    (veya gömülü `assets/store`) korunur; bozuk uzak veri uygulanmaz.
//! 3. Başarılı doğrulamada katalog `trusted-store.toml` + `.minisig` olarak
//!    diske yazılır; sonraki açılışlarda bu kopya yeniden doğrulanır.
//!
//! Uzak adresler Nova-Store GitHub Releases’a stub’lanmıştır; yayın yokken
//! istekler yumuşak başarısız olur. `NOVA_STORE_CATALOG_URL` /
//! `NOVA_STORE_CATALOG_SIG_URL` ile geçersiz kılınabilir.
//! `NOVA_STORE_CATALOG_DISABLE_REMOTE=1` uzak denemeyi kapatır.

use crate::store::loader::{self, load_builtin_store};
use crate::store::net::{self, MAX_TEXT_BYTES};
use crate::store::{store_install_dir, Store, StoreFile};
use minisign_verify::{PublicKey, Signature};
use std::fs;
use std::path::{Path, PathBuf};

/// Nova-Store katalog minisign genel anahtarı (base64, `RW…`).
///
/// Yayınlar `Teknoloji-Filozoflari/Nova-Store` Releases altında bu anahtarla
/// imzalanmalıdır. Anahtar döndürme Nova sürümüyle gelir.
pub const NOVA_STORE_CATALOG_PUBLIC_KEY: &str =
    "RWTq0P6AM5pR0HlqD09S6QW1Yf0X5sIWRah5Q1UQDMJFBxzzI/GuCNpg";

/// Varsayılan katalog varlık adı (GitHub Release asset).
pub const DEFAULT_CATALOG_ASSET: &str = "store-catalog.toml";
/// Varsayılan imza varlık adı.
pub const DEFAULT_CATALOG_SIG_ASSET: &str = "store-catalog.toml.minisig";

/// Nova-Store Releases stub kökü.
pub const DEFAULT_RELEASES_BASE: &str =
    "https://github.com/Teknoloji-Filozoflari/Nova-Store/releases/latest/download";

/// Katalog yenileme sonucu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogRefresh {
    /// Uzak katalog doğrulandı ve uygulandı.
    Updated { entries: usize },
    /// Uzak deneme atlandı veya başarısız; mevcut güvenilen katalog korundu.
    KeptTrusted { reason: String },
}

/// Son güvenilen katalog dosya yolları (`<store>/catalog/`).
#[derive(Debug, Clone)]
pub struct TrustedCatalogPaths {
    /// Katalog TOML.
    pub catalog: PathBuf,
    /// Minisign imza dosyası.
    pub signature: PathBuf,
}

impl TrustedCatalogPaths {
    /// Verilen mağaza kökünden yolları üretir.
    pub fn under(store_root: &Path) -> Self {
        let dir = store_root.join("catalog");
        Self {
            catalog: dir.join("trusted-store.toml"),
            signature: dir.join("trusted-store.toml.minisig"),
        }
    }

    /// Kullanıcı mağaza dizininden (yoksa `None`).
    pub fn default_paths() -> Option<Self> {
        Some(Self::under(&store_install_dir()?))
    }
}

/// Varsayılan uzak katalog ve imza URL’leri (ortam değişkenleriyle override).
pub fn remote_catalog_urls() -> (String, String) {
    let catalog = std::env::var("NOVA_STORE_CATALOG_URL")
        .unwrap_or_else(|_| format!("{DEFAULT_RELEASES_BASE}/{DEFAULT_CATALOG_ASSET}"));
    let signature = std::env::var("NOVA_STORE_CATALOG_SIG_URL")
        .unwrap_or_else(|_| format!("{DEFAULT_RELEASES_BASE}/{DEFAULT_CATALOG_SIG_ASSET}"));
    (catalog, signature)
}

/// Uzak katalog denemesi kapalı mı?
pub fn remote_fetch_disabled() -> bool {
    matches!(
        std::env::var("NOVA_STORE_CATALOG_DISABLE_REMOTE")
            .unwrap_or_default()
            .as_str(),
        "1" | "true" | "TRUE" | "yes" | "YES"
    )
}

/// Gömülü Nova-Store genel anahtarını yükler.
pub fn catalog_public_key() -> Result<PublicKey, String> {
    PublicKey::from_base64(NOVA_STORE_CATALOG_PUBLIC_KEY)
        .map_err(|e| format!("gömülü Nova-Store genel anahtarı geçersiz: {e}"))
}

/// Katalog baytlarını minisign imzasıyla doğrular.
pub fn verify_catalog_signature(catalog: &[u8], signature_text: &str) -> Result<(), String> {
    let pk = catalog_public_key()?;
    let signature =
        Signature::decode(signature_text).map_err(|e| format!("katalog imzası çözülemedi: {e}"))?;
    pk.verify(catalog, &signature, false)
        .map_err(|e| format!("katalog imzası doğrulanamadı: {e}"))
}

/// TOML baytlarını ayrıştırıp [`Store`] doğrular.
pub fn parse_catalog_toml(bytes: &[u8]) -> Result<Store, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("katalog UTF-8 değil: {e}"))?;
    let file: StoreFile =
        toml::from_str(text).map_err(|e| format!("katalog TOML geçersiz: {e}"))?;
    Store::from_entries(file.apps)
}

/// Katalog / imza metinleri için daha kısa zaman sınırı.
const CATALOG_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// HTTPS metin indirir (boyut sınırlı, kısa zaman aşımı).
pub fn fetch_https_text(url: &str) -> Result<String, String> {
    net::require_https(url)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(CATALOG_FETCH_TIMEOUT))
        .https_only(true)
        .max_redirects(net::MAX_REDIRECTS)
        .max_redirects_will_error(true)
        .build()
        .into();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| format!("'{url}' indirilemedi: {e}"))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("'{url}' sunucusu {status} yanıtı verdi"));
    }
    if let Some(len) = response.body().content_length() {
        if len > MAX_TEXT_BYTES {
            return Err(format!(
                "'{url}' çok büyük: Content-Length {len} (sınır {MAX_TEXT_BYTES})"
            ));
        }
    }
    response
        .body_mut()
        .with_config()
        .limit(MAX_TEXT_BYTES)
        .read_to_string()
        .map_err(|e| format!("'{url}' okunamadı: {e}"))
}

/// Diskteki son güvenilen kataloğu imza doğrulayarak yükler.
pub fn load_last_trusted(paths: &TrustedCatalogPaths) -> Result<Store, String> {
    let catalog = fs::read(&paths.catalog).map_err(|e| {
        format!(
            "güvenilen katalog okunamadı ({}): {e}",
            paths.catalog.display()
        )
    })?;
    let sig = fs::read_to_string(&paths.signature).map_err(|e| {
        format!(
            "güvenilen imza okunamadı ({}): {e}",
            paths.signature.display()
        )
    })?;
    verify_catalog_signature(&catalog, &sig)?;
    parse_catalog_toml(&catalog)
}

/// Doğrulanmış katalog + imzayı diske yazar.
pub fn save_last_trusted(
    paths: &TrustedCatalogPaths,
    catalog: &[u8],
    signature_text: &str,
) -> Result<(), String> {
    verify_catalog_signature(catalog, signature_text)?;
    if let Some(parent) = paths.catalog.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("katalog dizini oluşturulamadı ({}): {e}", parent.display()))?;
    }
    fs::write(&paths.catalog, catalog).map_err(|e| {
        format!(
            "güvenilen katalog yazılamadı ({}): {e}",
            paths.catalog.display()
        )
    })?;
    fs::write(&paths.signature, signature_text.as_bytes()).map_err(|e| {
        format!(
            "güvenilen imza yazılamadı ({}): {e}",
            paths.signature.display()
        )
    })?;
    Ok(())
}

/// Açılış için güvenilen mağaza: son doğrulanmış yerel kopya, yoksa gömülü.
///
/// Ağ çağrısı **yapmaz**. Uzak yenileme için [`try_refresh_from_remote`].
pub fn load_trusted_store() -> Store {
    if let Some(paths) = TrustedCatalogPaths::default_paths() {
        if let Ok(store) = load_last_trusted(&paths) {
            return store;
        }
    }
    load_builtin_store().unwrap_or_default()
}

/// Uzak Nova-Store kataloğunu indirip imza doğrular; başarılıysa kaydeder.
///
/// Başarısızlıkta mevcut güvenilen katalog **değiştirilmez**.
pub fn try_refresh_from_remote(
    paths: Option<&TrustedCatalogPaths>,
) -> Result<(Store, CatalogRefresh), String> {
    if remote_fetch_disabled() {
        return Err("uzak katalog denemesi kapalı (NOVA_STORE_CATALOG_DISABLE_REMOTE)".into());
    }
    // Birim testlerde (App `U` yolu) ağa çıkma; uzak fetch
    // `accept_signed_catalog_bytes` / `fetch_https_text` ile ayrı test edilir.
    #[cfg(test)]
    if std::env::var_os("NOVA_STORE_CATALOG_ALLOW_REMOTE_IN_TESTS").is_none() {
        return Err("test: uzak katalog atlandı".into());
    }
    let (catalog_url, sig_url) = remote_catalog_urls();
    let catalog_text = fetch_https_text(&catalog_url)?;
    let signature_text = fetch_https_text(&sig_url)?;
    verify_catalog_signature(catalog_text.as_bytes(), &signature_text)?;
    let store = parse_catalog_toml(catalog_text.as_bytes())?;
    if let Some(paths) = paths {
        save_last_trusted(paths, catalog_text.as_bytes(), &signature_text)?;
    } else if let Some(paths) = TrustedCatalogPaths::default_paths() {
        let _ = save_last_trusted(&paths, catalog_text.as_bytes(), &signature_text);
    }
    let n = store.entries().len();
    Ok((store, CatalogRefresh::Updated { entries: n }))
}

/// Uzak yenilemeyi dener; hata olursa gömülü/son güvenileni koruyan özet döner.
pub fn refresh_or_keep_trusted(current: &Store) -> (Store, CatalogRefresh) {
    let paths = TrustedCatalogPaths::default_paths();
    match try_refresh_from_remote(paths.as_ref()) {
        Ok((store, outcome)) => (store, outcome),
        Err(reason) => (current.clone(), CatalogRefresh::KeptTrusted { reason }),
    }
}

/// Bellekteki baytlardan (test / özel kaynak) doğrula + isteğe bağlı kaydet.
pub fn accept_signed_catalog_bytes(
    catalog: &[u8],
    signature_text: &str,
    paths: Option<&TrustedCatalogPaths>,
) -> Result<Store, String> {
    verify_catalog_signature(catalog, signature_text)?;
    let store = parse_catalog_toml(catalog)?;
    if let Some(paths) = paths {
        save_last_trusted(paths, catalog, signature_text)?;
    }
    Ok(store)
}

/// Gömülü kaynakların birleşik TOML metnini üretir (yerel “güvenilen” tohum).
///
/// Uzak yayın yokken gömülü kataloğu imzalamak Nova-Store işidir; bu yardımcı
/// yalnızca testlerde birleşik gövde isterken kullanılır.
pub fn builtin_catalog_toml_for_tests() -> String {
    let mut out = String::from("# Nova builtin store aggregate\n\n");
    for (name, text) in loader::BUILTIN_STORE_SOURCES {
        out.push_str(&format!("# --- {name} ---\n"));
        out.push_str(text);
        if !text.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nova-store")
    }

    fn fixture_catalog() -> Vec<u8> {
        fs::read(fixture_dir().join("store-catalog.toml")).expect("fixture catalog")
    }

    fn fixture_sig() -> String {
        fs::read_to_string(fixture_dir().join("store-catalog.toml.minisig")).expect("fixture sig")
    }

    #[test]
    fn embedded_public_key_parses() {
        catalog_public_key().expect("pubkey");
    }

    #[test]
    fn fixture_signature_verifies() {
        verify_catalog_signature(&fixture_catalog(), &fixture_sig()).expect("verify");
        let store = parse_catalog_toml(&fixture_catalog()).expect("parse");
        assert_eq!(store.entries().len(), 4);
    }

    #[test]
    fn tampered_catalog_is_rejected() {
        let mut bytes = fixture_catalog();
        bytes[0] ^= 0x7f;
        let err = verify_catalog_signature(&bytes, &fixture_sig()).unwrap_err();
        assert!(
            err.contains("doğrulanamadı") || err.to_lowercase().contains("verif"),
            "{err}"
        );
    }

    #[test]
    fn last_trusted_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "nova-trusted-catalog-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = TrustedCatalogPaths::under(&dir);
        let catalog = fixture_catalog();
        let sig = fixture_sig();
        save_last_trusted(&paths, &catalog, &sig).expect("save");
        let store = load_last_trusted(&paths).expect("load");
        assert_eq!(store.entries().len(), 4);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn accept_signed_rejects_bad_sig_without_writing() {
        let dir = std::env::temp_dir().join(format!(
            "nova-trusted-bad-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = TrustedCatalogPaths::under(&dir);
        let err = accept_signed_catalog_bytes(b"not-a-catalog", &fixture_sig(), Some(&paths))
            .unwrap_err();
        assert!(!paths.catalog.exists(), "bozuk katalog yazılmamalı: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remote_urls_honour_env_override() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("NOVA_STORE_CATALOG_URL", "https://example.org/c.toml");
        std::env::set_var(
            "NOVA_STORE_CATALOG_SIG_URL",
            "https://example.org/c.minisig",
        );
        let (c, s) = remote_catalog_urls();
        assert_eq!(c, "https://example.org/c.toml");
        assert_eq!(s, "https://example.org/c.minisig");
        std::env::remove_var("NOVA_STORE_CATALOG_URL");
        std::env::remove_var("NOVA_STORE_CATALOG_SIG_URL");
        let (c, s) = remote_catalog_urls();
        assert!(c.contains("Nova-Store"));
        assert!(s.ends_with(".minisig"));
    }

    #[test]
    fn refresh_disabled_by_env() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("NOVA_STORE_CATALOG_DISABLE_REMOTE", "1");
        let err = try_refresh_from_remote(None).unwrap_err();
        assert!(err.contains("kapalı"), "{err}");
        std::env::remove_var("NOVA_STORE_CATALOG_DISABLE_REMOTE");
    }

    #[test]
    fn require_https_on_fetch() {
        let err = fetch_https_text("http://example.org/x").unwrap_err();
        assert!(err.contains("https"), "{err}");
    }
}
