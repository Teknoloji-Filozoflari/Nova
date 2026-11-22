//! İmzalı uzak Nova-Commands bankası.
//!
//! Güvenlik modeli ([Nova-Commands `docs/catalog-signing.md`][signing]):
//!
//! 1. Uzak banka **yalnızca** gömülü minisign genel anahtarıyla imza
//!    doğrulanırsa kabul edilir (`catalog.json` + `manifest.json` +
//!    `search-index.json` ve eşleşen `.minisig` asset’leri).
//! 2. Doğrulama başarısız / ağ hatası / 404 → son güvenilen yerel kopya
//!    (veya gömülü `assets/commands`) korunur; bozuk uzak veri uygulanmaz.
//! 3. Başarılı doğrulamada üç JSON + imzalar `trusted-*` olarak diske
//!    yazılır; sonraki açılışlarda bu kopya yeniden doğrulanır.
//!
//! Uzak adresler Nova-Commands GitHub Releases’a stub’lanmıştır; yayın yokken
//! istekler yumuşak başarısız olur. `NOVA_COMMANDS_RELEASE_*_URL` ile
//! geçersiz kılınabilir. `NOVA_COMMANDS_DISABLE_REMOTE=1` uzak denemeyi kapatır
//! (imzasız `repo_sync` ile paylaşılır).
//!
//! [signing]: https://github.com/Teknoloji-Filozoflari/Nova-Commands/blob/main/docs/catalog-signing.md

use crate::commands::repo_sync::{
    accept_commands_artifacts, commands_cache_dir, FetchedCommandsBank,
};
use minisign_verify::{PublicKey, Signature};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::store::net::{self, MAX_TEXT_BYTES};

/// Nova-Commands banka minisign genel anahtarı (base64, `RW…`).
///
/// Kaynak: Nova-Commands `keys/minisign.pub` (pin `assets/keys/nova-commands-catalog.pub`).
/// Yayınlar `Teknoloji-Filozoflari/Nova-Commands` Releases (`catalog-v*`) altında
/// bu anahtarla imzalanmalıdır. Anahtar döndürme Nova sürümüyle gelir.
pub const NOVA_COMMANDS_CATALOG_PUBLIC_KEY: &str =
    "RWTo6CiY+npFqZ+AZVwAkLAik1OQrNVhW7/6hbHnz8nPkWZF00TR9Mof";

/// Varsayılan katalog varlık adı (GitHub Release asset).
pub const DEFAULT_CATALOG_ASSET: &str = "catalog.json";
/// Varsayılan manifest varlık adı.
pub const DEFAULT_MANIFEST_ASSET: &str = "manifest.json";
/// Varsayılan search-index varlık adı.
pub const DEFAULT_SEARCH_INDEX_ASSET: &str = "search-index.json";

/// Nova-Commands Releases stub kökü (`catalog-v*` etiketli yayınlar).
pub const DEFAULT_RELEASES_BASE: &str =
    "https://github.com/Teknoloji-Filozoflari/Nova-Commands/releases/latest/download";

/// İmzalı banka yenileme sonucu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BankCatalogRefresh {
    /// Uzak banka doğrulandı ve uygulandı.
    Updated {
        commands: usize,
        concepts: usize,
        content_sha256: String,
        source: String,
    },
    /// Uzak deneme atlandı veya başarısız; mevcut güvenilen banka korundu.
    KeptTrusted { reason: String },
}

/// Son güvenilen (imzalı) banka dosya yolları (`<commands>/trusted/`).
#[derive(Debug, Clone)]
pub struct TrustedBankPaths {
    pub catalog: PathBuf,
    pub catalog_sig: PathBuf,
    pub manifest: PathBuf,
    pub manifest_sig: PathBuf,
    pub search_index: PathBuf,
    pub search_index_sig: PathBuf,
}

impl TrustedBankPaths {
    /// Verilen komut önbellek kökünden yolları üretir.
    pub fn under(commands_root: &Path) -> Self {
        let dir = commands_root.join("trusted");
        Self {
            catalog: dir.join("catalog.json"),
            catalog_sig: dir.join("catalog.json.minisig"),
            manifest: dir.join("manifest.json"),
            manifest_sig: dir.join("manifest.json.minisig"),
            search_index: dir.join("search-index.json"),
            search_index_sig: dir.join("search-index.json.minisig"),
        }
    }

