//! Özel araç formu: alanlar, doğrulama ve gözden geçirme özeti.
//!
//! Form **hiçbir dosya sistemi değişikliği yapmaz**. Yalnızca kullanıcının
//! girdiği değerleri tutar, doğrular ve kaydetmeden önce gösterilecek özeti
//! üretir. Kaydetme kararı [`crate::app::App`] tarafındadır.
//!
//! Güven sınırı: Nova komutu kabuk üzerinden çalıştırmaz; argümanlar tek tek
//! `std::process::Command`'a verilir. Formda tırnak, genişletme veya boru
//! yorumlanmaz.

use crate::catalog::CATEGORIES;
use crate::config::{LaunchKind, ToolEntry};

/// Formdaki tek bir alan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Görünen ad.
    Name,
    /// Kısa açıklama.
    Description,
    /// Kategori (seçici).
    Category,
    /// Çalıştırılabilir program.
    Executable,
    /// Boşlukla ayrılmış argümanlar.
    Args,
    /// Çalışma dizini (boş = ev dizini).
    WorkingDir,
    /// Çalıştırma türü.
    LaunchKind,
}

/// Alanların odak sırası.
pub const FIELD_ORDER: [Field; 7] = [
    Field::Name,
    Field::Description,
    Field::Category,
    Field::Executable,
    Field::Args,
    Field::WorkingDir,
    Field::LaunchKind,
];

impl Field {
    /// Ekranda gösterilen etiket.
    pub fn label(self) -> &'static str {
        match self {
            Field::Name => "Görünen ad",
            Field::Description => "Açıklama",
            Field::Category => "Kategori",
            Field::Executable => "Program",
            Field::Args => "Argümanlar",
            Field::WorkingDir => "Çalışma dizini",
            Field::LaunchKind => "Çalıştırma türü",
        }
    }

    /// Seçici alanı mı (Left/Right ile değişir)?
    pub fn is_picker(self) -> bool {
        matches!(self, Field::Category | Field::LaunchKind)
    }
}

/// Formda girilen değerler ve doğrulama durumu.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolForm {
    /// Görünen ad.
    pub name: String,
    /// Açıklama.
    pub description: String,
    /// Kategori indeksi (`CATEGORIES` içinde).
    pub category_index: usize,
    /// Çalıştırılabilir program.
    pub executable: String,
    /// Ham argüman satırı (boşlukla ayrılır).
    pub args: String,
    /// Çalışma dizini metni (boş = ev dizini).
    pub working_dir: String,
    /// Programın çalıştırılma türü.
    pub launch_kind: LaunchKind,
    /// Odaklı alan indeksi (`FIELD_ORDER`).
    pub focus: usize,
    /// Alan başına doğrulama hatası (gösterim sırası `FIELD_ORDER`).
    pub errors: Vec<Option<String>>,
    /// Son denemeden kalan genel hata (doğrulama hiç çalışmadıysa).
    pub general_error: Option<String>,
}

impl Default for ToolForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            category_index: CATEGORIES.len() - 1, // "Özel Araçlar"
            executable: String::new(),
            args: String::new(),
            working_dir: String::new(),
            launch_kind: LaunchKind::CustomCommand,
            focus: 0,
            errors: vec![None; FIELD_ORDER.len()],
            general_error: None,
        }
    }
}

/// Doğrulanmış form çıktısı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidTool {
    /// Görünen ad (kırpılmış).
    pub name: String,
    /// Açıklama (kırpılmış; boş olabilir).
    pub description: String,
    /// Kategori.
    pub category: String,
    /// Program.
    pub executable: String,
    /// Ayrıştırılmış argümanlar.
    pub args: Vec<String>,
    /// Çalışma dizini (boş = yok).
    pub working_dir: String,
    /// Seçilen çalıştırma türü.
    pub launch_kind: LaunchKind,
}

impl ToolForm {
    /// Şu an odaklı alan.
    pub fn focused_field(&self) -> Field {
        FIELD_ORDER[self.focus.min(FIELD_ORDER.len() - 1)]
    }

