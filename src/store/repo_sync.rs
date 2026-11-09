//! Nova-Store **repo** (main) üzerinden katalog senkronu.
//!
//! İmzalı Releases yoluna ek olarak `metadata/catalog.json` (veya
//! `NOVA_STORE_REPO_CATALOG_URL`) HTTPS ile çekilir. Bu yol **imzasızdır**;
//! güven, sabit GitHub ham içerik URL’si + HTTPS ile sınırlıdır. UI bunu
//! açıkça söylemeli. Başarılı senkron bellek içi kataloğu günceller ve
//! isteğe bağlı olarak `repo-store.json` olarak saklar (minisign
//! `trusted-store` dosyasının üzerine yazmaz).

use crate::store::remote::{fetch_https_text, TrustedCatalogPaths};
use crate::store::{store_install_dir, InstallMethod, Store, StoreEntry, STORE_CATEGORIES};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Nova-Store GitHub deposu.
pub const NOVA_STORE_GITHUB_REPO: &str = "Teknoloji-Filozoflari/Nova-Store";
/// Varsayılan dal.
pub const NOVA_STORE_DEFAULT_BRANCH: &str = "main";
/// Repo üzerindeki birleşik katalog (imzasız; Releases imzalı yayının kaynağı).
pub const DEFAULT_REPO_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/Teknoloji-Filozoflari/Nova-Store/main/metadata/catalog.json";

/// Repo senkron sonucu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoCatalogSync {
    /// Katalog uygulandı.
    Updated { entries: usize, source: String },
    /// Senkron atlandı / başarısız.
    Failed { reason: String },
}

#[derive(Debug, Deserialize)]
struct RepoCatalogFile {
    #[serde(default)]
    apps: Vec<RepoApp>,
}

/// Nova-Store `apps/*.toml` → `catalog.json` kaydı (esnek alanlar).
#[derive(Debug, Deserialize)]
struct RepoApp {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    executable: String,
    #[serde(default)]
    install_method: Option<InstallMethod>,
    #[serde(default)]
    supported_platforms: Vec<String>,
    #[serde(default)]
    checksum: Option<String>,
    #[serde(default)]
    checksum_type: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    artifacts: Vec<crate::store::ArchArtifact>,
    #[serde(default)]
    install_command: Option<Vec<String>>,
    #[serde(default)]
    install_notes: Option<String>,
    #[serde(default)]
    release: Option<crate::store::manifest::ReleaseSource>,
    #[serde(default)]
    repository_url: Option<String>,
    #[serde(default)]
    signature: Option<crate::store::trust::SignatureSpec>,
}

/// Repo katalog URL’si (ortam değişkeniyle override).
pub fn repo_catalog_url() -> String {
    std::env::var("NOVA_STORE_REPO_CATALOG_URL").unwrap_or_else(|_| DEFAULT_REPO_CATALOG_URL.into())
}

/// Kısa kategori adını Nova `STORE_CATEGORIES` etiketine çevirir.
pub fn map_repo_category(raw: &str) -> Option<&'static str> {
    let key = raw.trim().to_lowercase();
    let mapped = match key.as_str() {
        "ai" => "AI",
        "git" => "Git",
        "files" | "dosya ve arama" => "Dosya ve Arama",
        "development" | "geliştirme" | "gelistirme" => "Geliştirme",
        "editors" | "editörler" | "editorler" => "Editörler",
        "system" | "sistem" => "Sistem",
        "panels" | "paneller" => "Paneller",
        "networking" | "ağ" | "ag" => "Ağ",
        "containers" | "konteyner" => "Konteyner",
        "database" | "veritabanı" | "veritabani" => "Veritabanı",
        "security" | "güvenlik" | "guvenlik" => "Güvenlik",
        "media" | "medya" => "Medya",
        "messaging" | "mesajlaşma" | "mesajlasma" => "Mesajlaşma",
        "productivity" | "verimlilik" => "Verimlilik",
        "games" | "oyunlar" => "Oyunlar",
        "screensavers" | "ekran koruyucular" => "Ekran Koruyucular",
        "web" => "Web",
        "daily" | "günlük" | "gunluk" => "Günlük",
        other => {
            // Zaten Türkçe tam ad olabilir.
            return STORE_CATEGORIES
                .iter()
                .copied()
                .find(|c| c.eq_ignore_ascii_case(other) || c.to_lowercase() == other);
        }
    };
    Some(mapped)
}

fn repo_app_to_entry(app: RepoApp) -> Result<StoreEntry, String> {
    let category = map_repo_category(&app.category)
        .ok_or_else(|| format!("bilinmeyen kategori '{}'", app.category))?
        .to_string();
    Ok(StoreEntry {
        id: app.id,
        name: app.name,
        description: app.description,
        category,
        source: app.source,
        license: app.license,
        executable: app.executable,
        install_method: app.install_method.unwrap_or_default(),
        supported_platforms: app.supported_platforms,
        checksum: app.checksum,
        checksum_type: app.checksum_type,
        version: app.version,
        download_url: app.download_url,
        artifacts: app.artifacts,
        install_command: app.install_command,
        install_notes: app.install_notes,
        release: app.release,
        repository_url: app.repository_url,
        signature: app.signature,
    })
}