    /// Kullanıcı komut dizininden (yoksa `None`).
    pub fn default_paths() -> Option<Self> {
        Some(Self::under(&commands_cache_dir()?))
    }
}

/// Varsayılan uzak banka + imza URL’leri (ortam değişkenleriyle override).
pub fn remote_bank_urls() -> RemoteBankUrls {
    let base = std::env::var("NOVA_COMMANDS_RELEASES_BASE")
        .unwrap_or_else(|_| DEFAULT_RELEASES_BASE.to_string());
    let catalog = std::env::var("NOVA_COMMANDS_RELEASE_CATALOG_URL")
        .unwrap_or_else(|_| format!("{base}/{DEFAULT_CATALOG_ASSET}"));
    let catalog_sig = std::env::var("NOVA_COMMANDS_RELEASE_CATALOG_SIG_URL")
        .unwrap_or_else(|_| format!("{base}/{DEFAULT_CATALOG_ASSET}.minisig"));
    let manifest = std::env::var("NOVA_COMMANDS_RELEASE_MANIFEST_URL")
        .unwrap_or_else(|_| format!("{base}/{DEFAULT_MANIFEST_ASSET}"));
    let manifest_sig = std::env::var("NOVA_COMMANDS_RELEASE_MANIFEST_SIG_URL")
        .unwrap_or_else(|_| format!("{base}/{DEFAULT_MANIFEST_ASSET}.minisig"));
    let search_index = std::env::var("NOVA_COMMANDS_RELEASE_SEARCH_INDEX_URL")
        .unwrap_or_else(|_| format!("{base}/{DEFAULT_SEARCH_INDEX_ASSET}"));
    let search_index_sig = std::env::var("NOVA_COMMANDS_RELEASE_SEARCH_INDEX_SIG_URL")
        .unwrap_or_else(|_| format!("{base}/{DEFAULT_SEARCH_INDEX_ASSET}.minisig"));
    RemoteBankUrls {
        catalog,
        catalog_sig,
        manifest,
        manifest_sig,
        search_index,
        search_index_sig,
    }
}

/// Altı HTTPS hedefi (üç JSON + üç imza).
#[derive(Debug, Clone)]
pub struct RemoteBankUrls {
    pub catalog: String,
    pub catalog_sig: String,
    pub manifest: String,
    pub manifest_sig: String,
    pub search_index: String,
    pub search_index_sig: String,
}

/// İmzalı üç JSON + üç imza gövdesi (bellek / disk / ağ).
#[derive(Debug, Clone)]
pub struct SignedBankArtifacts {
    pub catalog: Vec<u8>,
    pub catalog_sig: String,
    pub manifest: Vec<u8>,
    pub manifest_sig: String,
    pub search_index: Vec<u8>,
    pub search_index_sig: String,
}

impl SignedBankArtifacts {
    /// Verilen anahtarla üç imzayı doğrular.
    pub fn verify_with(&self, pk: &PublicKey) -> Result<(), String> {
        verify_signed_bank_triplet(
            &self.catalog,
            &self.catalog_sig,
            &self.manifest,
            &self.manifest_sig,
            &self.search_index,
            &self.search_index_sig,
            pk,
        )
    }

    /// Sözleşme + kind / SHA doğrulamasıyla [`FetchedCommandsBank`] üretir.
    pub fn accept(&self, source: impl Into<String>) -> Result<FetchedCommandsBank, String> {
        let catalog_text =
            std::str::from_utf8(&self.catalog).map_err(|e| format!("catalog UTF-8 değil: {e}"))?;
        let manifest_text = std::str::from_utf8(&self.manifest)
            .map_err(|e| format!("manifest UTF-8 değil: {e}"))?;
        let search_text = std::str::from_utf8(&self.search_index)
            .map_err(|e| format!("search-index UTF-8 değil: {e}"))?;
        accept_commands_artifacts(catalog_text, manifest_text, search_text, source)
    }
}

