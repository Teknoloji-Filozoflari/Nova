//! Mağaza kurulumlarını ana araç kataloğuna senkronize etme.
//!
//! Mağaza (`assets/store/*.toml`) ile ana katalog (`assets/catalog/*.toml`) ayrıdır. Kurulum
//! sonrası mağazada ✓ görünen bir uygulama AI vb. sol kategoride yoksa
//! kullanıcı oradan başlatamaz. Bu modül:
//!
//! 1. Aynı `id` veya `executable` ile katalogda eşleşme varsa yalnızca
//!    tespit yenilenir (zaten listelenir).
//! 2. Eşleşme yoksa mağaza kaydından kullanıcı araç girişi üretilir
//!    (`state.toml` `custom_tools`) ve ilgili katalog kategorisine eklenir.

use crate::config::{LaunchKind, ToolAction, ToolEntry};
use crate::store::StoreEntry;

/// Bilinen AI CLI'ler için varsayılan eylemler (mağazadan senkronize edilenler).
fn default_ai_actions(id: &str, executable: &str) -> Vec<ToolAction> {
    let key = if id.is_empty() { executable } else { id };
    let key = key.to_ascii_lowercase();
    let exe = executable.to_ascii_lowercase();
    let known = match key.as_str() {
        "codex" => Some(vec![
            action("open", "Aç", &[]),
            action("resume", "Devam et", &["resume"]),
            action("logout", "Çıkış yap", &["logout"]),
            action("help", "Yardım", &["--help"]),
        ]),
        "claude" => Some(vec![
            action("open", "Aç", &[]),
            action("resume", "Devam et", &["--resume"]),
            action("logout", "Çıkış yap", &["auth", "logout"]),
            action("help", "Yardım", &["--help"]),
        ]),
        "opencode" => Some(vec![
            action("open", "Aç", &[]),
            action("resume", "Devam et", &["-c"]),
            action("logout", "Çıkış yap", &["auth", "logout"]),
            action("help", "Yardım", &["--help"]),
        ]),
        "gemini-cli" | "gemini" => Some(vec![
            action("open", "Aç", &[]),
            action("resume", "Devam et", &["--resume"]),
            action("help", "Yardım", &["--help"]),
        ]),
        _ => None,
    };
    if let Some(actions) = known {
        return actions;
    }
    if exe == "gemini" {
        return vec![
            action("open", "Aç", &[]),
            action("resume", "Devam et", &["--resume"]),
            action("help", "Yardım", &["--help"]),
        ];
    }
    Vec::new()
}

fn action(id: &str, label: &str, args: &[&str]) -> ToolAction {
    ToolAction {
        id: id.into(),
        label: label.into(),
        args: args.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// Mağaza kategorisini ana katalog `CATEGORIES` adına çevirir.
pub fn catalog_category_for_store(store_category: &str) -> &'static str {
    match store_category {
        "AI" => "AI ve Kodlama",
        "Git" | "Geliştirme" | "Editörler" => "Git ve Geliştirici Araçları",
        "Dosya ve Arama" => "Dosya Yönetimi ve Arama",
        "Sistem" | "Paneller" | "Güvenlik" | "Verimlilik" => "Sistem ve Kaynak Kullanımı",
        "Ağ" | "Web" => "Ağ ve Bağlantı",
        "Konteyner" | "Veritabanı" => "Docker ve Sunucu",
        "Günlük" => "Günlük Terminal Yardımcıları",
        "Medya" | "Mesajlaşma" | "Oyunlar" | "Ekran Koruyucular" => "Özel Araçlar",
        _ => "Özel Araçlar",
    }
}

/// Mağaza kaydından kalıcı katalog `ToolEntry` üretir.
pub fn tool_entry_from_store(entry: &StoreEntry) -> ToolEntry {
    let category = catalog_category_for_store(&entry.category).to_string();
    let actions = if category == "AI ve Kodlama" {
        default_ai_actions(&entry.id, &entry.executable)
    } else {
        Vec::new()
    };
    ToolEntry {
        id: entry.id.clone(),
        name: entry.name.clone(),
        description: entry.description.clone(),
        category,
        executable: entry.executable.clone(),
        args: Vec::new(),
        launch_kind: LaunchKind::Interactive,
        launch_mode: "terminal".into(),
        working_directory: "home".into(),
        working_dir_path: None,
        favorite: false,
        help_text: None,
        handlers: Vec::new(),
        info_text: None,
        alternative_to: None,
        examples: vec![
            format!("{}   ← {} arayüzünü açar", entry.executable, entry.name),
            format!("{} --help   ← yardım ve bayraklar", entry.executable),
            format!(
                "Nova’da Enter ile başlatın veya terminalde `{}` yazın.",
                entry.executable
            ),
        ],
        setup_info: Some(format!(
            "Mağaza’dan kuruldu ({}). Terminalde '{}' yazın; detay panelinde kullanım örneklerine bakın.",
            entry.install_method.short_label(),
            entry.executable
        )),
        actions,
    }
}

/// Katalogda mağaza kaydıyla eşleşen giriş var mı (`id` veya `executable`).
pub fn catalog_matches_store(catalog_id: &str, catalog_exe: &str, store: &StoreEntry) -> bool {
    catalog_id == store.id || (!store.executable.is_empty() && catalog_exe == store.executable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InstallMethod;

    fn sample(id: &str, category: &str, exe: &str) -> StoreEntry {
        StoreEntry {
            id: id.into(),
            name: id.into(),
            description: "d".into(),
            category: category.into(),
            source: "https://example.org/".into(),
            license: "MIT".into(),
            executable: exe.into(),
            install_method: InstallMethod::Npm,
            supported_platforms: vec!["linux-x64".into()],
            version: None,
            download_url: None,
            checksum: None,
            checksum_type: None,
            artifacts: vec![],
            install_command: Some(vec![
                "npm".into(),
                "install".into(),
                "-g".into(),
                "x".into(),
            ]),
            install_notes: None,
            release: None,
            signature: None,
        }
    }

    #[test]
    fn ai_store_maps_to_ai_catalog_category() {
        assert_eq!(catalog_category_for_store("AI"), "AI ve Kodlama");
    }

    #[test]
    fn tool_entry_keeps_store_id_and_executable() {
        let e = sample("gemini-cli", "AI", "gemini");
        let t = tool_entry_from_store(&e);
        assert_eq!(t.id, "gemini-cli");
        assert_eq!(t.executable, "gemini");
        assert_eq!(t.category, "AI ve Kodlama");
        assert_eq!(t.launch_kind, LaunchKind::Interactive);
        assert!(
            t.actions.iter().any(|a| a.id == "resume"),
            "AI mağaza senkronu bilinen aksiyonları eklemeli: {:?}",
            t.actions
        );
    }

    #[test]
    fn match_by_executable_even_when_ids_differ() {
        let store = sample("qwen-code", "AI", "qwen");
        assert!(catalog_matches_store("other-id", "qwen", &store));
        assert!(!catalog_matches_store("other", "other-exe", &store));
        assert!(catalog_matches_store("qwen-code", "anything", &store));
    }
}
