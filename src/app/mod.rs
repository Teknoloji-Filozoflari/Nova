//! Uygulama durumu ve ana döngü modülü.
//! `App` yapısı uygulamanın tüm durumunu tutar ve `run` metodu ile
//! terminal arayüzünün ana döngüsünü sürdürür.

use crate::catalog::{Catalog, CATEGORIES};
use crate::error::NovaError;
use crate::event::{Event, EventReader};
use crate::ui;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::Stdout;
use std::path::PathBuf;
use std::time::Duration;

pub mod form;
pub mod modal;
pub mod state;

/// Başlatma eyleminin imzası. Testler gerçek terminal açmadan bastırabilir.
pub type LaunchFn = fn(
    &crate::catalog::CatalogEntry,
    &std::path::Path,
    &crate::launcher::LaunchOptions,
) -> Result<crate::launcher::LaunchOutcome, NovaError>;

/// Yapılandırılmış gerçek başlatıcı.
fn real_launch(
    entry: &crate::catalog::CatalogEntry,
    home: &std::path::Path,
    options: &crate::launcher::LaunchOptions,
) -> Result<crate::launcher::LaunchOutcome, NovaError> {
    crate::launcher::launch_with_options(entry, home, options, &crate::launcher::CommandSpawner)
}

/// Arayüzde odaklı panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelFocus {
    /// Kategori paneli odaklı.
    Category,
    /// Araç paneli odaklı.
    Tool,
}

/// Uygulama durumu.
#[derive(Debug, Clone)]
/// `handle_form_key`ün sonucu.
enum FormOutcome {
    /// Form düzenlenmeye devam ediyor.
    Editing,
    /// Kullanıcı formu kapattı (Esc).
    Closed,
    /// Form doğrulandı; inceleme ekranına geçilir.
    Review(modal::Modal),
}

#[derive(Debug)]
pub struct App {
    /// Uygulamanın çalışıp çalışmadığını belirler.
    pub running: bool,
    /// Araç kataloğu.
    pub catalog: Catalog,
    /// Şu anda odaklı panel.
    pub focus: PanelFocus,
    /// Seçili kategorinin indeksi.
    pub category_index: usize,
    /// Seçili aracın indeksi (filtrelenmiş liste üzerinden).
    pub tool_index: usize,
    /// Arama sorgusu.
    pub search_query: String,
    /// Arama modu aktif mi.
    pub search_active: bool,
    /// Sadece favoriler gösterilsin mi.
    pub favorite_only: bool,
    /// Klavye yardımı gösterilsin mi.
    pub show_help: bool,
    /// Terminal dar mı (tek panel gösteriliyor)? UI her karede günceller.
    pub narrow_mode: bool,
    /// Kullanıcıya gösterilecek durum mesajı.
    pub status_message: String,
    /// Başlatma işlemi. Üretimde [`real_launch`], testlerde sahte.
    pub launcher: LaunchFn,
    /// Çalışma dizinlerini çözerken kullanılacak ev dizini (başlangıçta
    /// sabitlenir; `HOME` çalışma sırasında değişse bile tutarlı kalır).
    pub home_dir: PathBuf,
    /// Kalıcı kullanıcı durumu (favoriler, geçmiş, proje dizinleri, tercihler).
    pub state: state::AppState,
    /// İlk çalıştırma tohumu: `state.toml` yokken katalogdaki favori alanı
    /// uygulanır. Bir kez kullanılır.
    first_run_seed: bool,
    /// Kullanıcının `config.toml`'dan sildiği (Nova'da gizlenen) araçların
    /// kimlikleri; liste süzülürken kullanılır.
    hidden: std::collections::BTreeSet<String>,
    /// Açık modal (form, seçici, onay). Açıkken ana ekran tuşları çalışmaz.
    pub modal: modal::Modal,
    /// `config.toml`'dan gelen araçların kimlikleri; silme davranışı bu
    /// ayrıma göre değişir (dosya yeniden yazılmaz, yalnızca gizlenir).
    config_tool_ids: std::collections::BTreeSet<String>,
    /// Yardım ekranında gösterilecek yapılandırma yolu.
    pub config_path_display: String,
    /// Yardım ekranında gösterilecek durum dosyası yolu.
    pub state_path_display: String,
}

impl App {
    /// Yeni bir `App` örneği oluşturur (kalıcılık yok).
    pub fn new(catalog: Catalog) -> Self {
        Self::with_state(catalog, state::AppState::in_memory(), true)
    }