/// JSON baytlarından mağaza kataloğu üretir (kategori eşlemesiyle).
pub fn parse_repo_catalog_json(bytes: &[u8]) -> Result<Store, String> {
    let file: RepoCatalogFile =
        serde_json::from_slice(bytes).map_err(|e| format!("repo katalog JSON okunamadı: {e}"))?;
    if file.apps.is_empty() {
        return Err("repo kataloğu boş".into());
    }
    let mut entries = Vec::with_capacity(file.apps.len());
    for app in file.apps {
        match repo_app_to_entry(app) {
            Ok(e) => entries.push(e),
            Err(e) => {
                // Tek bozuk kayıt tüm senkronu düşürmesin; atla.
                eprintln!("nova: repo katalog kaydı atlandı: {e}");
            }
        }
    }
    Store::from_entries(entries)
}

/// Repo kataloğunu indirip ayrıştırır.
pub fn fetch_repo_catalog() -> Result<(Store, String), String> {
    #[cfg(test)]
    if std::env::var_os("NOVA_STORE_CATALOG_ALLOW_REMOTE_IN_TESTS").is_none() {
        return Err("test: repo katalog atlandı".into());
    }
    let url = repo_catalog_url();
    let text = fetch_https_text(&url)?;
    let store = parse_repo_catalog_json(text.as_bytes())?;
    let _ = save_repo_catalog_cache(text.as_bytes());
    Ok((store, url))
}

fn repo_cache_path() -> Option<PathBuf> {
    Some(store_install_dir()?.join("catalog").join("repo-store.json"))
}

fn save_repo_catalog_cache(bytes: &[u8]) -> Result<(), String> {
    let Some(path) = repo_cache_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("repo katalog dizini oluşturulamadı: {e}"))?;
    }
    fs::write(&path, bytes).map_err(|e| format!("repo katalog önbelleği yazılamadı: {e}"))?;
    Ok(())
}

/// Repo senkronunu dener; hata olursa mevcut kataloğu koruyan özet döner.
pub fn sync_repo_or_keep(current: &Store) -> (Store, RepoCatalogSync) {
    match fetch_repo_catalog() {
        Ok((store, source)) => {
            let n = store.entries().len();
            (store, RepoCatalogSync::Updated { entries: n, source })
        }
        Err(reason) => (current.clone(), RepoCatalogSync::Failed { reason }),
    }
}

/// İmzalı Releases + repo yollarını birlikte dener.
///
/// Önce imzalı Releases; başarısızsa veya istenirse repo `catalog.json`.
/// Dönüş: (uygulanan katalog, insan özeti).
pub fn refresh_signed_then_repo(current: &Store) -> (Store, String) {
    let paths = TrustedCatalogPaths::default_paths();
    let mut notes = Vec::new();

    let after_signed = match crate::store::remote::try_refresh_from_remote(paths.as_ref()) {
        Ok((store, crate::store::remote::CatalogRefresh::Updated { entries })) => {
            notes.push(format!(
                "Releases imzalı katalog uygulandı ({entries} kayıt)."
            ));
            store
        }
        Ok((_, crate::store::remote::CatalogRefresh::KeptTrusted { reason })) => {
            notes.push(format!("Releases: korundu — {reason}"));
            current.clone()
        }
        Err(reason) => {
            notes.push(format!("Releases: {reason}"));
            current.clone()
        }
    };

    let (final_store, repo) = sync_repo_or_keep(&after_signed);
    match repo {
        RepoCatalogSync::Updated { entries, source } => {
            notes.push(format!(
                "Repo ({source}) senkronu uygulandı ({entries} kayıt; imzasız main)."
            ));
            (final_store, notes.join(" "))
        }
        RepoCatalogSync::Failed { reason } => {
            notes.push(format!("Repo: {reason}"));
            // Repo başarısızsa imzalı sonucu (veya eskiyi) koru.
            (after_signed, notes.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_short_categories() {
        assert_eq!(map_repo_category("ai"), Some("AI"));
        assert_eq!(map_repo_category("files"), Some("Dosya ve Arama"));
        assert_eq!(map_repo_category("AI"), Some("AI"));
    }

    #[test]
    fn parses_minimal_repo_json() {
        let json = r#"{
          "apps": [{
            "id": "demo",
            "name": "Demo",
            "description": "Deneme aracı",
            "category": "ai",
            "source": "https://example.com",
            "license": "MIT",
            "executable": "demo",
            "install_method": "manual",
            "supported_platforms": ["linux-x64"]
          }]
        }"#;
        let store = parse_repo_catalog_json(json.as_bytes()).unwrap();
        assert_eq!(store.entries().len(), 1);
        assert_eq!(store.entries()[0].category, "AI");
    }
}
