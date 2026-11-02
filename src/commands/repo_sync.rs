//! Nova-Commands **repo** (main) üzerinden banka senkronu.
//!
//! İmzalı Releases yokken `metadata/{manifest,catalog,search-index}.json`
//! (veya `NOVA_COMMANDS_*_URL`) HTTPS ile çekilir. Bu yol **imzasızdır**;
//! güven sabit GitHub ham URL + HTTPS + `kind`/`content_sha256` sözleşmesiyle
//! sınırlıdır. UI bunu açıkça söylemeli.
//!
//! Başarılı senkron bellek içi bankayı günceller ve isteğe bağlı olarak
//! `~/.local/share/nova/commands/` altına önbellek yazar (gömülü
//! `assets/commands` üzerine yazmaz — gömülü vendor kopyası için
//! `tools/sync_commands_catalog.sh`).

use crate::commands::bank::{CommandBank, BANK_KIND};
use crate::commands::loader::{
    load_bank_from_str, load_manifest_from_str, load_search_index_from_str, CommandsManifest,
    SearchIndex, MANIFEST_KIND, SEARCH_INDEX_KIND,
};
use crate::store::net::{self, MAX_TEXT_BYTES};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Nova-Commands GitHub deposu.
pub const NOVA_COMMANDS_GITHUB_REPO: &str = "Teknoloji-Filozoflari/Nova-Commands";
/// Varsayılan dal.
pub const NOVA_COMMANDS_DEFAULT_BRANCH: &str = "main";

/// Repo `metadata/manifest.json` (imzasız main).
pub const DEFAULT_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/Teknoloji-Filozoflari/Nova-Commands/main/metadata/manifest.json";
/// Repo `metadata/catalog.json`.
pub const DEFAULT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/Teknoloji-Filozoflari/Nova-Commands/main/metadata/catalog.json";
/// Repo `metadata/search-index.json`.
pub const DEFAULT_SEARCH_INDEX_URL: &str =
    "https://raw.githubusercontent.com/Teknoloji-Filozoflari/Nova-Commands/main/metadata/search-index.json";

/// Komut bankası JSON’ları için üst sınır (katalog ~400 KB+ büyüyebilir).
const COMMANDS_FETCH_MAX_BYTES: u64 = 8 * 1024 * 1024;
const COMMANDS_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Repo senkron sonucu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandsRepoSync {
    /// Banka uygulandı (veya önbelleğe yazıldı).
    Updated {
        commands: usize,
        concepts: usize,
        content_sha256: String,
        source: String,
    },
    /// Uzak SHA mevcutla aynı — indirme atlandı.
    Unchanged {
        content_sha256: String,
        source: String,
    },
    /// Senkron atlandı / başarısız.
    Failed { reason: String },
}

/// İndirilen üç artifact (doğrulanmış).
#[derive(Debug, Clone)]
pub struct FetchedCommandsBank {
    pub bank: CommandBank,
    pub manifest: CommandsManifest,
    pub search_index: SearchIndex,
    pub catalog_json: String,
    pub manifest_json: String,
    pub search_index_json: String,
    pub source: String,
}

/// Önbellek dizini: `$XDG_DATA_HOME/nova/commands` veya `~/.local/share/nova/commands`.
///
/// `NOVA_COMMANDS_DIR` test / taşınabilir kurulum için override.
pub fn commands_cache_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("NOVA_COMMANDS_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    // Mağaza kökünün kardeşi: .../nova/store → .../nova/commands
    if let Some(store) = crate::store::store_install_dir() {
        if let Some(parent) = store.parent() {
            return Some(parent.join("commands"));
        }
    }
    None
}

pub fn repo_manifest_url() -> String {
    std::env::var("NOVA_COMMANDS_MANIFEST_URL").unwrap_or_else(|_| DEFAULT_MANIFEST_URL.into())
}

pub fn repo_catalog_url() -> String {
    std::env::var("NOVA_COMMANDS_CATALOG_URL").unwrap_or_else(|_| DEFAULT_CATALOG_URL.into())
}

pub fn repo_search_index_url() -> String {
    std::env::var("NOVA_COMMANDS_SEARCH_INDEX_URL")
        .unwrap_or_else(|_| DEFAULT_SEARCH_INDEX_URL.into())
}

/// Uzak deneme kapalı mı?
pub fn remote_fetch_disabled() -> bool {
    matches!(
        std::env::var("NOVA_COMMANDS_DISABLE_REMOTE")
            .unwrap_or_default()
            .as_str(),
        "1" | "true" | "TRUE" | "yes" | "YES"
    )
}

