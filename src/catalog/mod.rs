//! Araç kataloğu modülü.
//! UI kodundan ayrı tutulan araç tanımları.

use crate::config::{EntryOrigin, LaunchKind, ToolAction, ToolEntry};
use crate::detection::{self, InstalledStatus};
use crate::error::NovaError;
use crate::launcher::{resolve_directory, FsValidator};
use std::path::PathBuf;

pub mod loader;

/// Bir aracın nasıl başlatılacağını belirler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaunchMode {
    #[default]
    Terminal, // Yeni bir terminal penceresi/sekmesinde aç
    Background, // Arka planda çalış
    Detached,   // Bağlantıdan bağımsız çalış
}

/// Çalışma dizini davranışı.
///
/// Yeni sekme için "geçerli dizin" kavramı anlamsızdır: Nova'nın kendi süreç
/// dizini sekmenin dizini olmamalıdır. Bu yüzden `Current` güvenli varsayılan
/// olarak **ev dizinine** çözülür.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WorkingDirectory {
    /// Güvenli varsayılan: kullanıcının ev dizini.
    #[default]
    Current,
    /// Açıkça kullanıcı ev dizini.
    Home,
    /// Araç için tanımlanmış özel bir dizin.
    Custom(PathBuf),
    /// Bir proje dizini (gelecekte proje yönetimi bunu dolduracak).
    Project(PathBuf),
}

impl WorkingDirectory {
    /// Bir giriş için çalışma dizinini **doğrulayarak** tam yola çevirir.
    ///
    /// Doğrulama Nova'da yapılır çünkü Konsole geçersiz bir `--workdir`
    /// verildiğinde hata vermez, sessizce ev dizinini kullanır (Konsole
    /// 26.08.2 üzerinde doğrulandı). Sessiz geri düşme yanıltıcı olacağı
    /// için araç özelinde geçersiz bir dizin açık hatadır.
    pub fn resolve(&self, home: &std::path::Path, tool_name: &str) -> Result<PathBuf, NovaError> {
        resolve_directory(self, home, &FsValidator, tool_name)
    }

    /// Arayüzde gösterilecek kısa etiket.
    pub fn label(&self) -> String {
        match self {
            WorkingDirectory::Current | WorkingDirectory::Home => "Ev dizini".into(),
            WorkingDirectory::Custom(path) | WorkingDirectory::Project(path) => {
                format!("Dizin: {}", path.display())
            }
        }
    }
}

/// Tek bir katalog girişi.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    /// Değişmez ve benzersiz kimlik.
    pub id: String,
    /// Ekranda gösterilecek ad.
    pub name: String,
    /// Kısa ve anlaşılır açıklama.
    pub description: String,
    /// Ait olduğu kategori.
    pub category: String,
    /// Çalıştırılacak program.
    pub executable: String,
    /// Ayrı argüman listesi.
    pub args: Vec<String>,
    /// Çalıştırma biçimi.
    pub launch_mode: LaunchMode,
    /// Varsayılan çalışma dizini davranışı.
    pub working_directory: WorkingDirectory,
    /// Başlatma türü (etkileşimli, kullanım bilgisi, bilgi görünümü, özel komut).
    pub launch_kind: LaunchKind,
    /// `command_help` için TOML'dan gelen sabit metin.
    pub help_text: Option<String>,
    /// `command_help` metnini tutacak sayfalayıcı adayları (bulunan ilki).
    pub handlers: Vec<String>,
    /// `shell_integration_info` için TOML'dan gelen açıklama.
    pub info_text: Option<String>,
    /// Bir alternatifse asıl aracın kimliği.
    pub alternative_to: Option<String>,
    /// Doğrulanmış kullanım örnekleri / kısayollar (katalog metaverisi).
    /// Kullanıcı açıkça başlatmadıkça Nova bunları çalıştırmaz.
    pub examples: Vec<String>,
    /// Kabuk entegrasyonu veya ek kurulum bilgisi. Nova tarafından yürütülmez.
    pub setup_info: Option<String>,
    /// İsteğe bağlı eylem listesi (AI CLI alt komutları vb.).
    pub actions: Vec<ToolAction>,
    /// İsteğe bağlı proje / ana sayfa URL’si (`https://`).
    pub project_url: Option<String>,
    /// İsteğe bağlı depo URL’si (`https://`).
    pub repo_url: Option<String>,
    /// Favori durumu.
    pub favorite: bool,
    /// Çalıştırılabilir dosyanın PATH üzerinden bulunduğu durumda `Some(path)`.
    pub resolved_path: Option<PathBuf>,
    /// Kurulu olma durumu (kurulu / kurulu değil / kontrol edilemedi).
    pub status: InstalledStatus,
    /// Kaydın kaynağı: gömülü katalog mu, kullanıcı dosyası mı.
    pub origin: EntryOrigin,
}