/// Uzak deneme kapalı mı? (`repo_sync` ile aynı bayrak).
pub fn remote_fetch_disabled() -> bool {
    crate::commands::repo_sync::remote_fetch_disabled()
}

/// Gömülü Nova-Commands genel anahtarını yükler.
pub fn bank_public_key() -> Result<PublicKey, String> {
    PublicKey::from_base64(NOVA_COMMANDS_CATALOG_PUBLIC_KEY)
        .map_err(|e| format!("gömülü Nova-Commands genel anahtarı geçersiz: {e}"))
}

/// Tek bir banka dosyasını verilen anahtarla doğrular.
pub fn verify_bytes_with_key(
    data: &[u8],
    signature_text: &str,
    pk: &PublicKey,
) -> Result<(), String> {
    let signature =
        Signature::decode(signature_text).map_err(|e| format!("banka imzası çözülemedi: {e}"))?;
    pk.verify(data, &signature, false)
        .map_err(|e| format!("banka imzası doğrulanamadı: {e}"))
}

/// Katalog / manifest / search-index baytlarını gömülü anahtarla doğrular.
pub fn verify_bank_file_signature(data: &[u8], signature_text: &str) -> Result<(), String> {
    let pk = bank_public_key()?;
    verify_bytes_with_key(data, signature_text, &pk)
}

/// Üç artifact + imzalarını gömülü (veya verilen) anahtarla doğrular.
pub fn verify_signed_bank_triplet(
    catalog: &[u8],
    catalog_sig: &str,
    manifest: &[u8],
    manifest_sig: &str,
    search_index: &[u8],
    search_index_sig: &str,
    pk: &PublicKey,
) -> Result<(), String> {
    verify_bytes_with_key(catalog, catalog_sig, pk)
        .map_err(|e| format!("catalog.json: {e}"))?;
    verify_bytes_with_key(manifest, manifest_sig, pk)
        .map_err(|e| format!("manifest.json: {e}"))?;
    verify_bytes_with_key(search_index, search_index_sig, pk)
        .map_err(|e| format!("search-index.json: {e}"))?;
    Ok(())
}

/// Banka JSON’ları için üst sınır (katalog büyüyebilir).
const BANK_FETCH_MAX_BYTES: u64 = 8 * 1024 * 1024;
const BANK_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// HTTPS metin indirir (boyut sınırlı, kısa zaman aşımı).
pub fn fetch_https_text(url: &str) -> Result<String, String> {
    net::require_https(url)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(BANK_FETCH_TIMEOUT))
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
    let limit = BANK_FETCH_MAX_BYTES.max(MAX_TEXT_BYTES);
    if let Some(len) = response.body().content_length() {
        if len > limit {
            return Err(format!(
                "'{url}' çok büyük: Content-Length {len} (sınır {limit})"
            ));
        }
    }
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_string()
        .map_err(|e| format!("'{url}' okunamadı: {e}"))
}

/// Diskteki son güvenilen (imzalı) bankayı imza doğrulayarak yükler.
pub fn load_last_trusted(paths: &TrustedBankPaths) -> Result<FetchedCommandsBank, String> {
    let arts = SignedBankArtifacts {
        catalog: fs::read(&paths.catalog).map_err(|e| {
            format!(
                "güvenilen catalog okunamadı ({}): {e}",
                paths.catalog.display()
            )
        })?,
        catalog_sig: fs::read_to_string(&paths.catalog_sig).map_err(|e| {
            format!(
                "güvenilen catalog imzası okunamadı ({}): {e}",
                paths.catalog_sig.display()
            )
        })?,
        manifest: fs::read(&paths.manifest).map_err(|e| {
            format!(
                "güvenilen manifest okunamadı ({}): {e}",
                paths.manifest.display()
            )
        })?,
        manifest_sig: fs::read_to_string(&paths.manifest_sig).map_err(|e| {
            format!(
                "güvenilen manifest imzası okunamadı ({}): {e}",
                paths.manifest_sig.display()
            )
        })?,
        search_index: fs::read(&paths.search_index).map_err(|e| {
            format!(
                "güvenilen search-index okunamadı ({}): {e}",
                paths.search_index.display()
            )
        })?,
        search_index_sig: fs::read_to_string(&paths.search_index_sig).map_err(|e| {
            format!(
                "güvenilen search-index imzası okunamadı ({}): {e}",
                paths.search_index_sig.display()
            )
        })?,
    };
    let pk = bank_public_key()?;
    arts.verify_with(&pk)?;
    arts.accept(paths.catalog.display().to_string())
}

