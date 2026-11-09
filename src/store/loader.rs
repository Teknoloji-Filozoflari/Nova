//! Mağaza kataloğu yükleyici.
//!
//! `assets/store/*.toml` dosyalarını (kategoriye göre ayrılmış) okur,
//! ayrıştırır ve [`Store`] modeline çevirir. Katalog verisi UI koduna
//! gömülmez; ayrı TOML dosyalarından `include_str!` ile yüklenir.

#[allow(unused_imports)]
use crate::store::{Store, StoreEntry, StoreFile, STORE_CATEGORIES};

/// Gömülü mağaza kaynakları: `(görünen ad, içerik)`.
///
/// Sıra [`STORE_CATEGORIES`] ile uyumludur.
pub const BUILTIN_STORE_SOURCES: &[(&str, &str)] = &[
    (
        "assets/store/ai.toml",
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/store/ai.toml")),
    ),
    (
        "assets/store/git.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/git.toml"
        )),
    ),
    (
        "assets/store/files.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/files.toml"
        )),
    ),
    (
        "assets/store/dev.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/dev.toml"
        )),
    ),
    (
        "assets/store/editors.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/editors.toml"
        )),
    ),
    (
        "assets/store/system.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/system.toml"
        )),
    ),
    (
        "assets/store/panels.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/panels.toml"
        )),
    ),
    (
        "assets/store/network.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/network.toml"
        )),
    ),
    (
        "assets/store/container.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/container.toml"
        )),
    ),
    (
        "assets/store/database.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/database.toml"
        )),
    ),
    (
        "assets/store/security.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/security.toml"
        )),
    ),
    (
        "assets/store/media.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/media.toml"
        )),
    ),
    (
        "assets/store/messaging.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/messaging.toml"
        )),
    ),
    (
        "assets/store/productivity.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/productivity.toml"
        )),
    ),
    (
        "assets/store/games.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/games.toml"
        )),
    ),
    (
        "assets/store/screensavers.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/screensavers.toml"
        )),
    ),
    (
        "assets/store/web.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/web.toml"
        )),
    ),
    (
        "assets/store/daily.toml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/store/daily.toml"
        )),
    ),
];

/// Gömülü mağaza kataloğunu yükler.
///
/// `assets/store/*.toml` dosyaları `include_str!` ile derlenir, sırayla
/// ayrıştırılır ve tek bir [`Store`] modelinde birleştirilir.
pub fn load_builtin_store() -> Result<Store, Box<dyn std::error::Error>> {
    let mut apps = Vec::new();
    for (name, text) in BUILTIN_STORE_SOURCES {
        let store_file: StoreFile =
            toml::from_str(text).map_err(|e| format!("'{name}' geçerli TOML değil: {e}"))?;
        apps.extend(store_file.apps);
    }
    Store::from_entries(apps).map_err(|e| e.into())
}

/// Uygulama açılışı için mağaza kataloğu.
///
/// Önce diskteki son **imzası doğrulanmış** Nova-Store kopyasını dener;
/// yoksa veya imza bozulmuşsa gömülü `assets/store` kullanılır. Uzak
/// ağ çağrısı yapmaz — bkz. [`crate::store::remote`].
pub fn load_store() -> Store {
    crate::store::remote::load_trusted_store()
}

#[cfg(test)]
mod tests {
    use super::{load_builtin_store, BUILTIN_STORE_SOURCES, STORE_CATEGORIES};