/// AI kategorisi kimliği (aksiyon paneli görünürlüğü için).
pub const AI_CATEGORY: &str = "AI ve Kodlama";

impl CatalogEntry {
    /// Bu giriş AI kategorisinde ve eylem listesi dolu mu?
    pub fn has_ai_actions(&self) -> bool {
        self.category == AI_CATEGORY && !self.actions.is_empty()
    }

    /// Seçili eylemi bu girişe uygular (başlatma için kopya üretir).
    ///
    /// Boş `args` → etkileşimli birincil açma. Dolu `args` → `custom_command`
    /// ile argv launcher'a geçer (interactive araçlarda gömülü args yok sayılır).
    pub fn with_action(&self, action: &ToolAction) -> Self {
        let mut entry = self.clone();
        if action.args.is_empty() {
            entry.args.clear();
            entry.launch_kind = LaunchKind::Interactive;
        } else {
            entry.args = action.args.clone();
            entry.launch_kind = LaunchKind::CustomCommand;
        }
        entry
    }

    /// Yeni bir giriş üretir ve çalıştırılabilir dosyanın durumunu hemen belirler.
    ///
    /// Durum her zaman bu yapıcı üzerinden atandığı için hiçbir giriş
    /// yanlışlıkla varsayılan (`Unknown`) durumda kalmaz.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        category: impl Into<String>,
        executable: impl Into<String>,
        args: Vec<String>,
        launch_mode: LaunchMode,
        working_directory: WorkingDirectory,
        favorite: bool,
    ) -> Self {
        let executable = executable.into();
        let (status, resolved_path) = detection::resolve(&executable);
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            category: category.into(),
            executable,
            args,
            launch_mode,
            working_directory,
            launch_kind: LaunchKind::Interactive,
            help_text: None,
            handlers: Vec::new(),
            info_text: None,
            alternative_to: None,
            examples: Vec::new(),
            setup_info: None,
            actions: Vec::new(),
            project_url: None,
            repo_url: None,
            favorite,
            status,
            resolved_path,
            origin: EntryOrigin::Builtin,
        }
    }

    /// Arayüzde gösterilecek, türe özgü kısa açıklama.
    ///
    /// - Etkileşimli araç: `None` (açıklama zaten detay satırında var).
    /// - Kullanım bilgisi: komutun çalışmadığı, yalnızca metin gösterildiği.
    /// - Kabuk entegrasyonu: bu görünümden başlatılamayacağı ve nedeninin.
    /// - Özel komut: hangi komutun çalışacağı.
    pub fn launch_hint(&self) -> Option<String> {
        match self.launch_kind {
            LaunchKind::Interactive => None,
            LaunchKind::CommandHelp => Some(format!(
                "Enter: yeni sekmede '{}' kullanım metni (araç çalıştırılmaz)",
                self.executable
            )),
            LaunchKind::ShellIntegrationInfo => {
                Some("Enter: kabuk entegrasyonu bilgisi — bu görünümden başlatılmaz.".to_string())
            }
            LaunchKind::CustomCommand => Some(
                format!(
                    "Enter: özel komut '{} {}'",
                    self.executable,
                    self.args.join(" ")
                )
                .trim_end()
                .to_string(),
            ),
        }
    }

    /// Aynı kimlikli kullanıcı kaydını bu kaydın üzerine uygular.
    ///
    /// Kullanıcının boş bıraktığı alanlar varsayılanı korur; kullanıcı bir
    /// alanı açıkça doldurduysa o kazanır. Kullanıcı `args`'ı açıkça yazmasa
    /// bile varsayılan kodlanmış argümanlar korunur.
    pub(crate) fn merge_user(&mut self, user: &ToolEntry) -> Result<(), String> {
        let user_launch_mode = if user.launch_mode.is_empty() {
            None
        } else {
            Some(parse_launch_mode(&user.id, &user.launch_mode)?)
        };
        let default_kind = LaunchKind::default();
        if !user.name.is_empty() {
            self.name = user.name.clone();
        }
        if !user.description.is_empty() {
            self.description = user.description.clone();
        }
        if !user.category.is_empty() {
            self.category = user.category.clone();
        }
        if !user.executable.is_empty() {
            self.executable = user.executable.clone();
        }
        if !user.args.is_empty() {
            self.args = user.args.clone();
        }
        // Verilmeyen `launch_kind` serde'de `interactive` olur; bu, varsayılan
        // her türü ezip etkileşimli yapmamalıdır. Bu yüzden varsayılana eşit
        // kullanıcı değeri yalnızca varsayılan da etkileşimliyse uygulanır:
        // gömülü kayıt zaten `interactive` ise sonuç değişmez.
        if user.launch_kind != default_kind || self.launch_kind == default_kind {
            self.launch_kind = user.launch_kind;
        }
        if let Some(launch_mode) = user_launch_mode {
            self.launch_mode = launch_mode;
        }
        // Geçersiz çalışma dizini ayarı `merge_user` çağrısından önce
        // (loader'da) doğrulanır; burada yalnızca geçerli seçim uygulanır.
        if let WorkingDirChoice::Plain(wd) = working_directory_from(user) {
            self.working_directory = wd;
        }
        if user.help_text.is_some() {
            self.help_text = user.help_text.clone();
        }
        if !user.handlers.is_empty() {
            self.handlers = user.handlers.clone();
        }
        if user.info_text.is_some() {
            self.info_text = user.info_text.clone();
        }
        if user.alternative_to.is_some() {
            self.alternative_to = user.alternative_to.clone();
        }
        if !user.examples.is_empty() {
            self.examples = user.examples.clone();
        }
        if user.setup_info.is_some() {
            self.setup_info = user.setup_info.clone();
        }
        if !user.actions.is_empty() {
            self.actions = user.actions.clone();
        }
        if user.project_url.is_some() {
            self.project_url = user.project_url.clone();
        }
        if user.repo_url.is_some() {
            self.repo_url = user.repo_url.clone();
        }
        self.favorite = user.favorite;
        self.refresh_status();
        Ok(())
    }

    /// Çalıştırma biçimini değiştirir (çalışma dizini var olsa bile).
    ///
    /// Kullanıcının Nova içinde eklediği araçlar `custom_command` biçimindedir:
    /// argümanları kendisi yazmıştır ve bu yeniden başlatmadan sonra da
    /// korunmalıdır.
    pub fn set_launch_kind(&mut self, kind: LaunchKind) {
        self.launch_kind = kind;
    }

    /// Çalıştırılabilir dosyanın durumunu yeniden belirler.
    /// Kurulum durumunu güncel tutmak için kullanılır.
    pub fn refresh_status(&mut self) {
        let (status, path) = detection::resolve(&self.executable);
        self.status = status;
        self.resolved_path = path;
    }

    /// Alanlar zaten normalize edilmiş bir sorguyla eşleşiyor mu?
    fn matches_normalized(&self, normalized_query: &str) -> bool {
        normalize(&self.name).contains(normalized_query)
            || normalize(&self.executable).contains(normalized_query)
            || normalize(&self.category).contains(normalized_query)
            || short_category_name(&self.category)
                .map(|s| normalize(s).contains(normalized_query))
                .unwrap_or(false)
            || normalize(&self.description).contains(normalized_query)
    }
}