/// Doğrulanmış banka + imzaları diske yazar.
pub fn save_last_trusted(
    paths: &TrustedBankPaths,
    arts: &SignedBankArtifacts,
    pk: &PublicKey,
) -> Result<(), String> {
    arts.verify_with(pk)?;
    if let Some(parent) = paths.catalog.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "güvenilen banka dizini oluşturulamadı ({}): {e}",
                parent.display()
            )
        })?;
    }
    write_file(&paths.catalog, &arts.catalog)?;
    write_file(&paths.catalog_sig, arts.catalog_sig.as_bytes())?;
    write_file(&paths.manifest, &arts.manifest)?;
    write_file(&paths.manifest_sig, arts.manifest_sig.as_bytes())?;
    write_file(&paths.search_index, &arts.search_index)?;
    write_file(&paths.search_index_sig, arts.search_index_sig.as_bytes())?;
    Ok(())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("{} yazılamadı: {e}", path.display()))
}

/// Açılış için güvenilen banka: son doğrulanmış imzalı kopya.
///
/// Ağ çağrısı **yapmaz**. Uzak yenileme için [`try_refresh_from_remote`].
pub fn load_trusted_bank() -> Option<FetchedCommandsBank> {
    let paths = TrustedBankPaths::default_paths()?;
    load_last_trusted(&paths).ok()
}

/// Uzak Nova-Commands bankasını indirip imza doğrular; başarılıysa kaydeder.
///
/// Başarısızlıkta mevcut güvenilen banka **değiştirilmez**.
pub fn try_refresh_from_remote(
    paths: Option<&TrustedBankPaths>,
) -> Result<(FetchedCommandsBank, BankCatalogRefresh), String> {
    if remote_fetch_disabled() {
        return Err("uzak komut bankası denemesi kapalı (NOVA_COMMANDS_DISABLE_REMOTE)".into());
    }
    // Birim testlerde ağa çıkma; imza/kabul `accept_signed_bank_bytes` ile test edilir.
    #[cfg(test)]
    if std::env::var_os("NOVA_COMMANDS_ALLOW_REMOTE_IN_TESTS").is_none() {
        return Err("test: uzak imzalı banka atlandı".into());
    }

    let urls = remote_bank_urls();
    let arts = SignedBankArtifacts {
        catalog: fetch_https_text(&urls.catalog)?.into_bytes(),
        catalog_sig: fetch_https_text(&urls.catalog_sig)?,
        manifest: fetch_https_text(&urls.manifest)?.into_bytes(),
        manifest_sig: fetch_https_text(&urls.manifest_sig)?,
        search_index: fetch_https_text(&urls.search_index)?.into_bytes(),
        search_index_sig: fetch_https_text(&urls.search_index_sig)?,
    };

    let pk = bank_public_key()?;
    arts.verify_with(&pk)?;
    let fetched = arts.accept(urls.catalog.clone())?;

    let save_paths = paths.cloned().or_else(TrustedBankPaths::default_paths);
    if let Some(ref p) = save_paths {
        save_last_trusted(p, &arts, &pk)?;
    }

    let refresh = BankCatalogRefresh::Updated {
        commands: fetched.bank.commands.len(),
        concepts: fetched.bank.concepts.len(),
        content_sha256: fetched.manifest.content_sha256.clone(),
        source: fetched.source.clone(),
    };
    Ok((fetched, refresh))
}

