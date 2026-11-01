//! Uygulama durumu ve ana döngü modülü.
//! `App` yapısı uygulamanın tüm durumunu tutar ve `run` metodu ile
//! terminal arayüzünün ana döngüsünü sürdürür.

use crate::catalog::{Catalog, CATEGORIES};
use crate::error::NovaError;
use crate::event::{Event, EventReader};
use crate::file_picker::FilePickerState;
use crate::media_launcher::MediaLauncher;
use crate::store::host::{self, PmUpdatePlan};
use crate::store::manifest::{
    self, ConditionalRelease, ReleaseProvider, ReleaseSource, ResolvedRelease,
};
use crate::store::{Store, StoreEntry, STORE_CATEGORIES};
use crate::ui;
use crate::update::{UpdateCheck, UpdateProvider};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::collections::BTreeMap;
use std::io::Stdout;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
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
    /// Sekme (kategori) paneli odaklı.
    Category,
    /// Araç paneli odaklı.
    Tool,
    /// Mağaza kategorileri paneli odaklı.
    StoreCategory,
    /// Mağaza uygulamaları paneli odaklı.
    StoreApp,
}

/// Uygulama durumu.
#[derive(Debug, Clone)]
/// `handle_form_key`ün sonucu.
#[allow(clippy::large_enum_variant)]
enum FormOutcome {
    /// Form düzenlenmeye devam ediyor.
    Editing,
    /// Kullanıcı formu kapattı (Esc).
    Closed,
    /// Form doğrulandı; inceleme ekranına geçilir.
    Review(modal::Modal),
}

/// Arka planda süren mağaza kurulum işi.
///
/// İndirme ve doğrulama ağ üzerinden yapıldığı için ana döngüyü
/// kilitlememek adına ayrı bir iş parçacığında yürütülür. Sonuç kanaldan
/// alınır ([`App::poll_install`]).
#[derive(Debug)]
pub struct InstallJob {
    /// Kurulan uygulamanın kimliği.
    pub app_id: String,
    /// Kurulan uygulamanın adı.
    pub app_name: String,
    /// İşin sonucunu taşıyan kanal.
    receiver: Receiver<Result<crate::store::install::InstallOutcome, String>>,
}

/// sudo parola isteyen apt/dnf planı — ana döngüde TUI askıya alınarak çalışır.
#[derive(Debug, Clone)]
pub struct PendingTtyPm {
    pub entry: crate::store::StoreEntry,
    pub plan: host::InstallPlan,
    pub is_upgrade: bool,
}

/// Arka planda süren mağaza canlı sürüm / PM güncelleme denetimi.
///
/// GitHub Releases sorgusu + (kurulu) paket yöneticisi outdated taraması.
/// Release sonuçları [`crate::store::manifest::ManifestCache`] içinde
/// önbelleklenir.
#[derive(Debug)]
pub struct StoreCheckJob {
    /// İşin sonucunu taşıyan kanal.
    receiver: Receiver<StoreCheckOutcome>,
}

/// `U` denetiminin arka plan sonucu.
#[derive(Debug, Default)]
pub struct StoreCheckOutcome {
    /// GitHub Releases çözümleri.
    pub releases: BTreeMap<String, Result<ResolvedRelease, String>>,
    /// Paket yöneticisi güncelleme planları (kurulu + outdated / iyimser).
    pub pm_updates: BTreeMap<String, PmUpdatePlan>,
}