    /// Odaklı alanın metin değeri.
    pub fn focused_text_mut(&mut self) -> Option<&mut String> {
        match self.focused_field() {
            Field::Name => Some(&mut self.name),
            Field::Description => Some(&mut self.description),
            Field::Executable => Some(&mut self.executable),
            Field::Args => Some(&mut self.args),
            Field::WorkingDir => Some(&mut self.working_dir),
            Field::Category | Field::LaunchKind => None,
        }
    }

    /// Bir sonraki alana geçer (sondan başa döner).
    pub fn next_field(&mut self) {
        self.focus = (self.focus + 1) % FIELD_ORDER.len();
    }

    /// Bir önceki alana geçer.
    pub fn prev_field(&mut self) {
        self.focus = (self.focus + FIELD_ORDER.len() - 1) % FIELD_ORDER.len();
    }

    /// Kategori seçicisini ilerletir/geriletir.
    pub fn cycle_category(&mut self, forward: bool) {
        let n = CATEGORIES.len();
        self.category_index = if forward {
            (self.category_index + 1) % n
        } else {
            (self.category_index + n - 1) % n
        };
    }

    /// Formda kullanılabilen program türleri arasında geçiş yapar.
    pub fn cycle_launch_kind(&mut self) {
        self.launch_kind = match self.launch_kind {
            LaunchKind::CustomCommand => LaunchKind::Interactive,
            LaunchKind::Interactive => LaunchKind::CustomCommand,
            // Bilgi türleri ek sabit metin gerektirdiğinden TOML üzerinden
            // tanımlanır; bu formda seçilemez.
            LaunchKind::CommandHelp | LaunchKind::ShellIntegrationInfo => LaunchKind::CustomCommand,
        };
    }

    /// Seçili kategori adı.
    pub fn category(&self) -> &'static str {
        CATEGORIES[self.category_index.min(CATEGORIES.len() - 1)]
    }

    /// Argüman satırını ayırır.
    ///
    /// Yalnızca boşlukla bölme yapılır: kabuk yok, tırnak yok, genişletme
    /// yok. Böylece kullanıcının yazdığı `*` veya `$HOME` olduğu gibi
    /// argüman olarak geçer.
    pub fn parsed_args(&self) -> Vec<String> {
        self.args
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    }

    /// Formu doğrular.
    ///
    /// Hatalar alan bazında `errors` içine yazılır; tümü boşsa `Ok` döner.
    /// Doğrulama **dosya sistemi değiştirmez**; yalnızca `PATH`'te arama ve
    /// `metadata` okuması yapılır.
    pub fn validate(&mut self) -> Result<ValidTool, Vec<String>> {
        let mut errors: Vec<Option<String>> = vec![None; FIELD_ORDER.len()];

        let name = self.name.trim().to_string();
        if name.is_empty() {
            errors[0] = Some("Ad boş olamaz.".into());
        }

        let executable = self.executable.trim().to_string();
        if executable.is_empty() {
            errors[3] = Some(
                "Program boş olamaz (ör. 'htop'). Çalıştırma durumunu Nova \
                 başlatma sırasında kontrol eder."
                    .into(),
            );
        } else if executable.contains('/') || executable.contains('\\') {
            errors[3] = Some(
                "Program yalnızca ad olmalı (ör. 'htop'); yolu yazma. Nova programı \
                 PATH üzerinde arar."
                    .into(),
            );
        } else if executable.contains(char::is_whitespace) {
            errors[3] = Some("Program adı boşluk içeremez; yolu 'Argümanlar' alanına yaz.".into());
        }

        if self.launch_kind == LaunchKind::Interactive && !self.parsed_args().is_empty() {
            errors[4] = Some(
                "Etkileşimli tür bu formda argüman almaz; argümanları geçirmek için özel komut türünü seç."
                    .into(),
            );
        }

        let working_dir = self.working_dir.trim().to_string();
        if !working_dir.is_empty() {
            if let Err(e) = crate::app::state::normalize_project_path(&working_dir) {
                errors[5] = Some(e);
            }
        }

        self.errors = errors.clone();
        self.general_error = None;

        let messages: Vec<String> = errors.into_iter().flatten().collect();
        if !messages.is_empty() {
            return Err(messages);
        }
        Ok(ValidTool {
            name,
            description: self.description.trim().to_string(),
            category: self.category().to_string(),
            executable,
            args: self.parsed_args(),
            working_dir,
            launch_kind: self.launch_kind,
        })
    }

    /// Doğrulanmış formdan katalog girdisi üretir (kaydetmeden önce özet
    /// için).
    ///
    /// `id` çakışmaları [`unique_id`] ile çözülür.
    pub fn to_entry(&self, valid: &ValidTool, existing: &[String]) -> ToolEntry {
        let id = unique_id(&valid.name, existing);
        let (working_directory, working_dir_path) = if valid.working_dir.is_empty() {
            ("home".to_string(), None)
        } else {
            ("custom".to_string(), Some(valid.working_dir.clone()))
        };
        ToolEntry {
            id,
            name: valid.name.clone(),
            description: valid.description.clone(),
            category: valid.category.clone(),
            executable: valid.executable.clone(),
            args: valid.args.clone(),
            launch_kind: valid.launch_kind,
            launch_mode: "terminal".into(),
            working_directory,
            working_dir_path,
            ..ToolEntry::default()
        }
    }
}