/// Arama karşılaştırması için metni sadeleştirir.
///
/// Rust'ın `to_lowercase` fonksiyonu Türkçe metinlerde iki tuzak yaratır:
/// - `İ` harfi `i` + birleşik nokta (U+0307) olarak açılır; bu yüzden
///   "SİSTEM" araması "Sistem" ile eşleşmez.
/// - Büyük `I` ASCII `i` (U+0069) olur, ama Türkçe küçük `ı` U+0131'dir;
///   bu yüzden "BAĞLANTI" araması "Bağlantı" ile eşleşmez.
///
/// Arama amacıyla bu farklar önemsizdir; birleşik nokta atılır ve `ı`
/// yerine `i` kullanılır.
pub(crate) fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| *c != '\u{307}')
        .map(|c| if c == 'ı' { 'i' } else { c })
        .collect()
}

/// Önceden tanımlanmış kategoriler.
pub const CATEGORIES: &[&str] = &[
    "AI ve Kodlama",
    "Git ve Geliştirici Araçları",
    "Dosya Yönetimi ve Arama",
    "Sistem ve Kaynak Kullanımı",
    "Ağ ve Bağlantı",
    "Docker ve Sunucu",
    "Günlük Terminal Yardımcıları",
    "Özel Araçlar",
];

/// Görünümde gösterilen kısa kategori adları (sadece UI). `CATEGORIES`
/// kimlikleri ve yapılandırma dosyaları değişmez; yalnızca ekranda gösterilen
/// ad kısaltılır. Sırası `CATEGORIES` ile aynıdır.
pub const SHORT_CATEGORY_NAMES: &[&str] = &[
    "AI",
    "Git",
    "Dosya",
    "Sistem",
    "Ağ",
    "Docker",
    "Yardımcılar",
    "Özel",
];

/// Tam kategori adından kısa görünen adı döner; eşleşme yoksa `None`.
pub fn short_category_name(full: &str) -> Option<&'static str> {
    CATEGORIES
        .iter()
        .position(|c| *c == full)
        .and_then(|i| SHORT_CATEGORY_NAMES.get(i).copied())
}