    #[test]
    fn builtin_store_loads_with_entries() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        assert!(!store.entries().is_empty());
    }

    #[test]
    fn builtin_store_sources_match_store_categories() {
        assert_eq!(
            BUILTIN_STORE_SOURCES.len(),
            STORE_CATEGORIES.len(),
            "her mağaza kategorisinin bir TOML dosyası olmalı"
        );
        for (name, text) in BUILTIN_STORE_SOURCES {
            assert!(name.starts_with("assets/store/"), "{name}");
            assert!(name.ends_with(".toml"), "{name}");
            let _: crate::store::StoreFile =
                toml::from_str(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }

    #[test]
    fn every_store_entry_has_a_known_category() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        for entry in store.entries() {
            assert!(
                STORE_CATEGORIES.contains(&entry.category.as_str()),
                "tanımsız mağaza kategorisi: {} ({})",
                entry.category,
                entry.id
            );
        }
    }

    #[test]
    fn store_entry_ids_are_unique() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        let ids: Vec<&str> = store.entries().iter().map(|e| e.id.as_str()).collect();
        let len = ids.len();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), len, "mağaza id'leri benzersiz olmalı");
    }

    #[test]
    fn store_search_filters_by_name_description_category_and_source() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        let results = store.search("ripgrep");
        assert!(!results.is_empty(), "ripgrep bulunmalı");
        let results = store.search("Git");
        assert!(!results.is_empty(), "Git kategorisinde araç olmalı");
        assert!(store.search("").is_empty());
    }

    #[test]
    fn store_by_category_filters_correctly() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        let sys = store.by_category("Sistem");
        assert!(!sys.is_empty());
        assert!(sys.iter().all(|e| e.category == "Sistem"));
        assert!(store.by_category("Yok Böyle Bir Kategori").is_empty());
    }

    #[test]
    fn builtin_store_archive_entries_are_fully_described() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        let archives: Vec<_> = store
            .entries()
            .iter()
            .filter(|e| e.install_method.is_archive())
            .collect();
        assert!(!archives.is_empty(), "indirilebilir en az bir kayıt olmalı");
        for entry in archives {
            assert!(
                entry
                    .version
                    .as_deref()
                    .is_some_and(|v| !v.trim().is_empty()),
                "{}: sürüm belirtilmeli",
                entry.id
            );
            if entry.artifacts.is_empty() {
                let url = entry.download_url.as_deref().unwrap_or_default();
                assert!(url.starts_with("https://"), "{}: {}", entry.id, url);
                let sum = entry.checksum.as_deref().unwrap_or_default();
                assert!(
                    crate::store::is_sha256_hex(sum),
                    "{}: geçersiz checksum '{}'",
                    entry.id,
                    sum
                );
            } else {
                assert!(
                    entry.artifacts.len() >= 2,
                    "{}: x86_64 ve aarch64 artifact beklenir",
                    entry.id
                );
                for art in &entry.artifacts {
                    assert!(
                        art.download_url.starts_with("https://"),
                        "{} ({}): {}",
                        entry.id,
                        art.arch,
                        art.download_url
                    );
                    assert!(
                        crate::store::is_sha256_hex(&art.checksum),
                        "{} ({}): geçersiz checksum '{}'",
                        entry.id,
                        art.arch,
                        art.checksum
                    );
                    assert!(
                        art.cpu_arch().is_some(),
                        "{}: arch '{}'",
                        entry.id,
                        art.arch
                    );
                }
                assert!(
                    entry.supports_arch(crate::store::CpuArch::X86_64)
                        && entry.supports_arch(crate::store::CpuArch::Aarch64),
                    "{}: her iki mimari desteklenmeli",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn builtin_store_package_manager_entries_have_commands() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        let pms: Vec<_> = store
            .entries()
            .iter()
            .filter(|e| e.install_method.is_package_manager())
            .collect();
        assert!(
            pms.len() >= 50,
            "paket yöneticisiyle kurulabilir kayıtlar beklenir, bulunan {}",
            pms.len()
        );
        for entry in pms {
            let argv = entry.install_command.as_deref().unwrap_or_default();
            crate::store::validate_install_command(entry.install_method, argv)
                .unwrap_or_else(|e| panic!("{}: {e}", entry.id));
        }
    }

    #[test]
    fn builtin_store_manual_entries_have_install_notes() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        for entry in store.entries() {
            if entry.install_method.is_installable() {
                continue;
            }
            assert!(
                entry
                    .install_notes
                    .as_deref()
                    .is_some_and(|n| !n.trim().is_empty()),
                "{}: resmî kurulum yönergesi olmalı",
                entry.id
            );
        }
    }

    #[test]
    fn builtin_store_release_sources_are_valid() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        let version_tracked = store
            .entries()
            .iter()
            .filter(|e| e.release.as_ref().is_some_and(|r| r.tracks_releases()))
            .count();
        assert!(version_tracked > 0, "canlı sürüm takibi için kayıt olmalı");
        for entry in store.entries() {
            let Some(release) = &entry.release else {
                continue;
            };
            release
                .validate()
                .unwrap_or_else(|e| panic!("{}: {e}", entry.id));
            if release.tracks_releases() {
                release
                    .validate_for_resolve()
                    .unwrap_or_else(|e| panic!("{}: {e}", entry.id));
            }
            // Depo mümkünse kaynak / repository_url ile tutarlı olmalı.
            let repo_ok = entry.source.contains(&release.repo)
                || entry
                    .repository_url
                    .as_deref()
                    .is_some_and(|u| u.contains(&release.repo));
            assert!(
                repo_ok,
                "{}: depo '{}' kaynak/repository_url ile uyuşmuyor",
                entry.id, release.repo
            );
        }
    }

    #[test]
    fn builtin_store_release_sources_cover_archive_entries() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        for entry in store.entries() {
            if !entry.install_method.is_archive() {
                continue;
            }
            assert!(
                entry.release.as_ref().is_some_and(|r| r.tracks_releases()),
                "{}: arşiv kayıtlarının canlı takip kaynağı (asset_pattern) olmalı",
                entry.id
            );
        }
    }

    #[test]
    fn builtin_store_pinned_checksum_matches_the_release_source() {
        // Gömülü özet, canlı takipte çözülecek özetle aynı kaynaktan gelir;
        // bu yüzden ikisi de "hepsi ya da hiç" olmalıdır.
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        for entry in store.entries() {
            let Some(release) = &entry.release else {
                continue;
            };
            if !release.tracks_releases() {
                continue; // yıldız-only release.repo
            }
            let has_top = entry.checksum.is_some();
            let has_artifacts = !entry.artifacts.is_empty()
                && entry
                    .artifacts
                    .iter()
                    .all(|a| crate::store::is_sha256_hex(&a.checksum));
            assert!(
                has_top || has_artifacts,
                "{}: gömülü özet (üst alan veya artifacts) olmalı",
                entry.id
            );
            assert!(entry.version.is_some(), "{}: sürüm olmalı", entry.id);
        }
    }

    #[test]
    fn builtin_store_linux_entries_declare_both_arches() {
        // macOS/Windows-only kayıtlar (OS etiketi) Linux mimarisini iddia etmez.
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        for entry in store.entries() {
            let has_linux = entry
                .supported_platforms
                .iter()
                .any(|p| p.starts_with("linux-"));
            if !has_linux {
                continue;
            }
            assert!(
                entry.supports_arch(crate::store::CpuArch::X86_64),
                "{}: linux-x64 / x86_64",
                entry.id
            );
            assert!(
                entry.supports_arch(crate::store::CpuArch::Aarch64),
                "{}: linux-aarch64 / aarch64",
                entry.id
            );
        }
    }
}