/// Gözden geçirme özetinin tek satırı.
///
/// Değerler kaydedilecek biçimde (tırnaksız, kabuksuz) gösterilir; kullanıcı
/// tam olarak ne kaydedeceğini görür.
pub fn review_lines(valid: &ValidTool) -> Vec<(String, String)> {
    vec![
        ("Ad".to_string(), valid.name.clone()),
        ("Kategori".to_string(), valid.category.clone()),
        (
            "Program".to_string(),
            if valid.args.is_empty() {
                valid.executable.clone()
            } else {
                format!("{} {}", valid.executable, valid.args.join(" "))
            },
        ),
        (
            "Çalışma dizini".to_string(),
            if valid.working_dir.is_empty() {
                "ev dizini".to_string()
            } else {
                valid.working_dir.clone()
            },
        ),
        (
            "Çalıştırma türü".to_string(),
            format!("terminal sekmesi ({})", valid.launch_kind.label()),
        ),
        (
            "Açıklama".to_string(),
            if valid.description.is_empty() {
                "—".to_string()
            } else {
                valid.description.clone()
            },
        ),
    ]
}

/// Addan benzersiz bir kimlik üretir.
///
/// Kimlik yalnızca ASCII küçük harf, rakam ve `-` içerir. Çakışma olursa
/// `-2`, `-3`… eklenir. Ada hiç harf kalmazsa `"ozel-arac"` taban kullanılır.
pub fn unique_id(name: &str, existing: &[String]) -> String {
    let base = slugify(name);
    if !existing.iter().any(|e| e == &base) {
        return base;
    }
    for n in 2..u32::MAX {
        let candidate = format!("{base}-{n}");
        if !existing.iter().any(|e| e == &candidate) {
            return candidate;
        }
    }
    base
}

