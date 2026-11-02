//! Yerel PATH keşfi stub'ı (`nova-local-commands`).
//!
//! Nova-Commands reposu yalnızca sözleşmeyi tutar; tarama Nova istemcisinde
//! yapılır. Bu modül **bankayı kirletmez**: PATH varlığı `verified` yapmaz.

use crate::commands::bank::CommandBank;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Yerel keşif `kind` — bank `nova-commands-bank` ile karıştırılmaz.
pub const LOCAL_KIND: &str = "nova-local-commands";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalSource {
    Path,
    ShellHash,
    PackageFile,
    Other,
}

impl LocalSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::ShellHash => "shell-hash",
            Self::PackageFile => "package-file",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalPackage {
    pub manager: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalEntry {
    pub name: String,
    pub path: String,
    pub source: LocalSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<LocalPackage>,
    /// Bank eşleşmesi varsa id; yoksa `None`. Doğrulama miras alınmaz.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bank_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bank_match: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalCommandsSnapshot {
    pub schema_version: u32,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname_hash: Option<String>,
    pub scanned_at: String,
    pub entries: Vec<LocalEntry>,
    /// Stub mü? (tam tarama değil, sınırlı örnek).
    #[serde(default)]
    pub stub: bool,
}

impl LocalCommandsSnapshot {
    pub fn empty_stub() -> Self {
        Self {
            schema_version: 1,
            kind: LOCAL_KIND.to_string(),
            hostname_hash: None,
            scanned_at: now_rfc3339(),
            entries: Vec::new(),
            stub: true,
        }
    }

    pub fn validate_contract(&self) -> Result<(), String> {
        if self.kind != LOCAL_KIND {
            return Err(format!("beklenen kind={LOCAL_KIND}, gelen={}", self.kind));
        }
        if self.schema_version != 1 {
            return Err(format!(
                "beklenen schema_version=1, gelen={}",
                self.schema_version
            ));
        }
        Ok(())
    }
}

/// PATH üzerindeki yürütülebilir dosyaları **salt okunur** tarar (stub).
///
/// - Binary çalıştırılmaz.
/// - En fazla `limit` giriş (UI stub; tam indeks değil).
/// - Bank eşleşmesi yalnız `bank_id` / `bank_match` ile; `verified` kopyalanmaz.
pub fn discover_path_stub(bank: Option<&CommandBank>, limit: usize) -> LocalCommandsSnapshot {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();

    let path_var = env::var_os("PATH").unwrap_or_default();
    for dir in env::split_paths(&path_var) {
        if entries.len() >= limit {
            break;
        }
        scan_dir(&dir, bank, limit, &mut seen, &mut entries);
    }

    LocalCommandsSnapshot {
        schema_version: 1,
        kind: LOCAL_KIND.to_string(),
        hostname_hash: None,
        scanned_at: now_rfc3339(),
        entries,
        stub: true,
    }
}

fn scan_dir(
    dir: &Path,
    bank: Option<&CommandBank>,
    limit: usize,
    seen: &mut BTreeSet<String>,
    entries: &mut Vec<LocalEntry>,
) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        if entries.len() >= limit {
            return;
        }
        let path: PathBuf = ent.path();
        let Ok(meta) = ent.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o111 == 0 {
                continue;
            }
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !seen.insert(name.to_string()) {
            continue;
        }
        let (bank_id, bank_match) = match bank.and_then(|b| b.find_by_id_or_name(name)) {
            Some(cmd) => (Some(cmd.id.clone()), Some("exact-name".into())),
            None => (None, None),
        };
        entries.push(LocalEntry {
            name: name.to_string(),
            path: path.display().to_string(),
            source: LocalSource::Path,
            package: None,
            bank_id,
            bank_match,
        });
    }
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Basit UTC zaman damgası (harici chrono bağımlılığı yok).
    format!("{secs}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stub_has_local_kind() {
        let s = LocalCommandsSnapshot::empty_stub();
        assert_eq!(s.kind, LOCAL_KIND);
        s.validate_contract().unwrap();
        assert!(s.stub);
        assert!(s.entries.is_empty());
    }

    #[test]
    fn discover_stub_does_not_claim_bank_kind() {
        let s = discover_path_stub(None, 8);
        assert_eq!(s.kind, LOCAL_KIND);
        assert_ne!(s.kind, crate::commands::bank::BANK_KIND);
        assert!(s.stub);
        assert!(s.entries.len() <= 8);
    }
}