/// Uzak yenilemeyi dener; hata olursa mevcut bankayı koruyan özet döner.
pub fn refresh_or_keep_trusted(
    current: Option<&FetchedCommandsBank>,
) -> (Option<FetchedCommandsBank>, BankCatalogRefresh) {
    let paths = TrustedBankPaths::default_paths();
    match try_refresh_from_remote(paths.as_ref()) {
        Ok((fetched, outcome)) => (Some(fetched), outcome),
        Err(reason) => (
            current.cloned(),
            BankCatalogRefresh::KeptTrusted { reason },
        ),
    }
}

/// Bellekteki baytlardan (test / özel kaynak) doğrula + isteğe bağlı kaydet.
///
/// `pk` verilmezse gömülü üretim anahtarı kullanılır.
pub fn accept_signed_bank_bytes(
    arts: &SignedBankArtifacts,
    paths: Option<&TrustedBankPaths>,
    pk: Option<&PublicKey>,
) -> Result<FetchedCommandsBank, String> {
    let owned;
    let key = match pk {
        Some(k) => k,
        None => {
            owned = bank_public_key()?;
            &owned
        }
    };
    arts.verify_with(key)?;
    let fetched = arts.accept("signed-bytes")?;
    if let Some(paths) = paths {
        save_last_trusted(paths, arts, key)?;
    }
    Ok(fetched)
}