fn fetch_https_text_commands(url: &str) -> Result<String, String> {
    net::require_https(url)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(COMMANDS_FETCH_TIMEOUT))
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
    let limit = COMMANDS_FETCH_MAX_BYTES.max(MAX_TEXT_BYTES);
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

/// Üç JSON metnini sözleşmeye göre doğrular ve modeller üretir.
pub fn accept_commands_artifacts(
    catalog_json: &str,
    manifest_json: &str,
    search_index_json: &str,
    source: impl Into<String>,
) -> Result<FetchedCommandsBank, String> {
    let bank = load_bank_from_str(catalog_json, Some(manifest_json))?;
    let manifest = load_manifest_from_str(manifest_json)?;
    let search_index = load_search_index_from_str(search_index_json)?;

    if bank.kind != BANK_KIND {
        return Err(format!(
            "bank kind beklenen {BANK_KIND}, gelen {}",
            bank.kind
        ));
    }
    if manifest.kind != MANIFEST_KIND {
        return Err(format!(
            "manifest kind beklenen {MANIFEST_KIND}, gelen {}",
            manifest.kind
        ));
    }
    if search_index.kind != SEARCH_INDEX_KIND {
        return Err(format!(
            "search-index kind beklenen {SEARCH_INDEX_KIND}, gelen {}",
            search_index.kind
        ));
    }
    // Mağaza sızıntısı yok.
    if bank.kind.contains("store")
        || manifest.kind.contains("store")
        || search_index.kind.contains("store")
    {
        return Err("komut bankası kind mağaza ile karışmış".into());
    }
    if let Some(ref sha) = search_index.content_sha256 {
        if *sha != manifest.content_sha256 {
            return Err(format!(
                "search-index content_sha256 ({sha}) ≠ manifest ({})",
                manifest.content_sha256
            ));
        }
    }

    Ok(FetchedCommandsBank {
        bank,
        manifest,
        search_index,
        catalog_json: catalog_json.to_string(),
        manifest_json: manifest_json.to_string(),
        search_index_json: search_index_json.to_string(),
        source: source.into(),
    })
}

/// Disk önbelleğine yazar (`catalog.json` / `manifest.json` / `search-index.json`).
pub fn save_commands_cache(fetched: &FetchedCommandsBank) -> Result<PathBuf, String> {
    let Some(dir) = commands_cache_dir() else {
        return Err("komut bankası önbellek dizini yok (HOME / NOVA_COMMANDS_DIR)".into());
    };
    fs::create_dir_all(&dir)
        .map_err(|e| format!("önbellek dizini oluşturulamadı ({}): {e}", dir.display()))?;
    write_file(&dir.join("catalog.json"), fetched.catalog_json.as_bytes())?;
    write_file(&dir.join("manifest.json"), fetched.manifest_json.as_bytes())?;
    write_file(
        &dir.join("search-index.json"),
        fetched.search_index_json.as_bytes(),
    )?;
    Ok(dir)
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("{} yazılamadı: {e}", path.display()))
}

/// Disk önbelleğinden bankayı yükler (sözleşme + SHA doğrulamalı).
pub fn load_cached_commands_bank() -> Result<FetchedCommandsBank, String> {
    let dir = commands_cache_dir().ok_or("önbellek dizini yok")?;
    let catalog = fs::read_to_string(dir.join("catalog.json"))
        .map_err(|e| format!("önbellek catalog okunamadı: {e}"))?;
    let manifest = fs::read_to_string(dir.join("manifest.json"))
        .map_err(|e| format!("önbellek manifest okunamadı: {e}"))?;
    let search = fs::read_to_string(dir.join("search-index.json"))
        .map_err(|e| format!("önbellek search-index okunamadı: {e}"))?;
    accept_commands_artifacts(&catalog, &manifest, &search, dir.display().to_string())
}

/// Nova-Commands main’den bankayı çeker.
///
/// `current_sha` uzak manifest ile aynıysa catalog/search-index **indirilmez**
/// ve [`CommandsRepoSync::Unchanged`] döner.
pub fn fetch_repo_commands_bank(
    current_sha: Option<&str>,
) -> Result<(Option<FetchedCommandsBank>, CommandsRepoSync), String> {
    if remote_fetch_disabled() {
        return Err("uzak komut bankası denemesi kapalı (NOVA_COMMANDS_DISABLE_REMOTE)".into());
    }
    #[cfg(test)]
    if std::env::var_os("NOVA_COMMANDS_ALLOW_REMOTE_IN_TESTS").is_none() {
        return Err("test: uzak komut bankası atlandı".into());
    }

    let manifest_url = repo_manifest_url();
    let manifest_json = fetch_https_text_commands(&manifest_url)?;
    let manifest = load_manifest_from_str(&manifest_json)?;

    if let Some(cur) = current_sha {
        if cur == manifest.content_sha256 {
            return Ok((
                None,
                CommandsRepoSync::Unchanged {
                    content_sha256: manifest.content_sha256,
                    source: manifest_url,
                },
            ));
        }
    }

    let catalog_json = fetch_https_text_commands(&repo_catalog_url())?;
    let search_json = fetch_https_text_commands(&repo_search_index_url())?;
    let fetched =
        accept_commands_artifacts(&catalog_json, &manifest_json, &search_json, &manifest_url)?;
    let sync = CommandsRepoSync::Updated {
        commands: fetched.bank.commands.len(),
        concepts: fetched.bank.concepts.len(),
        content_sha256: fetched.manifest.content_sha256.clone(),
        source: fetched.source.clone(),
    };
    Ok((Some(fetched), sync))
}