/// Kısa görünen adından tam kategori adını döner; eşleşme yoksa `None`.
pub fn category_from_short(short: &str) -> Option<&'static str> {
    SHORT_CATEGORY_NAMES
        .iter()
        .position(|s| *s == short)
        .and_then(|i| CATEGORIES.get(i).copied())
}

/// TOML'daki çalışma dizini alanlarından bir seçim üretir.
///
/// - Alanlar hiç verilmemişse [`WorkingDirChoice::Unset`] (varsayılandan farkı
///   yok).
/// - `"custom"`/`"project"` yol olmadan verilmişse açık bir doğrulama hatası
///   üretilir ([`WorkingDirChoice::Invalid`]); sessizce `None` dönmek
///   birleştirme sırasında gömülü değeri gizlerdi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkingDirChoice {
    /// Alan hiç belirtilmemiş.
    Unset,
    /// Geçerli bir seçim.
    Plain(WorkingDirectory),
    /// Geçersiz alan; metin hata mesajında kullanılır.
    Invalid(String),
}

pub(crate) fn working_directory_from(entry: &ToolEntry) -> WorkingDirChoice {
    let path = entry
        .working_dir_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from);
    match entry.working_directory.as_str() {
        "custom" => match path {
            Some(p) => WorkingDirChoice::Plain(WorkingDirectory::Custom(p)),
            None => WorkingDirChoice::Invalid(format!(
                "'{}' için working_directory = \"custom\" ama 'working_dir_path' boş",
                entry.id
            )),
        },
        "project" => match path {
            Some(p) => WorkingDirChoice::Plain(WorkingDirectory::Project(p)),
            None => WorkingDirChoice::Invalid(format!(
                "'{}' için working_directory = \"project\" ama 'working_dir_path' boş",
                entry.id
            )),
        },
        "home" => WorkingDirChoice::Plain(WorkingDirectory::Home),
        // Boş metin "verilmemiş" demektir; yalnızca gerçek bir yol
        // `Current` için anlamlıdır.
        "" if path.is_none() => WorkingDirChoice::Unset,
        "current" | "" => WorkingDirChoice::Plain(WorkingDirectory::Current),
        other => WorkingDirChoice::Invalid(format!(
            "'{}' için bilinmeyen working_directory değeri '{}'; \
             geçerli değerler: current, home, project, custom",
            entry.id, other
        )),
    }
}

/// TOML kaydını doğrulayıp katalog girişine çevirir.
///
/// Doğrulama **süreç başlatmadan** ve **araç çalıştırmadan** yapılır:
/// yalnızca alanlar tutarlı mı, kategori bilinen kategorilerden biri mi ve
/// `command_help` metni var mı diye bakılır. Hata mesajı her zaman kaydın
/// kimliğini anar. `name` boşsa `id` kullanılır, yoksa ekranda adsız araç
/// görünürdü.
pub fn catalog_entry_from_tool(t: &ToolEntry) -> Result<CatalogEntry, String> {
    if t.id.trim().is_empty() {
        return Err("bir araç kaydının 'id' alanı boş olamaz".to_string());
    }
    if !CATEGORIES.contains(&t.category.as_str()) {
        return Err(format!(
            "'{}' için bilinmeyen kategori '{}'; geçerli kategoriler: {}",
            t.id,
            t.category,
            CATEGORIES.join(", ")
        ));
    }
    let executable = if t.executable.trim().is_empty() {
        t.id.clone()
    } else {
        t.executable.clone()
    };
    // `executable` bir program adı (`ssh`) veya mutlak dosya yolu
    // (`~/.minimax-code/bin/mcode`) olabilir. `detection::resolve` PATH'te
    // arar ve `/` içeren bir girdi için dosya varlığı ve çalıştırılabilirliği
    // güvenli bir şekilde kontrol eder; çalıştırılmadan önce her şey
    // doğrulanır.
    for arg in &t.args {
        // Argümanlar kabuk olmadan ayrı ayrı geçirilir; boş bir argüman
        // genellikle yazım hatasıdır ve programa beklenmedik bir değer gider.
        if arg.is_empty() {
            return Err(format!(
                "'{}' için boş bir 'args' öğesi var; argümanlar boş olamaz",
                t.id
            ));
        }
    }
    for (i, action) in t.actions.iter().enumerate() {
        if action.id.trim().is_empty() {
            return Err(format!("'{}' actions[{i}] için 'id' boş olamaz", t.id));
        }
        if action.label.trim().is_empty() {
            return Err(format!(
                "'{}' actions[{i}] ({}) için 'label' boş olamaz",
                t.id, action.id
            ));
        }
        for arg in &action.args {
            if arg.is_empty() {
                return Err(format!(
                    "'{}' actions[{i}] ({}) için boş bir 'args' öğesi var",
                    t.id, action.id
                ));
            }
        }
    }
    if t.launch_kind == LaunchKind::CommandHelp {
        match t.help_text.as_deref() {
            Some(text) if !text.trim().is_empty() => {}
            _ => {
                return Err(format!(
                    "'{}' launch_kind = \"command_help\" ama 'help_text' boş; \
                     gösterilecek sabit kullanım metni gerekli",
                    t.id
                ));
            }
        }
    }
    let launch_mode = parse_launch_mode(&t.id, &t.launch_mode)?;
    let working_directory = match working_directory_from(t) {
        WorkingDirChoice::Plain(dir) => dir,
        WorkingDirChoice::Invalid(err) => return Err(err),
        WorkingDirChoice::Unset => WorkingDirectory::Current,
    };
    let mut entry = CatalogEntry::new(
        t.id.clone(),
        if t.name.trim().is_empty() {
            t.id.clone()
        } else {
            t.name.clone()
        },
        t.description.clone(),
        t.category.clone(),
        executable,
        t.args.clone(),
        launch_mode,
        working_directory,
        t.favorite,
    );
    entry.launch_kind = t.launch_kind;
    entry.help_text = t.help_text.clone();
    entry.handlers = t.handlers.clone();
    entry.info_text = t.info_text.clone();
    entry.alternative_to = t.alternative_to.clone();
    entry.examples = t.examples.clone();
    entry.setup_info = t.setup_info.clone();
    entry.actions = t.actions.clone();
    entry.project_url = t.project_url.clone();
    entry.repo_url = t.repo_url.clone();
    Ok(entry)
}

