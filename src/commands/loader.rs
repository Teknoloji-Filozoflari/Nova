//! Nova-Commands artifact yükleyici.
//!
//! Öncelik: gömülü `assets/commands/{manifest,catalog,search-index}.json`
//! (NOVA_INTEGRATION.md). İsteğe bağlı dosya yolu override testler içindir.

use crate::commands::bank::{validate_bank_schema_version, CommandBank, BANK_KIND};
use serde::Deserialize;
use std::path::Path;

pub const MANIFEST_KIND: &str = "nova-commands-manifest";
pub const SEARCH_INDEX_KIND: &str = "nova-commands-search-index";

const EMBEDDED_CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/commands/catalog.json"
));
const EMBEDDED_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/commands/manifest.json"
));
const EMBEDDED_SEARCH_INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/commands/search-index.json"
));

#[derive(Debug, Clone, Deserialize)]
pub struct CommandsManifest {
    pub schema_version: u32,
    pub kind: String,
    #[serde(default)]
    pub generated_at: Option<String>,
    pub content_sha256: String,
    #[serde(default)]
    pub command_count: Option<usize>,
    #[serde(default)]
    pub concept_count: Option<usize>,
}

impl CommandsManifest {
    pub fn validate_contract(&self) -> Result<(), String> {
        if self.kind != MANIFEST_KIND {
            return Err(format!(
                "beklenen kind={MANIFEST_KIND}, gelen={}",
                self.kind
            ));
        }
        validate_bank_schema_version(self.schema_version)?;
        if self.content_sha256.len() != 64 {
            return Err("content_sha256 64 hex olmalı".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchIndex {
    pub schema_version: u32,
    pub kind: String,
    #[serde(default)]
    pub entry_count: Option<usize>,
    #[serde(default)]
    pub content_sha256: Option<String>,
    #[serde(default)]
    pub entries: Vec<SearchEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub risk: Option<String>,
    #[serde(default)]
    pub verification: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl SearchIndex {
    pub fn validate_contract(&self) -> Result<(), String> {
        if self.kind != SEARCH_INDEX_KIND {
            return Err(format!(
                "beklenen kind={SEARCH_INDEX_KIND}, gelen={}",
                self.kind
            ));
        }
        validate_bank_schema_version(self.schema_version)?;
        Ok(())
    }
}

/// Gömülü bankayı yükler ve sözleşme + manifest SHA ile doğrular.
pub fn load_bank() -> Result<CommandBank, String> {
    load_bank_from_str(EMBEDDED_CATALOG, Some(EMBEDDED_MANIFEST))
}

/// Test / dış dosya için.
pub fn load_bank_from_path(catalog: &Path, manifest: Option<&Path>) -> Result<CommandBank, String> {
    let cat = std::fs::read_to_string(catalog)
        .map_err(|e| format!("{} okunamadı: {e}", catalog.display()))?;
    let man = match manifest {
        Some(p) => Some(
            std::fs::read_to_string(p).map_err(|e| format!("{} okunamadı: {e}", p.display()))?,
        ),
        None => None,
    };
    load_bank_from_str(&cat, man.as_deref())
}

pub fn load_bank_from_str(
    catalog_json: &str,
    manifest_json: Option<&str>,
) -> Result<CommandBank, String> {
    let bank: CommandBank =
        serde_json::from_str(catalog_json).map_err(|e| format!("catalog.json parse: {e}"))?;
    bank.validate_contract()?;

    if let Some(mjson) = manifest_json {
        let man = load_manifest_from_str(mjson)?;
        if let Some(ref sha) = bank.content_sha256 {
            if *sha != man.content_sha256 {
                return Err(format!(
                    "catalog content_sha256 ({sha}) ≠ manifest ({})",
                    man.content_sha256
                ));
            }
        }
    }

    // Mağaza kind'ı sızmasın.
    if bank.kind.contains("store") {
        return Err("bank kind mağaza ile karışmış".into());
    }
    debug_assert_eq!(bank.kind, BANK_KIND);
    Ok(bank)
}

pub fn load_manifest() -> Result<CommandsManifest, String> {
    load_manifest_from_str(EMBEDDED_MANIFEST)
}

pub fn load_manifest_from_str(json: &str) -> Result<CommandsManifest, String> {
    let m: CommandsManifest =
        serde_json::from_str(json).map_err(|e| format!("manifest.json parse: {e}"))?;
    m.validate_contract()?;
    Ok(m)
}

pub fn load_search_index() -> Result<SearchIndex, String> {
    load_search_index_from_str(EMBEDDED_SEARCH_INDEX)
}

pub fn load_search_index_from_str(json: &str) -> Result<SearchIndex, String> {
    let idx: SearchIndex =
        serde_json::from_str(json).map_err(|e| format!("search-index.json parse: {e}"))?;
    idx.validate_contract()?;
    Ok(idx)
}

/// Gömülü bankanın `content_sha256` değeri (yoksa `None`).
pub fn embedded_content_sha256() -> Option<String> {
    load_manifest().ok().map(|m| m.content_sha256)
}

/// Açılış yüklemesi: geçerli disk önbelleği varsa o, yoksa gömülü.
///
/// Ağ çağrısı **yapmaz**. Uzak yenileme için [`crate::commands::repo_sync`].
pub fn load_bank_preferring_cache() -> Result<(CommandBank, SearchIndex, String), String> {
    if let Ok(cached) = crate::commands::repo_sync::load_cached_commands_bank() {
        return Ok((
            cached.bank,
            cached.search_index,
            format!("önbellek ({})", cached.source),
        ));
    }
    let bank = load_bank()?;
    let idx = load_search_index()?;
    Ok((bank, idx, "gömülü assets/commands".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::bank::BANK_KIND;

    #[test]
    fn embedded_bank_loads_and_matches_manifest() {
        let bank = load_bank().expect("bank");
        let man = load_manifest().expect("manifest");
        assert_eq!(bank.kind, BANK_KIND);
        assert!(
            crate::commands::bank::SUPPORTED_BANK_SCHEMA_VERSIONS.contains(&bank.schema_version),
            "schema {}",
            bank.schema_version
        );
        assert_eq!(bank.schema_version, man.schema_version);
        assert_eq!(
            bank.content_sha256.as_deref(),
            Some(man.content_sha256.as_str())
        );
        assert!(!bank.commands.is_empty());
        assert_eq!(
            bank.command_count.unwrap_or(bank.commands.len()),
            bank.commands.len()
        );
        // Canlı banka schema v2 + beklenen digest (Nova-Commands #20 / Faz 7).
        assert_eq!(bank.schema_version, 2);
        assert_eq!(
            man.content_sha256,
            "cca5bd35662ba1c69d521669367942de222e2b9a9630bb0f3dc85f6ac5156ce5"
        );
        let with_refs = bank
            .commands
            .iter()
            .filter(|c| !c.package_refs.is_empty())
            .count();
        assert!(with_refs > 0, "package_refs beklenirdi");
        // Prefer a multi-distro sample (e.g. git); apk-only rows also carry package_refs.
        let sample = bank
            .commands
            .iter()
            .find(|c| {
                c.package_refs
                    .iter()
                    .any(|r| matches!(r.manager.as_str(), "pacman" | "apt" | "dnf"))
            })
            .expect("pacman/apt/dnf ref");
        let pairs = sample.package_labeled_pairs(true);
        assert!(!pairs.is_empty());
        assert!(
            pairs
                .iter()
                .any(|(l, _)| l.starts_with("arch") || l == "pacman" || l == "apt" || l == "dnf"),
            "{pairs:?}"
        );
    }

    #[test]
    fn search_index_is_distinct_kind() {
        let idx = load_search_index().expect("search-index");
        assert_eq!(idx.kind, SEARCH_INDEX_KIND);
        assert_ne!(idx.kind, BANK_KIND);
        assert_ne!(idx.kind, MANIFEST_KIND);
        assert!(!idx.entries.is_empty());
        assert_eq!(idx.schema_version, 2);
    }

    #[test]
    fn rejects_wrong_kind() {
        let bad = r#"{"schema_version":2,"kind":"nova-store-catalog","commands":[]}"#;
        let err = load_bank_from_str(bad, None).unwrap_err();
        assert!(err.contains("nova-commands-bank"), "{err}");
    }

    #[test]
    fn accepts_schema_v1_and_v2_rejects_other() {
        let v1 = r#"{"schema_version":1,"kind":"nova-commands-bank","commands":[],"concepts":[]}"#;
        load_bank_from_str(v1, None).expect("v1 ok");
        let v2 = r#"{"schema_version":2,"kind":"nova-commands-bank","commands":[],"concepts":[]}"#;
        load_bank_from_str(v2, None).expect("v2 ok");
        let v3 = r#"{"schema_version":3,"kind":"nova-commands-bank","commands":[],"concepts":[]}"#;
        let err = load_bank_from_str(v3, None).unwrap_err();
        assert!(err.contains("schema_version"), "{err}");
    }

    #[test]
    fn prefers_package_refs_over_legacy_packages() {
        let json = r#"{
            "schema_version": 2,
            "kind": "nova-commands-bank",
            "commands": [{
                "id": "demo",
                "name": "demo",
                "packages": {"arch": "legacy-pkg", "debian": "legacy-deb"},
                "package_refs": [
                    {"manager": "apt", "package": "ref-apt", "distributions": ["debian"]},
                    {"manager": "pacman", "package": "ref-pacman", "distributions": ["arch"]}
                ]
            }],
            "concepts": []
        }"#;
        let bank = load_bank_from_str(json, None).expect("bank");
        let cmd = &bank.commands[0];
        let pairs = cmd.package_labeled_pairs(true);
        assert_eq!(pairs[0].0, "arch/pacman");
        assert_eq!(pairs[0].1, "ref-pacman");
        assert!(pairs.iter().any(|(l, p)| l == "apt" && p == "ref-apt"));
        assert!(!pairs.iter().any(|(_, p)| p.starts_with("legacy")));
    }
}