/// İnsan özeti (Ayarlar durum satırı).
pub fn format_refresh_note(refresh: &BankCatalogRefresh) -> String {
    match refresh {
        BankCatalogRefresh::Updated {
            commands,
            concepts,
            content_sha256,
            source,
        } => format!(
            "Releases imzalı komut bankası uygulandı: {commands} komut, {concepts} konsept (sha {}…; {source}).",
            &content_sha256[..12.min(content_sha256.len())]
        ),
        BankCatalogRefresh::KeptTrusted { reason } => {
            format!("Releases imzalı banka: korundu — {reason}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nova-commands")
    }

    fn fixture_bytes(name: &str) -> Vec<u8> {
        fs::read(fixture_dir().join(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
    }

    fn fixture_text(name: &str) -> String {
        fs::read_to_string(fixture_dir().join(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
    }

    /// Fixture imzaları üretim anahtarıyla değil, test anahtarıyla atılmıştır.
    fn fixture_public_key() -> PublicKey {
        let b64 = fixture_text("minisign.pub.b64");
        PublicKey::from_base64(b64.trim()).expect("fixture pubkey")
    }

    fn fixture_arts() -> SignedBankArtifacts {
        SignedBankArtifacts {
            catalog: fixture_bytes("catalog.json"),
            catalog_sig: fixture_text("catalog.json.minisig"),
            manifest: fixture_bytes("manifest.json"),
            manifest_sig: fixture_text("manifest.json.minisig"),
            search_index: fixture_bytes("search-index.json"),
            search_index_sig: fixture_text("search-index.json.minisig"),
        }
    }

    #[test]
    fn embedded_public_key_parses() {
        bank_public_key().expect("pubkey");
    }

    #[test]
    fn embedded_key_matches_assets_file() {
        let assets = fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("assets/keys/nova-commands-catalog.pub"),
        )
        .expect("assets key file");
        let line = assets
            .lines()
            .find(|l| l.starts_with("RW"))
            .expect("RW line");
        assert_eq!(line.trim(), NOVA_COMMANDS_CATALOG_PUBLIC_KEY);
    }

    #[test]
    fn fixture_signatures_verify_with_fixture_key() {
        let pk = fixture_public_key();
        let arts = fixture_arts();
        arts.verify_with(&pk).expect("verify triplet");
        let fetched = accept_signed_bank_bytes(&arts, None, Some(&pk)).expect("accept");
        assert_eq!(fetched.bank.commands.len(), 1);
        assert_eq!(fetched.bank.commands[0].id, "fixture-echo");
        assert_eq!(fetched.bank.kind, crate::commands::BANK_KIND);
    }

    #[test]
    fn tampered_catalog_is_rejected() {
        let pk = fixture_public_key();
        let mut arts = fixture_arts();
        arts.catalog[0] ^= 0x7f;
        let err = arts.verify_with(&pk).unwrap_err();
        assert!(
            err.contains("doğrulanamadı") || err.to_lowercase().contains("verif"),
            "{err}"
        );
    }

    #[test]
    fn last_trusted_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "nova-trusted-bank-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = TrustedBankPaths::under(&dir);
        let pk = fixture_public_key();
        let arts = fixture_arts();
        save_last_trusted(&paths, &arts, &pk).expect("save");
        // load_last_trusted uses production key — verify disk with fixture key instead
        assert!(paths.catalog.exists());
        assert!(paths.catalog_sig.exists());
        let on_disk = SignedBankArtifacts {
            catalog: fs::read(&paths.catalog).unwrap(),
            catalog_sig: fs::read_to_string(&paths.catalog_sig).unwrap(),
            manifest: fs::read(&paths.manifest).unwrap(),
            manifest_sig: fs::read_to_string(&paths.manifest_sig).unwrap(),
            search_index: fs::read(&paths.search_index).unwrap(),
            search_index_sig: fs::read_to_string(&paths.search_index_sig).unwrap(),
        };
        on_disk.verify_with(&pk).expect("disk verify");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn accept_signed_rejects_bad_sig_without_writing() {
        let dir = std::env::temp_dir().join(format!(
            "nova-trusted-bank-bad-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = TrustedBankPaths::under(&dir);
        let pk = fixture_public_key();
        let mut arts = fixture_arts();
        arts.catalog = b"not-a-catalog".to_vec();
        let err = accept_signed_bank_bytes(&arts, Some(&paths), Some(&pk)).unwrap_err();
        assert!(!paths.catalog.exists(), "bozuk katalog yazılmamalı: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remote_urls_honour_env_override() {
        let _g = crate::test_env::lock();
        std::env::set_var(
            "NOVA_COMMANDS_RELEASE_CATALOG_URL",
            "https://example.org/c.json",
        );
        std::env::set_var(
            "NOVA_COMMANDS_RELEASE_CATALOG_SIG_URL",
            "https://example.org/c.minisig",
        );
        let urls = remote_bank_urls();
        assert_eq!(urls.catalog, "https://example.org/c.json");
        assert_eq!(urls.catalog_sig, "https://example.org/c.minisig");
        std::env::remove_var("NOVA_COMMANDS_RELEASE_CATALOG_URL");
        std::env::remove_var("NOVA_COMMANDS_RELEASE_CATALOG_SIG_URL");
        let urls = remote_bank_urls();
        assert!(urls.catalog.contains("Nova-Commands"));
        assert!(urls.catalog_sig.ends_with(".minisig"));
    }

    #[test]
    fn refresh_disabled_by_env() {
        let _g = crate::test_env::lock();
        std::env::set_var("NOVA_COMMANDS_DISABLE_REMOTE", "1");
        let err = try_refresh_from_remote(None).unwrap_err();
        assert!(err.contains("kapalı"), "{err}");
        std::env::remove_var("NOVA_COMMANDS_DISABLE_REMOTE");
    }

    #[test]
    fn remote_skipped_in_tests_by_default() {
        let _g = crate::test_env::lock();
        std::env::remove_var("NOVA_COMMANDS_DISABLE_REMOTE");
        std::env::remove_var("NOVA_COMMANDS_ALLOW_REMOTE_IN_TESTS");
        let err = try_refresh_from_remote(None).unwrap_err();
        assert!(err.contains("test:"), "{err}");
    }

    #[test]
    fn require_https_on_fetch() {
        let err = fetch_https_text("http://example.org/x").unwrap_err();
        assert!(err.contains("https"), "{err}");
    }

    #[test]
    fn format_notes_cover_variants() {
        let u = format_refresh_note(&BankCatalogRefresh::Updated {
            commands: 1,
            concepts: 2,
            content_sha256: "abcdefghijklmnop".into(),
            source: "https://example.com/c".into(),
        });
        assert!(u.contains("imzalı"), "{u}");
        let k = format_refresh_note(&BankCatalogRefresh::KeptTrusted {
            reason: "404".into(),
        });
        assert!(k.contains("korundu"), "{k}");
    }
}