/// Mevcut banka SHA’sına göre yeniler; başarıda önbelleğe yazar.
pub fn refresh_commands_from_repo(
    current_sha: Option<&str>,
) -> (Option<FetchedCommandsBank>, CommandsRepoSync) {
    match fetch_repo_commands_bank(current_sha) {
        Ok((Some(fetched), sync)) => {
            let _ = save_commands_cache(&fetched);
            (Some(fetched), sync)
        }
        Ok((None, sync)) => (None, sync),
        Err(reason) => (None, CommandsRepoSync::Failed { reason }),
    }
}

/// İnsan özeti (Ayarlar durum satırı).
pub fn format_sync_note(sync: &CommandsRepoSync) -> String {
    match sync {
        CommandsRepoSync::Updated {
            commands,
            concepts,
            content_sha256,
            source,
        } => format!(
            "Komut bankası güncellendi: {commands} komut, {concepts} konsept (sha {}…; {source}; imzasız main).",
            &content_sha256[..12.min(content_sha256.len())]
        ),
        CommandsRepoSync::Unchanged {
            content_sha256,
            source,
        } => format!(
            "Komut bankası güncel (sha {}…; {source}).",
            &content_sha256[..12.min(content_sha256.len())]
        ),
        CommandsRepoSync::Failed { reason } => {
            format!("Komut bankası yenilenemedi — gömülü/önbellek korundu: {reason}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::loader::{load_bank, load_manifest, load_search_index};

    #[test]
    fn accept_embedded_artifacts() {
        let catalog = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/commands/catalog.json"
        ));
        let manifest = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/commands/manifest.json"
        ));
        let search = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/commands/search-index.json"
        ));
        let fetched = accept_commands_artifacts(catalog, manifest, search, "embedded").unwrap();
        assert_eq!(fetched.bank.kind, BANK_KIND);
        assert_eq!(
            fetched.manifest.content_sha256,
            load_manifest().unwrap().content_sha256
        );
        assert_eq!(
            fetched.bank.commands.len(),
            load_bank().unwrap().commands.len()
        );
        assert_eq!(
            fetched.search_index.entries.len(),
            load_search_index().unwrap().entries.len()
        );
        assert!(!fetched.bank.kind.contains("store"));
    }

    #[test]
    fn rejects_store_kind_catalog() {
        let bad = r#"{"schema_version":1,"kind":"nova-store-catalog","commands":[],"concepts":[]}"#;
        let man = r#"{"schema_version":1,"kind":"nova-commands-manifest","content_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
        let search = r#"{"schema_version":1,"kind":"nova-commands-search-index","entries":[]}"#;
        let err = accept_commands_artifacts(bad, man, search, "t").unwrap_err();
        assert!(
            err.contains("nova-commands-bank") || err.contains("kind"),
            "{err}"
        );
    }

    #[test]
    fn remote_skipped_in_tests_by_default() {
        let err = fetch_repo_commands_bank(None).unwrap_err();
        assert!(err.contains("test:"), "{err}");
    }

    #[test]
    fn format_notes_cover_variants() {
        let u = format_sync_note(&CommandsRepoSync::Updated {
            commands: 1,
            concepts: 2,
            content_sha256: "abcdefghijklmnop".into(),
            source: "https://example.com/m".into(),
        });
        assert!(u.contains("güncellendi"), "{u}");
        let c = format_sync_note(&CommandsRepoSync::Unchanged {
            content_sha256: "abcdefghijklmnop".into(),
            source: "https://example.com/m".into(),
        });
        assert!(c.contains("güncel"), "{c}");
        let f = format_sync_note(&CommandsRepoSync::Failed {
            reason: "ağ yok".into(),
        });
        assert!(f.contains("yenilenemedi"), "{f}");
    }
}