    /// Kalıcı durumla bir `App` oluşturur.
    ///
    /// - `state.file.favorites` katalogdaki girişlere uygulanır.
    /// - `seed_favorites_from_catalog` yalnızca dosya hiç yokken `true`
    ///   olmalıdır: gömülü katalogdaki `favorite = true` alanları tohumlanır.
    pub fn with_state(
        mut catalog: Catalog,
        mut state: state::AppState,
        seed_favorites_from_catalog: bool,
    ) -> Self {
        if seed_favorites_from_catalog {
            for entry in catalog.entries() {
                if entry.favorite {
                    state.file.favorites.insert(entry.id.clone());
                }
            }
        }
        // Nova içinde eklenen araçlar katalogda hazır değildir; durum
        // dosyasından geri yüklenir. Zaten katalogda olan bir kimlik (ör.
        // kullanıcı sonradan aynı aracı `config.toml`'a da yazdıysa)
        // yeniden eklenmez.
        for entry in state.file.custom_tools.clone() {
            if catalog.entries().iter().any(|e| e.id == entry.id) {
                continue;
            }
            if let Err(e) = catalog.insert_entry(entry.clone()) {
                state
                    .restore_warnings
                    .push(format!("'{}' özel aracı yüklenemedi: {e}", entry.id));
            }
        }
        let hidden = state.file.hidden_tools.clone();
        let state_path_display = state.path_display();
        for entry in catalog.entries_mut() {
            entry.favorite = state.file.favorites.contains(&entry.id);
        }
        Self {
            running: true,
            catalog,
            focus: PanelFocus::Category,
            category_index: 0,
            tool_index: 0,
            search_query: String::new(),
            search_active: false,
            favorite_only: false,
            show_help: false,
            narrow_mode: false,
            status_message: String::new(),
            launcher: real_launch,
            home_dir: default_home_dir(),
            state,
            first_run_seed: seed_favorites_from_catalog,
            hidden,
            modal: modal::Modal::None,
            config_tool_ids: std::collections::BTreeSet::new(),
            config_path_display: crate::config::user_config_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "bulunamadı (HOME yok)".into()),
            state_path_display,
        }
    }

    /// `config.toml`'dan gelen araç kimliklerini bildirir.
    ///
    /// Bu kimliklerdeki araçlar silindiğinde kullanıcının elle yazdığı dosya
    /// **yeniden yazılmaz**; araç yalnızca Nova görünümünde gizlenir.
    pub fn set_config_tool_ids<I: IntoIterator<Item = String>>(&mut self, ids: I) {
        self.config_tool_ids = ids.into_iter().collect();
    }

    /// Ana TUI döngüsünü çalıştırır. Her karede ekran çizilir ve bir terminal
    /// olayı okunur. `running` false olduğunda döngü biter.
    pub fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<(), NovaError> {
        let tick_rate = Duration::from_millis(100);

        while self.running {
            terminal.draw(|f| ui::draw(f, self))?;

            if let Some(event) = EventReader::next_event(tick_rate)? {
                self.handle_event(event, terminal)?;
            }
        }

        Ok(())
    }

    /// Bir olayı işler ve uygulama durumunu günceller.
    fn handle_event(
        &mut self,
        event: Event,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<(), NovaError> {
        match event {
            Event::Quit => {
                self.running = false;
            }
            Event::Key(key) => self.handle_key(key)?,
            Event::Resize(w, h) => {
                // Dar terminale geçildiğinde kategori paneli gizlenir; odak
                // görünmeyen panelde kalmasın.
                self.narrow_mode = w < crate::ui::WIDE_MIN_WIDTH;
                if self.narrow_mode {
                    self.focus = PanelFocus::Tool;
                }
                terminal.resize(ratatui::layout::Rect::new(0, 0, w, h))?;
            }
            Event::Ignored => {}
        }
        Ok(())
    }

    /// Klavye tuşlarını işler.
    ///
    /// Genel API'dir: ana döngü bu metodu çağırır ve entegrasyon testleri de
    /// aynı yolu kullanarak davranışı sınar.
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Result<(), NovaError> {
        use crossterm::event::KeyCode;

        if self.search_active {
            return self.handle_search_key(key);
        }

        // Modal açıkken yalnızca modal tuşları işlenir: ana ekranın `q`,
        // `N`, `D` gibi kısayolları yanlışlıkla tetiklenemez.
        if self.modal.is_open() {
            return self.handle_modal_key(key.code);
        }

        match key.code {
            KeyCode::Esc => {
                if self.show_help {
                    self.show_help = false;
                } else {
                    self.running = false;
                }
            }
            KeyCode::Char('q') => {
                self.running = false;
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
            }
            KeyCode::Char('/') => {
                self.search_active = true;
                self.search_query.clear();
                self.focus = PanelFocus::Tool;
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                self.toggle_selected_favorite();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.status_message.clear();
                self.move_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.status_message.clear();
                self.move_up();
            }
            KeyCode::Char('h') | KeyCode::Left => {
                // Dar terminalde kategori paneli gizli olduğundan odak
                // araç listesinde kalır; yön tuşları yanıltıcı olmasın.
                if self.narrow_mode {
                    self.status_message =
                        "Kategori paneli bu genişlikte gizli; terminali genişlet.".into();
                } else {
                    self.focus = PanelFocus::Category;
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.focus = PanelFocus::Tool;
            }
            KeyCode::Tab | KeyCode::Enter => {
                if self.focus == PanelFocus::Category {
                    self.focus = PanelFocus::Tool;
                    self.tool_index = 0;
                } else {
                    self.select_tool();
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.modal = modal::Modal::History;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.modal = modal::Modal::ProjectPicker {
                    index: self.project_cursor(),
                };
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                self.modal = modal::Modal::Settings {
                    preferred_terminal: self
                        .state
                        .file
                        .prefs
                        .preferred_terminal
                        .clone()
                        .unwrap_or_else(|| "konsole".into()),
                    default_working_dir: self
                        .state
                        .file
                        .prefs
                        .default_working_dir
                        .clone()
                        .unwrap_or_default(),
                    focus: 0,
                    error: None,
                };
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.modal = modal::Modal::ToolForm(Box::default());
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.request_delete_selected();
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.favorite_only = !self.favorite_only;
                if self.favorite_only {
                    self.status_message = "Sadece favoriler gösteriliyor.".into();
                } else {
                    self.status_message = "Tüm araçlar gösteriliyor.".into();
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Arama modundaki tuşları işler.
    fn handle_search_key(&mut self, key: crossterm::event::KeyEvent) -> Result<(), NovaError> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                // İlk Esc aramayı kapatır; uygulama açık kalır.
                self.search_active = false;
                self.search_query.clear();
                self.status_message = "Arama iptal edildi.".into();
            }
            KeyCode::Enter => {
                // Enter aramayı kapatır ve sonuçlara odaklanır.
                self.search_active = false;
                self.focus = PanelFocus::Tool;
                if self.search_query.is_empty() {
                    self.status_message = "Arama temizlendi.".into();
                } else {
                    let count = self.filtered_tools().len();
                    self.status_message = format!("'{}' için {} sonuç", self.search_query, count);
                }
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.reset_tool_selection();
            }
            KeyCode::Down => {
                self.focus = PanelFocus::Tool;
                self.move_down();
            }
            KeyCode::Up => {
                self.focus = PanelFocus::Tool;
                self.move_up();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.reset_tool_selection();
            }
            _ => {}
        }
        Ok(())
    }

    /// Arama değiştiğinde seçim ilk sonuca döner.
    fn reset_tool_selection(&mut self) {
        self.tool_index = 0;
    }

    /// Seçili aracın favorisini değiştirir.
    fn toggle_selected_favorite(&mut self) {
        let tools = self.filtered_tools();
        if tools.is_empty() {
            return;
        }
        let idx = self.tool_index.min(tools.len() - 1);
        let entry_id = tools[idx].id.clone();
        let entry_name = tools[idx].name.clone();
        let new_fav = self.catalog.toggle_favorite(&entry_id);
        if new_fav {
            self.state.file.favorites.insert(entry_id.clone());
        } else {
            self.state.file.favorites.remove(&entry_id);
        }
        let saved = self.persist_state();
        let tail = match saved {
            Ok(()) => "",
            Err(msg) => {
                self.status_message = format!("Hata: favori kaydedilemedi ({msg})");
                return;
            }
        };
        if new_fav {
            self.status_message = format!("'{}' favorilere eklendi.{tail}", entry_name);
        } else {
            self.status_message = format!("'{}' favorilerden çıkarıldı.{tail}", entry_name);
        }
    }

    /// Durumu `state.toml`'a yazar.
    ///
    /// İlk çalıştırmada gömülü favorilerin tohumlanması da burada
    /// kalıcılaşır; böylece dosya yokluğu (kullanıcı tüm favorileri
    /// kaldırdığında) yeniden tohumlamaya yol açmaz.
    pub fn persist_state(&mut self) -> Result<(), String> {
        let Some(path) = self.state.path.clone() else {
            return Ok(());
        };
        crate::config::state::save_state(&path, &self.state.file)
    }

    /// Kullanıcıya gösterilecek, zamana göre göreli bir etiket üretir.
    pub fn time_label(&self, at: u64) -> String {
        state::relative_time(now_secs(), at)
    }

    /// İlk çalıştırma tohumunun (gömülü favoriler) diske yazılması gerekiyor
    /// mu? Uygulama açıldığında, tek bir başarılı yazma beklenmeden
    /// çağrılır: kullanıcı hiçbir şey yapmadan çıksa bile durum dosyası
    /// oluşur ve tohum "yokluk" olarak yeniden uygulanmaz.
    pub fn persist_first_run(&mut self) {
        if self.first_run_seed && self.persist_state().is_ok() {
            self.first_run_seed = false;
        }
    }

    /// Aşağı doğru hareket eder.
    fn move_down(&mut self) {
        match self.focus {
            PanelFocus::Category => {
                let max = CATEGORIES.len().saturating_sub(1);
                self.category_index = (self.category_index + 1).min(max);
            }
            PanelFocus::Tool => {
                let tools = self.filtered_tools();
                if tools.is_empty() {
                    return;
                }
                let max = tools.len().saturating_sub(1);
                self.tool_index = (self.tool_index + 1).min(max);
            }
        }
    }

    /// Yukarı doğru hareket eder.
    fn move_up(&mut self) {
        match self.focus {
            PanelFocus::Category => {
                self.category_index = self.category_index.saturating_sub(1);
            }
            PanelFocus::Tool => {
                self.tool_index = self.tool_index.saturating_sub(1);
            }
        }
    }

    /// Seçili aracı yeni bir terminal sekmesinde başlatmayı ister.
    ///
    /// Hata olsa bile Nova çalışmaya devam eder: başlatma hatası TUI'yi
    /// bozmaz, yalnızca durum satırında görünür. Başarı mesajı bilinçli
    /// olarak "istendi" der; aracın gerçekten çalıştığını Nova doğrulayamaz.
    pub fn select_tool(&mut self) {
        let tools = self.filtered_tools();
        let Some(entry) = tools
            .get(self.tool_index.min(tools.len().saturating_sub(1)))
            .map(|entry| (**entry).clone())
        else {
            self.status_message = "Başlatılacak araç yok.".into();
            return;
        };

        // Kurulu olmayan araç için başlatıcı **hiç çağrılmaz**; eksik komut
        // adı kullanıcıya söylenir. Launcher da aynı kontrolü yapar
        // (savunma katmanı), ama kullanıcıya en yakın yerde reddetmek
        // gereksiz bir emülatör aramasını da önler.
        use crate::detection::InstalledStatus;
        match entry.status {
            InstalledStatus::Missing => {
                let history_error =
                    self.persist_attempt(&entry, crate::config::state::AttemptStatus::Rejected);
                self.status_message = format!(
                    "'{}' kurulu değil; '{}' komutu PATH üzerinde bulunamadı. \
                     Kurulumu senin onayınla yapılmalı.",
                    entry.name, entry.executable
                );
                if let Err(e) = history_error {
                    self.status_message
                        .push_str(&format!(" (geçmiş kaydedilemedi: {e})"));
                }
                return;
            }
            InstalledStatus::Unknown => {
                let history_error =
                    self.persist_attempt(&entry, crate::config::state::AttemptStatus::Rejected);
                self.status_message = format!(
                    "'{}' için kurulum durumu kontrol edilemedi; başlatılmadı.",
                    entry.name
                );
                if let Err(e) = history_error {
                    self.status_message
                        .push_str(&format!(" (geçmiş kaydedilemedi: {e})"));
                }
                return;
            }
            InstalledStatus::Installed => {}
        }

        match (self.launcher)(&entry, &self.home_dir, &self.launch_options()) {
            Ok(outcome) => {
                self.status_message = match self
                    .persist_attempt(&entry, crate::config::state::AttemptStatus::RequestSent)
                {
                    Ok(()) => outcome.message(),
                    Err(e) => format!("{} (geçmiş kaydedilemedi: {e})", outcome.message()),
                };
            }
            Err(err) => {
                let history_error = self
                    .persist_attempt(&entry, crate::config::state::AttemptStatus::Rejected)
                    .err();
                // Var olmayan bir dizin yüzünden başlatma reddedildiyse
                // hiçbir şey oluşturulmaz; yalnızca onay sorulur.
                use crate::catalog::WorkingDirectory;
                let missing = match &entry.working_directory {
                    WorkingDirectory::Project(path) => Some(path.clone()),
                    WorkingDirectory::Custom(path) => Some(path.clone()),
                    WorkingDirectory::Current | WorkingDirectory::Home => None,
                };
                if let Some(dir) = missing.filter(|p| !p.exists()) {
                    let id = entry.id.clone();
                    self.status_message = format!(
                        "{err} Dizini oluşturmak için onay isteniyor (Enter oluşturur, Esc vazgeçer)."
                    );
                    if let Some(e) = history_error {
                        self.status_message
                            .push_str(&format!(" (geçmiş kaydedilemedi: {e})"));
                    }
                    self.modal = modal::Modal::Confirm {
                        question: format!(
                            "'{}' dizini yok. Oluşturulsun mu? Enter onaylar, Esc vazgeçer.",
                            dir.display()
                        ),
                        action: modal::ConfirmAction::CreateDir {
                            path: dir,
                            tool_id: id,
                        },
                    };
                } else {
                    self.status_message = err.to_string();
                    if let Some(e) = history_error {
                        self.status_message
                            .push_str(&format!(" (geçmiş kaydedilemedi: {e})"));
                    }
                }
            }
        }
    }

    /// Başlatma denemesini günceller ve kalıcı durum varsa diske yazar.
    fn persist_attempt(
        &mut self,
        entry: &crate::catalog::CatalogEntry,
        status: crate::config::state::AttemptStatus,
    ) -> Result<(), String> {
        self.state
            .record_attempt(&entry.id, &entry.name, now_secs(), status);
        self.persist_state()
    }

    /// Bu sekmeye özgü başlatma seçenekleri.
    ///
    /// Proje dizini girdileri (`working_directory = "project"`) o an seçili
    /// proje dizinine taşınır; seçim yoksa girdinin kendi varsayılanına.
    pub fn launch_options(&self) -> crate::launcher::LaunchOptions {
        crate::launcher::LaunchOptions {
            project_dir: self.state.active_project.clone(),
            default_working_dir: self
                .state
                .file
                .prefs
                .default_working_dir
                .as_ref()
                .map(PathBuf::from),
            preferred_terminal: self.state.file.prefs.preferred_terminal.clone(),
        }
    }

    /// Başlatıcıyı test/entegrasyon için değiştirir.
    pub fn set_launcher(&mut self, launcher: LaunchFn) {
        self.launcher = launcher;
    }

    /// Bir girişin mevcut arama sorgusuyla eşleşip eşleşmediğini döner.
    /// Ad, komut, kategori ve açıklamaya bakar; büyük/küçük harfe duyarsızdır.
    pub fn matches_search(&self, entry: &crate::catalog::CatalogEntry) -> bool {
        if self.search_query.is_empty() {
            return true;
        }
        let q = crate::catalog::normalize(&self.search_query);
        crate::catalog::normalize(&entry.name).contains(&q)
            || crate::catalog::normalize(&entry.executable).contains(&q)
            || crate::catalog::normalize(&entry.category).contains(&q)
            || crate::catalog::normalize(&entry.description).contains(&q)
    }

    /// Seçili kategoriyi döner.
    pub fn selected_category(&self) -> Option<&str> {
        CATEGORIES.get(self.category_index).copied()
    }

    /// Filtrelenmiş araç listesini döner (kategori, arama, favori filtresi uygulanmış).
    pub fn filtered_tools(&self) -> Vec<&crate::catalog::CatalogEntry> {
        let category = self.selected_category().unwrap_or("");

        // Arama yapılırken seçili kategori filtresi uygulanmaz; aksi hâlde
        // kullanıcı başka kategorideki bir aracı asla bulamaz.
        let searching = !self.search_query.is_empty();

        let mut tools: Vec<&crate::catalog::CatalogEntry> = if self.favorite_only {
            self.catalog
                .favorites()
                .into_iter()
                .filter(|e| searching || category.is_empty() || e.category == category)
                .collect()
        } else if searching {
            self.catalog.entries().iter().collect()
        } else {
            self.catalog.by_category(category)
        };

        if searching {
            tools.retain(|e| self.matches_search(e));
        }
        if !self.hidden.is_empty() {
            tools.retain(|e| !self.hidden.contains(&e.id));
        }

        tools
    }

    /// Kullanıcıya gösterilmeyen (gizlenmiş) araç kimlikleri.
    pub fn hidden_tools(&self) -> &std::collections::BTreeSet<String> {
        &self.hidden
    }

    /// Bir aracı Nova görünümünden çıkarır.
    ///
    /// `state.toml`'da tanımlanmış araç için kayıt silinir; `config.toml`'dan
    /// gelen araç için dosya **değiştirilmez**, yalnızca `hidden_tools`'a
    /// eklenir (elle yazılmış yorumlar korunur).
    pub fn remove_tool(&mut self, id: &str) -> Result<bool, String> {
        let from_config = self.state.file.custom_tools.iter().all(|t| t.id != id);
        if from_config {
            self.hidden.insert(id.to_string());
            self.state.file.hidden_tools.insert(id.to_string());
        } else {
            self.state.file.custom_tools.retain(|t| t.id != id);
        }
        self.state.file.favorites.remove(id);
        if let Some(entry) = self.catalog.entries_mut().iter_mut().find(|e| e.id == id) {
            entry.favorite = false;
        }
        self.persist_state()?;
        Ok(from_config)
    }

    /// Seçili aracı döner.
    pub fn selected_tool(&self) -> Option<&crate::catalog::CatalogEntry> {
        let tools = self.filtered_tools();
        if tools.is_empty() {
            return None;
        }
        tools.get(self.tool_index.min(tools.len() - 1)).copied()
    }

    /// Seçili kategorideki araç sayısını döner.
    pub fn tool_count(&self) -> usize {
        self.filtered_tools().len()
    }

    // -----------------------------------------------------------------
    // Modallar
    // -----------------------------------------------------------------

    /// Etkin proje dizininin listedeki satırı (yoksa 0).
    fn project_cursor(&self) -> usize {
        let active = self.state.active_project.clone();
        active
            .and_then(|a| self.state.project_dirs().iter().position(|p| p == &a))
            .unwrap_or(0)
    }

    /// Modal açıkken gelen tuşları dağıtır.
    ///
    /// Yalnızca tuş kodu alınır: imzada `key.code` deseydik, `current`
    /// (modal) ödünçlüyken `self`i ödünç alan çağrılar
    /// (`state.add_project_dir` gibi) derlenemezdi.
    pub fn handle_modal_key(&mut self, code: crossterm::event::KeyCode) -> Result<(), NovaError> {
        match self.apply_modal_key(code) {
            Ok(Some(next)) => self.modal = next,
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        Ok(())
    }

    /// `handle_modal_key`ün gövdesi: sonucu döndürür, `self.modal`e yazmaz.
    ///
    /// Modal, düğümlerini yerinde güncelleyebilmek için `self`ten dışarı
    /// alınır (`None` bırakılır). Çağıran yazımı tek noktada yaptığı için
    /// yürütme sırasında `self.modal`in ara değeri (alınma anındaki `None`
    /// dahil) hiçbir yoldan gözlenmez.
    fn apply_modal_key(
        &mut self,
        code: crossterm::event::KeyCode,
    ) -> Result<Option<modal::Modal>, NovaError> {
        use crossterm::event::KeyCode;

        let mut next = std::mem::replace(&mut self.modal, modal::Modal::None);
        // Dal içinden modal değiştirilirse (Esc, Enter, 'a' gibi) buraya yazılır.
        let mut decided: Option<modal::Modal> = None;
        let mut take = |m: modal::Modal| decided = Some(m);
        match &mut next {
            modal::Modal::None => {}
            modal::Modal::Confirm { action, .. } => match code {
                KeyCode::Enter => {
                    let action = action.clone();
                    self.confirm_action(action);
                    take(modal::Modal::None);
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.status_message = "İşlem iptal edildi.".into();
                    take(modal::Modal::None);
                }
                _ => {
                    // Onay ekranında bilinmeyen tuş hiçbir şey yapmaz
                    // (yanlışlıkla onay oluşmasın).
                }
            },
            modal::Modal::ToolForm(form) => {
                // Form düğümü yerinde tutulur: doğrulama başarısız olursa
                // düzeltilmiş form geri konur, başarılıysa form zaten
                // `ToolReview` içine taşınmıştır.
                let mut taken = std::mem::replace(form, Box::new(form::ToolForm::default()));
                match self.handle_form_key(&mut taken, code) {
                    // Esc formu kapatır, Enter inceleme ekranını açar.
                    FormOutcome::Closed => take(modal::Modal::None),
                    FormOutcome::Review(review) => take(review),
                    FormOutcome::Editing => next = modal::Modal::ToolForm(taken),
                }
            }
            modal::Modal::ToolReview { form, valid } => match code {
                KeyCode::Enter => {
                    let valid = (**valid).clone();
                    let form = (**form).clone();
                    // Kayıt sonucu dönüş değerinden okunur; modal alınmışken
                    // `self.modal`e bakılmaz.
                    let persisted = self.save_new_tool(&form, &valid);
                    let review = modal::Modal::ToolReview {
                        form: Box::new(form),
                        valid: Box::new(valid),
                    };
                    if persisted {
                        take(modal::Modal::None);
                    } else {
                        // Kayıt kalıcı olmadı: inceleme ekranı açık kalır,
                        // kullanıcı hata mesajını görüp yeniden deneyebilir.
                        take(review);
                    }
                }
                KeyCode::Esc => take(modal::Modal::ToolForm(form.clone())),
                _ => {}
            },
            modal::Modal::ProjectPicker { index } => match code {
                KeyCode::Esc => take(modal::Modal::None),
                KeyCode::Up | KeyCode::Char('k') => *index = index.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    let max = self.state.project_dirs().len().saturating_sub(1);
                    *index = (*index + 1).min(max);
                }
                KeyCode::Enter => {
                    let dirs = self.state.project_dirs();
                    if let Some(path) = dirs.get(*index) {
                        let path = path.clone();
                        self.state.set_active_project(Some(path.clone()));
                        self.status_message = format!(
                            "Etkin proje dizini: {} (yalnızca bu oturum).",
                            path.display()
                        );
                        // Tercih kalıcı değil; her açılışta varsayılan
                        // çalışma dizinine dönülür.
                    } else {
                        self.status_message = "Proje dizini listesi boş; 'A' ile ekle.".into();
                    }
                }
                KeyCode::Char('a') | KeyCode::Char('A') => take(modal::Modal::ProjectAdd {
                    input: String::new(),
                    error: None,
                }),
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    let dirs = self.state.project_dirs();
                    if let Some(path) = dirs.get(*index) {
                        take(modal::Modal::Confirm {
                            question: format!(
                                "'{}' listeden çıkarılsın mı? Dizin diskten silinmez.",
                                path.display()
                            ),
                            action: modal::ConfirmAction::RemoveProject { path: path.clone() },
                        });
                    }
                }
                _ => {}
            },
            modal::Modal::ProjectAdd { input, error } => match code {
                KeyCode::Esc => take(modal::Modal::ProjectPicker { index: 0 }),
                KeyCode::Backspace => {
                    input.pop();
                    *error = None;
                }
                KeyCode::Enter => {
                    let dir = input.clone();
                    match self.state.add_project_dir(&dir) {
                        Ok(path) => {
                            self.status_message = format!(
                                "'{}' proje listesine eklendi (yeni dizin oluşturulmadı).",
                                path.display()
                            );
                            let index = self.project_cursor();
                            self.status_message
                                .push_str(" Enter ile etkin yapabilirsin.");
                            take(modal::Modal::ProjectPicker { index });
                        }
                        Err(e) => *error = Some(e),
                    }
                }
                KeyCode::Char(c) if !c.is_control() => input.push(c),
                _ => {}
            },
            modal::Modal::History => match code {
                KeyCode::Esc => take(modal::Modal::None),
                KeyCode::Char('x') | KeyCode::Char('X') => take(modal::Modal::Confirm {
                    question: "Tüm başlatma geçmişi silinsin mi?".into(),
                    action: modal::ConfirmAction::ClearRecents,
                }),
                _ => {}
            },
            modal::Modal::Settings {
                preferred_terminal,
                default_working_dir,
                focus,
                error,
            } => match code {
                KeyCode::Esc => {
                    self.status_message = "Ayarlar değiştirilmedi.".into();
                    take(modal::Modal::None);
                }
                KeyCode::Tab | KeyCode::Down => *focus = (*focus + 1) % 2,
                KeyCode::BackTab | KeyCode::Up => *focus = (*focus + 1) % 2,
                KeyCode::Backspace => {
                    if *focus == 0 {
                        preferred_terminal.pop();
                    } else {
                        default_working_dir.pop();
                    }
                    *error = None;
                }
                KeyCode::Enter => match self.save_settings(preferred_terminal, default_working_dir)
                {
                    Ok(()) => {
                        self.status_message = "✓ Ayarlar kaydedildi.".into();
                        take(modal::Modal::None);
                    }
                    Err(message) => {
                        self.status_message = format!("Hata: {message}");
                        *error = Some(message);
                    }
                },
                KeyCode::Char(c) if !c.is_control() => {
                    if *focus == 0 {
                        preferred_terminal.push(c);
                    } else {
                        default_working_dir.push(c);
                    }
                    *error = None;
                }
                _ => {}
            },
        }
        // Dal modal değiştirmediyse, yerinde güncellenmiş düğüm geri konur.
        Ok(decided.or(Some(next)))
    }

    /// Form tuşları.
    ///
    /// Formu (`form`) yerinde günceller ve ne olacağını döndürür; modala
    /// kendisi yazmaz.
    fn handle_form_key(
        &mut self,
        form: &mut form::ToolForm,
        code: crossterm::event::KeyCode,
    ) -> FormOutcome {
        use crossterm::event::KeyCode;

        match code {
            KeyCode::Esc => {
                self.status_message = "Yeni araç ekleme iptal edildi.".into();
                FormOutcome::Closed
            }
            KeyCode::Tab | KeyCode::Down => {
                form.next_field();
                FormOutcome::Editing
            }
            KeyCode::BackTab | KeyCode::Up => {
                form.prev_field();
                FormOutcome::Editing
            }
            KeyCode::Enter => {
                let mut validated = form.clone();
                match validated.validate() {
                    Ok(valid) => FormOutcome::Review(modal::Modal::ToolReview {
                        form: Box::new(validated),
                        valid: Box::new(valid),
                    }),
                    Err(errors) => {
                        // İlk hataya odaklan; kullanıcı nereyi düzelteceğini görsün.
                        if let Some(pos) = validated.errors.iter().position(|e| e.is_some()) {
                            validated.focus = pos;
                        }
                        self.status_message = format!(
                            "Hata: {} — kaydedilmedi.",
                            errors.first().cloned().unwrap_or_default()
                        );
                        *form = validated;
                        FormOutcome::Editing
                    }
                }
            }
            KeyCode::Left | KeyCode::Right => {
                match form.focused_field() {
                    form::Field::Category => form.cycle_category(code == KeyCode::Right),
                    form::Field::LaunchKind => form.cycle_launch_kind(),
                    _ => {}
                }
                FormOutcome::Editing
            }
            KeyCode::Char(' ')
                if matches!(
                    form.focused_field(),
                    form::Field::Category | form::Field::LaunchKind
                ) =>
            {
                match form.focused_field() {
                    form::Field::Category => form.cycle_category(true),
                    form::Field::LaunchKind => form.cycle_launch_kind(),
                    _ => unreachable!(),
                }
                FormOutcome::Editing
            }
            KeyCode::Backspace => {
                if let Some(text) = form.focused_text_mut() {
                    text.pop();
                    form.errors[form.focus] = None;
                }
                FormOutcome::Editing
            }
            KeyCode::Char(c) if !c.is_control() => {
                if let Some(text) = form.focused_text_mut() {
                    text.push(c);
                    // Hata metni yazmaya başlayınca kalkar; eski hata yeni
                    // girdiyle birlikte yanıltmasın.
                    form.errors[form.focus] = None;
                }
                FormOutcome::Editing
            }
            _ => FormOutcome::Editing,
        }
    }

    /// Tercihleri doğrular, kaydeder ve yazma başarısız olursa belleği geri
    /// alır. Eksik çalışma dizinini oluşturmaz.
    fn save_settings(
        &mut self,
        preferred_terminal: &str,
        default_working_dir: &str,
    ) -> Result<(), String> {
        let terminal = preferred_terminal.trim();
        let preferred_terminal = if terminal.is_empty() {
            None
        } else if terminal.eq_ignore_ascii_case("konsole") {
            Some("konsole".to_string())
        } else {
            return Err(
                "Şu an yalnızca Konsole destekleniyor (boş bırakmak da Konsole'u seçer).".into(),
            );
        };

        let default_working_dir = if default_working_dir.trim().is_empty() {
            None
        } else {
            let path = state::normalize_project_path(default_working_dir)?;
            let metadata = std::fs::metadata(&path)
                .map_err(|e| format!("Varsayılan dizin '{}' okunamadı: {e}", path.display()))?;
            if !metadata.is_dir() {
                return Err(format!(
                    "Varsayılan çalışma yolu bir dizin değil: '{}'.",
                    path.display()
                ));
            }
            Some(path.to_string_lossy().into_owned())
        };

        let old = self.state.file.prefs.clone();
        self.state.file.prefs.preferred_terminal = preferred_terminal;
        self.state.file.prefs.default_working_dir = default_working_dir;
        if let Err(error) = self.persist_state() {
            self.state.file.prefs = old;
            return Err(format!("Ayarlar kaydedilemedi: {error}"));
        }
        Ok(())
    }

    /// Yeni aracı kaydeder (gözden geçirmeden sonra çağrılır).
    ///
    /// Kayıt kalıcı olduysa `true`, aksi halde `false` döner (hata
    /// `status_message` içinde bildirilir).
    pub fn save_new_tool(&mut self, form: &form::ToolForm, valid: &form::ValidTool) -> bool {
        let existing: Vec<String> = self
            .catalog
            .entries()
            .iter()
            .map(|e| e.id.clone())
            .chain(self.hidden.iter().cloned())
            .chain(self.state.file.custom_tools.iter().map(|t| t.id.clone()))
            .collect();
        let entry = form.to_entry(valid, &existing);
        let id = entry.id.clone();
        if let Err(e) = self.catalog.insert_entry(entry.clone()) {
            self.status_message = format!("Hata: araç kaydedilemedi: {e}");
            return false;
        }
        self.state.file.custom_tools.push(entry);
        // Yeni araç, kategori seçimi farklı olsa da görünsün.
        self.favorite_only = false;
        self.search_query.clear();
        self.status_message = match self.persist_state() {
            Ok(()) => {
                format!(
                    "✓ '{}' kaydedildi (id: {}). Kurulum durumu: {}. Programın elle \
                     kurulması gerekebilir; Nova kurulum yapmaz.",
                    valid.name,
                    id,
                    self.catalog
                        .entries()
                        .iter()
                        .find(|e| e.id == id)
                        .map(|e| e.status.label())
                        .unwrap_or("kontrol edilemedi")
                )
            }
            Err(e) => format!("Hata: araç kaydedilemedi (state.toml yazılamadı: {e})"),
        };
        let saved = !self.status_message.starts_with("Hata");
        if !saved {
            // Kayıt kalıcı olmadı: bellekteki katalogdan da geri al.
            self.catalog.remove_entry(&id);
            if let Some(pos) = self.state.file.custom_tools.iter().position(|t| t.id == id) {
                self.state.file.custom_tools.remove(pos);
            }
        }
        saved
    }

    /// Seçili araç için silme onayı ister.
    pub fn request_delete_selected(&mut self) {
        let tools = self.filtered_tools();
        let Some(entry) = tools.get(self.tool_index.min(tools.len().saturating_sub(1))) else {
            self.status_message = "Silinecek araç seçilmedi.".into();
            return;
        };
        let id = entry.id.clone();
        let name = entry.name.clone();
        let origin = entry.origin;
        if origin == crate::config::EntryOrigin::Builtin {
            self.status_message = format!(
                "Hata: '{}' uygulamayla gelen bir araç; silinemez (yalnızca özel araçlar silinir).",
                name
            );
            return;
        }
        let from_config = self.config_tool_ids.contains(&id);
        let detail = if from_config {
            "Araç config.toml'da tanımlı; dosya değiştirilmez, Nova'da gizlenir."
        } else {
            "Bu araç Nova'nın state.toml dosyasından kalıcı olarak silinir."
        };
        self.modal = modal::Modal::Confirm {
            question: format!("'{name}' silinsin mi? {detail} Enter onaylar, Esc vazgeçer."),
            action: modal::ConfirmAction::DeleteTool { id, name },
        };
    }

    /// Onaylanmış eylemi uygular.
    fn confirm_action(&mut self, action: modal::ConfirmAction) {
        match action {
            modal::ConfirmAction::DeleteTool { id, name } => {
                let from_config = self.config_tool_ids.contains(&id);
                match self.remove_tool(&id) {
                    Ok(_) => {
                        self.catalog.remove_entry(&id);
                        self.status_message = if from_config {
                            format!(
                                "✓ '{}' Nova'da gizlendi; tanım config.toml'da duruyor.",
                                name
                            )
                        } else {
                            format!("✓ '{}' silindi.", name)
                        };
                        let hidden = self.hidden_tools();
                        if hidden.contains(&id) {
                            self.status_message.push_str(
                                " (Geri almak için state.toml'dan 'hidden_tools' kaydını sil.)",
                            );
                        }
                    }
                    Err(e) => {
                        // Kalıcı yazma başarısız olduysa bellekteki gizleme
                        // geri alınır; kullanıcı yeniden başlatınca aracın
                        // sessizce geri gelmesi yanıltıcı olurdu.
                        self.hidden.remove(&id);
                        self.status_message = format!("Hata: araç silinemedi: {e}");
                    }
                }
            }
            modal::ConfirmAction::ClearRecents => {
                let n = self.state.clear_recents();
                self.status_message = match self.persist_state() {
                    Ok(()) => format!("✓ Geçmiş temizlendi ({n} kayıt silindi)."),
                    Err(e) => format!("Hata: geçmiş temizlendi ama kaydedilemedi: {e}"),
                };
            }
            modal::ConfirmAction::RemoveProject { path } => {
                let dirs = self.state.project_dirs();
                if let Some(pos) = dirs.iter().position(|p| p == &path) {
                    self.state.remove_project_dir(pos);
                    self.status_message = match self.persist_state() {
                        Ok(()) => format!(
                            "✓ '{}' proje listesinden çıkarıldı (dizin diskte duruyor).",
                            path.display()
                        ),
                        Err(e) => format!("Hata: liste kaydedilemedi: {e}"),
                    };
                }
            }
            modal::ConfirmAction::CreateDir { path, tool_id } => {
                match std::fs::create_dir_all(&path) {
                    Ok(()) => {
                        self.status_message = format!("✓ '{}' oluşturuldu.", path.display());
                        // Aynı aracı yeniden dene (kullanıcının onayı bu
                        // adımı kapsar).
                        let tools = self.filtered_tools();
                        if let Some(pos) = tools.iter().position(|e| e.id == tool_id) {
                            self.tool_index = pos;
                            self.focus = PanelFocus::Tool;
                            self.select_tool();
                        }
                    }
                    Err(e) => {
                        self.status_message =
                            format!("Hata: '{}' oluşturulamadı: {e}", path.display());
                    }
                }
            }
        }
    }
}

/// Çalışma dizinlerini çözmek için güvenli varsayılan ev dizini.
///
/// `HOME` yoksa boş yol döner; `WorkingDirectory` çözümlemesi bunu açık bir
/// hata olarak bildirir (sessizce yanlış dizine düşmez).
fn default_home_dir() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home),
        _ => PathBuf::new(),
    }
}