fn parse_launch_mode(id: &str, value: &str) -> Result<LaunchMode, String> {
    match value {
        "" | "terminal" => Ok(LaunchMode::Terminal),
        "background" => Ok(LaunchMode::Background),
        "detached" => Ok(LaunchMode::Detached),
        other => Err(format!(
            "'{}' için bilinmeyen 'launch_mode' değeri '{}'; geçerli değerler: \
             terminal, background, detached",
            id, other
        )),
    }
}

/// Tüm araçların tutulduğu katalog.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: Vec<CatalogEntry>,
}

impl Catalog {
    /// Hazır girişlerden katalog oluşturur.
    ///
    /// Kimlikler benzersiz ve boş olmamalı, her kategori `CATEGORIES`
    /// listesinden biri olmalıdır. Girişler durumları belirlenmiş olarak
    /// gelmelidir (bkz. [`CatalogEntry::new`]).
    pub fn from_entries(entries: Vec<CatalogEntry>) -> Result<Self, Box<dyn std::error::Error>> {
        if entries.is_empty() {
            return Err("katalog boş olamaz".into());
        }
        let mut seen = std::collections::HashSet::new();
        for entry in &entries {
            if !seen.insert(entry.id.clone()) {
                return Err(format!("katalogda yinelenen kimlik: '{}'", entry.id).into());
            }
        }
        Ok(Self { entries })
    }

    /// Tüm girişlere salt okunur erişim.
    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    /// Tüm girişlere yazılabilir erişim (favori durumunun tohumlanması).
    pub fn entries_mut(&mut self) -> &mut Vec<CatalogEntry> {
        &mut self.entries
    }

    /// Yeni bir giriş ekler; kimlik çakışırsa **hata** döner.
    ///
    /// Durum [`crate::detection`] ile hesaplanır (program `PATH`'te aranır).
    /// Hiçbir şey kurulmaz veya çalıştırılmaz.
    pub fn insert_entry(&mut self, entry: ToolEntry) -> Result<(), String> {
        if entry.id.trim().is_empty() {
            return Err("araç kimliği boş olamaz".into());
        }
        if self.entries.iter().any(|e| e.id == entry.id) {
            return Err(format!("'{}' kimliği zaten kullanılıyor", entry.id));
        }
        // Doğrulama ve durum tespiti mevcut tek yoldan yapılır: yol içeren
        // program adı, boş argüman, bilinmeyen kategori burada reddedilir.
        let mut resolved = catalog_entry_from_tool(&entry)?;
        // Kullanıcı eklemeleri (form veya `state.toml`) gömülü katalogla
        // karışmamalı: silinebilirlik buna bağlı.
        resolved.origin = EntryOrigin::User;
        self.entries.push(resolved);
        Ok(())
    }

    /// Kimliğe göre girişi kaldırır; yoksa `None`.
    pub fn remove_entry(&mut self, id: &str) -> Option<CatalogEntry> {
        let pos = self.entries.iter().position(|e| e.id == id)?;
        Some(self.entries.remove(pos))
    }