/// Metni kimlik biçimine çevirir.
///
/// Türkçe karakterler ASCII'ye katlanır (ı→i, ş→s, ğ→g, ü→u, ö→o, ç→c);
/// diğer harfler küçük harfe indirilir. Boşluk ve noktalama tek bir `-`
/// olur; baştaki/sondaki `-` atılır.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true; // baştaki ayırıcıları at
    for ch in name.chars() {
        let folded: Option<char> = match ch {
            'ı' | 'İ' | 'i' | 'I' => Some('i'),
            'ş' | 'Ş' | 's' | 'S' => Some('s'),
            'ğ' | 'Ğ' | 'g' | 'G' => Some('g'),
            'ü' | 'Ü' | 'u' | 'U' => Some('u'),
            'ö' | 'Ö' | 'o' | 'O' => Some('o'),
            'ç' | 'Ç' | 'c' | 'C' => Some('c'),
            _ => None,
        };
        if let Some(f) = folded {
            if !last_dash || out.is_empty() {
                out.push(f);
            }
            last_dash = false;
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "ozel-arac".into()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_form() -> ToolForm {
        ToolForm {
            name: "Disk Temizleyici".into(),
            description: "Gereksiz dosyaları bulur".into(),
            executable: "duf".into(),
            args: "  --only  local ".into(),
            ..ToolForm::default()
        }
    }

    #[test]
    fn slugify_folds_turkish_characters() {
        assert_eq!(slugify("Çöp Kutusu ŞĞÜİÖ"), "cop-kutusu-guio");
        assert_eq!(slugify("  ../x  "), "x");
        assert_eq!(slugify("!!!"), "ozel-arac");
    }

    #[test]
    fn unique_id_appends_counter_on_clash() {
        let existing = vec!["disk-temizleyici".to_string()];
        assert_eq!(
            unique_id("Disk Temizleyici", &existing),
            "disk-temizleyici-2"
        );
        assert_eq!(
            unique_id(
                "Disk Temizleyici",
                &["disk-temizleyici".into(), "disk-temizleyici-2".into()]
            ),
            "disk-temizleyici-3"
        );
    }

    #[test]
    fn blank_name_is_rejected() {
        let mut f = valid_form();
        f.name = "  ".into();
        let err = f.validate().unwrap_err();
        assert!(err.iter().any(|e| e.contains("Ad boş")));
        assert!(f.errors[0].is_some());
    }

    #[test]
    fn blank_executable_is_rejected_even_though_detection_says_unknown() {
        let mut f = valid_form();
        f.executable = String::new();
        let err = f.validate().unwrap_err();
        assert!(err.iter().any(|e| e.contains("Program boş")));
    }

    #[test]
    fn valid_form_parses_arguments_without_shell() {
        let mut f = valid_form();
        let valid = f.validate().unwrap();
        assert_eq!(valid.args, vec!["--only", "local"]);
        assert_eq!(valid.category, "Özel Araçlar");
        assert_eq!(valid.working_dir, "");
    }

    #[test]
    fn entry_uses_custom_working_directory_only_when_given() {
        let mut f = valid_form();
        let valid = f.validate().unwrap();
        let entry = f.to_entry(&valid, &[]);
        assert_eq!(entry.working_directory, "home");
        assert!(entry.working_dir_path.is_none());

        f.working_dir = "/tmp".into();
        let valid = f.validate().unwrap();
        let entry = f.to_entry(&valid, &[]);
        assert_eq!(entry.working_directory, "custom");
        assert_eq!(entry.working_dir_path.as_deref(), Some("/tmp"));
        assert_eq!(entry.launch_kind, LaunchKind::CustomCommand);
    }

    #[test]
    fn launch_kind_picker_selects_interactive_and_validates_its_arguments() {
        let mut f = valid_form();
        assert_eq!(f.launch_kind, LaunchKind::CustomCommand);
        f.cycle_launch_kind();
        assert_eq!(f.launch_kind, LaunchKind::Interactive);
        assert!(f
            .validate()
            .unwrap_err()
            .iter()
            .any(|error| error.contains("argüman almaz")));

        f.args.clear();
        let valid = f.validate().unwrap();
        assert_eq!(valid.launch_kind, LaunchKind::Interactive);
        assert_eq!(f.to_entry(&valid, &[]).launch_kind, LaunchKind::Interactive);
        f.cycle_launch_kind();
        assert_eq!(f.launch_kind, LaunchKind::CustomCommand);
    }

    #[test]
    fn category_picker_wraps_and_never_leaves_list() {
        let mut f = ToolForm::default();
        assert_eq!(f.category(), "Özel Araçlar");
        f.cycle_category(true);
        assert_eq!(f.category(), CATEGORIES[0]);
        f.cycle_category(false);
        assert_eq!(f.category(), "Özel Araçlar");
    }

    #[test]
    fn review_shows_exact_program_and_args() {
        let mut f = valid_form();
        let valid = f.validate().unwrap();
        let lines = review_lines(&valid);
        assert!(lines
            .iter()
            .any(|(k, v)| k == "Program" && v == "duf --only local"));
        assert!(lines
            .iter()
            .any(|(k, v)| k == "Çalışma dizini" && v == "ev dizini"));
        assert!(lines
            .iter()
            .any(|(k, v)| k == "Çalıştırma türü" && v.contains("özel komut")));
    }

    #[test]
    fn field_order_covers_every_editor_field() {
        // Kategori ve çalıştırma türü seçicidir; beş alan metin alır.
        let mut f = ToolForm::default();
        let mut with_text = 0;
        for i in 0..FIELD_ORDER.len() {
            f.focus = i;
            if f.focused_text_mut().is_some() {
                with_text += 1;
            }
        }
        assert_eq!(with_text, 5);
    }
}