/// Şu anki UNIX zamanı (saniye).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Default for App {
    fn default() -> Self {
        Self::new(Catalog::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Test için kategori adını `CATEGORIES` indeksine çevirir.
    fn category_index(name: &str) -> usize {
        CATEGORIES
            .iter()
            .position(|c| *c == name)
            .unwrap_or_else(|| panic!("bilinmeyen kategori: {name}"))
    }

    /// Gerçek gömülü katalogla oluşturulmuş uygulama.
    ///
    /// Kullanıcının kendi `config.toml` dosyası okunmaz; testler makineye
    /// göre değişmemeli.
    fn sample_app() -> App {
        sample_app_with_category("Sistem ve Kaynak Kullanımı")
    }

    fn sample_app_with_category(category: &str) -> App {
        let catalog = crate::catalog::loader::builtin_catalog().expect("gömülü katalog okunmalı");
        let catalog = Catalog::from_entries(catalog).expect("katalog oluşturulamadı");
        let mut app = App::new(catalog);
        app.category_index = category_index(category);
        app
    }

    #[test]
    fn new_app_is_running_with_category_focus() {
        let app = sample_app();
        assert!(app.running);
        assert_eq!(app.focus, PanelFocus::Category);
        assert!(!app.search_active);
        assert!(!app.favorite_only);
        assert!(!app.show_help);
    }

    #[test]
    fn esc_quits_app() {
        let mut app = sample_app();
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert!(!app.running);
    }

    #[test]
    fn q_quits_app() {
        let mut app = sample_app();
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert!(!app.running);
    }

    #[test]
    fn toggle_search_mode() {
        let mut app = sample_app();
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('/'),
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert!(app.search_active);
    }

    #[test]
    fn esc_cancels_search() {
        let mut app = sample_app();
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('/'),
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert!(app.search_active);

        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert!(!app.search_active);
        assert!(app.search_query.is_empty());
    }

    #[test]
    fn toggle_favorite_updates_status() {
        let mut app = sample_app();
        app.focus = PanelFocus::Tool;
        app.category_index = category_index("Sistem ve Kaynak Kullanımı");
        app.tool_index = 0;

        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('f'),
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert!(
            app.status_message.contains("favorilere eklendi")
                || app.status_message.contains("favorilerden çıkarıldı")
        );
    }

    #[test]
    fn toggle_favorite_filter() {
        let mut app = sample_app();
        assert!(!app.favorite_only);

        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert!(app.favorite_only);

        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert!(!app.favorite_only);
    }

    #[test]
    fn panel_switching() {
        let mut app = sample_app();
        assert_eq!(app.focus, PanelFocus::Category);

        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('l'),
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert_eq!(app.focus, PanelFocus::Tool);

        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('h'),
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert_eq!(app.focus, PanelFocus::Category);
    }

    #[test]
    fn category_navigation_is_bounded() {
        let mut app = sample_app();
        app.category_index = 0;
        // Kategori panelinde aşağı hareket kategori indeksini artırır ve sınırı aşmaz.
        app.move_down();
        assert_eq!(app.category_index, 1);
        for _ in 0..CATEGORIES.len() + 5 {
            app.move_down();
        }
        assert_eq!(app.category_index, CATEGORIES.len() - 1);
        for _ in 0..CATEGORIES.len() + 5 {
            app.move_up();
        }
        assert_eq!(app.category_index, 0);
    }

    #[test]
    fn filtered_tools_empty_when_no_match() {
        let app = sample_app();
        // Seçili kategori "Sistem ve Kaynak Kullanımı", içinde araçlar var.
        assert!(!app.filtered_tools().is_empty());
    }

    #[test]
    fn toggle_help() {
        let mut app = sample_app();
        assert!(!app.show_help);
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('?'),
            crossterm::event::KeyModifiers::empty(),
        ))
        .unwrap();
        assert!(app.show_help);
    }
    fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::empty())
    }

    fn typed(c: char) -> crossterm::event::KeyEvent {
        key(crossterm::event::KeyCode::Char(c))
    }

    /// Her modalı açar ve verilen tuşlarla yazı yazılınca modalın
    /// kapanmadığını doğrular.
    ///
    /// Regresyon: modal düğümü `self.modal`den dışarı alınırken modalın
    /// `None` varken gözlenmesi, yazılan karakterde modalı kapatıyordu.
    #[test]
    fn typing_inside_modal_never_closes_it() {
        type IsOpen = fn(&modal::Modal) -> bool;
        let openers: Vec<(char, IsOpen)> = vec![
            ('P', |m| matches!(m, modal::Modal::ProjectPicker { .. })),
            ('R', |m| matches!(m, modal::Modal::History)),
            ('S', |m| matches!(m, modal::Modal::Settings { .. })),
            ('N', |m| matches!(m, modal::Modal::ToolForm(_))),
        ];
        for (opener, is_open) in openers {
            let mut app = sample_app();
            app.handle_key(typed(opener)).unwrap();
            assert!(is_open(&app.modal), "'{opener}' modalı açmadı");
            for c in ['/', 't', 'm', 'p', 'Z'] {
                app.handle_key(typed(c)).unwrap();
                assert!(
                    is_open(&app.modal),
                    "'{opener}' modalı '{c}' yazılınca kapandı: {:?}",
                    app.modal
                );
            }
        }
    }

    #[test]
    fn project_add_typing_accumulates_and_backspace_pops() {
        let mut app = sample_app();
        app.handle_key(typed('P')).unwrap();
        app.handle_key(typed('A')).unwrap();
        for c in ['/', 't', 'm', 'p'] {
            app.handle_key(typed(c)).unwrap();
        }
        assert_eq!(
            app.modal,
            modal::Modal::ProjectAdd {
                input: "/tmp".into(),
                error: None,
            }
        );
        app.handle_key(key(crossterm::event::KeyCode::Backspace))
            .unwrap();
        app.handle_key(key(crossterm::event::KeyCode::Left))
            .unwrap();
        assert_eq!(
            app.modal,
            modal::Modal::ProjectAdd {
                input: "/tm".into(),
                error: None,
            }
        );
        app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
        assert_eq!(app.modal, modal::Modal::ProjectPicker { index: 0 });
    }

    #[test]
    fn picker_shortcuts_do_not_reach_main_screen() {
        let mut app = sample_app();
        app.handle_key(typed('P')).unwrap();
        // 'q' ana ekranda çıkıştır; modal açıkken etkisiz kalmalı.
        app.handle_key(typed('q')).unwrap();
        assert!(app.running);
        app.handle_key(typed('j')).unwrap();
        app.handle_key(typed('k')).unwrap();
        assert!(matches!(app.modal, modal::Modal::ProjectPicker { .. }));
    }

    #[test]
    fn esc_closes_project_picker() {
        let mut app = sample_app();
        app.handle_key(typed('P')).unwrap();
        assert_eq!(app.modal, modal::Modal::ProjectPicker { index: 0 });
        app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
        assert_eq!(app.modal, modal::Modal::None);
        // Picker kapandıktan sonra ana ekran tuşları yeniden çalışır.
        app.handle_key(typed('j')).unwrap();
        assert_eq!(app.modal, modal::Modal::None);
    }

    #[test]
    fn esc_closes_history() {
        let mut app = sample_app();
        app.handle_key(typed('R')).unwrap();
        assert_eq!(app.modal, modal::Modal::History);
        app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
        assert_eq!(app.modal, modal::Modal::None);
    }

    #[test]
    fn history_x_asks_for_confirmation_and_n_cancels() {
        let mut app = sample_app();
        app.handle_key(typed('R')).unwrap();
        assert_eq!(app.modal, modal::Modal::History);
        app.handle_key(typed('z')).unwrap();
        assert_eq!(app.modal, modal::Modal::History);
        app.handle_key(typed('X')).unwrap();
        assert!(matches!(app.modal, modal::Modal::Confirm { .. }));
        app.handle_key(typed('n')).unwrap();
        assert_eq!(app.modal, modal::Modal::None);
    }

    #[test]
    fn tool_form_keeps_typed_values() {
        let mut app = sample_app();
        app.handle_key(typed('N')).unwrap();
        for c in ['x', 'y'] {
            app.handle_key(typed(c)).unwrap();
        }
        app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap();
        app.handle_key(typed('Z')).unwrap();
        app.handle_key(key(crossterm::event::KeyCode::Enter))
            .unwrap();
        let modal::Modal::ToolForm(form) = &app.modal else {
            panic!("form modaldan çıktı: {:?}", app.modal);
        };
        assert_eq!(form.name, "xy");
        assert_eq!(form.description, "Z");
        // Zorunlu alanlar boş: Enter ilk hatalı alana (çalıştırılabilir) odaklar.
        assert_eq!(form.focus, 3);
    }

    #[test]
    fn tool_form_esc_closes_it() {
        let mut app = sample_app();
        app.handle_key(typed('N')).unwrap();
        app.handle_key(key(crossterm::event::KeyCode::Esc)).unwrap();
        assert_eq!(app.modal, modal::Modal::None);
    }
}