    /// Kategoriye göre filtreler.
    pub fn by_category(&self, category: &str) -> Vec<&CatalogEntry> {
        self.entries
            .iter()
            .filter(|e| e.category == category)
            .collect()
    }

    /// Favori olanları döner.
    pub fn favorites(&self) -> Vec<&CatalogEntry> {
        self.entries.iter().filter(|e| e.favorite).collect()
    }

    /// İsim, çalıştırılabilir, kategori ve açıklamada arama yapar
    /// (büyük/küçük harfe duyarsız).
    pub fn search(&self, query: &str) -> Vec<&CatalogEntry> {
        if query.is_empty() {
            return Vec::new();
        }
        let q = normalize(query);
        self.entries
            .iter()
            .filter(|e| e.matches_normalized(&q))
            .collect()
    }

    /// Kesin olarak kurulu olmadığı bilinen girdileri döner.
    /// "Kontrol edilemedi" durumundakiler güvenilir olmadığı için buraya girmez.
    pub fn missing(&self) -> Vec<&CatalogEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == InstalledStatus::Missing)
            .collect()
    }

    /// Durumu henüz belirlenemeyen girdileri döner.
    pub fn unknown(&self) -> Vec<&CatalogEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == InstalledStatus::Unknown)
            .collect()
    }

    /// ID'ye göre favoriyi değiştirir. Yeni durum (`true` = favoride) döner.
    /// ID bulunamazsa `false` döner.
    pub fn toggle_favorite(&mut self, id: &str) -> bool {
        for entry in &mut self.entries {
            if entry.id == id {
                entry.favorite = !entry.favorite;
                return entry.favorite;
            }
        }
        false
    }

    /// ID'ye göre bir giriş döner.
    pub fn get_by_id(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(id: &str, category: &str) -> ToolEntry {
        ToolEntry {
            id: id.into(),
            name: id.into(),
            executable: id.into(),
            description: "test".into(),
            category: category.into(),
            ..ToolEntry::default()
        }
    }

    fn catalog_from(tools: &[ToolEntry]) -> Catalog {
        let entries = tools
            .iter()
            .map(catalog_entry_from_tool)
            .collect::<Result<Vec<_>, _>>()
            .expect("giriş dönüştürülemedi");
        Catalog::from_entries(entries).expect("katalog oluşturulamadı")
    }

    #[test]
    fn search_filters_by_name_executable_category_and_description() {
        let cat = catalog_from(&[
            tool("true", "Sistem ve Kaynak Kullanımı"),
            ToolEntry {
                id: "nova-test-tool".into(),
                name: "Nova Test Aracı".into(),
                executable: "nova-test-tool-xyz".into(),
                category: "Özel Araçlar".into(),
                description: "Mevcut olmayan bir test aracı".into(),
                ..ToolEntry::default()
            },
        ]);

        let results = cat.search("true");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "true");

        let results = cat.search("nova test");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "nova-test-tool");

        // Kategori ve açıklama içinde de aranır; Türkçe büyük İ eşleşmeli.
        assert_eq!(cat.search("ÖZEL").len(), 1);
        assert_eq!(cat.search("mevcut").len(), 1);

        // Boş sorgu sonuç döndürmez.
        assert!(cat.search("").is_empty());
    }

    #[test]
    fn missing_reports_uninstalled_tools() {
        let cat = catalog_from(&[
            tool("true", "Sistem ve Kaynak Kullanımı"),
            tool("nova-test-tool-xyz", "Özel Araçlar"),
        ]);
        let ids: Vec<&str> = cat.missing().iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"nova-test-tool-xyz"), "{ids:?}");
    }

    #[test]
    fn by_category_filters_correctly() {
        let cat = catalog_from(&[tool("true", "Sistem ve Kaynak Kullanımı")]);
        let sys = cat.by_category("Sistem ve Kaynak Kullanımı");
        assert_eq!(sys.len(), 1);
        assert_eq!(sys[0].name, "true");
        assert!(cat.by_category("Özel Araçlar").is_empty());
    }

    #[test]
    fn toggle_favorite_changes_state() {
        let mut cat = catalog_from(&[tool("true", "Sistem ve Kaynak Kullanımı")]);

        assert!(cat.toggle_favorite("true"));
        assert!(cat.get_by_id("true").unwrap().favorite);

        assert!(!cat.toggle_favorite("true"));
        assert!(!cat.get_by_id("true").unwrap().favorite);

        // Bilinmeyen ID
        assert!(!cat.toggle_favorite("yok"));
    }

    #[test]
    fn favorites_return_only_favorited() {
        let mut t = tool("nova-test-tool", "Özel Araçlar");
        t.favorite = true;
        let cat = catalog_from(&[tool("true", "Sistem ve Kaynak Kullanımı"), t]);
        let favs = cat.favorites();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].id, "nova-test-tool");
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let entries = vec![
            catalog_entry_from_tool(&tool("dup", "Özel Araçlar")).unwrap(),
            catalog_entry_from_tool(&tool("dup", "Özel Araçlar")).unwrap(),
        ];
        let err = Catalog::from_entries(entries).unwrap_err();
        assert!(err.to_string().contains("dup"), "{err}");
    }

    #[test]
    fn empty_catalog_is_rejected() {
        assert!(Catalog::from_entries(Vec::new()).is_err());
    }

    #[test]
    fn unknown_category_is_a_clear_error() {
        let err = catalog_entry_from_tool(&tool("x", "Sistem İzleme")).unwrap_err();
        assert!(err.contains("x"), "{err}");
        assert!(err.contains("Sistem İzleme"), "{err}");
        assert!(
            err.contains("AI ve Kodlama"),
            "geçerli kategoriler sayılmalı: {err}"
        );
    }

    #[test]
    fn empty_id_is_rejected() {
        let err = catalog_entry_from_tool(&tool("", "Özel Araçlar")).unwrap_err();
        assert!(err.contains("id"), "{err}");
    }

    #[test]
    fn executable_can_be_a_path_to_a_file() {
        let mut t = tool("x", "Özel Araçlar");
        t.executable = "/usr/bin/x".into();
        // `/` içeren executable artık hata vermez; detection::resolve dosya
        // yoksa Missing durumunda bırakır, ancak giriş oluşturulur.
        let entry = catalog_entry_from_tool(&t).unwrap();
        assert_eq!(entry.executable, "/usr/bin/x");
    }

    #[test]
    fn unknown_launch_mode_is_rejected_and_terminal_is_explicitly_valid() {
        let mut t = tool("x", "Özel Araçlar");
        t.launch_mode = "backgroun".into();
        let err = catalog_entry_from_tool(&t).unwrap_err();
        assert!(err.contains("launch_mode"), "{err}");
        assert!(err.contains("backgroun"), "{err}");
        assert!(err.contains("terminal, background, detached"), "{err}");

        t.launch_mode = "terminal".into();
        assert_eq!(
            catalog_entry_from_tool(&t).unwrap().launch_mode,
            LaunchMode::Terminal
        );
        t.launch_mode.clear();
        assert_eq!(
            catalog_entry_from_tool(&t).unwrap().launch_mode,
            LaunchMode::Terminal
        );
    }

    #[test]
    fn command_help_requires_help_text() {
        let mut t = tool("x", "Özel Araçlar");
        t.launch_kind = LaunchKind::CommandHelp;
        let err = catalog_entry_from_tool(&t).unwrap_err();
        assert!(err.contains("help_text"), "{err}");

        t.help_text = Some("kullanım: x --yardim".into());
        assert!(catalog_entry_from_tool(&t).is_ok());
    }

    #[test]
    fn custom_and_project_directory_require_a_path() {
        let mut t = tool("x", "Özel Araçlar");
        t.working_directory = "custom".into();
        t.working_dir_path = None;
        let err = catalog_entry_from_tool(&t).unwrap_err();
        assert!(err.contains("working_dir_path"), "{err}");

        t.working_directory = "project".into();
        t.working_dir_path = Some("   ".into());
        assert!(catalog_entry_from_tool(&t).is_err());
    }

    #[test]
    fn configured_custom_directory_is_preserved() {
        let mut t = tool("x", "Özel Araçlar");
        t.working_directory = "custom".into();
        t.working_dir_path = Some("/srv/proje".into());
        let entry = catalog_entry_from_tool(&t).unwrap();
        assert_eq!(
            entry.working_directory,
            WorkingDirectory::Custom(PathBuf::from("/srv/proje"))
        );
    }

    #[test]
    fn unset_or_current_working_directory_means_home() {
        assert_eq!(WorkingDirectory::default(), WorkingDirectory::Current);
        assert_eq!(WorkingDirectory::Current.label(), "Ev dizini");
        assert_eq!(
            WorkingDirectory::Project(PathBuf::from("/p")).label(),
            "Dizin: /p"
        );
    }

    #[test]
    fn merging_user_entry_keeps_defaults_and_applies_overrides() {
        let mut default = catalog_entry_from_tool(&ToolEntry {
            id: "curl".into(),
            name: "curl".into(),
            executable: "curl".into(),
            category: "Ağ ve Bağlantı".into(),
            args: vec!["--version".into()],
            ..ToolEntry::default()
        })
        .unwrap();

        // Yalnızca favori değişikliği: diğer alanlar varsayılanda kalmalı.
        default
            .merge_user(&ToolEntry {
                id: "curl".into(),
                favorite: true,
                ..ToolEntry::default()
            })
            .unwrap();
        assert!(default.favorite);
        assert_eq!(default.name, "curl");
        assert_eq!(default.args, vec!["--version".to_string()]);

        // Açıkça verilen alan kullanıcı seçimidir.
        default
            .merge_user(&ToolEntry {
                id: "curl".into(),
                name: "cURL".into(),
                favorite: true,
                ..ToolEntry::default()
            })
            .unwrap();
        assert_eq!(default.name, "cURL");
    }

    #[test]
    fn merging_does_not_downgrade_a_non_interactive_kind_by_default() {
        let mut entry = catalog_entry_from_tool(&ToolEntry {
            id: "atuin".into(),
            executable: "atuin".into(),
            category: "Günlük Terminal Yardımcıları".into(),
            launch_kind: LaunchKind::ShellIntegrationInfo,
            info_text: Some("bilgi".into()),
            ..ToolEntry::default()
        })
        .unwrap();

        // Kullanıcı dosyasında yalnızca favori var: tür korunmalı.
        entry
            .merge_user(&ToolEntry {
                id: "atuin".into(),
                favorite: true,
                ..ToolEntry::default()
            })
            .unwrap();
        assert_eq!(entry.launch_kind, LaunchKind::ShellIntegrationInfo);

        // Kullanıcı varsayılandan farklı bir tür isterse uygulanır.
        entry
            .merge_user(&ToolEntry {
                id: "atuin".into(),
                launch_kind: LaunchKind::CustomCommand,
                args: vec!["--ornek".into()],
                ..ToolEntry::default()
            })
            .unwrap();
        assert_eq!(entry.launch_kind, LaunchKind::CustomCommand);
    }

    #[test]
    fn launch_hints_explain_non_interactive_kinds() {
        let mut t = tool("mtr", "Ağ ve Bağlantı");
        t.launch_kind = LaunchKind::CommandHelp;
        t.help_text = Some("kullanım".into());
        let entry = catalog_entry_from_tool(&t).unwrap();
        let hint = entry.launch_hint().expect("ipucu olmalı");
        assert!(hint.contains("kullanım metni"), "{hint}");

        let info = catalog_entry_from_tool(&ToolEntry {
            id: "atuin".into(),
            executable: "atuin".into(),
            category: "Günlük Terminal Yardımcıları".into(),
            launch_kind: LaunchKind::ShellIntegrationInfo,
            ..ToolEntry::default()
        })
        .unwrap();
        assert!(info.launch_hint().unwrap().contains("kabuk entegrasyonu"));

        let custom = catalog_entry_from_tool(&ToolEntry {
            id: "ozel".into(),
            executable: "echo".into(),
            args: vec!["merhaba".into()],
            category: "Özel Araçlar".into(),
            launch_kind: LaunchKind::CustomCommand,
            ..ToolEntry::default()
        })
        .unwrap();
        assert!(custom.launch_hint().unwrap().contains("echo merhaba"));

        let interactive =
            catalog_entry_from_tool(&tool("btop", "Sistem ve Kaynak Kullanımı")).unwrap();
        assert!(interactive.launch_hint().is_none());
    }

    #[test]
    fn invalid_configured_directory_fails_resolution_with_clear_message() {
        let wd = WorkingDirectory::Custom(PathBuf::from("/nova/yok/boyle/dizin"));
        let err = wd
            .resolve(std::path::Path::new("/tmp"), "lazygit")
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("lazygit"), "{msg}");
        assert!(msg.contains("/nova/yok/boyle/dizin"), "{msg}");
    }

    #[test]
    fn short_category_names_round_trip_with_full_names() {
        let pairs = [
            ("AI ve Kodlama", "AI"),
            ("Git ve Geliştirici Araçları", "Git"),
            ("Dosya Yönetimi ve Arama", "Dosya"),
            ("Sistem ve Kaynak Kullanımı", "Sistem"),
            ("Ağ ve Bağlantı", "Ağ"),
            ("Docker ve Sunucu", "Docker"),
            ("Günlük Terminal Yardımcıları", "Yardımcılar"),
            ("Özel Araçlar", "Özel"),
        ];
        for (full, short) in pairs {
            assert_eq!(short_category_name(full), Some(short), "kısa ad: {full}");
            assert_eq!(category_from_short(short), Some(full), "tam ad: {short}");
        }
    }

    #[test]
    fn unknown_short_category_name_returns_none() {
        assert_eq!(short_category_name("Yok Böyle Bir Şey"), None);
        assert_eq!(category_from_short("Yok"), None);
        assert_eq!(category_from_short(""), None);
    }
}