pub struct App {
    /// Uygulamanın çalışıp çalışmadığını belirler.
    pub running: bool,
    /// Araç kataloğu.
    pub catalog: Catalog,
    /// Şu anda odaklı panel.
    pub focus: PanelFocus,
    /// Seçili kategorinin indeksi.
    pub category_index: usize,
    /// Mağaza sekmesi etkin mi?
    ///
    /// Sol panelde Mağaza, Ayarlar'dan hemen önce gelir. `settings_tab` ile
    /// aynı anda açılmaz; mağaza ayrı bir modal değil, ana ekran sekmesidir.
    pub store_tab: bool,
    /// Ayarlar sekmesi etkin mi? (Mağaza'nın altındaki satır)
    pub settings_tab: bool,
    /// Ayarlar sekmesinde seçili eylem: 0 = Güncelle, 1 = Tema.
    pub settings_action_index: usize,
    /// Seçili mağaza kategorisinin indeksi.
    pub store_category_index: usize,
    /// Seçili mağaza uygulamasının indeksi (filtrelenmiş liste üzerinden).
    pub store_app_index: usize,
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
    /// Son bilinen terminal boyutu (fare tıklama alanı hesaplaması için).
    pub terminal_size: Option<(u16, u16)>,
    /// Mağaza kataloğu (uygulama kataloğundan ayrı).
    pub store: Store,
    /// Mağaza uygulamalarının kurulum durumu (mağaza kimliği → durum).
    ///
    /// Her karede `which` çağrısı yapmamak için durum önbellekte tutulur.
    /// Kurulum sonrasında [`App::refresh_detection`] ile tazelenir.
    pub store_status: BTreeMap<String, crate::detection::InstalledStatus>,
    /// Medya dosyalarını ayarlarda belirlenen uygulamayla açan başlatıcı.
    pub media_launcher: MediaLauncher,
    /// Güncelleme denetimi sağlayıcısı.
    pub update_provider: Box<dyn UpdateProvider + Send + Sync>,
    /// Arka planda süren mağaza kurulum işi (varsa).
    pub install_job: Option<InstallJob>,
    /// sudo TTY gerektiren PM kurulum/güncelleme (ana döngüde işlenir).
    pub pending_tty_pm: Option<PendingTtyPm>,
    /// Arka planda süren canlı sürüm denetimi (varsa).
    pub store_check_job: Option<StoreCheckJob>,
    /// Canlı denetimle çözülen sürümler (uygulama kimliği → sürüm).
    pub store_updates: BTreeMap<String, ResolvedRelease>,
    /// `U` sonrası paket yöneticisi güncelleme planları.
    pub store_pm_updates: BTreeMap<String, PmUpdatePlan>,
    /// Denetlenemeyen uygulamalar ve hataları.
    pub store_check_errors: BTreeMap<String, String>,
    /// Canlı sürüm çözümleyicisi (testlerde sahte olanı verilebilir).
    pub store_release_provider: Arc<dyn ReleaseProvider + Send + Sync>,
    /// Canlı sürüm önbelleğinin dosya yolu.
    ///
    /// `None` ise varsayılan konum kullanılır
    /// (`<store>/manifest-cache.toml`). Testler her teste ayrı bir yol verir.
    pub store_cache_path: Option<PathBuf>,
    /// Testlerde PM outdated taramasını sabitlemek için.
    ///
    /// `Some` ise host komutları çalıştırılmaz; verilen küme kullanılır.
    pub store_outdated_override: Option<host::OutdatedPackages>,
    /// Testlerde host PM varlığını sabitlemek için (`None` → gerçek `which`).
    pub store_host_tools_override: Option<host::HostTools>,
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
        let mut app = Self {
            running: true,
            catalog,
            focus: PanelFocus::Category,
            category_index: 0,
            store_tab: false,
            settings_tab: false,
            settings_action_index: 0,
            store_category_index: 0,
            store_app_index: 0,
            tool_index: 0,
            search_query: String::new(),
            search_active: false,
            favorite_only: false,
            show_help: false,
            narrow_mode: false,
            status_message: String::new(),
            launcher: real_launch,
            home_dir: default_home_dir(),
            media_launcher: MediaLauncher::new(
                state.file.prefs.default_image_app.clone(),
                state.file.prefs.default_video_app.clone(),
            ),
            state,
            first_run_seed: seed_favorites_from_catalog,
            hidden,
            modal: modal::Modal::None,
            config_tool_ids: std::collections::BTreeSet::new(),
            config_path_display: crate::config::user_config_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "bulunamadı (HOME yok)".into()),
            state_path_display,
            terminal_size: None,
            store: crate::store::loader::load_builtin_store().unwrap_or_default(),
            store_status: BTreeMap::new(),
            update_provider: Box::new(crate::update::DefaultUpdateProvider),
            install_job: None,
            pending_tty_pm: None,
            store_check_job: None,
            store_updates: BTreeMap::new(),
            store_pm_updates: BTreeMap::new(),
            store_check_errors: BTreeMap::new(),
            store_release_provider: Arc::new(crate::store::github::GitHubReleaseProvider::new()),
            store_cache_path: None,
            store_outdated_override: None,
            store_host_tools_override: None,
        };
        // Mağaza uygulamalarının kurulum durumu başlangıçta bir kez hesaplanır;
        // liste her karede bu önbellekten okur. Kurulu mağaza uygulamaları
        // ana katalogda yoksa (ör. gemini-cli) AI vb. kategoriye eklenir.
        app.refresh_store_status();
        app.sync_installed_store_apps_to_catalog();
        app
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

            // Arka plandaki kurulum ve sürüm denetimi işlerini kontrol et.
            self.poll_install();
            self.poll_store_check();
            // sudo parola isteyen apt/dnf: TUI’yi bırakıp gerçek TTY ile çalıştır.
            self.run_pending_tty_pm(terminal)?;
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
            Event::Mouse(mouse) => self.handle_mouse(mouse)?,
            Event::Resize(w, h) => {
                self.apply_resize(w, h);
                terminal.resize(ratatui::layout::Rect::new(0, 0, w, h))?;
            }
            Event::Ignored => {}
        }
        Ok(())
    }

    /// Fare olayını işler: kategori/araç tıklaması ve kaydırma. Tıklama
    /// alanları mevcut terminal boyutundan yeniden hesaplanır.
    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) -> Result<(), NovaError> {
        use crossterm::event::{MouseButton, MouseEventKind};

        let size = match self.terminal_size {
            Some(s) => s,
            None => return Ok(()),
        };
        let areas = crate::ui::click_areas(size, self.show_help, self.store_tab);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let x = mouse.column;
                let y = mouse.row;
                // İçerik alanı dışındaki tıklamalar yok sayılır.
                if y < areas.content_top
                    || y >= areas.content_top.saturating_add(areas.content_height)
                {
                    return Ok(());
                }
                let row = (y - areas.content_top) as usize;
                if areas.wide && x < areas.category_end_x {
                    // Sekme listesi: araç kategorileri ve mağaza sekmesi.
                    self.set_tab_index(row);
                    self.focus = PanelFocus::Category;
                    self.tool_index = 0;
                } else if self.store_tab {
                    // Mağaza görünümü: ortadaki sütun mağaza kategorileri,
                    // sağdaki sütun o kategorideki uygulamalar.
                    if areas.wide {
                        if x < areas.store_category_end_x {
                            self.store_category_index =
                                row.min(STORE_CATEGORIES.len().saturating_sub(1));
                            self.store_app_index = 0;
                            self.focus = PanelFocus::StoreCategory;
                        } else {
                            let apps = self.filtered_store_apps();
                            if !apps.is_empty() {
                                self.store_app_index = row.min(apps.len().saturating_sub(1));
                                self.focus = PanelFocus::StoreApp;
                            }
                        }
                    } else {
                        // Dar modda kategoriler üstte, uygulamalar altta.
                        let cat_rows =
                            areas.store_category_end_y.saturating_sub(areas.content_top) as usize;
                        if row < cat_rows {
                            self.store_category_index =
                                row.min(STORE_CATEGORIES.len().saturating_sub(1));
                            self.store_app_index = 0;
                            self.focus = PanelFocus::StoreCategory;
                        } else {
                            let apps = self.filtered_store_apps();
                            if !apps.is_empty() {
                                self.store_app_index = row
                                    .saturating_sub(cat_rows)
                                    .min(apps.len().saturating_sub(1));
                                self.focus = PanelFocus::StoreApp;
                            }
                        }
                    }
                } else if self.settings_tab {
                    self.settings_action_index = row.min(1);
                    self.focus = PanelFocus::Tool;
                } else {
                    // Araç paneli.
                    let tools = self.filtered_tools();
                    if !tools.is_empty() {
                        self.tool_index = row.min(tools.len().saturating_sub(1));
                        self.focus = PanelFocus::Tool;
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                self.status_message.clear();
                self.move_up();
            }
            MouseEventKind::ScrollDown => {
                self.status_message.clear();
                self.move_down();
            }
            _ => {}
        }
        Ok(())
    }

    /// Terminal yeniden boyutlandığında dar/geniş modu, odak ve fare
    /// tıklama alanı boyutunu günceller. Terminalin yeniden çizilmesi
    /// çağrıcıya bırakılır.
    fn apply_resize(&mut self, w: u16, h: u16) {
        // Dar terminale geçildiğinde kategori paneli gizlenir; odak
        // görünmeyen panelde kalmasın.
        self.narrow_mode = w < crate::ui::WIDE_MIN_WIDTH;
        if self.narrow_mode {
            self.focus = PanelFocus::Tool;
        }
        self.terminal_size = Some((w, h));
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
                self.focus = self.search_target_focus();
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                // Favoriler araç kataloğuna aittir; mağazada karşılığı yok.
                if self.store_tab {
                    self.status_message = "Favoriler araç listesinde çalışır.".into();
                } else {
                    self.toggle_selected_favorite();
                }
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
                // Dar terminalde araç kategorisi paneli gizli olduğundan odak
                // listede kalır; yön tuşları yanıltıcı olmasın.
                if self.narrow_mode && !self.store_tab {
                    self.status_message =
                        "Kategori paneli bu genişlikte gizli; terminali genişlet.".into();
                } else {
                    self.focus = self.prev_focus();
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.focus = self.next_focus();
            }
            KeyCode::Tab => {
                self.focus = self.next_focus();
            }
            KeyCode::Enter => match self.focus {
                PanelFocus::Category => {
                    self.focus = self.next_focus();
                }
                PanelFocus::Tool => self.select_tool(),
                PanelFocus::StoreCategory => {
                    self.focus = PanelFocus::StoreApp;
                }
                PanelFocus::StoreApp => self.select_store_app(),
            },
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.modal = modal::Modal::History;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.modal = modal::Modal::ProjectPicker {
                    index: self.project_cursor(),
                };
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Sol paneldeki Ayarlar sekmesine gider (Güncelle + Tema).
                // Gelişmiş tercihler (terminal, medya…) Ayarlar içinde
                // "Gelişmiş" ile veya Esc sonrası eski modal ile açılabilir.
                self.select_settings_tab();
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                // Mağaza artık ayrı bir modal değil, sol paneldeki "Mağaza"
                // sekmesidir. Bu tuş yalnızca o sekmeye kısayoldur.
                self.select_store_tab();
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                // Mağaza sekmesindeyken bu tuş Nova'nın kendi güncellemesini
                // değil, mağaza uygulamalarının canlı sürüm denetimini yapar.
                if self.store_tab {
                    self.check_store_updates();
                } else {
                    self.start_nova_update_check();
                }
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                self.modal = modal::Modal::FilePicker {
                    state: FilePickerState::new(&self.home_dir),
                };
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.modal = modal::Modal::ToolForm(Box::default());
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if self.store_tab {
                    self.status_message = "Silme/gizleme araç listesinde çalışır.".into();
                } else {
                    self.request_delete_selected();
                }
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if self.store_tab {
                    self.status_message = "Favori filtresi araç listesinde çalışır.".into();
                } else {
                    self.favorite_only = !self.favorite_only;
                    if self.favorite_only {
                        self.status_message = "Sadece favoriler gösteriliyor.".into();
                    } else {
                        self.status_message = "Tüm araçlar gösteriliyor.".into();
                    }
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
                self.focus = self.search_target_focus();
                if self.search_query.is_empty() {
                    self.status_message = "Arama temizlendi.".into();
                } else {
                    let count = self.search_result_count();
                    self.status_message = format!("'{}' için {} sonuç", self.search_query, count);
                }
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.reset_tool_selection();
            }
            KeyCode::Down => {
                self.focus = self.search_target_focus();
                self.move_down();
            }
            KeyCode::Up => {
                self.focus = self.search_target_focus();
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
        self.store_app_index = 0;
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
            PanelFocus::Category => self.move_tab_down(),
            PanelFocus::Tool => {
                if self.settings_tab {
                    self.settings_action_index = (self.settings_action_index + 1).min(1);
                    return;
                }
                let tools = self.filtered_tools();
                if tools.is_empty() {
                    return;
                }
                let max = tools.len().saturating_sub(1);
                self.tool_index = (self.tool_index + 1).min(max);
            }
            PanelFocus::StoreCategory => {
                let max = STORE_CATEGORIES.len().saturating_sub(1);
                self.store_category_index = (self.store_category_index + 1).min(max);
                // Kategori değişince uygulama seçimi başa döner.
                self.store_app_index = 0;
            }
            PanelFocus::StoreApp => {
                let apps = self.filtered_store_apps();
                if apps.is_empty() {
                    return;
                }
                let max = apps.len().saturating_sub(1);
                self.store_app_index = (self.store_app_index + 1).min(max);
            }
        }
    }

    /// Yukarı doğru hareket eder.
    fn move_up(&mut self) {
        match self.focus {
            PanelFocus::Category => self.move_tab_up(),
            PanelFocus::Tool => {
                if self.settings_tab {
                    self.settings_action_index = self.settings_action_index.saturating_sub(1);
                    return;
                }
                self.tool_index = self.tool_index.saturating_sub(1);
            }
            PanelFocus::StoreCategory => {
                self.store_category_index = self.store_category_index.saturating_sub(1);
                self.store_app_index = 0;
            }
            PanelFocus::StoreApp => {
                self.store_app_index = self.store_app_index.saturating_sub(1);
            }
        }
    }

    /// Sekme listesinde aşağı hareket eder.
    ///
    /// Görünür (kurulu araçlı) kategoriler → Mağaza → Ayarlar.
    /// Odak Category panelinde kalır (`m` / `select_store_tab` odak değiştirir).
    fn move_tab_down(&mut self) {
        if self.settings_tab {
            return;
        }
        if self.store_tab {
            self.store_tab = false;
            self.settings_tab = true;
            self.settings_action_index = 0;
            return;
        }
        let visible = self.visible_category_indices();
        if visible.is_empty() {
            self.store_tab = true;
            self.settings_tab = false;
            self.store_category_index = 0;
            self.store_app_index = 0;
            return;
        }
        if let Some(pos) = visible.iter().position(|&i| i == self.category_index) {
            if pos + 1 < visible.len() {
                self.category_index = visible[pos + 1];
                self.tool_index = 0;
            } else {
                self.store_tab = true;
                self.settings_tab = false;
                self.store_category_index = 0;
                self.store_app_index = 0;
            }
        } else {
            self.category_index = visible[0];
            self.tool_index = 0;
        }
    }

    /// Sekme listesinde yukarı hareket eder.
    fn move_tab_up(&mut self) {
        if self.settings_tab {
            self.store_tab = true;
            self.settings_tab = false;
            self.store_category_index = 0;
            self.store_app_index = 0;
            return;
        }
        if self.store_tab {
            self.store_tab = false;
            self.settings_tab = false;
            let visible = self.visible_category_indices();
            self.category_index = visible.last().copied().unwrap_or(0);
            self.tool_index = 0;
            return;
        }
        let visible = self.visible_category_indices();
        if let Some(pos) = visible.iter().position(|&i| i == self.category_index) {
            if pos > 0 {
                self.category_index = visible[pos - 1];
                self.tool_index = 0;
            }
        } else if let Some(&first) = visible.first() {
            self.category_index = first;
            self.tool_index = 0;
        }
    }

    /// Mağaza sekmesine geçer.
    ///
    /// `m` kısayolu da budur. Odağı doğrudan uygulama listesine götürür:
    /// kullanıcı kurulum eylemini hemen görebilsin diye.
    pub fn select_store_tab(&mut self) {
        self.store_tab = true;
        self.settings_tab = false;
        self.store_category_index = 0;
        self.store_app_index = 0;
        self.focus = PanelFocus::StoreApp;
        self.status_message =
            "Mağaza: solda mağaza kategorileri, sağda o kategorideki uygulamalar.".into();
    }

    /// Ayarlar sekmesine geçer (Mağaza'nın altı).
    pub fn select_settings_tab(&mut self) {
        self.settings_tab = true;
        self.store_tab = false;
        self.settings_action_index = 0;
        self.focus = PanelFocus::Tool;
        self.status_message = "Ayarlar: Güncelle (GitHub) veya Tema — Enter ile uygula.".into();
    }

    /// Sol panelde seçili sekmenin satır indeksi (çizilen liste üzerinden).
    ///
    /// Satırlar: görünür kategoriler + Mağaza + Ayarlar.
    pub fn selected_tab_index(&self) -> usize {
        let n = self.visible_category_indices().len();
        if self.settings_tab {
            n + 1
        } else if self.store_tab {
            n
        } else {
            self.visible_category_indices()
                .iter()
                .position(|&i| i == self.category_index)
                .unwrap_or(0)
        }
    }

    /// Sol paneldeki satıra göre sekmeyi seçer.
    ///
    /// Görünür kategori satırlarından sonra Mağaza, en sonda Ayarlar gelir.
    pub fn set_tab_index(&mut self, row: usize) {
        let visible = self.visible_category_indices();
        if row < visible.len() {
            self.store_tab = false;
            self.settings_tab = false;
            self.category_index = visible[row];
            self.tool_index = 0;
            self.store_app_index = 0;
        } else if row == visible.len() {
            self.select_store_tab();
        } else {
            self.select_settings_tab();
        }
    }

    /// Etkin odak zinciri (soldan sağa).
    ///
    /// Mağaza sekmesinde üç panel vardır: sekme listesi, mağaza kategorileri
    /// ve mağaza uygulamaları. Araç/Ayarlar sekmesinde iki panel vardır. Dar
    /// terminalde gizlenen paneller zincirden çıkarılır.
    fn focus_chain(&self) -> &'static [PanelFocus] {
        match (self.store_tab, self.settings_tab, self.narrow_mode) {
            (true, _, false) => &[
                PanelFocus::Category,
                PanelFocus::StoreCategory,
                PanelFocus::StoreApp,
            ],
            (true, _, true) => &[PanelFocus::StoreCategory, PanelFocus::StoreApp],
            (_, true, false) | (false, false, false) => &[PanelFocus::Category, PanelFocus::Tool],
            (_, true, true) | (false, false, true) => &[PanelFocus::Tool],
        }
    }

    /// Odak zincirinde bir sonraki panele geçer (son panelde kalır).
    fn next_focus(&self) -> PanelFocus {
        let chain = self.focus_chain();
        let idx = chain.iter().position(|f| *f == self.focus).unwrap_or(0);
        chain
            .get(idx + 1)
            .copied()
            .unwrap_or_else(|| *chain.last().expect("odak zinciri boş olamaz"))
    }

    /// Odak zincirinde bir önceki panele geçer (ilk panelde kalır).
    fn prev_focus(&self) -> PanelFocus {
        let chain = self.focus_chain();
        let idx = chain.iter().position(|f| *f == self.focus).unwrap_or(0);
        chain
            .get(idx.saturating_sub(1))
            .copied()
            .unwrap_or(chain[0])
    }

    /// Aramadan sonra odaklanılacak panel.
    fn search_target_focus(&self) -> PanelFocus {
        if self.store_tab {
            PanelFocus::StoreApp
        } else {
            PanelFocus::Tool
        }
    }

    /// Arama sonuç sayısı (etkin panele göre).
    fn search_result_count(&self) -> usize {
        if self.store_tab {
            self.filtered_store_apps().len()
        } else {
            self.filtered_tools().len()
        }
    }

    /// Seçili aracı yeni bir terminal sekmesinde başlatmayı ister.
    ///
    /// Hata olsa bile Nova çalışmaya devam eder: başlatma hatası TUI'yi
    /// bozmaz, yalnızca durum satırında görünür. Başarı mesajı bilinçli
    /// olarak "istendi" der; aracın gerçekten çalıştığını Nova doğrulayamaz.
    pub fn select_tool(&mut self) {
        if self.settings_tab {
            self.activate_settings_action();
            return;
        }
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
            || crate::catalog::short_category_name(&entry.category)
                .map(|s| crate::catalog::normalize(s).contains(&q))
                .unwrap_or(false)
            || crate::catalog::normalize(&entry.description).contains(&q)
    }

    /// Seçili kategoriyi döner.
    pub fn selected_category(&self) -> Option<&str> {
        CATEGORIES.get(self.category_index).copied()
    }

    /// Filtrelenmiş araç listesini döner (kategori, arama, favori filtresi uygulanmış).
    ///
    /// Ana başlatıcı **yalnızca kurulu** araçları gösterir. Kurulu olmayanlar
    /// Mağaza sekmesinde kalır (`filtered_store_apps` her iki durumu da listeler).
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
        tools.retain(|e| e.status == crate::detection::InstalledStatus::Installed);

        tools
    }

    /// Ana listede (kurulu + gizlisiz + isteğe bağlı favori/arama) en az bir
    /// araç olan kategoriler. Boş kategoriler sol panelde gizlenir.
    pub fn visible_category_indices(&self) -> Vec<usize> {
        CATEGORIES
            .iter()
            .enumerate()
            .filter(|(_, cat)| self.category_has_launcher_tools(cat))
            .map(|(i, _)| i)
            .collect()
    }

    /// Kategoride ana başlatıcıda gösterilecek en az bir araç var mı?
    pub fn category_has_launcher_tools(&self, category: &str) -> bool {
        self.catalog.by_category(category).into_iter().any(|e| {
            e.status == crate::detection::InstalledStatus::Installed
                && !self.hidden.contains(&e.id)
                && (!self.favorite_only || e.favorite)
                && (self.search_query.is_empty() || self.matches_search(e))
        })
    }

    /// Etkin UI teması (`prefs.theme`).
    pub fn ui_theme(&self) -> crate::ui::UiTheme {
        crate::ui::UiTheme::parse(self.state.file.prefs.theme.as_deref().unwrap_or(""))
    }

    /// Temayı bir sonraki değere alır ve state.toml'a yazar.
    pub fn cycle_theme(&mut self) -> Result<crate::ui::UiTheme, String> {
        let next = self.ui_theme().next();
        self.state.file.prefs.theme = Some(next.id().to_string());
        self.persist_state()
            .map_err(|e| format!("Tema kaydedilemedi: {e}"))?;
        Ok(next)
    }

    /// Nova güncelleme denetimini başlatır (Ayarlar / `U`).
    pub fn start_nova_update_check(&mut self) {
        self.modal = modal::Modal::UpdateCheck {
            status: modal::UpdateStatus::Idle,
            check: None,
            error: None,
        };
    }

    /// Ayarlar sekmesindeki seçili eylemi uygular.
    pub fn activate_settings_action(&mut self) {
        match self.settings_action_index {
            0 => {
                self.start_nova_update_check();
                // Denetimi hemen çalıştır (modal içindeki U ile aynı).
                if let modal::Modal::UpdateCheck {
                    status,
                    check,
                    error,
                } = &mut self.modal
                {
                    *status = modal::UpdateStatus::Checking;
                    *check = None;
                    *error = None;
                    let provider = self.update_provider.as_ref();
                    let current = provider.current_version();
                    match provider.fetch_latest_release() {
                        Ok(Some(release)) => {
                            let latest = release.version.clone();
                            if release.version > current {
                                *check = Some(UpdateCheck::UpdateAvailable {
                                    current,
                                    latest,
                                    release: Box::new(release),
                                });
                            } else {
                                *check = Some(UpdateCheck::UpToDate { current });
                            }
                            *status = modal::UpdateStatus::Done;
                        }
                        Ok(None) => {
                            *check = Some(UpdateCheck::UpToDate { current });
                            *status = modal::UpdateStatus::Done;
                        }
                        Err(e) => {
                            *error = Some(e);
                            *status = modal::UpdateStatus::Done;
                        }
                    }
                }
            }
            _ => match self.cycle_theme() {
                Ok(theme) => {
                    self.status_message = format!("✓ Tema: {} ({})", theme.label(), theme.id());
                }
                Err(e) => {
                    self.status_message = format!("Hata: {e}");
                }
            },
        }
    }

    /// Gelişmiş ayarlar modalını açar (terminal, medya varsayılanları…).
    pub fn open_advanced_settings_modal(&mut self) {
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
            theme: self.state.file.prefs.theme.clone().unwrap_or_default(),
            default_ai_tool: self
                .state
                .file
                .prefs
                .default_ai_tool
                .clone()
                .unwrap_or_default(),
            default_image_app: self
                .state
                .file
                .prefs
                .default_image_app
                .clone()
                .unwrap_or_default(),
            default_video_app: self
                .state
                .file
                .prefs
                .default_video_app
                .clone()
                .unwrap_or_default(),
            focus: 0,
            error: None,
        };
    }

    /// Mağaza için filtrelenmiş uygulama listesi (kategori ve arama filtresi).
    ///
    /// Arama sırasında kategori filtresi uygulanmaz: kullanıcı bütün
    /// mağazada aradığını bulabilmelidir.
    pub fn filtered_store_apps(&self) -> Vec<&StoreEntry> {
        let category = self.selected_store_category();
        let searching = !self.search_query.is_empty();
        if searching {
            self.store.search(&self.search_query)
        } else {
            self.store.by_category(category)
        }
    }

    /// Seçili mağaza kategorisinin adı.
    pub fn selected_store_category(&self) -> &str {
        STORE_CATEGORIES
            .get(self.store_category_index)
            .copied()
            .unwrap_or("")
    }

    /// Seçili mağaza uygulamasını döner (detay paneli için).
    pub fn selected_store_app(&self) -> Option<&StoreEntry> {
        let apps = self.filtered_store_apps();
        if apps.is_empty() {
            return None;
        }
        apps.get(self.store_app_index.min(apps.len() - 1)).copied()
    }

    /// Seçili mağaza uygulamasına yönelik eylemi başlatır.
    ///
    /// Öncelik sırası:
    ///
    /// 1. Canlı denetimde bulunmuş **yeni sürüm** varsa güncelleme onayı
    ///    sorulur.
    /// 2. Doğrulanabilir özeti olmayan kayıtta Nova kurulum yapmaz; resmî
    ///    yönergeyi gösterir.
    /// 3. Diğer hâllerde indirme + SHA-256 doğrulamalı kurulum onayı
    ///    sorulur.
    pub fn select_store_app(&mut self) {
        let apps = self.filtered_store_apps();
        let Some(entry) = apps
            .get(self.store_app_index.min(apps.len().saturating_sub(1)))
            .cloned()
        else {
            self.status_message = "Seçili mağaza uygulaması yok.".into();
            return;
        };

        let update = self.store_update_for(&entry.id).cloned();
        let has_archive_update = update.as_ref().is_some_and(|r| {
            manifest::is_newer(&r.version, &entry.version.clone().unwrap_or_default())
        });
        let pm_update = self.store_pm_update_for(&entry.id).cloned();

        if has_archive_update {
            let release = update.expect("güncelleme var");
            self.modal = modal::Modal::Confirm {
                question: format!(
                    "'{}' {} sürümünden {} sürümüne güncellensin mi? \
                     Enter onaylar, Esc vazgeçer.",
                    entry.name,
                    entry.version.clone().unwrap_or_default(),
                    release.version
                ),
                action: modal::ConfirmAction::UpdateStoreApp {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                    version: release.version.clone(),
                },
            };
        } else if let Some(pm) = pm_update {
            let cmd = pm.rendered();
            self.modal = modal::Modal::Confirm {
                question: format!(
                    "'{}' paket yöneticisiyle güncellensin mi? ({}) {cmd} \
                     Enter onaylar, Esc vazgeçer.",
                    entry.name,
                    pm.method.short_label()
                ),
                action: modal::ConfirmAction::UpdateStoreApp {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                    version: format!("pm:{}", pm.hint),
                },
            };
        } else if !entry.install_method.is_installable() {
            // curl|bash vb. manual kayıtlarda "Kur" sunulmaz; talimat gösterilir.
            let note = entry
                .install_notes
                .as_deref()
                .unwrap_or("resmî kurulum yönergelerini takip edin");
            self.status_message = format!("'{}' için {note}", entry.name);
        } else if entry.install_method.is_package_manager() {
            let plan = crate::store::install::resolve_entry_install_plan(entry).unwrap_or(
                crate::store::host::InstallPlan {
                    method: entry.install_method,
                    argv: entry.install_command.clone().unwrap_or_default(),
                },
            );
            let cmd = plan.rendered();
            self.modal = modal::Modal::Confirm {
                question: format!(
                    "'{}' kurulsun mu? Komut ({}) — kabuksuz çalışır: {cmd} \
                     Enter onaylar, Esc vazgeçer.",
                    entry.name,
                    plan.method.short_label()
                ),
                action: modal::ConfirmAction::InstallStoreApp {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                },
            };
        } else {
            self.modal = modal::Modal::Confirm {
                question: format!(
                    "'{}' (id: {}) uygulaması indirilip SHA-256 ile doğrulanıp \
                     kullanıcı alanına kurulsun mu? Enter onaylar, Esc vazgeçer.",
                    entry.name, entry.id
                ),
                action: modal::ConfirmAction::InstallStoreApp {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                },
            };
        }
    }

    /// Kullanıcıya gösterilmeyen (gizlenmiş) araç kimlikleri.
    pub fn hidden_tools(&self) -> &std::collections::BTreeSet<String> {
        &self.hidden
    }

    /// Mağazadaki bir uygulamanın kurulumunu başlatır.
    ///
    /// Kurulum arka planda yürütülür: indirme, SHA-256 doğrulama ve arşiv
    /// çıkarma ana döngüyü kilitlemez. Sonuç
    /// [`App::poll_install`] ile alınır.
    ///
    /// `download_archive` ve paket yöneticisi (`pipx`/`npm`/`cargo`/`brew`/
    /// `apt`/`dnf`/`uv`) kayıtları kurulabilir; `manual` kayıtlarda resmî
    /// talimat gösterilir. sudo parola isteyen apt/dnf planları
    /// [`App::run_pending_tty_pm`] ile TUI askıda çalışır.
    /// Kurulum her zaman önce onay modalından geçer.
    pub fn start_store_install(&mut self, id: &str) {
        if self.install_job.is_some() || self.pending_tty_pm.is_some() {
            self.status_message = "Zaten süren bir kurulum var; önce onun bitmesini bekle.".into();
            return;
        }
        let Some(entry) = self.store.get_by_id(id).cloned() else {
            self.status_message = format!("Mağazada '{}' bulunamadı.", id);
            return;
        };
        if !entry.install_method.is_installable() {
            let note = entry
                .install_notes
                .as_deref()
                .unwrap_or("resmî kurulum yönergelerini takip edin");
            self.status_message = format!("'{}' için Nova kurulum yapmıyor. {note}", entry.name);
            return;
        }

        let (tx, rx) = mpsc::channel();
        let app_name = entry.name.clone();

        if entry.install_method.is_package_manager() {
            let plan = crate::store::install::resolve_entry_install_plan(&entry).unwrap_or(
                crate::store::host::InstallPlan {
                    method: entry.install_method,
                    argv: entry.install_command.clone().unwrap_or_default(),
                },
            );
            let cmd = plan.rendered();
            let method_label = plan.method.short_label().to_string();
            if plan.needs_tty_suspend() {
                self.pending_tty_pm = Some(PendingTtyPm {
                    entry,
                    plan,
                    is_upgrade: false,
                });
                self.status_message =
                    format!("'{app_name}' kuruluyor ({method_label}, sudo TTY): {cmd}...");
                return;
            }
            let plan_bg = plan.clone();
            std::thread::spawn(move || {
                let result = crate::store::install::run_package_manager_argv(
                    &entry,
                    &plan_bg,
                    false,
                    crate::store::install::PmIoMode::Captured,
                )
                .map_err(|e| e.to_string());
                let _ = tx.send(result);
            });
            self.install_job = Some(InstallJob {
                app_id: id.to_string(),
                app_name: app_name.clone(),
                receiver: rx,
            });
            self.status_message = format!("'{app_name}' kuruluyor ({method_label}): {cmd}...");
            return;
        }

        let Some(dest_dir) = crate::store::store_app_dir(&entry.id) else {
            self.status_message = "Kurulum dizini belirlenemedi ($HOME bulunamadı).".into();
            return;
        };

        std::thread::spawn(move || {
            let downloader = crate::store::http::HttpDownloader::new();
            let result = crate::store::install::install_app(&entry, &dest_dir, &downloader)
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });

        self.install_job = Some(InstallJob {
            app_id: id.to_string(),
            app_name: app_name.clone(),
            receiver: rx,
        });
        self.status_message = format!("'{}' indiriliyor ve SHA-256 ile doğrulanıyor...", app_name);
    }

    /// Mağazadaki bir uygulamanın **yeni sürümle** güncellenmesini başlatır.
    ///
    /// İki yol:
    /// 1. Arşiv / GitHub Releases: canlı denetimde çözülen adres + SHA-256.
    /// 2. Paket yöneticisi: `U` ile bulunan upgrade argv
    ///    (pipx/npm/brew/apt/dnf/pacman/uv/cargo).
    pub fn start_store_update(&mut self, id: &str) {
        if self.install_job.is_some() || self.pending_tty_pm.is_some() {
            self.status_message = "Zaten süren bir kurulum var; önce onun bitmesini bekle.".into();
            return;
        }
        let Some(entry) = self.store.get_by_id(id).cloned() else {
            self.status_message = format!("Mağazada '{}' bulunamadı.", id);
            return;
        };

        if let Some(pm) = self.store_pm_updates.get(id).cloned() {
            let (tx, rx) = mpsc::channel();
            let app_name = entry.name.clone();
            let cmd = pm.rendered();
            let method_label = pm.method.short_label().to_string();
            let plan = pm.as_install_plan();
            if plan.needs_tty_suspend() {
                self.pending_tty_pm = Some(PendingTtyPm {
                    entry,
                    plan,
                    is_upgrade: true,
                });
                self.status_message =
                    format!("'{app_name}' güncelleniyor ({method_label}, sudo TTY): {cmd}...");
                return;
            }
            std::thread::spawn(move || {
                let result = crate::store::install::run_package_manager_argv(
                    &entry,
                    &plan,
                    true,
                    crate::store::install::PmIoMode::Captured,
                )
                .map_err(|e| e.to_string());
                let _ = tx.send(result);
            });
            self.install_job = Some(InstallJob {
                app_id: id.to_string(),
                app_name: app_name.clone(),
                receiver: rx,
            });
            self.status_message = format!("'{app_name}' güncelleniyor ({method_label}): {cmd}...");
            return;
        }

        let Some(release) = self.store_updates.get(id).cloned() else {
            self.status_message = format!(
                "'{}' için çözülmüş bir sürüm yok; önce sürümleri denetle (U).",
                entry.name
            );
            return;
        };
        let Some(dest_dir) = crate::store::store_app_dir(&entry.id) else {
            self.status_message = "Kurulum dizini belirlenemedi ($HOME bulunamadı).".into();
            return;
        };

        let (tx, rx) = mpsc::channel();
        let app_name = entry.name.clone();
        let version = release.version.clone();
        std::thread::spawn(move || {
            let downloader = crate::store::http::HttpDownloader::new();
            let result =
                crate::store::install::install_release(&entry, &dest_dir, &release, &downloader)
                    .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });

        self.install_job = Some(InstallJob {
            app_id: id.to_string(),
            app_name: app_name.clone(),
            receiver: rx,
        });
        self.status_message = format!(
            "'{}' {} sürümüne güncelleniyor (SHA-256 doğrulamasıyla)...",
            app_name, version
        );
    }

    /// Arka plandaki kurulum işinin durumunu kontrol eder.
    ///
    /// İş tamamlandıysa sonucu durum satırına yazar ve katalogdaki tespit
    /// durumlarını tazeler (kurulan uygulama artık "kurulu" görünür).
    pub fn poll_install(&mut self) {
        let Some(job) = self.install_job.take() else {
            return;
        };
        match job.receiver.try_recv() {
            Ok(Ok(outcome)) => {
                self.apply_install_outcome(outcome);
            }
            Ok(Err(message)) => {
                self.status_message = format!("Hata: {message}");
            }
            Err(TryRecvError::Empty) => {
                // İş hâlâ sürüyor.
                self.install_job = Some(job);
            }
            Err(TryRecvError::Disconnected) => {
                self.status_message = "Hata: kurulum işi beklenmedik şekilde sonlandı.".into();
            }
        }
    }

    /// Kurulum/güncelleme sonucunu mağaza durumuna işler.
    fn apply_install_outcome(&mut self, outcome: crate::store::install::InstallOutcome) {
        let resolved = self.store_updates.get(&outcome.app_id).cloned();
        let current = self.store.get_by_id(&outcome.app_id).cloned();
        let checksum = resolved
            .as_ref()
            .map(|r| r.checksum.clone())
            .or_else(|| current.as_ref().and_then(|e| e.checksum.clone()))
            .unwrap_or_default();
        let download_url = resolved
            .as_ref()
            .map(|r| r.download_url.clone())
            .or_else(|| current.as_ref().and_then(|e| e.download_url.clone()))
            .unwrap_or_default();
        self.store
            .mark_installed(&outcome.app_id, &outcome.version, &checksum, &download_url);
        self.store_pm_updates.remove(&outcome.app_id);
        // Mağaza ✓ → ana katalog (eşleşen araç veya otomatik ekleme).
        self.sync_store_app_to_catalog(&outcome.app_id);
        self.status_message = format!("✓ {}", outcome.summary);
        self.refresh_detection();
    }

    /// Kurulu mağaza uygulamalarını ana katalogla senkronize eder.
    ///
    /// Başlangıçta yalnızca kullanıcı alanı PM / arşiv kurulumlarını
    /// (pipx/npm/cargo/brew/uv/archive) taşır — apt/dnf/pacman ile
    /// sistemde zaten var olan paketler kataloğu şişirmez. Enter ile
    /// kurulum sonrası [`Self::sync_store_app_to_catalog`] her yöntemi
    /// kapsar.
    ///
    /// - Katalogda aynı `id` veya `executable` varsa eklenmez.
    /// - Yoksa mağaza kaydı `custom_tools` + kategoriye eklenir
    ///   (ör. store `AI` → katalog `AI ve Kodlama`).
    /// - Gizlenmiş kimlikler yeniden eklenmez.
    pub fn sync_installed_store_apps_to_catalog(&mut self) {
        use crate::store::InstallMethod;
        let installed_ids: Vec<String> = self
            .store
            .entries()
            .iter()
            .filter(|e| {
                matches!(
                    e.install_method,
                    InstallMethod::Pipx
                        | InstallMethod::Npm
                        | InstallMethod::Cargo
                        | InstallMethod::Brew
                        | InstallMethod::Uv
                        | InstallMethod::DownloadArchive
                ) && self.store_status.get(&e.id).copied().unwrap_or_default()
                    == crate::detection::InstalledStatus::Installed
            })
            .map(|e| e.id.clone())
            .collect();
        for id in installed_ids {
            self.sync_store_app_to_catalog(&id);
        }
    }

    /// Tek bir mağaza uygulamasını ana kataloğa bağlar (eşleş veya ekle).
    pub fn sync_store_app_to_catalog(&mut self, store_id: &str) {
        let Some(store_entry) = self.store.get_by_id(store_id).cloned() else {
            return;
        };
        if self.hidden.contains(&store_entry.id) {
            return;
        }

        let already =
            self.catalog.entries().iter().any(|e| {
                crate::store::sync::catalog_matches_store(&e.id, &e.executable, &store_entry)
            });
        if already {
            return;
        }

        let tool = crate::store::sync::tool_entry_from_store(&store_entry);
        let id = tool.id.clone();
        if let Err(e) = self.catalog.insert_entry(tool.clone()) {
            // Kimlik çakışması vb. — sessizce atla; kurulum yine başarılı.
            self.status_message = format!(
                "Uyarı: '{}' kataloğa eklenemedi ({e}); Mağaza’dan çalıştırılabilir.",
                store_entry.name
            );
            return;
        }
        if !self.state.file.custom_tools.iter().any(|t| t.id == id) {
            self.state.file.custom_tools.push(tool);
            let _ = self.persist_state();
        }
    }

    /// sudo parola isteyen apt/dnf planını TUI askıda çalıştırır.
    ///
    /// Ham mod ve alt ekran bırakılır; komut `/dev/tty` ile çalışır; ardından
    /// TUI geri kurulur. Parola önbellekteyse bu yol seçilmez (arka plan).
    pub fn run_pending_tty_pm(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<(), NovaError> {
        let Some(pending) = self.pending_tty_pm.take() else {
            return Ok(());
        };
        let cmd = pending.plan.rendered();
        let name = pending.entry.name.clone();
        let verb = if pending.is_upgrade {
            "güncelleniyor"
        } else {
            "kuruluyor"
        };

        crate::terminal_guard::suspend_tui()?;
        println!();
        println!("Nova — '{name}' {verb}");
        println!("Komut: {cmd}");
        println!("sudo parola sorulursa bu terminalde yanıtlayın.");
        println!();

        let result = crate::store::install::run_package_manager_argv(
            &pending.entry,
            &pending.plan,
            pending.is_upgrade,
            crate::store::install::PmIoMode::Inherited,
        );

        println!();
        match &result {
            Ok(outcome) => println!("✓ {}", outcome.summary),
            Err(err) => println!("Hata: {err}"),
        }
        println!("Devam etmek için Enter…");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);

        crate::terminal_guard::resume_tui()?;
        terminal.clear()?;

        match result {
            Ok(outcome) => self.apply_install_outcome(outcome),
            Err(err) => self.status_message = format!("Hata: {err}"),
        }
        Ok(())
    }

    /// Katalogdaki tüm araçların kurulum durumunu yeniden belirler.
    pub fn refresh_detection(&mut self) {
        for entry in self.catalog.entries_mut().iter_mut() {
            entry.refresh_status();
        }
        self.refresh_store_status();
    }

    /// Mağazadaki tüm uygulamaların kurulum durumunu yeniden belirler.
    ///
    /// Durum her çizimde değil, yalnızca bu çağrıda hesaplanır; liste
    /// önbellekten okur.
    pub fn refresh_store_status(&mut self) {
        self.store_status.clear();
        for entry in self.store.entries() {
            let status = crate::detection::resolve(&entry.executable).0;
            self.store_status.insert(entry.id.clone(), status);
        }
    }

    /// Mağazadaki bir uygulamanın kurulum durumunu döner.
    pub fn store_status(&self, id: &str) -> crate::detection::InstalledStatus {
        self.store_status
            .get(id)
            .copied()
            .unwrap_or(crate::detection::InstalledStatus::Unknown)
    }

    /// Mağazadaki uygulamaların canlı sürümlerini ve PM güncellemelerini denetler.
    ///
    /// Denetim arka planda yürür: GitHub Releases + apt/brew/npm outdated
    /// (pipx/cargo için kuruluysa iyimser plan). Ana döngü kilitlenmez.
    pub fn check_store_updates(&mut self) {
        if self.store_check_job.is_some() {
            self.status_message = "Sürüm denetimi zaten sürüyor.".into();
            return;
        }
        let sources: Vec<(String, ReleaseSource)> = self
            .store
            .entries()
            .iter()
            .filter_map(|e| e.release.as_ref().map(|r| (e.id.clone(), r.clone())))
            .collect();
        let pm_entries: Vec<StoreEntry> = self
            .store
            .entries()
            .iter()
            .filter(|e| e.install_method.is_package_manager())
            .cloned()
            .collect();
        let installed_ids: std::collections::BTreeSet<String> = self
            .store_status
            .iter()
            .filter(|(_, s)| **s == crate::detection::InstalledStatus::Installed)
            .map(|(id, _)| id.clone())
            .collect();
        if sources.is_empty() && pm_entries.is_empty() {
            self.status_message = "Canlı takip veya PM güncellemesi için uygulama yok.".into();
            return;
        }

        let (tx, rx) = mpsc::channel();
        let provider = Arc::clone(&self.store_release_provider);
        let cache_path = self.store_cache_path.clone().or_else(manifest::cache_path);
        let outdated_override = self.store_outdated_override.clone();
        let host_override = self.store_host_tools_override;
        std::thread::spawn(move || {
            let mut results: BTreeMap<String, Result<ResolvedRelease, String>> = BTreeMap::new();
            let mut cache = cache_path
                .as_ref()
                .map(|p| manifest::load_cache(p))
                .unwrap_or_default();
            let now = manifest::now_secs();
            for (id, source) in sources {
                // Taze önbellek kaydı ağa çıkmaz.
                let resolved = match cache.fresh(&id, now) {
                    Some(cached) => Ok(cached.release.clone()),
                    None => {
                        // Süresi dolmuş kayıt **koşullu** sorgulanır: sunucu
                        // değişiklik yoksa 304 döner ve API hız sınırı harcanmaz.
                        let etag = cache.get(&id).and_then(|c| c.etag.clone());
                        match provider.resolve(&source, etag.as_deref()) {
                            Ok(ConditionalRelease::NotModified) => cache
                                .get(&id)
                                .map(|c| c.release.clone())
                                .ok_or_else(|| "önbellek kaydı bulunamadı".to_string()),
                            Ok(ConditionalRelease::Fresh { release, etag }) => {
                                cache.put(&id, release.clone(), now, etag);
                                Ok(release)
                            }
                            Err(e) => Err(e),
                        }
                    }
                };
                results.insert(id, resolved);
            }
            if let Some(path) = cache_path {
                let _ = manifest::save_cache(&path, &cache);
            }

            let host = host_override.unwrap_or_else(host::HostTools::detect);
            let os = host::detect_host_os();
            let outdated =
                outdated_override.unwrap_or_else(|| host::OutdatedPackages::probe(&host));
            let pm_updates =
                host::collect_pm_updates(&pm_entries, &installed_ids, &host, os, &outdated);
            let _ = tx.send(StoreCheckOutcome {
                releases: results,
                pm_updates,
            });
        });

        self.store_check_job = Some(StoreCheckJob { receiver: rx });
        self.status_message = "Mağaza sürümleri ve PM güncellemeleri denetleniyor...".into();
    }

    /// Arka plandaki sürüm denetimini kontrol eder.
    ///
    /// Sonuç geldiğinde [`App::store_updates`] ve [`App::store_pm_updates`]
    /// doldurulur; yeni sürüm / PM güncellemesi sayı ile bildirilir.
    pub fn poll_store_check(&mut self) {
        let Some(job) = self.store_check_job.take() else {
            return;
        };
        match job.receiver.try_recv() {
            Ok(outcome) => {
                let mut updates = BTreeMap::new();
                let mut errors = BTreeMap::new();
                let mut newer = 0usize;
                for (id, result) in outcome.releases {
                    match result {
                        Ok(release) => {
                            let current = self
                                .store
                                .get_by_id(&id)
                                .and_then(|e| e.version.clone())
                                .unwrap_or_default();
                            if manifest::is_newer(&release.version, &current) {
                                newer += 1;
                            }
                            updates.insert(id, release);
                        }
                        Err(message) => {
                            errors.insert(id, message);
                        }
                    }
                }
                let pm_count = outcome.pm_updates.len();
                self.store_updates = updates;
                self.store_pm_updates = outcome.pm_updates;
                self.store_check_errors = errors;
                self.status_message = match (newer, pm_count) {
                    (0, 0) if self.store_check_errors.is_empty() => {
                        "Mağaza uygulamalarının tümü güncel.".into()
                    }
                    (0, 0) => format!(
                        "{} uygulama denetlenemedi; diğerleri güncel.",
                        self.store_check_errors.len()
                    ),
                    (n, 0) => format!("{n} mağaza uygulaması için yeni sürüm bulundu."),
                    (0, p) => format!("{p} paket yöneticisi güncellemesi bulundu."),
                    (n, p) => format!("{n} yeni sürüm, {p} paket yöneticisi güncellemesi bulundu."),
                };
            }
            Err(TryRecvError::Empty) => {
                // Denetim hâlâ sürüyor.
                self.store_check_job = Some(job);
            }
            Err(TryRecvError::Disconnected) => {
                self.status_message = "Hata: sürüm denetimi beklenmedik şekilde sonlandı.".into();
            }
        }
    }

    /// Seçili mağaza uygulaması için bulunan yeni arşiv sürümü.
    pub fn store_update_for(&self, id: &str) -> Option<&ResolvedRelease> {
        self.store_updates.get(id)
    }

    /// `U` sonrası paket yöneticisi güncelleme planı.
    pub fn store_pm_update_for(&self, id: &str) -> Option<&PmUpdatePlan> {
        self.store_pm_updates.get(id)
    }

    /// Arşiv veya PM güncellemesi var mı (↑ işareti).
    pub fn store_has_pending_update(&self, id: &str) -> bool {
        if let Some(release) = self.store_update_for(id) {
            let current = self
                .store
                .get_by_id(id)
                .and_then(|e| e.version.clone())
                .unwrap_or_default();
            if manifest::is_newer(&release.version, &current) {
                return true;
            }
        }
        self.store_pm_updates.contains_key(id)
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
    #[allow(unused_variables)]
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
                theme,
                default_ai_tool,
                default_image_app,
                default_video_app,
                focus,
                error,
            } => match code {
                KeyCode::Esc => {
                    self.status_message = "Ayarlar değiştirilmedi.".into();
                    take(modal::Modal::None);
                }
                KeyCode::Tab | KeyCode::Down => *focus = (*focus + 1) % 6,
                KeyCode::BackTab | KeyCode::Up => *focus = (*focus + 5) % 6,
                KeyCode::Backspace => {
                    match *focus {
                        0 => {
                            preferred_terminal.pop();
                        }
                        1 => {
                            default_working_dir.pop();
                        }
                        2 => {
                            theme.pop();
                        }
                        3 => {
                            default_ai_tool.pop();
                        }
                        4 => {
                            default_image_app.pop();
                        }
                        5 => {
                            default_video_app.pop();
                        }
                        _ => {}
                    }
                    *error = None;
                }
                KeyCode::Enter => match self.save_settings(
                    preferred_terminal,
                    default_working_dir,
                    theme,
                    default_ai_tool,
                    default_image_app,
                    default_video_app,
                ) {
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
                    match *focus {
                        0 => preferred_terminal.push(c),
                        1 => default_working_dir.push(c),
                        2 => theme.push(c),
                        3 => default_ai_tool.push(c),
                        4 => default_image_app.push(c),
                        5 => default_video_app.push(c),
                        _ => {}
                    }
                    *error = None;
                }
                _ => {}
            },
            modal::Modal::UpdateCheck {
                status,
                check,
                error,
            } => match code {
                KeyCode::Esc => take(modal::Modal::None),
                KeyCode::Enter => {
                    if let Some(UpdateCheck::UpdateAvailable {
                        current,
                        latest,
                        release,
                    }) = check.clone()
                    {
                        take(modal::Modal::UpdateConfirm {
                            current,
                            latest,
                            release: *release,
                        });
                    }
                }
                KeyCode::Char('u') | KeyCode::Char('U') => {
                    *status = modal::UpdateStatus::Checking;
                    *check = None;
                    *error = None;
                    let provider = self.update_provider.as_ref();
                    let current = provider.current_version();
                    match provider.fetch_latest_release() {
                        Ok(Some(release)) => {
                            let latest = release.version.clone();
                            if release.version > current {
                                *check = Some(UpdateCheck::UpdateAvailable {
                                    current,
                                    latest,
                                    release: Box::new(release),
                                });
                            } else {
                                *check = Some(UpdateCheck::UpToDate { current });
                            }
                            *status = modal::UpdateStatus::Done;
                        }
                        Ok(None) => {
                            *check = Some(UpdateCheck::UpToDate { current });
                            *status = modal::UpdateStatus::Done;
                        }
                        Err(e) => {
                            *error = Some(e);
                            *status = modal::UpdateStatus::Done;
                        }
                    }
                }
                _ => {}
            },
            modal::Modal::UpdateConfirm {
                current,
                latest,
                release,
            } => match code {
                KeyCode::Esc => take(modal::Modal::None),
                KeyCode::Enter => {
                    take(modal::Modal::Confirm {
                        question: format!(
                            "Nova v{} → v{} güncellensin mi? Enter onaylar, Esc vazgeçer.",
                            current, latest
                        ),
                        action: modal::ConfirmAction::InstallUpdate {
                            version: latest.to_string(),
                            release: Box::new(release.clone()),
                        },
                    });
                }
                _ => {}
            },
            modal::Modal::FilePicker { state } => match code {
                KeyCode::Esc => take(modal::Modal::None),
                KeyCode::Char('j') | KeyCode::Down => state.move_down(),
                KeyCode::Char('k') | KeyCode::Up => state.move_up(),
                KeyCode::Char('h') | KeyCode::Left => state.navigate_up(),
                KeyCode::Char('l') | KeyCode::Right => state.enter_dir(),
                KeyCode::Enter => {
                    if let Some(path) = state.confirm_selection() {
                        match self.media_launcher.launch_media(&path) {
                            Ok(pid) => {
                                self.status_message = format!(
                                    "'{}' PID {} ile açılıyor.",
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                    pid
                                );
                                take(modal::Modal::None);
                            }
                            Err(e) => {
                                self.status_message = format!("Hata: {e}");
                            }
                        }
                    }
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
    #[allow(clippy::too_many_arguments)]
    fn save_settings(
        &mut self,
        preferred_terminal: &str,
        default_working_dir: &str,
        theme: &str,
        default_ai_tool: &str,
        default_image_app: &str,
        default_video_app: &str,
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

        // Varsayılan AI aracı: boş (seçim yok) veya kurulu bir AI kategorisindeki
        // araç olmalıdır.
        let default_ai_tool = if default_ai_tool.trim().is_empty() {
            None
        } else {
            let id = default_ai_tool.trim().to_string();
            let entry = self
                .catalog
                .get_by_id(&id)
                .ok_or_else(|| format!("'{id}' katalogda bulunamadı."))?;
            if entry.category != "AI ve Kodlama" {
                return Err(format!(
                    "'{id}' AI kategorisinde değil; yalnızca AI araçları varsayılan AI seçilebilir."
                ));
            }
            use crate::detection::InstalledStatus;
            if entry.status != InstalledStatus::Installed {
                return Err(format!(
                    "'{id}' kurulu değil; varsayılan AI olarak ayarlanamadı."
                ));
            }
            Some(id)
        };

        // Resim/video varsayılanları: boş (seçim yok) veya katalogda bulunan bir uygulama.
        let validate_app = |id: &str, kind: &str| -> Result<Option<String>, String> {
            if id.trim().is_empty() {
                return Ok(None);
            }
            let id = id.trim().to_string();
            let entry = self
                .catalog
                .get_by_id(&id)
                .ok_or_else(|| format!("'{id}' katalogda bulunamadı."))?;
            use crate::detection::InstalledStatus;
            if entry.status != InstalledStatus::Installed {
                return Err(format!(
                    "'{id}' kurulu değil; varsayılan {kind} uygulaması olarak ayarlanamadı."
                ));
            }
            Ok(Some(id))
        };
        let default_image_app = validate_app(default_image_app, "resim")?;
        let default_video_app = validate_app(default_video_app, "video")?;

        let old = self.state.file.prefs.clone();
        self.state.file.prefs.preferred_terminal = preferred_terminal;
        self.state.file.prefs.default_working_dir = default_working_dir;
        self.state.file.prefs.theme = Some(theme.trim().to_string());
        self.state.file.prefs.default_ai_tool = default_ai_tool;
        self.state.file.prefs.default_image_app = default_image_app;
        self.state.file.prefs.default_video_app = default_video_app;
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
            modal::ConfirmAction::InstallStoreApp { id, name } => {
                // Kurulumu başlat: indirme, SHA-256 doğrulama ve çıkarma
                // arka planda yürür, ana döngü kilitlenmez.
                self.status_message = format!("'{}' kurulumu başlatılıyor...", name);
                self.start_store_install(&id);
            }
            modal::ConfirmAction::UpdateStoreApp { id, name, version } => {
                self.status_message = format!("'{}' {} sürümüne güncelleniyor...", name, version);
                self.start_store_update(&id);
            }
            modal::ConfirmAction::InstallUpdate { version, release } => {
                match (
                    release.download_url.as_deref(),
                    release.asset_name.as_deref(),
                ) {
                    (Some(url), Some(asset)) => {
                        match crate::update::install_nova_from_url(&release.version, url, asset) {
                            Ok(outcome) => {
                                crate::store::path::ensure_local_bin_on_process_path();
                                self.status_message = format!("✓ {}", outcome.summary);
                            }
                            Err(e) => {
                                self.status_message =
                                    format!("Hata: Nova v{version} kurulamadı: {e}");
                            }
                        }
                    }
                    _ => {
                        self.status_message = format!(
                            "Hata: Nova v{version} yayında indirilebilir Linux ikilisi yok. \
                             Kaynak koddan derleyin veya release varlığı ekleyin."
                        );
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
    use crate::detection::InstalledStatus;

    /// Test için kategori adını `CATEGORIES` indeksine çevirir.
    fn category_index(name: &str) -> usize {
        CATEGORIES
            .iter()
            .position(|c| *c == name)
            .unwrap_or_else(|| panic!("bilinmeyen kategori: {name}"))
    }

    /// Ana listede tüm araçları "kurulu" gösterir (filtre testleri için).
    fn mark_all_installed(app: &mut App) {
        for entry in app.catalog.entries_mut() {
            entry.status = InstalledStatus::Installed;
        }
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
        mark_all_installed(&mut app);
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
        mark_all_installed(&mut app);
        let visible = app.visible_category_indices();
        assert!(visible.len() >= 2, "en az iki görünür kategori");
        app.category_index = visible[0];
        app.move_down();
        assert_eq!(app.category_index, visible[1]);
        for _ in 0..CATEGORIES.len() + 5 {
            app.move_down();
        }
        assert!(app.settings_tab, "son satır Ayarlar olmalı");
        for _ in 0..CATEGORIES.len() + 5 {
            app.move_up();
        }
        assert!(!app.store_tab && !app.settings_tab);
        assert_eq!(app.category_index, visible[0]);
    }

    #[test]
    fn filtered_tools_empty_when_no_match() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        // Seçili kategori "Sistem ve Kaynak Kullanımı", içinde araçlar var.
        assert!(!app.filtered_tools().is_empty());
    }

    #[test]
    fn filtered_tools_hides_uninstalled() {
        let mut app = sample_app();
        for entry in app.catalog.entries_mut() {
            entry.status = InstalledStatus::Missing;
        }
        assert!(
            app.filtered_tools().is_empty(),
            "kurulu olmayan araçlar ana listede görünmemeli"
        );
        // Mağaza hâlâ tüm kayıtları gösterir.
        assert!(!app.filtered_store_apps().is_empty());
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
        // S artık Ayarlar sekmesine gider; gelişmiş modal ayrı açılır.
        let mut app = sample_app();
        app.handle_key(typed('S')).unwrap();
        assert!(app.settings_tab);
        assert!(!app.modal.is_open());
        app.open_advanced_settings_modal();
        assert!(matches!(app.modal, modal::Modal::Settings { .. }));
        for c in ['/', 't', 'm', 'p', 'Z'] {
            app.handle_key(typed(c)).unwrap();
            assert!(
                matches!(app.modal, modal::Modal::Settings { .. }),
                "gelişmiş ayarlar '{c}' ile kapandı"
            );
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

    /// Fare sol-tuş tıklaması üretir.
    fn mouse_down(x: u16, y: u16) -> crossterm::event::MouseEvent {
        crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: x,
            row: y,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }
    }

    /// Fare tekerleği aşağı.
    fn mouse_scroll_down() -> crossterm::event::MouseEvent {
        crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }
    }

    #[test]
    fn mouse_click_selects_category() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.terminal_size = Some((100, 30));
        app.focus = PanelFocus::Tool;
        // Kategori paneli: x < 26; içerik satırı 2'den başlar, satır 1 → y = 3.
        app.handle_mouse(mouse_down(5, 3)).unwrap();
        assert_eq!(app.focus, PanelFocus::Category);
        let visible = app.visible_category_indices();
        assert_eq!(app.category_index, visible[1]);
        assert_eq!(app.tool_index, 0);
    }

    #[test]
    fn mouse_click_selects_tool() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.terminal_size = Some((100, 30));
        app.focus = PanelFocus::Category;
        app.category_index = category_index("Sistem ve Kaynak Kullanımı");
        // Araç paneli: x >= 26; ilk araç satırı y = 2.
        app.handle_mouse(mouse_down(40, 2)).unwrap();
        assert_eq!(app.focus, PanelFocus::Tool);
        assert!(!app.filtered_tools().is_empty());
        assert_eq!(app.tool_index, 0);
    }

    #[test]
    fn mouse_scroll_changes_tool_selection() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.terminal_size = Some((100, 30));
        app.focus = PanelFocus::Tool;
        app.category_index = category_index("Sistem ve Kaynak Kullanımı");
        let before = app.tool_index;
        app.handle_mouse(mouse_scroll_down()).unwrap();
        assert!(app.tool_index > before);
    }

    #[test]
    fn mouse_click_in_narrow_mode_selects_tool_only() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.terminal_size = Some((40, 30));
        app.focus = PanelFocus::Category;
        app.category_index = app.visible_category_indices()[0];
        // Dar modda kategori paneli yok; ekranın sağ tarafındaki tıklama
        // yine aracı seçmelidir.
        app.handle_mouse(mouse_down(39, 2)).unwrap();
        assert_eq!(app.focus, PanelFocus::Tool);
    }

    #[test]
    fn resize_to_narrow_moves_focus_to_tool() {
        let mut app = sample_app();
        app.focus = PanelFocus::Category;
        app.apply_resize(40, 30);
        assert!(app.narrow_mode);
        assert_eq!(app.focus, PanelFocus::Tool);
        assert_eq!(app.terminal_size, Some((40, 30)));
    }

    #[test]
    fn resize_to_wide_keeps_category_focus() {
        let mut app = sample_app();
        app.focus = PanelFocus::Category;
        app.apply_resize(100, 30);
        assert!(!app.narrow_mode);
        assert_eq!(app.focus, PanelFocus::Category);
        assert_eq!(app.terminal_size, Some((100, 30)));
    }

    #[test]
    fn keyboard_navigation_still_works_in_narrow_terminal() {
        let mut app = sample_app();
        app.apply_resize(40, 30);
        assert_eq!(app.focus, PanelFocus::Tool);
        // `h`/`←` dar modda odak değiştirmez.
        app.handle_key(key(crossterm::event::KeyCode::Left))
            .unwrap();
        assert_eq!(app.focus, PanelFocus::Tool);
        assert!(app.status_message.contains("gizli"));
        // `j`/`↓` arak listesinde gezinir.
        app.handle_key(key(crossterm::event::KeyCode::Down))
            .unwrap();
        assert_eq!(app.focus, PanelFocus::Tool);
    }

    #[test]
    fn store_tab_enter_on_package_manager_entry_asks_for_confirmation() {
        let mut app = sample_app();
        app.handle_key(typed('m')).unwrap();
        assert!(app.store_tab, "mağaza sekmesi seçili olmalı");
        assert!(matches!(app.modal, modal::Modal::None));
        // İlk AI kaydı (Aider) pipx; Enter onay ister, hemen kurmaz.
        app.handle_key(key(crossterm::event::KeyCode::Enter))
            .unwrap();
        match &app.modal {
            modal::Modal::Confirm {
                question,
                action: modal::ConfirmAction::InstallStoreApp { id, .. },
            } => {
                assert_eq!(id, "aider");
                assert!(
                    question.contains("pipx") && question.contains("aider-chat"),
                    "soru: {question}"
                );
            }
            other => panic!("onay ekranı bekleniyordu: {other:?}"),
        }
        assert!(
            app.install_job.is_none(),
            "onaydan önce kurulum başlamamalı"
        );
    }

    #[test]
    fn store_tab_enter_on_manual_entry_shows_instructions_without_installing() {
        let mut app = sample_app();
        app.handle_key(typed('m')).unwrap();
        // AI kategorisinde hâlâ manual olan bir kayıt seç (goose).
        let apps = app.filtered_store_apps();
        let goose_idx = apps
            .iter()
            .position(|e| e.id == "goose")
            .expect("goose manual kalmalı");
        app.store_app_index = goose_idx;
        app.handle_key(key(crossterm::event::KeyCode::Enter))
            .unwrap();
        assert_eq!(app.modal, modal::Modal::None, "onay ekranı açılmamalı");
        assert!(
            app.status_message.contains("Goose")
                || app.status_message.to_lowercase().contains("kurulum")
                || app.status_message.contains("goose"),
            "mesaj: {}",
            app.status_message
        );
        assert!(app.install_job.is_none(), "kurulum başlatılmamalı");
    }

    #[test]
    fn store_tab_enter_on_installable_entry_asks_for_confirmation() {
        let mut app = sample_app();
        app.handle_key(typed('m')).unwrap();
        // Mağaza kategori paneline geç: "Dosya ve Arama"
        // (AI → Git → Dosya ve Arama).
        app.handle_key(key(crossterm::event::KeyCode::Left))
            .unwrap();
        assert_eq!(app.focus, PanelFocus::StoreCategory);
        app.handle_key(typed('j')).unwrap();
        app.handle_key(typed('j')).unwrap();
        assert_eq!(app.selected_store_category(), "Dosya ve Arama");
        // Uygulama listesine geç ve ilk kaydı (ripgrep) kur.
        app.handle_key(typed('l')).unwrap();
        assert_eq!(app.focus, PanelFocus::StoreApp);
        app.handle_key(key(crossterm::event::KeyCode::Enter))
            .unwrap();
        match &app.modal {
            modal::Modal::Confirm {
                question,
                action: modal::ConfirmAction::InstallStoreApp { id, .. },
            } => {
                assert!(question.contains("SHA-256"), "soru: {question}");
                assert_eq!(id, "ripgrep");
            }
            other => panic!("onay ekranı bekleniyordu: {other:?}"),
        }
    }

    #[test]
    fn store_tab_is_followed_by_settings_in_the_category_list() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.focus = PanelFocus::Category;
        let visible = app.visible_category_indices();
        app.category_index = *visible.last().expect("görünür kategori");
        app.handle_key(typed('j')).unwrap();
        assert!(app.store_tab);
        assert_eq!(app.selected_tab_index(), visible.len());
        app.handle_key(typed('j')).unwrap();
        assert!(app.settings_tab);
        assert_eq!(app.selected_tab_index(), visible.len() + 1);
        app.handle_key(typed('k')).unwrap();
        assert!(app.store_tab);
        assert!(!app.settings_tab);
    }

    #[test]
    fn store_tab_focus_chain_moves_between_three_panels() {
        let mut app = sample_app();
        app.handle_key(typed('m')).unwrap();
        // `m` uygulama listesine odaklanır.
        assert_eq!(app.focus, PanelFocus::StoreApp);
        // Sola: mağaza kategorileri, sonra sekme listesi.
        app.handle_key(typed('h')).unwrap();
        assert_eq!(app.focus, PanelFocus::StoreCategory);
        app.handle_key(typed('h')).unwrap();
        assert_eq!(app.focus, PanelFocus::Category);
        // Sağa: tekrar mağaza kategorileri ve uygulamalar.
        app.handle_key(typed('l')).unwrap();
        assert_eq!(app.focus, PanelFocus::StoreCategory);
        app.handle_key(typed('l')).unwrap();
        assert_eq!(app.focus, PanelFocus::StoreApp);
        // En sağdaki panelde `l` odakı daha ileri taşımaz.
        app.handle_key(typed('l')).unwrap();
        assert_eq!(app.focus, PanelFocus::StoreApp);
    }

    #[test]
    fn store_tab_search_filters_store_apps() {
        let mut app = sample_app();
        app.handle_key(typed('m')).unwrap();
        app.handle_key(typed('/')).unwrap();
        assert!(app.search_active);
        for c in "fzf".chars() {
            app.handle_key(typed(c)).unwrap();
        }
        app.handle_key(key(crossterm::event::KeyCode::Enter))
            .unwrap();
        let apps = app.filtered_store_apps();
        assert!(!apps.is_empty());
        assert!(apps.iter().any(|e| e.id == "fzf"), "sonuçlar: {apps:?}");
        assert!(
            app.status_message.contains("sonuç"),
            "mesaj: {}",
            app.status_message
        );
    }

    #[test]
    fn store_tab_leaving_returns_to_tool_categories() {
        let mut app = sample_app();
        app.handle_key(typed('m')).unwrap();
        assert!(app.store_tab);
        // Sekme listesine dön ve yukarı hareketle mağazadan çık.
        app.handle_key(typed('h')).unwrap();
        app.handle_key(typed('h')).unwrap();
        assert_eq!(app.focus, PanelFocus::Category);
        app.handle_key(typed('k')).unwrap();
        assert!(!app.store_tab);
        // Araç listesi yeniden çizilir; odak zinciri iki panele döner.
        app.handle_key(typed('l')).unwrap();
        assert_eq!(app.focus, PanelFocus::Tool);
    }

    #[test]
    fn store_tab_u_key_starts_live_version_check() {
        let mut app = sample_app();
        app.store_release_provider = Arc::new(FakeProvider::succeeding("9.9.9"));
        app.handle_key(typed('m')).unwrap();
        app.handle_key(typed('U')).unwrap();
        assert!(app.store_check_job.is_some(), "canlı denetim başlamalı");
        // Nova'nın kendi güncelleme ekranı açılmamalı.
        assert!(matches!(app.modal, modal::Modal::None));
    }

    #[test]
    fn tool_tab_u_key_still_opens_nova_update_check() {
        let mut app = sample_app();
        app.handle_key(typed('u')).unwrap();
        assert!(matches!(app.modal, modal::Modal::UpdateCheck { .. }));
    }

    #[test]
    fn start_store_install_refuses_manual_entries() {
        let mut app = sample_app();
        // curl|bash kayıtları bilerek `manual` kalır (ör. plandex).
        app.start_store_install("plandex");
        assert!(app.install_job.is_none());
        assert!(
            app.status_message.contains("Nova kurulum yapmıyor"),
            "mesaj: {}",
            app.status_message
        );
    }

    #[test]
    fn start_store_install_reports_unknown_ids() {
        let mut app = sample_app();
        app.start_store_install("boyle-bir-uygulama-yok");
        assert!(app.install_job.is_none());
        assert!(
            app.status_message.contains("bulunamadı"),
            "mesaj: {}",
            app.status_message
        );
    }

    /// Canlı sürüm denetimi için sahte sağlayıcı.
    struct FakeProvider {
        releases: std::sync::Mutex<BTreeMap<String, Result<ResolvedRelease, String>>>,
        calls: std::sync::atomic::AtomicUsize,
        fallback: Option<Result<ResolvedRelease, String>>,
    }

    impl FakeProvider {
        fn new(releases: Vec<(String, Result<ResolvedRelease, String>)>) -> Self {
            Self {
                releases: std::sync::Mutex::new(releases.into_iter().collect()),
                calls: std::sync::atomic::AtomicUsize::new(0),
                fallback: None,
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(std::sync::atomic::Ordering::Relaxed)
        }

        /// Her depo için aynı başarılı sonucu döndüren sahte sağlayıcı.
        fn succeeding(version: &str) -> Self {
            let release = resolved(version);
            Self {
                releases: std::sync::Mutex::new(BTreeMap::new()),
                calls: std::sync::atomic::AtomicUsize::new(0),
                fallback: Some(Ok(release)),
            }
        }
    }

    impl ReleaseProvider for FakeProvider {
        fn resolve(
            &self,
            source: &crate::store::manifest::ReleaseSource,
            _etag: Option<&str>,
        ) -> Result<crate::store::manifest::ConditionalRelease, String> {
            self.calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // Depo adını kimlik olarak kullanırız: testler tek uygulama için
            // çalışır.
            let key = source.repo.clone();
            let result = match self.releases.lock().unwrap().get(&key) {
                Some(Ok(release)) => Ok(release.clone()),
                Some(Err(e)) => Err(e.clone()),
                None => match &self.fallback {
                    Some(Ok(release)) => Ok(release.clone()),
                    Some(Err(e)) => Err(e.clone()),
                    None => Err("bilinmeyen depo".into()),
                },
            };
            result.map(
                |release| crate::store::manifest::ConditionalRelease::Fresh {
                    release,
                    etag: None,
                },
            )
        }
    }

    fn resolved(version: &str) -> ResolvedRelease {
        ResolvedRelease {
            version: version.into(),
            download_url: format!("https://example.org/{version}.tar.gz"),
            checksum: "c".repeat(64),
            checksum_url: format!("https://example.org/{version}.sha256"),
        }
    }

    /// Arka plandaki denetimin bitmesini bekler.
    fn wait_for_store_check(app: &mut App) {
        for _ in 0..400 {
            app.poll_store_check();
            if app.store_check_job.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Testlere özel, çakışmayan önbellek dosyası.
    fn test_cache_path(tag: &str) -> PathBuf {
        let dir = crate::store::test_store_dir();
        let path = dir.join(format!("manifest-cache-{tag}.toml"));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn quiet_pm_probe(app: &mut App) {
        app.store_outdated_override = Some(host::OutdatedPackages::empty());
    }

    #[test]
    fn store_update_check_finds_newer_versions() {
        let cache_path = test_cache_path("yeni");
        let mut app = sample_app();
        quiet_pm_probe(&mut app);
        app.store_cache_path = Some(cache_path.clone());
        app.store_release_provider = Arc::new(FakeProvider::new(vec![(
            "BurntSushi/ripgrep".into(),
            Ok(resolved("99.0.0")),
        )]));

        app.check_store_updates();
        wait_for_store_check(&mut app);

        let release = app.store_update_for("ripgrep").expect("çözüm bulunmalı");
        assert_eq!(release.version, "99.0.0");
        assert!(
            app.status_message.contains("yeni sürüm"),
            "mesaj: {}",
            app.status_message
        );
        // Diğer depolar sahte sağlayıcıda çözülemez; hata olarak raporlanır.
        assert!(
            app.store_check_errors.contains_key("fzf"),
            "çözülemeyen depolar hata olarak görünmeli"
        );
        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn store_update_check_reports_unreachable_repos() {
        let cache_path = test_cache_path("hata");
        let mut app = sample_app();
        quiet_pm_probe(&mut app);
        app.store_cache_path = Some(cache_path.clone());
        app.store_release_provider = Arc::new(FakeProvider::new(vec![(
            "BurntSushi/ripgrep".into(),
            Err("ağ hatası".into()),
        )]));

        app.check_store_updates();
        wait_for_store_check(&mut app);

        assert!(app.store_update_for("ripgrep").is_none());
        assert!(
            app.store_check_errors.contains_key("ripgrep"),
            "hata kaydedilmeli"
        );
        assert!(
            app.status_message.contains("denetlenemedi"),
            "mesaj: {}",
            app.status_message
        );
        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn store_update_check_uses_the_cache_without_network() {
        let cache_path = test_cache_path("onbellek");
        let mut app = sample_app();
        quiet_pm_probe(&mut app);
        let provider = Arc::new(FakeProvider::succeeding("99.0.0"));
        app.store_release_provider = provider.clone();
        app.store_cache_path = Some(cache_path.clone());

        app.check_store_updates();
        wait_for_store_check(&mut app);
        let first_calls = provider.calls();
        assert!(first_calls > 0, "ilk denetim ağa çıkmalı");

        // İkinci denetim taze önbellekten okumalı; sağlayıcıya gitmemeli.
        let mut app2 = sample_app();
        quiet_pm_probe(&mut app2);
        app2.store_release_provider = provider.clone();
        app2.store_cache_path = Some(cache_path.clone());
        app2.check_store_updates();
        wait_for_store_check(&mut app2);
        assert_eq!(provider.calls(), first_calls, "önbellek kullanılmalıydı");
        assert_eq!(
            app2.store_update_for("ripgrep").map(|r| r.version.clone()),
            Some("99.0.0".to_string())
        );

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn store_update_without_a_resolved_version_is_refused() {
        let mut app = sample_app();
        app.start_store_update("ripgrep");
        assert!(app.install_job.is_none());
        assert!(
            app.status_message.contains("denetle"),
            "mesaj: {}",
            app.status_message
        );
    }

    /// Koşullu istekte alınan `ETag`'i kaydeden sahte sağlayıcı.
    struct EtagProvider {
        seen: std::sync::Mutex<Vec<Option<String>>>,
    }

    impl EtagProvider {
        fn new() -> Self {
            Self {
                seen: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl ReleaseProvider for EtagProvider {
        fn resolve(
            &self,
            _source: &crate::store::manifest::ReleaseSource,
            etag: Option<&str>,
        ) -> Result<crate::store::manifest::ConditionalRelease, String> {
            self.seen.lock().unwrap().push(etag.map(str::to_string));
            Ok(crate::store::manifest::ConditionalRelease::Fresh {
                release: resolved("1.0.0"),
                etag: Some("W/\"v1\"".into()),
            })
        }
    }

    #[test]
    fn stale_cache_is_revalidated_with_the_etag() {
        let cache_path = test_cache_path("etag");
        let mut app = sample_app();
        quiet_pm_probe(&mut app);
        let provider = Arc::new(EtagProvider::new());
        app.store_release_provider = provider.clone();
        app.store_cache_path = Some(cache_path.clone());

        // İlk denetim: önbellek boş, koşullu istek yapılmaz.
        app.check_store_updates();
        wait_for_store_check(&mut app);
        assert_eq!(
            provider.seen.lock().unwrap().first().cloned().flatten(),
            None,
            "ilk denetimde ETag yoktur"
        );

        // Önbelleği bayatlat: `checked_at` eskiye çekilir.
        let mut cache = manifest::load_cache(&cache_path);
        for entry in cache.entries.values_mut() {
            entry.checked_at = 0;
        }
        manifest::save_cache(&cache_path, &cache).unwrap();

        // İkinci denetim: bayat önbellek, koşullu istek atılmalı.
        let mut app2 = sample_app();
        quiet_pm_probe(&mut app2);
        app2.store_release_provider = provider.clone();
        app2.store_cache_path = Some(cache_path.clone());
        app2.check_store_updates();
        wait_for_store_check(&mut app2);

        let seen = provider.seen.lock().unwrap().clone();
        // Her denetim izlenen 6 uygulama için bir çağrı yapar.
        let tracked = 6usize;
        assert!(
            seen.len() >= tracked * 2,
            "iki denetim de çağrı yapmalı: {}",
            seen.len()
        );
        // İlk denetim koşullu değildir (önbellek boş).
        assert!(
            seen[..tracked].iter().all(|e| e.is_none()),
            "ilk denetimde ETag gönderilmemeli"
        );
        // İkinci denetim bayat önbelleği ETag ile doğrular.
        assert!(
            seen[tracked..]
                .iter()
                .all(|e| e.as_deref() == Some("W/\"v1\"")),
            "bayat önbellek ETag ile doğrulanmalı: {:?}",
            &seen[tracked..]
        );

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn store_u_discovers_package_manager_updates() {
        let cache_path = test_cache_path("pm-u");
        let mut app = sample_app();
        app.store_cache_path = Some(cache_path.clone());
        // GitHub tarafını sessizleştir; yalnızca PM yolunu ölç.
        app.store_release_provider = Arc::new(FakeProvider::new(vec![]));
        app.store_host_tools_override = Some(host::HostTools::from_flags(
            true, false, false, false, false,
        ));
        app.store_status
            .insert("htop".into(), crate::detection::InstalledStatus::Installed);
        let mut outdated = host::OutdatedPackages::empty();
        outdated.apt.insert("htop".into());
        app.store_outdated_override = Some(outdated);

        app.check_store_updates();
        wait_for_store_check(&mut app);

        let plan = app
            .store_pm_update_for("htop")
            .expect("htop PM güncellemesi");
        assert_eq!(plan.method, crate::store::InstallMethod::Apt);
        assert!(
            plan.argv.iter().any(|a| a == "--only-upgrade"),
            "{:?}",
            plan.argv
        );
        assert!(app.store_has_pending_update("htop"));
        assert!(
            app.status_message.contains("paket yöneticisi"),
            "mesaj: {}",
            app.status_message
        );
        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn store_enter_confirms_pm_update_after_u() {
        let mut app = sample_app();
        quiet_pm_probe(&mut app);
        app.select_store_tab();
        // htop kategorisine git.
        if let Some(idx) = STORE_CATEGORIES.iter().position(|c| *c == "Sistem") {
            app.store_category_index = idx;
        }
        let apps = app.filtered_store_apps();
        let htop_pos = apps.iter().position(|e| e.id == "htop").expect("htop");
        app.store_app_index = htop_pos;
        app.store_pm_updates.insert(
            "htop".into(),
            host::PmUpdatePlan {
                method: crate::store::InstallMethod::Apt,
                argv: vec![
                    "sudo".into(),
                    "apt".into(),
                    "install".into(),
                    "--only-upgrade".into(),
                    "-y".into(),
                    "htop".into(),
                ],
                hint: "htop".into(),
            },
        );
        app.select_store_app();
        match &app.modal {
            modal::Modal::Confirm { question, action } => {
                assert!(
                    question.contains("güncellensin") && question.contains("apt"),
                    "{question}"
                );
                assert!(matches!(
                    action,
                    modal::ConfirmAction::UpdateStoreApp { id, .. } if id == "htop"
                ));
            }
            other => panic!("PM güncelleme onayı beklenirdi: {other:?}"),
        }
    }

    #[test]
    fn host_aware_install_confirm_uses_available_pm() {
        // Katalog birincili brew olan crush; Linux + yalnızca cargo notes’ta
        // yok → brew kalır. gitui: apt birincil, notes’ta cargo var.
        // Bu test yalnızca resolve_install_plan birimini app confirm metninde
        // yansıtır: pipx-only host’ta aider pipx gösterir.
        let mut app = sample_app();
        app.select_store_tab();
        if let Some(idx) = STORE_CATEGORIES.iter().position(|c| *c == "AI") {
            app.store_category_index = idx;
        }
        let apps = app.filtered_store_apps();
        let pos = apps.iter().position(|e| e.id == "aider").expect("aider");
        app.store_app_index = pos;
        app.select_store_app();
        match &app.modal {
            modal::Modal::Confirm { question, .. } => {
                assert!(
                    question.contains("pipx") && question.contains("aider-chat"),
                    "{question}"
                );
            }
            other => panic!("kurulum onayı beklenirdi: {other:?}"),
        }
    }

    #[test]
    fn store_install_syncs_gemini_into_ai_catalog_category() {
        // gemini-cli mağazada AI’da; ana katalogda yok. Kurulum sonrası
        // "AI ve Kodlama" altında görünmeli ve başlatılabilir olmalı.
        let mut app = sample_app_with_category("AI ve Kodlama");
        assert!(
            app.catalog.get_by_id("gemini-cli").is_none(),
            "önkoşul: gemini-cli gömülü katalogda olmamalı"
        );
        let outcome = crate::store::install::InstallOutcome {
            app_id: "gemini-cli".into(),
            app_name: "Gemini CLI".into(),
            version: "0.0.0".into(),
            executable_path: PathBuf::from("/tmp/fake-gemini"),
            install_dir: PathBuf::from("/tmp"),
            summary: "test kurulum".into(),
        };
        app.apply_install_outcome(outcome);
        let entry = app
            .catalog
            .get_by_id("gemini-cli")
            .expect("kurulum sonrası katalogda olmalı");
        assert_eq!(entry.category, "AI ve Kodlama");
        assert_eq!(entry.executable, "gemini");
        assert!(
            app.state
                .file
                .custom_tools
                .iter()
                .any(|t| t.id == "gemini-cli"),
            "state.toml custom_tools’a yazılmalı"
        );
        // Kurulum sonrası PATH’te bulunursa ana listede görünür.
        if let Some(entry) = app
            .catalog
            .entries_mut()
            .iter_mut()
            .find(|e| e.id == "gemini-cli")
        {
            entry.status = InstalledStatus::Installed;
        }
        let tools = app.filtered_tools();
        assert!(
            tools.iter().any(|t| t.id == "gemini-cli"),
            "AI kategorisi filtresinde görünmeli"
        );
    }

    #[test]
    fn store_sync_skips_when_catalog_already_has_matching_executable() {
        // opencode hem store hem catalog’da; ikinci ekleme olmamalı.
        let mut app = sample_app();
        let before = app.catalog.entries().len();
        app.store_status.insert(
            "opencode".into(),
            crate::detection::InstalledStatus::Installed,
        );
        app.sync_store_app_to_catalog("opencode");
        assert_eq!(app.catalog.entries().len(), before);
        assert!(!app
            .state
            .file
            .custom_tools
            .iter()
            .any(|t| t.id == "opencode"));
    }

    #[test]
    fn apply_install_outcome_is_idempotent_for_catalog_sync() {
        let mut app = sample_app();
        let outcome = crate::store::install::InstallOutcome {
            app_id: "qwen-code".into(),
            app_name: "Qwen Code".into(),
            version: String::new(),
            executable_path: PathBuf::from("/tmp/qwen"),
            install_dir: PathBuf::from("/tmp"),
            summary: "ok".into(),
        };
        app.apply_install_outcome(outcome.clone());
        app.apply_install_outcome(outcome);
        assert_eq!(
            app.catalog
                .entries()
                .iter()
                .filter(|e| e.id == "qwen-code")
                .count(),
            1
        );
    }
}
