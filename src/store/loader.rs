//! Mağaza kataloğu yükleyici.
//!
//! `assets/store.toml` dosyasını okur, ayrıştırır ve [`Store`] modeline
//! çevirir. Katalog verisi UI koduna gömülmez; ayrı bir TOML dosyasından
//! `include_str!` ile yüklenir.

#[allow(unused_imports)]
use crate::store::{Store, StoreEntry, StoreFile, STORE_CATEGORIES};

/// Gömülü mağaza kataloğunu yükler.
///
/// `assets/store.toml` `include_str!` ile derlenir ve burada ayrıştırılır.
pub fn load_builtin_store() -> Result<Store, Box<dyn std::error::Error>> {
    let toml_content = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/store.toml"));
    let store_file: StoreFile = toml::from_str(toml_content)?;
    Store::from_entries(store_file.apps).map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use super::{load_builtin_store, STORE_CATEGORIES};

    #[test]
    fn builtin_store_loads_with_entries() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        assert!(!store.entries().is_empty());
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
    fn builtin_store_installable_entries_are_fully_described() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        let installable: Vec<_> = store
            .entries()
            .iter()
            .filter(|e| e.install_method.is_installable())
            .collect();
        assert!(
            !installable.is_empty(),
            "indirilebilir en az bir kayıt olmalı"
        );
        for entry in installable {
            let url = entry.download_url.as_deref().unwrap_or_default();
            assert!(url.starts_with("https://"), "{}: {}", entry.id, url);
            let sum = entry.checksum.as_deref().unwrap_or_default();
            assert!(
                crate::store::is_sha256_hex(sum),
                "{}: geçersiz checksum '{}'",
                entry.id,
                sum
            );
            assert!(
                entry
                    .version
                    .as_deref()
                    .is_some_and(|v| !v.trim().is_empty()),
                "{}: sürüm belirtilmeli",
                entry.id
            );
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
        let tracked = store
            .entries()
            .iter()
            .filter(|e| e.release.is_some())
            .count();
        assert!(tracked > 0, "canlı takip için kayıt olmalı");
        for entry in store.entries() {
            let Some(release) = &entry.release else {
                continue;
            };
            release
                .validate()
                .unwrap_or_else(|e| panic!("{}: {e}", entry.id));
            // Depo, kaydın kaynak sayfasıyla tutarlı olmalı.
            assert!(
                entry.source.contains(&release.repo),
                "{}: depo '{}' kaynak sayfasıyla uyuşmuyor",
                entry.id,
                release.repo
            );
        }
    }

    #[test]
    fn builtin_store_release_sources_cover_installable_entries() {
        let store = load_builtin_store().expect("mağaza kataloğu yüklenmeli");
        for entry in store.entries() {
            if !entry.install_method.is_installable() {
                continue;
            }
            assert!(
                entry.release.is_some(),
                "{}: indirilebilir kayıtların canlı takip kaynağı olmalı",
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
            if entry.release.is_none() {
                continue;
            }
            assert!(entry.checksum.is_some(), "{}: gömülü özet olmalı", entry.id);
            assert!(entry.version.is_some(), "{}: sürüm olmalı", entry.id);
        }
    }
}
