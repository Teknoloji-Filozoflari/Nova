//! Uygulama durumu ve ana döngü modülü.
//! `App` yapısı uygulamanın tüm durumunu tutar ve `run` metodu ile
//! terminal arayüzünün ana döngüsünü sürdürür.

use crate::catalog::{Catalog, CATEGORIES};
use crate::commands::{
    self, bank_categories, CommandBank, CommandsBrowseMode, CommandsSource, LocalCommandsSnapshot,
    SearchIndex,
};
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
    /// Kurulu araç detay paneli (yan veya alt şerit).
    ToolDetail,
    /// AI araç eylemleri listesi (alt dikey panel) odaklı.
    ToolActions,
    /// Mağaza kategorileri paneli odaklı.
    StoreCategory,
    /// Mağaza uygulamaları paneli odaklı.
    StoreApp,
    /// Mağaza detay paneli odaklı (yan veya alt; Enter → tarayıcı).
    StoreDetail,
    /// Komut Rehberi: gezinme modu (Başlangıç…Sistemde bulunanlar).
    CommandsSource,
    /// Komut Rehberi: grup sütunu (görev kat. / profil / teknik kat.).
    CommandsCategory,
    /// Komut Rehberi: görev / komut / yerel PATH listesi.
    CommandsList,
    /// Komut Rehberi detay paneli (yan veya alt şerit).
    CommandsDetail,
    /// Ayarlar → Tema seçici paneli odaklı.
    ThemePicker,
    /// Ayarlar → Dil seçici paneli odaklı.
    LanguagePicker,
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

/// Arka plan iş sonucu (mağaza veya Nova self-update).
#[derive(Debug)]
pub enum JobResult {
    /// Mağaza kurulum / güncelleme / kaldırma.
    Store(Result<crate::store::install::InstallOutcome, String>),
    /// Nova Releases veya `cargo install --git`.
    Nova(Result<crate::update::UpdateOutcome, String>),
}

/// Arka planda süren kurulum / güncelleme işi.
///
/// Ağ ve derleme ana döngüyü kilitlememek için ayrı iş parçacığında yürür.
/// Canlı satırlar [`crate::progress::ProgressEvent`] ile TUI modalına akar;
/// sonuç [`App::poll_install`] ile alınır.
#[derive(Debug)]
pub struct InstallJob {
    /// İş türü (ilerleme modalı / sonuç yolu).
    pub kind: crate::progress::JobKind,
    /// Kurulan uygulamanın kimliği (Nova için `"nova"`).
    pub app_id: String,
    /// Kurulan uygulamanın adı.
    pub app_name: String,
    /// Çalışan komutun gösterim metni.
    pub command: String,
    /// Kaldırma işi mi? (sonuç işleme yolu değişir).
    pub is_uninstall: bool,
    /// İşin sonucunu taşıyan kanal.
    receiver: Receiver<JobResult>,
    /// Canlı ilerleme olayları.
    progress_rx: Receiver<crate::progress::ProgressEvent>,
}

/// sudo parola isteyen apt/dnf planı — ana döngüde TUI askıya alınarak çalışır.
#[derive(Debug, Clone)]
pub struct PendingTtyPm {
    pub entry: crate::store::StoreEntry,
    pub plan: host::InstallPlan,
    pub operation: crate::store::install::PmOperation,
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
    /// İmzası doğrulanmış uzak Nova-Store kataloğu (yoksa mevcut korunur).
    pub signed_catalog: Option<Store>,
    /// Katalog yenileme durumu (durum satırı için).
    pub catalog_note: Option<String>,
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
    /// Komut Rehberi sekmesi etkin mi?
    ///
    /// Sol panelde araç kategorilerinden sonra, Mağaza'dan önce gelir.
    /// Nova-Commands bankası + yerel PATH stub'ı; Mağaza ile karışmaz.
    pub commands_tab: bool,
    /// Mağaza sekmesi etkin mi?
    ///
    /// Sol panelde Komut Rehberi'nden sonra, Ayarlar'dan hemen önce gelir.
    /// `settings_tab` / `commands_tab` ile aynı anda açılmaz; mağaza ayrı bir
    /// modal değil, ana ekran sekmesidir.
    pub store_tab: bool,
    /// Ayarlar sekmesi etkin mi? (Mağaza'nın altındaki satır)
    pub settings_tab: bool,
    /// Ayarlar sekmesinde seçili eylem:
    /// 0 = Nova güncelle, 1 = Mağaza kataloğu, 2 = Komut bankası,
    /// 3 = Özel araç, 4 = Tema, 5 = Dil.
    pub settings_action_index: usize,
    /// Tema seçici yan paneli açık mı?
    pub theme_picker_open: bool,
    /// Tema seçici listesindeki seçili indeks.
    pub theme_picker_index: usize,
    /// Dil seçici yan paneli açık mı?
    pub language_picker_open: bool,
    /// Dil seçici listesindeki seçili indeks.
    pub language_picker_index: usize,
    /// Seçili mağaza kategorisinin indeksi.
    pub store_category_index: usize,
    /// Seçili mağaza uygulamasının indeksi (filtrelenmiş liste üzerinden).
    pub store_app_index: usize,
    /// Mağaza detayındaki seçili bağlantı indeksi ([`crate::ui::store_detail_links`]).
    pub store_detail_link_index: usize,
    /// Komut Rehberi gezinme modu (görev odaklı IA).
    pub commands_mode: CommandsBrowseMode,
    /// Grup sütunu indeksi (görev kategorisi / profil / teknik kategori).
    pub commands_category_index: usize,
    /// Seçili liste satırı indeksi (görev / komut / PATH).
    pub commands_entry_index: usize,
    /// Kullanım bağlamı: çoklu seçili profil id'leri.
    pub commands_selected_profiles: std::collections::BTreeSet<String>,
    /// Göreve göre: seçilen görevden komutlara drill-down.
    pub commands_task_drill: Option<String>,
    /// Gömülü Nova-Commands bankası (`nova-commands-bank`).
    pub commands_bank: CommandBank,
    /// Gömülü arama indeksi (`nova-commands-search-index`) — banka ile ayrı kind.
    pub commands_search_index: SearchIndex,
    /// Yerel PATH keşfi anlık görüntüsü (`nova-local-commands` stub).
    pub commands_local: LocalCommandsSnapshot,
    /// Seçili aracın indeksi (filtrelenmiş liste üzerinden).
    pub tool_index: usize,
    /// Seçili AI araç eyleminin indeksi (detay yanı aksiyon listesi).
    pub tool_action_index: usize,
    /// Arama sorgusu.
    pub search_query: String,
    /// Arama modu aktif mi.
    pub search_active: bool,
    /// Sadece favoriler gösterilsin mi (araç listesi).
    pub favorite_only: bool,
    /// Komut Rehberi bankasında yalnızca favori komutlar (araç/Mağaza favorisinden ayrı).
    pub command_favorite_only: bool,
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
    /// Nova self-update başarılıysa: TUI çıktıktan sonra bu ikili `exec` edilir.
    ///
    /// Doğrulama poll anında yapılır; geçersizse alan `None` kalır ve oturum
    /// bozulmadan devam eder.
    pub pending_self_restart: Option<PathBuf>,
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
        let (commands_bank, commands_search_index) = match commands::load_bank_preferring_cache() {
            Ok((bank, idx, _)) => (bank, idx),
            Err(e) => {
                eprintln!("Nova-Commands bankası yüklenemedi: {e}");
                (
                    CommandBank {
                        schema_version: 2,
                        kind: commands::BANK_KIND.to_string(),
                        generated_at: None,
                        content_sha256: None,
                        command_count: Some(0),
                        concept_count: Some(0),
                        task_count: Some(0),
                        profile_count: Some(0),
                        commands: Vec::new(),
                        concepts: Vec::new(),
                        tasks: Vec::new(),
                        profiles: Vec::new(),
                    },
                    SearchIndex {
                        schema_version: 2,
                        kind: commands::loader::SEARCH_INDEX_KIND.to_string(),
                        entry_count: Some(0),
                        content_sha256: None,
                        entries: Vec::new(),
                    },
                )
            }
        };
        let mut app = Self {
            running: true,
            catalog,
            focus: PanelFocus::Category,
            category_index: 0,
            commands_tab: false,
            store_tab: false,
            settings_tab: false,
            settings_action_index: 0,
            theme_picker_open: false,
            theme_picker_index: 0,
            language_picker_open: false,
            language_picker_index: 0,
            store_category_index: 0,
            store_app_index: 0,
            store_detail_link_index: 0,
            commands_mode: CommandsBrowseMode::Start,
            commands_category_index: 0,
            commands_entry_index: 0,
            commands_selected_profiles: std::collections::BTreeSet::new(),
            commands_task_drill: None,
            commands_bank,
            commands_search_index,
            commands_local: LocalCommandsSnapshot::empty_stub(),
            tool_index: 0,
            tool_action_index: 0,
            search_query: String::new(),
            search_active: false,
            favorite_only: false,
            command_favorite_only: false,
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
            store: crate::store::loader::load_store(),
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
            pending_self_restart: None,
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
    /// alanları mevcut terminal boyutundan yeniden hesaplanır; liste
    /// panellerinde `Borders::ALL` ve seçime bağlı kaydırma penceresi
    /// hesaba katılır.
    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) -> Result<(), NovaError> {
        use crossterm::event::{MouseButton, MouseEventKind};

        // Modal açıkken ana ekran fare olayları yoksayılır (klavye gibi).
        if self.modal.is_open() {
            return Ok(());
        }

        let size = match self.terminal_size {
            Some(s) => s,
            None => return Ok(()),
        };
        let tools_tab = !self.store_tab && !self.settings_tab && !self.commands_tab;
        let panes = self.pane_widths();
        let tool_detail_side = tools_tab
            && crate::ui::tool_detail_side_layout(
                size.0 >= crate::ui::WIDE_MIN_WIDTH,
                size.0,
                panes.tabs,
            );
        let actions_count = if tools_tab {
            self.selected_tool()
                .map(|e| {
                    if e.has_ai_actions() {
                        e.actions.len()
                    } else {
                        0
                    }
                })
                .unwrap_or(0)
        } else {
            0
        };
        let areas = crate::ui::click_areas_ex(crate::ui::ClickAreasInput {
            size,
            show_help: self.show_help,
            store_tab: self.store_tab,
            commands_tab: self.commands_tab,
            tool_detail_side,
            actions_count,
            panes,
            secondary_cols: self.pane_secondary_cols(),
        });

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.handle_mouse_click(mouse.column, mouse.row, areas);
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

    /// Sol tık: kenarlıklı liste satırına isabet eden öğeyi seçer.
    fn handle_mouse_click(&mut self, x: u16, y: u16, areas: crate::ui::ClickAreas) {
        use crate::ui::list_index_at_y;

        // Dikey aksiyon paneli içerik alanının altında; önce onu dene.
        if !self.store_tab
            && !self.settings_tab
            && areas.actions_height > 0
            && y >= areas.actions_top
            && y < areas.actions_top.saturating_add(areas.actions_height)
        {
            let panel = ratatui::layout::Rect {
                x: 0,
                y: areas.actions_top,
                width: areas.tools_end_x.max(1),
                height: areas.actions_height,
            };
            if let Some(action_index) = crate::ui::action_index_at_y(self, y, panel) {
                self.tool_action_index = action_index;
                self.focus = PanelFocus::ToolActions;
            }
            return;
        }

        if y < areas.content_top || y >= areas.content_top.saturating_add(areas.content_height) {
            return;
        }

        let panel_top = areas.content_top;
        let panel_h = areas.content_height;

        if areas.wide && x < areas.category_end_x {
            let tab_len = self.visible_category_indices().len() + 3; // +Komut Rehberi +Mağaza +Ayarlar
            if let Some(row) =
                list_index_at_y(y, panel_top, panel_h, self.selected_tab_index(), tab_len)
            {
                self.set_tab_index(row);
                self.focus = PanelFocus::Category;
                self.tool_index = 0;
                self.reset_tool_action_index();
            }
            return;
        }

        if self.commands_tab {
            self.handle_commands_mouse_click(x, y, areas);
            return;
        }

        if self.store_tab {
            self.handle_store_mouse_click(x, y, areas);
            return;
        }

        if self.settings_tab {
            self.handle_settings_mouse_click(x, y, areas);
            return;
        }

        // Araç paneli (genişte orta/sağ sütun; darda tüm içerik).
        if areas.wide && x >= areas.tools_end_x {
            return;
        }
        let hits = crate::ui::tool_list_hits(self);
        let selected_visual = crate::ui::tool_list_selected_visual(self).unwrap_or(0);
        if let Some(row) = list_index_at_y(y, panel_top, panel_h, selected_visual, hits.len()) {
            let crate::ui::ToolListHit::Tool(idx) = hits[row];
            if idx != self.tool_index {
                self.tool_index = idx;
                self.reset_tool_action_index();
            } else {
                self.tool_index = idx;
            }
            self.focus = PanelFocus::Tool;
        }
    }

    /// Komut Rehberi fare tıklaması (mod / grup / liste / yan detay).
    fn handle_commands_mouse_click(&mut self, x: u16, y: u16, areas: crate::ui::ClickAreas) {
        use crate::ui::list_index_at_y;

        let panel_top = areas.content_top;
        let panel_h = areas.content_height;
        // Geniş: sekmeler | mod | (grup) | liste [| yan detay]
        if areas.wide {
            let source_end = areas.category_end_x.saturating_add(areas.secondary_width);
            let group_end = source_end.saturating_add(areas.secondary_width);
            let list_end = if areas.commands_detail_side {
                areas.commands_list_end_x
            } else {
                u16::MAX
            };

            if x < source_end {
                if let Some(row) = list_index_at_y(
                    y,
                    panel_top,
                    panel_h,
                    self.commands_mode.index(),
                    CommandsBrowseMode::ALL.len(),
                ) {
                    self.set_commands_mode(CommandsBrowseMode::from_index(row));
                    self.focus = PanelFocus::CommandsSource;
                }
            } else if self.commands_mode.needs_group_column() && x < group_end {
                let n = self.commands_group_len();
                if let Some(row) =
                    list_index_at_y(y, panel_top, panel_h, self.commands_category_index, n)
                {
                    self.commands_category_index = row;
                    self.commands_entry_index = 0;
                    self.commands_task_drill = None;
                    self.focus = PanelFocus::CommandsCategory;
                }
            } else if x < list_end {
                let n = self.commands_list_len();
                if let Some(row) =
                    list_index_at_y(y, panel_top, panel_h, self.commands_entry_index, n)
                {
                    self.commands_entry_index = row;
                    self.focus = PanelFocus::CommandsList;
                    self.show_selected_command_info();
                }
            }
            // Sağ detay tıklaması: yalnızca odak (bağlantı yok).
        } else {
            let n = self.commands_list_len();
            if let Some(row) = list_index_at_y(y, panel_top, panel_h, self.commands_entry_index, n)
            {
                self.commands_entry_index = row;
                self.focus = PanelFocus::CommandsList;
            }
        }
    }

    /// Mağaza sekmesi fare tıklaması (geniş: üç sütun; dar: dikey iki panel).
    fn handle_store_mouse_click(&mut self, x: u16, y: u16, areas: crate::ui::ClickAreas) {
        use crate::ui::list_index_at_y;

        let panel_top = areas.content_top;
        let panel_h = areas.content_height;

        if areas.wide {
            if x < areas.store_category_end_x {
                if let Some(row) = list_index_at_y(
                    y,
                    panel_top,
                    panel_h,
                    self.store_category_index,
                    STORE_CATEGORIES.len(),
                ) {
                    self.store_category_index = row;
                    self.store_app_index = 0;
                    self.focus = PanelFocus::StoreCategory;
                }
            } else if x < areas.store_apps_end_x {
                let apps = self.filtered_store_apps();
                if let Some(row) =
                    list_index_at_y(y, panel_top, panel_h, self.store_app_index, apps.len())
                {
                    self.store_app_index = row;
                    self.store_detail_link_index = 0;
                    self.focus = PanelFocus::StoreApp;
                }
            } else if areas.store_detail_side {
                // Sağ detay: tıklama kaynak/depo URL'sini tarayıcıda açar.
                self.focus = PanelFocus::StoreDetail;
                let links = crate::ui::store_detail_links(self);
                if links.is_empty() {
                    self.status_message = "Bu uygulama için açılabilir https kaynağı yok.".into();
                } else {
                    // Birden fazla bağlantı varsa tıklama satırına göre seç;
                    // aksi hâlde (veya satır eşleşmezse) ilk bağlantıyı aç.
                    let rel_y = y.saturating_sub(panel_top);
                    if links.len() > 1 && rel_y > 8 {
                        self.store_detail_link_index = 1.min(links.len() - 1);
                    } else {
                        self.store_detail_link_index = 0;
                    }
                    self.open_selected_store_detail_link();
                }
            }
            return;
        }

        // Dar: kategoriler üstte, uygulamalar altta.
        let cat_h = areas.store_category_end_y.saturating_sub(areas.content_top);
        if y < areas.store_category_end_y {
            if let Some(row) = list_index_at_y(
                y,
                panel_top,
                cat_h,
                self.store_category_index,
                STORE_CATEGORIES.len(),
            ) {
                self.store_category_index = row;
                self.store_app_index = 0;
                self.focus = PanelFocus::StoreCategory;
            }
        } else {
            let apps = self.filtered_store_apps();
            let apps_h = panel_h.saturating_sub(cat_h);
            if let Some(row) = list_index_at_y(
                y,
                areas.store_category_end_y,
                apps_h,
                self.store_app_index,
                apps.len(),
            ) {
                self.store_app_index = row;
                self.focus = PanelFocus::StoreApp;
            }
        }
    }

    /// Ayarlar sekmesi fare tıklaması (tema/dil paneli açık/kapalı, geniş/dar).
    fn handle_settings_mouse_click(&mut self, x: u16, y: u16, areas: crate::ui::ClickAreas) {
        use crate::ui::list_index_at_y;

        let panel_top = areas.content_top;
        let panel_h = areas.content_height;
        const SETTINGS_ACTIONS: usize = 6;

        if self.settings_side_picker_open() && areas.wide {
            let side_start = areas.category_end_x.saturating_add(areas.secondary_width);
            if x >= side_start {
                if self.language_picker_open {
                    let langs = crate::i18n::Language::all().len();
                    if let Some(row) =
                        list_index_at_y(y, panel_top, panel_h, self.language_picker_index, langs)
                    {
                        self.language_picker_index = row;
                        self.focus = PanelFocus::LanguagePicker;
                    }
                } else {
                    let themes = crate::ui::UiTheme::all().len();
                    if let Some(row) =
                        list_index_at_y(y, panel_top, panel_h, self.theme_picker_index, themes)
                    {
                        self.theme_picker_index = row;
                        self.focus = PanelFocus::ThemePicker;
                    }
                }
            } else if let Some(row) = list_index_at_y(
                y,
                panel_top,
                panel_h,
                self.settings_action_index,
                SETTINGS_ACTIONS,
            ) {
                self.apply_settings_action_click(row);
            }
            return;
        }

        if self.settings_side_picker_open() && !areas.wide {
            // `draw_content` ile aynı: üst yarı eylemler, alt yarı tema/dil.
            let half = (panel_h / 2).max(3);
            if y < panel_top.saturating_add(half) {
                if let Some(row) = list_index_at_y(
                    y,
                    panel_top,
                    half,
                    self.settings_action_index,
                    SETTINGS_ACTIONS,
                ) {
                    self.apply_settings_action_click(row);
                }
            } else if self.language_picker_open {
                let langs = crate::i18n::Language::all().len();
                let side_h = panel_h.saturating_sub(half);
                if let Some(row) = list_index_at_y(
                    y,
                    panel_top.saturating_add(half),
                    side_h,
                    self.language_picker_index,
                    langs,
                ) {
                    self.language_picker_index = row;
                    self.focus = PanelFocus::LanguagePicker;
                }
            } else {
                let themes = crate::ui::UiTheme::all().len();
                let theme_h = panel_h.saturating_sub(half);
                if let Some(row) = list_index_at_y(
                    y,
                    panel_top.saturating_add(half),
                    theme_h,
                    self.theme_picker_index,
                    themes,
                ) {
                    self.theme_picker_index = row;
                    self.focus = PanelFocus::ThemePicker;
                }
            }
            return;
        }

        if let Some(row) = list_index_at_y(
            y,
            panel_top,
            panel_h,
            self.settings_action_index,
            SETTINGS_ACTIONS,
        ) {
            self.apply_settings_action_click(row);
        }
    }

    /// Ayarlar listesinde tıklanan eylemi uygular (Tema / Dil → paneli aç).
    fn apply_settings_action_click(&mut self, index: usize) {
        self.settings_action_index = index.min(5);
        self.focus = PanelFocus::Tool;
        match self.settings_action_index {
            4 => self.open_theme_picker(),
            5 => self.open_language_picker(),
            3 => {
                self.theme_picker_open = false;
                self.language_picker_open = false;
                self.modal = modal::Modal::ToolForm(Box::default());
            }
            _ => {
                self.theme_picker_open = false;
                self.language_picker_open = false;
            }
        }
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
                if self.theme_picker_open {
                    self.close_theme_picker();
                } else if self.language_picker_open {
                    self.close_language_picker();
                } else if self.show_help {
                    self.show_help = false;
                } else if self.commands_tab && self.commands_task_drill.is_some() {
                    // Görev drill-down'dan komut listesine çık — çalıştırma yok.
                    self.commands_task_drill = None;
                    self.commands_entry_index = 0;
                    self.status_message = "Görev listesine dönüldü (komut çalıştırılmadı).".into();
                } else {
                    self.running = false;
                }
            }
            KeyCode::Char(' ') if self.commands_tab => {
                self.toggle_commands_profile_or_noop();
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
                if self.store_tab {
                    self.status_message =
                        "Favoriler Mağaza'da yok — araç listesi veya Komut Rehberi bankası.".into();
                } else if self.commands_tab {
                    // Banka favorileri (command_favorites); PATH / Mağaza ile karışmaz.
                    self.toggle_selected_command_favorite();
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
                // Dikey aksiyon panelinde ← araç listesine döner (↑↓ seçer).
                if self.focus == PanelFocus::ToolActions {
                    self.focus = PanelFocus::Tool;
                } else if self.narrow_mode && !self.store_tab {
                    // Dar terminalde araç kategorisi paneli gizli olduğundan odak
                    // listede kalır; yön tuşları yanıltıcı olmasın.
                    self.status_message =
                        "Kategori paneli bu genişlikte gizli; terminali genişlet.".into();
                } else {
                    self.focus = self.prev_focus();
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                // Aksiyon paneli dikey listedir; → odak taşır (seçim ↑↓ ile).
                self.focus = self.next_focus();
            }
            KeyCode::Tab => {
                self.focus = self.next_focus_wrapping();
            }
            KeyCode::BackTab => {
                self.focus = self.prev_focus_wrapping();
            }
            KeyCode::Enter => match self.focus {
                PanelFocus::Category => {
                    self.focus = self.next_focus();
                }
                PanelFocus::Tool => self.select_tool(),
                PanelFocus::ToolDetail => {}
                PanelFocus::ToolActions => self.select_tool_action(),
                PanelFocus::StoreCategory => {
                    self.focus = PanelFocus::StoreApp;
                }
                PanelFocus::StoreApp => self.select_store_app(),
                PanelFocus::StoreDetail => {
                    self.open_selected_store_detail_link();
                }
                PanelFocus::CommandsSource => {
                    self.focus = if self.commands_mode.needs_group_column() {
                        PanelFocus::CommandsCategory
                    } else {
                        PanelFocus::CommandsList
                    };
                }
                PanelFocus::CommandsCategory => {
                    if self.commands_mode == CommandsBrowseMode::Context {
                        self.toggle_selected_profile();
                    } else {
                        self.focus = PanelFocus::CommandsList;
                    }
                }
                PanelFocus::CommandsList => {
                    self.activate_commands_list_item();
                }
                PanelFocus::CommandsDetail => {}
                PanelFocus::ThemePicker => {
                    let _ = self.apply_theme_at_picker_index();
                }
                PanelFocus::LanguagePicker => {
                    let _ = self.apply_language_at_picker_index();
                }
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
            KeyCode::Char('g') | KeyCode::Char('G') => {
                // Komut Rehberi: Nova-Commands bankası + yerel PATH stub.
                self.select_commands_tab();
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
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.launch_default_ai_tool();
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
                    self.status_message =
                        "Favori filtresi Mağaza'da yok — araç veya Komut Rehberi.".into();
                } else if self.commands_tab {
                    if self.commands_mode == CommandsBrowseMode::SystemPath {
                        self.status_message =
                            "Favori filtresi yalnız katalog komutlarında (PATH değil).".into();
                    } else {
                        self.command_favorite_only = !self.command_favorite_only;
                        self.commands_entry_index = 0;
                        if self.command_favorite_only {
                            self.status_message =
                                "Sadece banka favori komutları (Mağaza/araç favorisi değil)."
                                    .into();
                        } else {
                            self.status_message = "Tüm banka komutları gösteriliyor.".into();
                        }
                    }
                } else {
                    self.favorite_only = !self.favorite_only;
                    if self.favorite_only {
                        self.status_message = "Sadece favoriler gösteriliyor.".into();
                    } else {
                        self.status_message = "Tüm araçlar gösteriliyor.".into();
                    }
                }
            }
            // Odaklı panel boyutu: yalnızca `-` / `+` (ve `=` = `+`).
            // `[` / `]` artık boyutlandırmaz; yardımda `-`/`+` gösterilir.
            KeyCode::Char('-') => {
                self.resize_focused_panel(-1);
            }
            KeyCode::Char('=') | KeyCode::Char('+') => {
                self.resize_focused_panel(1);
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
        self.commands_entry_index = 0;
        self.reset_tool_action_index();
    }

    /// Seçili banka komutunun favorisini değiştirir (PATH / Mağaza / görev değil).
    fn toggle_selected_command_favorite(&mut self) {
        if self.commands_mode == CommandsBrowseMode::SystemPath {
            self.status_message =
                "Favori yalnız katalog komutlarında (Sistemimde Bulunan değil).".into();
            return;
        }
        if self.commands_list_shows_tasks() {
            self.status_message = "Favori görev satırında değil; önce komuta gir (Enter).".into();
            return;
        }
        let cmds = self.filtered_bank_commands();
        if cmds.is_empty() {
            self.status_message = "Favorilenecek banka komutu yok.".into();
            return;
        }
        let idx = self.commands_entry_index.min(cmds.len() - 1);
        let id = cmds[idx].id.clone();
        let name = cmds[idx].name.clone();
        let now_fav = if self.state.file.command_favorites.contains(&id) {
            self.state.file.command_favorites.remove(&id);
            false
        } else {
            self.state.file.command_favorites.insert(id.clone());
            true
        };
        if let Err(msg) = self.persist_state() {
            self.status_message = format!("Hata: komut favorisi kaydedilemedi ({msg})");
            return;
        }
        self.status_message = if now_fav {
            format!("★ Banka favorisi: {name} (araç/Mağaza favorisi değil)")
        } else {
            format!("☆ Banka favorisi kaldırıldı: {name}")
        };
    }

    /// Banka komutu favori mi?
    pub fn is_command_favorite(&self, id: &str) -> bool {
        self.state.file.command_favorites.contains(id)
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
                    self.settings_action_index = (self.settings_action_index + 1).min(5);
                    return;
                }
                let tools = self.filtered_tools();
                if tools.is_empty() {
                    return;
                }
                let max = tools.len().saturating_sub(1);
                let next = (self.tool_index + 1).min(max);
                if next != self.tool_index {
                    self.tool_index = next;
                    self.reset_tool_action_index();
                }
            }
            PanelFocus::ToolDetail | PanelFocus::CommandsDetail => {}
            PanelFocus::ToolActions => {
                let max = self.selected_tool_actions_len().saturating_sub(1);
                self.tool_action_index = (self.tool_action_index + 1).min(max);
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
                self.store_detail_link_index = 0;
            }
            PanelFocus::StoreDetail => {
                let n = crate::ui::store_detail_links(self).len();
                if n == 0 {
                    return;
                }
                self.store_detail_link_index = (self.store_detail_link_index + 1).min(n - 1);
            }
            PanelFocus::CommandsSource => {
                self.set_commands_mode(self.commands_mode.next());
            }
            PanelFocus::CommandsCategory => {
                let max = self.commands_group_len().saturating_sub(1);
                self.commands_category_index = (self.commands_category_index + 1).min(max);
                self.commands_entry_index = 0;
                self.commands_task_drill = None;
            }
            PanelFocus::CommandsList => {
                let n = self.commands_list_len();
                if n == 0 {
                    return;
                }
                self.commands_entry_index =
                    (self.commands_entry_index + 1).min(n.saturating_sub(1));
            }
            PanelFocus::ThemePicker => {
                let max = crate::ui::UiTheme::all().len().saturating_sub(1);
                self.theme_picker_index = (self.theme_picker_index + 1).min(max);
            }
            PanelFocus::LanguagePicker => {
                let max = crate::i18n::Language::all().len().saturating_sub(1);
                self.language_picker_index = (self.language_picker_index + 1).min(max);
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
                let prev = self.tool_index.saturating_sub(1);
                if prev != self.tool_index {
                    self.tool_index = prev;
                    self.reset_tool_action_index();
                }
            }
            PanelFocus::ToolDetail | PanelFocus::CommandsDetail => {}
            PanelFocus::ToolActions => {
                // ↑ ilk aksiyondayken araç listesine döner.
                if self.tool_action_index == 0 {
                    self.focus = PanelFocus::Tool;
                } else {
                    self.tool_action_index = self.tool_action_index.saturating_sub(1);
                }
            }
            PanelFocus::StoreCategory => {
                self.store_category_index = self.store_category_index.saturating_sub(1);
                self.store_app_index = 0;
            }
            PanelFocus::StoreApp => {
                self.store_app_index = self.store_app_index.saturating_sub(1);
                self.store_detail_link_index = 0;
            }
            PanelFocus::StoreDetail => {
                self.store_detail_link_index = self.store_detail_link_index.saturating_sub(1);
            }
            PanelFocus::CommandsSource => {
                self.set_commands_mode(self.commands_mode.prev());
            }
            PanelFocus::CommandsCategory => {
                self.commands_category_index = self.commands_category_index.saturating_sub(1);
                self.commands_entry_index = 0;
                self.commands_task_drill = None;
            }
            PanelFocus::CommandsList => {
                self.commands_entry_index = self.commands_entry_index.saturating_sub(1);
            }
            PanelFocus::ThemePicker => {
                self.theme_picker_index = self.theme_picker_index.saturating_sub(1);
            }
            PanelFocus::LanguagePicker => {
                self.language_picker_index = self.language_picker_index.saturating_sub(1);
            }
        }
    }

    /// Sekme listesinde aşağı hareket eder.
    ///
    /// Görünür (kurulu araçlı) kategoriler → Komut Rehberi → Mağaza → Ayarlar.
    /// Odak Category panelinde kalır (`m` / `select_store_tab` odak değiştirir).
    fn move_tab_down(&mut self) {
        if self.settings_tab {
            return;
        }
        if self.store_tab {
            self.store_tab = false;
            self.commands_tab = false;
            self.settings_tab = true;
            self.settings_action_index = 0;
            return;
        }
        if self.commands_tab {
            self.commands_tab = false;
            self.store_tab = true;
            self.settings_tab = false;
            self.store_category_index = 0;
            self.store_app_index = 0;
            return;
        }
        let visible = self.visible_category_indices();
        if visible.is_empty() {
            self.select_commands_tab();
            self.focus = PanelFocus::Category;
            return;
        }
        if let Some(pos) = visible.iter().position(|&i| i == self.category_index) {
            if pos + 1 < visible.len() {
                self.category_index = visible[pos + 1];
                self.tool_index = 0;
                self.reset_tool_action_index();
            } else {
                self.select_commands_tab();
                self.focus = PanelFocus::Category;
                if self.focus == PanelFocus::ToolActions {
                    self.focus = PanelFocus::Category;
                }
            }
        } else {
            self.category_index = visible[0];
            self.tool_index = 0;
            self.reset_tool_action_index();
        }
    }

    /// Sekme listesinde yukarı hareket eder.
    fn move_tab_up(&mut self) {
        if self.settings_tab {
            self.settings_tab = false;
            self.theme_picker_open = false;
            self.language_picker_open = false;
            self.select_store_tab();
            self.focus = PanelFocus::Category;
            return;
        }
        if self.store_tab {
            self.store_tab = false;
            self.settings_tab = false;
            self.select_commands_tab();
            self.focus = PanelFocus::Category;
            return;
        }
        if self.commands_tab {
            self.commands_tab = false;
            self.store_tab = false;
            self.settings_tab = false;
            let visible = self.visible_category_indices();
            self.category_index = visible.last().copied().unwrap_or(0);
            self.tool_index = 0;
            self.reset_tool_action_index();
            return;
        }
        let visible = self.visible_category_indices();
        if let Some(pos) = visible.iter().position(|&i| i == self.category_index) {
            if pos > 0 {
                self.category_index = visible[pos - 1];
                self.tool_index = 0;
                self.reset_tool_action_index();
            }
        } else if let Some(&first) = visible.first() {
            self.category_index = first;
            self.tool_index = 0;
            self.reset_tool_action_index();
        }
    }

    /// Komut Rehberi sekmesine geçer (`g` kısayolu).
    ///
    /// Görev odaklı IA: Başlangıç → görev → bağlam → tüm komutlar → PATH.
    /// Mağaza kataloğu buraya karışmaz; komutlar otomatik çalıştırılmaz.
    pub fn select_commands_tab(&mut self) {
        self.commands_tab = true;
        self.store_tab = false;
        self.settings_tab = false;
        self.theme_picker_open = false;
        self.language_picker_open = false;
        self.commands_mode = CommandsBrowseMode::Start;
        self.commands_category_index = 0;
        self.commands_entry_index = 0;
        self.commands_task_drill = None;
        self.refresh_local_commands_stub();
        self.focus = PanelFocus::CommandsList;
        self.status_message =
            "Komut Rehberi: önce ne yapmak istediğini seç (Başlangıç). PATH ≠ katalog. Çalıştırma yok."
                .into();
    }

    /// Mağaza sekmesine geçer.
    ///
    /// `m` kısayolu da budur. Odağı doğrudan uygulama listesine götürür:
    /// kullanıcı kurulum eylemini hemen görebilsin diye.
    pub fn select_store_tab(&mut self) {
        self.store_tab = true;
        self.commands_tab = false;
        self.settings_tab = false;
        self.theme_picker_open = false;
        self.language_picker_open = false;
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
        self.commands_tab = false;
        self.theme_picker_open = false;
        self.language_picker_open = false;
        self.settings_action_index = 0;
        self.focus = PanelFocus::Tool;
        self.status_message = "Ayarlar: Nova güncelle / Mağaza kataloğu / Komut bankası / Özel araç / Tema / Dil. Enter uygular."
            .into();
    }

    /// Sol panelde seçili sekmenin satır indeksi (çizilen liste üzerinden).
    ///
    /// Satırlar: görünür kategoriler + Komut Rehberi + Mağaza + Ayarlar.
    pub fn selected_tab_index(&self) -> usize {
        let n = self.visible_category_indices().len();
        if self.settings_tab {
            n + 2
        } else if self.store_tab {
            n + 1
        } else if self.commands_tab {
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
    /// Görünür kategori satırlarından sonra Komut Rehberi, Mağaza, en sonda Ayarlar.
    pub fn set_tab_index(&mut self, row: usize) {
        let visible = self.visible_category_indices();
        if row < visible.len() {
            self.commands_tab = false;
            self.store_tab = false;
            self.settings_tab = false;
            self.theme_picker_open = false;
            self.language_picker_open = false;
            self.category_index = visible[row];
            self.tool_index = 0;
            self.store_app_index = 0;
            self.reset_tool_action_index();
        } else if row == visible.len() {
            self.select_commands_tab();
        } else if row == visible.len() + 1 {
            self.select_store_tab();
        } else {
            self.select_settings_tab();
        }
    }

    /// Yerel PATH keşfi stub'ını yeniler (bank eşleşmesi yalnız `bank_id`).
    pub fn refresh_local_commands_stub(&mut self) {
        self.commands_local = commands::local::discover_path_stub(Some(&self.commands_bank), 64);
    }

    /// Dual-concept kaynak (PATH stub ↔ banka).
    pub fn commands_source(&self) -> CommandsSource {
        self.commands_mode.source()
    }

    /// Banka kategorileri (sıralı, tekil) — Tüm komutlar modu.
    pub fn commands_bank_categories(&self) -> Vec<&str> {
        bank_categories(&self.commands_bank)
    }

    /// Gezinme modunu değiştirir; liste/grup seçimini sıfırlar.
    pub fn set_commands_mode(&mut self, mode: CommandsBrowseMode) {
        self.commands_mode = mode;
        self.commands_category_index = 0;
        self.commands_entry_index = 0;
        self.commands_task_drill = None;
        if mode == CommandsBrowseMode::SystemPath {
            self.refresh_local_commands_stub();
        }
    }

    /// Grup sütunu satır sayısı.
    pub fn commands_group_len(&self) -> usize {
        match self.commands_mode {
            CommandsBrowseMode::ByTask => self.commands_bank.task_categories().len(),
            CommandsBrowseMode::Context => self.commands_bank.profiles.len(),
            CommandsBrowseMode::AllCommands => self.commands_bank_categories().len(),
            _ => 0,
        }
    }

    /// Liste satır sayısı (görev / komut / PATH).
    pub fn commands_list_len(&self) -> usize {
        if self.commands_mode == CommandsBrowseMode::SystemPath {
            self.filtered_local_commands().len()
        } else if self.commands_list_shows_tasks() {
            self.filtered_tasks().len()
        } else {
            self.filtered_bank_commands().len()
        }
    }

    /// Liste şu an görev satırları mı gösteriyor?
    pub fn commands_list_shows_tasks(&self) -> bool {
        self.commands_mode == CommandsBrowseMode::ByTask && self.commands_task_drill.is_none()
    }

    /// Banka komutları — moda göre süzülür; Local ile birleştirilmez.
    ///
    /// Arama doluyken Tüm komutlar / Başlangıç / bağlamda geniş tarama;
    /// boşken moda özgü küme. Favori süzgeci yalnız komut listelerinde.
    pub fn filtered_bank_commands(&self) -> Vec<&commands::CommandEntry> {
        let searching = !self.search_query.is_empty();
        let mode = self.commands_mode;
        let start_order = if mode == CommandsBrowseMode::Start && !searching {
            commands::start_learning_command_ids(&self.commands_bank)
        } else {
            Vec::new()
        };
        let start_ids: std::collections::BTreeSet<&str> =
            start_order.iter().map(|s| s.as_str()).collect();
        let profile_cmd_ids = if mode == CommandsBrowseMode::Context && !searching {
            self.context_related_command_ids()
        } else {
            std::collections::BTreeSet::new()
        };
        let drill_ids: std::collections::BTreeSet<&str> =
            if let Some(ref tid) = self.commands_task_drill {
                self.commands_bank
                    .find_task(tid)
                    .map(|t| t.related_commands.iter().map(|s| s.as_str()).collect())
                    .unwrap_or_default()
            } else {
                std::collections::BTreeSet::new()
            };
        let cats = self.commands_bank_categories();
        let cat = cats
            .get(self.commands_category_index)
            .copied()
            .unwrap_or("");

        let mut cmds: Vec<&commands::CommandEntry> = self
            .commands_bank
            .commands
            .iter()
            .filter(|c| {
                if self.command_favorite_only && !self.is_command_favorite(&c.id) {
                    return false;
                }
                if !self.matches_command_search(c) {
                    return false;
                }
                if searching {
                    // Arama: katalog genelinde (PATH değil).
                    return mode != CommandsBrowseMode::SystemPath;
                }
                match mode {
                    CommandsBrowseMode::Start => {
                        start_ids.contains(c.id.as_str()) || start_ids.contains(c.name.as_str())
                    }
                    CommandsBrowseMode::ByTask => {
                        !drill_ids.is_empty()
                            && (drill_ids.contains(c.id.as_str())
                                || drill_ids.contains(c.name.as_str()))
                    }
                    CommandsBrowseMode::Context => {
                        profile_cmd_ids.contains(c.id.as_str())
                            || profile_cmd_ids.contains(c.name.as_str())
                    }
                    CommandsBrowseMode::AllCommands => c.category == cat,
                    CommandsBrowseMode::SystemPath => false,
                }
            })
            .collect();

        // Başlangıç: öğrenme sırasını koru.
        if mode == CommandsBrowseMode::Start && !searching {
            cmds.sort_by_key(|c| {
                start_order
                    .iter()
                    .position(|id| id == &c.id || id == &c.name)
                    .unwrap_or(usize::MAX)
            });
        }
        cmds
    }

    /// Seçili görev kategorisindeki görevler (arama: tüm görevler).
    pub fn filtered_tasks(&self) -> Vec<&commands::TaskEntry> {
        let cats = self.commands_bank.task_categories();
        let cat = cats
            .get(self.commands_category_index)
            .copied()
            .unwrap_or("");
        let searching = !self.search_query.is_empty();
        let lang = self.ui_lang().id();
        let q = self.search_query.to_ascii_lowercase();
        self.commands_bank
            .tasks
            .iter()
            .filter(|t| {
                if !searching && t.category != cat {
                    return false;
                }
                if !searching {
                    return true;
                }
                t.id.to_ascii_lowercase().contains(&q)
                    || t.name.pick(lang).to_ascii_lowercase().contains(&q)
                    || t.summary.pick(lang).to_ascii_lowercase().contains(&q)
                    || t.category.to_ascii_lowercase().contains(&q)
                    || t.tags
                        .iter()
                        .any(|tag| tag.to_ascii_lowercase().contains(&q))
                    || t.intent
                        .all_for_lang(lang)
                        .iter()
                        .any(|p| p.to_ascii_lowercase().contains(&q))
                    || self.commands_search_index.entries.iter().any(|e| {
                        e.entry_type == "task"
                            && e.id == t.id
                            && (e.name.as_ref().is_some_and(|n| n.matches_query(&q, lang))
                                || e.tags
                                    .iter()
                                    .any(|tag| tag.to_ascii_lowercase().contains(&q)))
                    })
            })
            .collect()
    }

    /// Seçili profillerin birleşik ilgili komut id'leri (hiç seçili değilse tüm profiller).
    fn context_related_command_ids(&self) -> std::collections::BTreeSet<&str> {
        let mut ids = std::collections::BTreeSet::new();
        let selected = &self.commands_selected_profiles;
        for p in &self.commands_bank.profiles {
            if !selected.is_empty() && !selected.contains(&p.id) {
                continue;
            }
            for c in &p.related_commands {
                ids.insert(c.as_str());
            }
        }
        ids
    }

    /// Yerel PATH girişleri (arama süzgeçli) — bank verification süzgeci yok.
    pub fn filtered_local_commands(&self) -> Vec<&commands::LocalEntry> {
        self.commands_local
            .entries
            .iter()
            .filter(|e| {
                if self.search_query.is_empty() {
                    return true;
                }
                let q = self.search_query.to_ascii_lowercase();
                e.name.to_ascii_lowercase().contains(&q)
                    || e.path.to_ascii_lowercase().contains(&q)
                    || e.bank_id
                        .as_ref()
                        .map(|id| id.to_ascii_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .collect()
    }

    fn matches_command_search(&self, cmd: &commands::CommandEntry) -> bool {
        if self.search_query.is_empty() {
            return true;
        }
        let q = self.search_query.to_ascii_lowercase();
        let lang = self.ui_lang().id();
        if cmd.id.to_ascii_lowercase().contains(&q)
            || cmd.name.to_ascii_lowercase().contains(&q)
            || cmd.category.to_ascii_lowercase().contains(&q)
            || cmd.summary.pick(lang).to_ascii_lowercase().contains(&q)
            || cmd.description.pick(lang).to_ascii_lowercase().contains(&q)
            || cmd.risk.as_str().contains(&q)
            || cmd.verification.status.as_str().contains(&q)
            || cmd.tags.iter().any(|t| t.to_ascii_lowercase().contains(&q))
            || cmd
                .intent
                .all_for_lang(lang)
                .iter()
                .any(|p| p.to_ascii_lowercase().contains(&q))
            || cmd
                .examples
                .iter()
                .any(|ex| ex.command.to_ascii_lowercase().contains(&q))
        {
            return true;
        }
        // search-index.json (ayrı kind) — intent/tag UI araması.
        self.commands_search_index.entries.iter().any(|e| {
            e.entry_type == "command"
                && e.id == cmd.id
                && (e.name.as_ref().is_some_and(|n| n.matches_query(&q, lang))
                    || e.tags.iter().any(|t| t.to_ascii_lowercase().contains(&q))
                    || e.category
                        .as_ref()
                        .map(|c| c.to_ascii_lowercase().contains(&q))
                        .unwrap_or(false)
                    || e.risk
                        .as_ref()
                        .map(|r| r.to_ascii_lowercase().contains(&q))
                        .unwrap_or(false)
                    || e.verification
                        .as_ref()
                        .map(|v| v.to_ascii_lowercase().contains(&q))
                        .unwrap_or(false))
        })
    }

    /// Etkin ekranın sütun / yükseklik tercihleri (`state.toml` → kırpılmış).
    pub fn pane_widths(&self) -> crate::ui::PaneWidths {
        crate::ui::PaneWidths::for_screen(self.ui_screen(), &self.state.file.prefs)
    }

    /// Aktif ana sekme (boyut kalıcılığı anahtarı).
    pub fn ui_screen(&self) -> crate::config::state::UiScreen {
        use crate::config::state::UiScreen;
        if self.store_tab {
            UiScreen::Store
        } else if self.commands_tab {
            UiScreen::Commands
        } else if self.settings_tab {
            UiScreen::Settings
        } else {
            UiScreen::Tools
        }
    }

    /// Yan-detay eşiği için ikincil sütun sayısı (gruplu modlar=2, diğer=1).
    ///
    /// Pane-resize davranışı aynı kalır: yalnızca Commands ekranı `secondary_cols` kullanır.
    pub fn pane_secondary_cols(&self) -> u16 {
        if self.commands_tab && self.commands_mode.needs_group_column() {
            2
        } else {
            1
        }
    }

    /// Detay paneli (yan veya alt) şu an görünür mü?
    pub fn detail_panel_visible(&self) -> bool {
        let Some((w, h)) = self.terminal_size else {
            // Boyut bilinmiyorsa (birim test) alt detayı zincire alma;
            // yan detay ayrı `*_side_active` kontrolleriyle eklenir.
            return false;
        };
        w >= crate::ui::MEDIUM_MIN_WIDTH && h >= 10
    }

    /// Odaklı paneli `-` / `+` ile bir adım boyutlandırır; yalnızca bu ekrana yazar.
    fn resize_focused_panel(&mut self, delta: i16) {
        let screen = self.ui_screen();
        let panes = self.pane_widths();
        let focus = self.focus;

        let is_vertical = match focus {
            PanelFocus::ToolActions => true,
            PanelFocus::ToolDetail => !self.tool_detail_side_layout_active(),
            PanelFocus::StoreDetail => !self.store_detail_side_active(),
            PanelFocus::CommandsDetail => !self.commands_detail_side_active(),
            PanelFocus::StoreCategory if self.narrow_mode && self.store_tab => true,
            PanelFocus::CommandsSource if self.narrow_mode && self.commands_tab => true,
            _ => false,
        };

        let slot = match focus {
            PanelFocus::Category => "tabs",
            PanelFocus::StoreCategory
            | PanelFocus::CommandsSource
            | PanelFocus::CommandsCategory => {
                if is_vertical {
                    "secondary_height"
                } else {
                    "secondary"
                }
            }
            PanelFocus::Tool
            | PanelFocus::StoreApp
            | PanelFocus::CommandsList
            | PanelFocus::ThemePicker
            | PanelFocus::LanguagePicker => {
                // Ayarlar: picker açıkken ayar paneli `secondary`, liste `list`.
                if self.settings_tab
                    && focus == PanelFocus::Tool
                    && self.settings_side_picker_open()
                {
                    "secondary"
                } else {
                    "list"
                }
            }
            PanelFocus::ToolDetail | PanelFocus::StoreDetail | PanelFocus::CommandsDetail => {
                if is_vertical {
                    "detail_height"
                } else {
                    "detail_width"
                }
            }
            PanelFocus::ToolActions => "actions_height",
        };

        let (cur, min_v, max_v, label) = match slot {
            "tabs" => (
                panes.tabs,
                crate::ui::TABS_PANEL_WIDTH_MIN,
                crate::ui::TABS_PANEL_WIDTH_MAX,
                "genişlik",
            ),
            "secondary" => (
                panes.secondary,
                crate::ui::SECONDARY_PANEL_WIDTH_MIN,
                crate::ui::SECONDARY_PANEL_WIDTH_MAX,
                "genişlik",
            ),
            "list" => (
                panes.list,
                crate::ui::LIST_PANEL_WIDTH_MIN,
                crate::ui::LIST_PANEL_WIDTH_MAX,
                "genişlik",
            ),
            "detail_width" => (
                panes.detail_width,
                crate::ui::STORE_DETAIL_PANEL_WIDTH.min(crate::ui::TOOL_DETAIL_PANEL_WIDTH),
                crate::ui::DETAIL_PANEL_WIDTH_MAX,
                "genişlik",
            ),
            "detail_height" => (
                panes.detail_height,
                crate::ui::DETAIL_HEIGHT_MIN,
                crate::ui::DETAIL_HEIGHT_MAX,
                "yükseklik",
            ),
            "actions_height" => {
                let needed = crate::ui::actions_panel_rows(self.selected_tool_actions_len());
                let cur = if panes.actions_height > 0 {
                    panes.actions_height.max(needed)
                } else {
                    needed
                };
                (
                    cur,
                    needed.max(crate::ui::ACTIONS_PANEL_BORDER_ROWS + 1),
                    crate::ui::ACTIONS_HEIGHT_MAX,
                    "yükseklik",
                )
            }
            "secondary_height" => (
                panes.secondary_height,
                crate::ui::STORE_CAT_HEIGHT_MIN,
                crate::ui::STORE_CAT_HEIGHT_MAX,
                "yükseklik",
            ),
            _ => unreachable!(),
        };

        let next_i = cur as i16 + delta;
        if next_i < min_v as i16 {
            self.status_message =
                format!("{}: en küçük {label} ({min_v})", self.focus_panel_name());
            return;
        }
        if next_i > max_v as i16 {
            self.status_message =
                format!("{}: en büyük {label} ({max_v})", self.focus_panel_name());
            return;
        }
        let next = next_i as u16;

        // Kardeşten yer çal: yatayda list↔detail veya tabs↔list.
        let sizes = self.state.file.prefs.pane_sizes.get_mut(screen);
        match slot {
            "tabs" => sizes.tabs = Some(next),
            "secondary" => sizes.secondary = Some(next),
            "list" => {
                sizes.list = Some(next);
                // Detay yan paneldeyken listeden çalınan/eklenen yer detaya yansır.
            }
            "detail_width" => {
                sizes.detail_width = Some(next);
                // Detay büyürken listeden çal (kardeş).
                if delta > 0 {
                    let list_cur = panes.list;
                    if list_cur > crate::ui::LIST_PANEL_WIDTH_MIN {
                        sizes.list = Some(list_cur - 1);
                    }
                } else {
                    let list_cur = panes.list;
                    if list_cur < crate::ui::LIST_PANEL_WIDTH_MAX {
                        sizes.list = Some(list_cur + 1);
                    }
                }
            }
            "detail_height" => sizes.detail_height = Some(next),
            "actions_height" => sizes.actions_height = Some(next),
            "secondary_height" => sizes.secondary_height = Some(next),
            _ => {}
        }

        // tabs/secondary/list büyürken mümkünse listeden veya secondary'den çal.
        if matches!(slot, "tabs" | "secondary") && delta > 0 {
            let list_cur = self.pane_widths().list;
            if list_cur > crate::ui::LIST_PANEL_WIDTH_MIN {
                self.state.file.prefs.pane_sizes.get_mut(screen).list = Some(list_cur - 1);
            }
        } else if slot == "list" && delta > 0 {
            // Liste büyürken detay genişlik tercihini daralt (yan düzen).
            let d = self.pane_widths().detail_width;
            let dmin = crate::ui::STORE_DETAIL_PANEL_WIDTH.min(crate::ui::TOOL_DETAIL_PANEL_WIDTH);
            if d > dmin {
                self.state
                    .file
                    .prefs
                    .pane_sizes
                    .get_mut(screen)
                    .detail_width = Some(d - 1);
            }
        }

        match self.persist_state() {
            Ok(()) => {
                self.status_message = format!(
                    "{} {label}: {next}  (−/+ boyut · Tab panel)",
                    self.focus_panel_name()
                );
            }
            Err(e) => {
                self.status_message = format!("Panel boyutu kaydedilemedi: {e}");
            }
        }
    }

    /// Odaklı panelin durum çubuğu adı.
    pub fn focus_panel_name(&self) -> &'static str {
        match self.focus {
            PanelFocus::Category => "Sekmeler",
            PanelFocus::Tool if self.settings_tab => "Ayarlar",
            PanelFocus::Tool => "Araçlar",
            PanelFocus::ToolDetail => "Detay",
            PanelFocus::ToolActions => "Aksiyonlar",
            PanelFocus::StoreCategory => "Mağaza kategorileri",
            PanelFocus::StoreApp => "Uygulamalar",
            PanelFocus::StoreDetail => "Mağaza detay",
            PanelFocus::CommandsSource => "Komut kaynağı",
            PanelFocus::CommandsCategory => "Banka kategorileri",
            PanelFocus::CommandsList => "Komut listesi",
            PanelFocus::CommandsDetail => "Komut detay",
            PanelFocus::ThemePicker => "Tema",
            PanelFocus::LanguagePicker => "Dil",
        }
    }

    /// Kurulu yan-detay düzeni etkin mi?
    pub fn tool_detail_side_layout_active(&self) -> bool {
        let Some((w, _)) = self.terminal_size else {
            return false;
        };
        let tools_tab = !self.store_tab && !self.settings_tab && !self.commands_tab;
        tools_tab && crate::ui::tool_detail_side_layout(true, w, self.pane_widths().tabs)
    }

    /// Komut Rehberi yan detay etkin mi?
    pub fn commands_detail_side_active(&self) -> bool {
        let Some((w, _)) = self.terminal_size else {
            return false;
        };
        let panes = self.pane_widths();
        self.commands_tab
            && w >= crate::ui::WIDE_MIN_WIDTH
            && w >= crate::ui::detail_side_min_width(
                panes.tabs,
                panes.secondary,
                self.pane_secondary_cols(),
            )
    }

    /// Etkin odak zinciri (görsel sıra: soldan sağa, sonra yukarıdan aşağı).
    ///
    /// Dar terminalde gizlenen paneller zincirden çıkarılır. Detay ve alt
    /// aksiyon paneli görünürken zincire eklenir.
    fn focus_chain(&self) -> Vec<PanelFocus> {
        let detail_vis = self.detail_panel_visible();
        if self.commands_tab {
            let mut v = Vec::with_capacity(6);
            if !self.narrow_mode {
                v.push(PanelFocus::Category);
            }
            v.push(PanelFocus::CommandsSource);
            if self.commands_mode.needs_group_column() {
                v.push(PanelFocus::CommandsCategory);
            }
            v.push(PanelFocus::CommandsList);
            if detail_vis {
                v.push(PanelFocus::CommandsDetail);
            }
            return v;
        }
        if self.store_tab {
            let mut v = Vec::with_capacity(5);
            if !self.narrow_mode {
                v.push(PanelFocus::Category);
            }
            v.push(PanelFocus::StoreCategory);
            v.push(PanelFocus::StoreApp);
            if detail_vis {
                v.push(PanelFocus::StoreDetail);
            }
            return v;
        }
        if self.settings_tab {
            let mut v = Vec::with_capacity(3);
            if !self.narrow_mode {
                v.push(PanelFocus::Category);
            }
            v.push(PanelFocus::Tool);
            if self.theme_picker_open {
                v.push(PanelFocus::ThemePicker);
            } else if self.language_picker_open {
                v.push(PanelFocus::LanguagePicker);
            }
            return v;
        }
        // Kurulu araçlar
        let mut v = Vec::with_capacity(4);
        if !self.narrow_mode {
            v.push(PanelFocus::Category);
        }
        v.push(PanelFocus::Tool);
        if self.tool_detail_side_layout_active() {
            v.push(PanelFocus::ToolDetail);
        }
        // AI aksiyonları alt panel değil; Enter → ToolActionsMenu modal.
        if detail_vis && !self.tool_detail_side_layout_active() && !self.settings_tab {
            v.push(PanelFocus::ToolDetail);
        }
        v
    }

    /// Ayarlar yan paneli (tema veya dil) açık mı?
    pub fn settings_side_picker_open(&self) -> bool {
        self.theme_picker_open || self.language_picker_open
    }

    /// Geniş mağazada sağ detay paneli etkin mi?
    pub fn store_detail_side_active(&self) -> bool {
        let Some((w, _)) = self.terminal_size else {
            return false;
        };
        let panes = self.pane_widths();
        self.store_tab
            && w >= crate::ui::WIDE_MIN_WIDTH
            && w >= crate::ui::detail_side_min_width(panes.tabs, panes.secondary, 1)
    }

    /// Seçili mağaza detay bağlantısını sistem tarayıcısında açar (`xdg-open`).
    pub fn open_selected_store_detail_link(&mut self) {
        let links = crate::ui::store_detail_links(self);
        if links.is_empty() {
            self.status_message = "Açılacak https kaynak/depo bağlantısı yok.".into();
            return;
        }
        let idx = self.store_detail_link_index.min(links.len() - 1);
        self.store_detail_link_index = idx;
        let link = &links[idx];
        match crate::browser::open_https_url(&link.url) {
            Ok(()) => {
                self.status_message = format!(
                    "{} tarayıcıda açılıyor (xdg-open): {}",
                    link.label, link.url
                );
            }
            Err(e) => {
                self.status_message = format!("Bağlantı açılamadı: {e}");
            }
        }
    }

    /// Odak zincirinde bir sonraki panele geçer (son panelde kalır; ←→ için).
    fn next_focus(&self) -> PanelFocus {
        let chain = self.focus_chain();
        let idx = chain.iter().position(|f| *f == self.focus).unwrap_or(0);
        chain
            .get(idx + 1)
            .copied()
            .unwrap_or_else(|| *chain.last().expect("odak zinciri boş olamaz"))
    }

    /// Odak zincirinde bir önceki panele geçer (ilk panelde kalır; ←→ için).
    fn prev_focus(&self) -> PanelFocus {
        let chain = self.focus_chain();
        let idx = chain.iter().position(|f| *f == self.focus).unwrap_or(0);
        chain
            .get(idx.saturating_sub(1))
            .copied()
            .unwrap_or(chain[0])
    }

    /// Tab: sonraki panel; sonda başa sarar.
    fn next_focus_wrapping(&self) -> PanelFocus {
        let chain = self.focus_chain();
        let idx = chain.iter().position(|f| *f == self.focus).unwrap_or(0);
        chain[(idx + 1) % chain.len()]
    }

    /// Shift+Tab: önceki panel; başta sona sarar.
    fn prev_focus_wrapping(&self) -> PanelFocus {
        let chain = self.focus_chain();
        let idx = chain.iter().position(|f| *f == self.focus).unwrap_or(0);
        chain[(idx + chain.len() - 1) % chain.len()]
    }

    /// Aramadan sonra odaklanılacak panel.
    fn search_target_focus(&self) -> PanelFocus {
        if self.commands_tab {
            PanelFocus::CommandsList
        } else if self.store_tab {
            PanelFocus::StoreApp
        } else {
            PanelFocus::Tool
        }
    }

    /// Arama sonuç sayısı (etkin panele göre).
    fn search_result_count(&self) -> usize {
        if self.commands_tab {
            self.commands_list_len()
        } else if self.store_tab {
            self.filtered_store_apps().len()
        } else {
            self.filtered_tools().len()
        }
    }

    /// Liste satırında Enter: görev drill veya bilgi (asla çalıştırmaz).
    fn activate_commands_list_item(&mut self) {
        if self.commands_list_shows_tasks() {
            let (task_id, task_name) = {
                let tasks = self.filtered_tasks();
                let Some(task) = tasks
                    .get(self.commands_entry_index.min(tasks.len().saturating_sub(1)))
                    .copied()
                else {
                    self.status_message = "Seçili görev yok.".into();
                    return;
                };
                let lang = self.ui_lang().id();
                (task.id.clone(), task.name.pick(lang).to_string())
            };
            self.commands_task_drill = Some(task_id);
            self.commands_entry_index = 0;
            self.status_message =
                format!("Görev → ilgili komutlar: {task_name} (Esc geri · çalıştırılmaz)");
            return;
        }
        self.show_selected_command_info();
    }

    /// Space / Enter ile profil çoklu seçim (Kullanım bağlamı).
    fn toggle_commands_profile_or_noop(&mut self) {
        if self.commands_mode != CommandsBrowseMode::Context {
            return;
        }
        if self.focus == PanelFocus::CommandsCategory || self.focus == PanelFocus::CommandsList {
            // Liste odaktayken Space yine seçili profil satırını (grup) değiştirmez;
            // grup panelindeyken profili aç/kapa.
            if self.focus == PanelFocus::CommandsCategory {
                self.toggle_selected_profile();
            }
        }
    }

    fn toggle_selected_profile(&mut self) {
        if self.commands_mode != CommandsBrowseMode::Context {
            return;
        }
        let Some(p) = self
            .commands_bank
            .profiles
            .get(self.commands_category_index)
        else {
            return;
        };
        let id = p.id.clone();
        let lang = self.ui_lang().id();
        let name = p.name.pick(lang).to_string();
        let draft = p.verification.status == commands::VerificationStatus::Draft;
        if self.commands_selected_profiles.contains(&id) {
            self.commands_selected_profiles.remove(&id);
            self.status_message = format!("Profil kaldırıldı: {name}");
        } else {
            self.commands_selected_profiles.insert(id);
            let draft_note = if draft {
                " · draft/doğrulanmamış"
            } else {
                ""
            };
            self.status_message = format!("Profil eklendi: {name}{draft_note}");
        }
        self.commands_entry_index = 0;
    }

    /// Seçili banka / yerel / görev hakkında durum satırı bilgisi (çalıştırmaz).
    pub fn show_selected_command_info(&mut self) {
        if self.commands_list_shows_tasks() {
            let tasks = self.filtered_tasks();
            let Some(task) = tasks
                .get(self.commands_entry_index.min(tasks.len().saturating_sub(1)))
                .copied()
            else {
                self.status_message = "Seçili görev yok.".into();
                return;
            };
            let lang = self.ui_lang().id();
            self.status_message = format!(
                "Görev [{}] {} · {} · Enter: ilgili komutlar (çalıştırılmaz)",
                task.category,
                task.name.pick(lang),
                task.verification.status.as_str()
            );
            return;
        }
        match self.commands_source() {
            CommandsSource::Bank => {
                let cmds = self.filtered_bank_commands();
                let Some(cmd) =
                    cmds.get(self.commands_entry_index.min(cmds.len().saturating_sub(1)))
                else {
                    self.status_message = "Bilgi bankasında seçili komut yok.".into();
                    return;
                };
                let lang = self.ui_lang().id();
                self.status_message = format!(
                    "Bank [{}] {} · risk={} · doğrulama={} · {}",
                    cmd.category,
                    cmd.name,
                    cmd.risk.as_str(),
                    cmd.verification.status.as_str(),
                    cmd.summary.pick(lang)
                );
            }
            CommandsSource::Local => {
                let ents = self.filtered_local_commands();
                let Some(ent) =
                    ents.get(self.commands_entry_index.min(ents.len().saturating_sub(1)))
                else {
                    self.status_message =
                        "Bu cihazda (PATH) eşleşen giriş yok — bank doğrulaması değil.".into();
                    return;
                };
                let bank_note = match &ent.bank_id {
                    Some(id) => format!("bank_id={id} (eşleşme; verified miras yok)"),
                    None => "bank eşleşmesi yok".into(),
                };
                self.status_message = format!(
                    "Cihazda bulundu: {} → {} · {} · kind={}",
                    ent.name,
                    ent.path,
                    bank_note,
                    commands::LOCAL_KIND
                );
            }
        }
    }

    /// Seçili aracı yeni bir terminal sekmesinde başlatmayı ister.
    ///
    /// Hata olsa bile Nova çalışmaya devam eder: başlatma hatası TUI'yi
    /// bozmaz, yalnızca durum satırında görünür. Başarı mesajı bilinçli
    /// olarak "istendi" der; aracın gerçekten çalıştığını Nova doğrulayamaz.
    ///
    /// AI aksiyonları olan araçlarda Enter Mağaza kurulu menüsü gibi
    /// ortalanmış aksiyon modalını açar; diğerlerinde doğrudan başlatır.
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
        if entry.has_ai_actions() {
            self.open_tool_actions_menu(&entry);
            return;
        }
        self.launch_catalog_entry(entry);
    }

    /// Kurulu AI aracı için Mağaza tarzı aksiyon modalını açar.
    pub fn open_tool_actions_menu(&mut self, entry: &crate::catalog::CatalogEntry) {
        if !entry.has_ai_actions() {
            self.status_message = "Bu araç için aksiyon yok.".into();
            return;
        }
        self.tool_action_index = 0;
        self.modal = modal::Modal::ToolActionsMenu {
            tool_id: entry.id.clone(),
            name: entry.name.clone(),
            index: 0,
        };
    }

    /// Modal / eski odak yolu: seçili AI eylemini başlatır.
    pub fn select_tool_action(&mut self) {
        let Some(entry) = self.selected_tool().cloned() else {
            self.status_message = "Başlatılacak araç yok.".into();
            return;
        };
        if !entry.has_ai_actions() {
            self.focus = PanelFocus::Tool;
            self.status_message = "Bu araç için aksiyon yok.".into();
            return;
        }
        let idx = self
            .tool_action_index
            .min(entry.actions.len().saturating_sub(1));
        let Some(action) = entry.actions.get(idx).cloned() else {
            self.status_message = "Başlatılacak aksiyon yok.".into();
            return;
        };
        self.launch_catalog_entry(entry.with_action(&action));
    }

    /// Verilen araç kimliği ve aksiyon indeksinden başlatır (modal Enter).
    fn launch_tool_action_by_id(&mut self, tool_id: &str, index: usize) {
        let Some(entry) = self.catalog.get_by_id(tool_id).cloned() else {
            self.status_message = format!("Araç '{tool_id}' katalogda yok.");
            return;
        };
        if entry.actions.is_empty() {
            self.status_message = "Bu araç için aksiyon yok.".into();
            return;
        }
        let idx = index.min(entry.actions.len().saturating_sub(1));
        let Some(action) = entry.actions.get(idx).cloned() else {
            self.status_message = "Başlatılacak aksiyon yok.".into();
            return;
        };
        self.tool_action_index = idx;
        self.launch_catalog_entry(entry.with_action(&action));
    }

    /// Seçili araç AI aksiyonları taşıyor mu?
    pub fn selected_tool_has_ai_actions(&self) -> bool {
        self.selected_tool()
            .map(|e| e.has_ai_actions())
            .unwrap_or(false)
    }

    fn selected_tool_actions_len(&self) -> usize {
        self.selected_tool().map(|e| e.actions.len()).unwrap_or(0)
    }

    fn reset_tool_action_index(&mut self) {
        self.tool_action_index = 0;
        if self.focus == PanelFocus::ToolActions && !self.selected_tool_has_ai_actions() {
            self.focus = PanelFocus::Tool;
        }
    }

    /// Ayarlardaki varsayılan AI CLI aracını başlatır (`a` kısayolu).
    ///
    /// Tercih yoksa veya araç artık uygun değilse başlatıcı çağrılmaz; durum
    /// satırında ne yapılacağı söylenir.
    pub fn launch_default_ai_tool(&mut self) {
        let Some(id) = self.state.file.prefs.default_ai_tool.clone() else {
            self.status_message =
                "Varsayılan AI aracı seçilmedi. Ayarlar → Gelişmiş'ten ayarla.".into();
            return;
        };
        let Some(entry) = self.catalog.get_by_id(&id).cloned() else {
            self.status_message =
                format!("Varsayılan AI '{id}' katalogda yok; Ayarlar → Gelişmiş'ten güncelle.");
            return;
        };
        if entry.category != "AI ve Kodlama" {
            self.status_message =
                format!("'{id}' AI kategorisinde değil; varsayılan AI başlatılmadı.");
            return;
        }
        self.launch_catalog_entry(entry);
    }

    /// Katalog girdisini kurulum durumuna göre başlatır (veya reddeder).
    fn launch_catalog_entry(&mut self, entry: crate::catalog::CatalogEntry) {
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

    /// Etkin arayüz dili (`prefs.language`).
    pub fn ui_lang(&self) -> crate::i18n::Language {
        crate::i18n::Language::parse(self.state.file.prefs.language.as_deref().unwrap_or(""))
    }

    /// Temayı bir sonraki değere alır ve state.toml'a yazar (geriye dönük).
    pub fn cycle_theme(&mut self) -> Result<crate::ui::UiTheme, String> {
        self.set_theme(self.ui_theme().next())
    }

    /// Seçilen temayı uygular ve kalıcılaştırır.
    pub fn set_theme(&mut self, theme: crate::ui::UiTheme) -> Result<crate::ui::UiTheme, String> {
        self.state.file.prefs.theme = Some(theme.id().to_string());
        self.persist_state()
            .map_err(|e| format!("Tema kaydedilemedi: {e}"))?;
        Ok(theme)
    }

    /// Seçilen dili uygular ve kalıcılaştırır.
    pub fn set_language(
        &mut self,
        language: crate::i18n::Language,
    ) -> Result<crate::i18n::Language, String> {
        self.state.file.prefs.language = Some(language.id().to_string());
        self.persist_state()
            .map_err(|e| format!("Dil kaydedilemedi: {e}"))?;
        Ok(language)
    }

    /// Tema seçici yan panelini açar.
    pub fn open_theme_picker(&mut self) {
        self.language_picker_open = false;
        self.theme_picker_open = true;
        self.theme_picker_index = self.ui_theme().index();
        self.settings_action_index = 4;
        self.focus = PanelFocus::ThemePicker;
        self.status_message = self.ui_lang().t(crate::i18n::Msg::ThemePickHint).into();
    }

    /// Tema seçiciyi kapatır (Ayarlar listesine döner).
    pub fn close_theme_picker(&mut self) {
        self.theme_picker_open = false;
        if self.focus == PanelFocus::ThemePicker {
            self.focus = PanelFocus::Tool;
        }
        self.status_message = self.ui_lang().t(crate::i18n::Msg::ThemeClosed).into();
    }

    /// Dil seçici yan panelini açar.
    pub fn open_language_picker(&mut self) {
        self.theme_picker_open = false;
        self.language_picker_open = true;
        self.language_picker_index = self.ui_lang().index();
        self.settings_action_index = 5;
        self.focus = PanelFocus::LanguagePicker;
        self.status_message = self.ui_lang().t(crate::i18n::Msg::LanguagePickHint).into();
    }

    /// Dil seçiciyi kapatır (Ayarlar listesine döner).
    pub fn close_language_picker(&mut self) {
        self.language_picker_open = false;
        if self.focus == PanelFocus::LanguagePicker {
            self.focus = PanelFocus::Tool;
        }
        self.status_message = self.ui_lang().t(crate::i18n::Msg::LanguageClosed).into();
    }

    /// Seçici satırındaki temayı uygular.
    pub fn apply_theme_at_picker_index(&mut self) -> Result<crate::ui::UiTheme, String> {
        let themes = crate::ui::UiTheme::all();
        let theme = themes
            .get(self.theme_picker_index.min(themes.len().saturating_sub(1)))
            .copied()
            .unwrap_or_default();
        match self.set_theme(theme) {
            Ok(theme) => {
                let lang = self.ui_lang();
                self.status_message = format!(
                    "{}: {} ({})",
                    lang.t(crate::i18n::Msg::ThemeSaved),
                    theme.label_for(lang),
                    theme.id()
                );
                Ok(theme)
            }
            Err(e) => {
                self.status_message =
                    format!("{} {e}", self.ui_lang().t(crate::i18n::Msg::ErrorPrefix));
                Err(e)
            }
        }
    }

    /// Seçici satırındaki dili uygular.
    pub fn apply_language_at_picker_index(&mut self) -> Result<crate::i18n::Language, String> {
        let languages = crate::i18n::Language::all();
        let language = languages
            .get(
                self.language_picker_index
                    .min(languages.len().saturating_sub(1)),
            )
            .copied()
            .unwrap_or_default();
        match self.set_language(language) {
            Ok(language) => {
                self.status_message = format!(
                    "{}: {} ({})",
                    language.t(crate::i18n::Msg::LanguageSaved),
                    language.native_name(),
                    language.id()
                );
                Ok(language)
            }
            Err(e) => {
                self.status_message =
                    format!("{} {e}", self.ui_lang().t(crate::i18n::Msg::ErrorPrefix));
                Err(e)
            }
        }
    }

    /// Nova güncelleme modalını açar ve denetimi hemen çalıştırır (Ayarlar / `U`).
    pub fn start_nova_update_check(&mut self) {
        self.modal = modal::Modal::UpdateCheck {
            status: modal::UpdateStatus::Idle,
            check: None,
            error: None,
        };
        self.run_nova_update_check();
    }

    /// GitHub Releases üzerinden Nova ikili self-update denetimini çalıştırır.
    ///
    /// Bu yol `cargo build` / kaynak güncellemesi değildir; yalnızca yayınlanmış
    /// ikili varsa `~/.local/bin/nova` kurulumunu önerir. Sonuç hem modalda hem
    /// durum satırında görünür (sessiz no-op yok).
    pub fn run_nova_update_check(&mut self) {
        if !matches!(self.modal, modal::Modal::UpdateCheck { .. }) {
            self.modal = modal::Modal::UpdateCheck {
                status: modal::UpdateStatus::Checking,
                check: None,
                error: None,
            };
        }
        if let modal::Modal::UpdateCheck {
            status,
            check,
            error,
        } = &mut self.modal
        {
            *status = modal::UpdateStatus::Checking;
            *check = None;
            *error = None;
        }
        self.status_message =
            "Nova güncellemesi: GitHub Releases denetleniyor (repo main için G)…".into();

        let provider = self.update_provider.as_ref();
        let current = provider.current_version();
        let fetched = provider.fetch_latest_release();

        let (result, status_msg) = match fetched {
            Ok(Some(release)) => {
                let latest = release.version.clone();
                if release.version > current {
                    let msg = format!(
                        "Yeni Nova sürümü: v{latest} (şu an v{current}). Enter ile kurulum onayı."
                    );
                    (
                        UpdateCheck::UpdateAvailable {
                            current,
                            latest,
                            release: Box::new(release),
                        },
                        msg,
                    )
                } else {
                    (
                        UpdateCheck::UpToDate { current: current.clone() },
                        format!(
                            "Nova Releases’a göre güncel (v{current}). Repo main için G: cargo install --git."
                        ),
                    )
                }
            }
            Ok(None) => (
                UpdateCheck::NoPublishedRelease {
                    current: current.clone(),
                },
                format!(
                    "GitHub’da Nova ikili yayını yok (v{current}). G: repo main’den cargo install --git (ağ+derleme)."
                ),
            ),
            Err(e) => {
                let msg = format!("Güncelleme denetimi başarısız: {e}");
                (UpdateCheck::Error(e), msg)
            }
        };

        if let modal::Modal::UpdateCheck {
            status,
            check,
            error,
        } = &mut self.modal
        {
            match &result {
                UpdateCheck::Error(e) => {
                    *error = Some(e.clone());
                    *check = Some(result);
                }
                other => {
                    *error = None;
                    *check = Some(other.clone());
                }
            }
            *status = modal::UpdateStatus::Done;
        }
        self.status_message = status_msg;
    }

    /// Ayarlar sekmesindeki seçili eylemi uygular.
    pub fn activate_settings_action(&mut self) {
        match self.settings_action_index {
            0 => self.start_nova_update_check(),
            1 => self.start_store_catalog_refresh(),
            2 => self.start_commands_bank_refresh(),
            3 => {
                self.modal = modal::Modal::ToolForm(Box::default());
                self.status_message =
                    "Özel araç: ad, program, kategori… Esc iptal, Enter gözden geçir.".into();
            }
            4 => self.open_theme_picker(),
            _ => self.open_language_picker(),
        }
    }

    /// Mağaza kataloğunu imzalı Releases + Nova-Store repo (main) ile yeniler.
    pub fn start_store_catalog_refresh(&mut self) {
        self.status_message = "Mağaza kataloğu yenileniyor (imzalı Releases + repo main)…".into();
        let (store, note) = crate::store::repo_sync::refresh_signed_then_repo(&self.store);
        self.store = store;
        self.refresh_store_status();
        self.status_message = note;
    }

    /// Komut bankasını Nova-Commands repo (main) ile yeniler (imzasız HTTPS + SHA).
    ///
    /// Mağaza kataloğundan ayrıdır; `kind: nova-commands-bank` korunur.
    pub fn start_commands_bank_refresh(&mut self) {
        self.status_message =
            "Komut bankası yenileniyor (Nova-Commands repo main; imzasız HTTPS)…".into();
        let current_sha = self
            .commands_bank
            .content_sha256
            .clone()
            .or_else(commands::loader::embedded_content_sha256);
        let (fetched, sync) = commands::refresh_commands_from_repo(current_sha.as_deref());
        if let Some(fetched) = fetched {
            self.commands_bank = fetched.bank;
            self.commands_search_index = fetched.search_index;
            self.commands_category_index = 0;
            self.commands_entry_index = 0;
            self.refresh_local_commands_stub();
        }
        self.status_message = commands::format_sync_note(&sync);
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
                .unwrap_or_default(),
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
            language: self.ui_lang().id().to_string(),
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
        let host_arch = crate::store::CpuArch::host();
        let matches_arch = |e: &&StoreEntry| match host_arch {
            Some(arch) => e.supports_arch(arch),
            None => true,
        };
        if searching {
            self.store
                .search(&self.search_query)
                .into_iter()
                .filter(matches_arch)
                .collect()
        } else {
            self.store
                .by_category(category)
                .into_iter()
                .filter(matches_arch)
                .collect()
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
    /// 2. **Kurulu** uygulamada yeniden kurulum sorulmaz; kaldırma /
    ///    güncelleme denetimi menüsü açılır.
    /// 3. Doğrulanabilir özeti olmayan kayıtta Nova kurulum yapmaz; resmî
    ///    yönergeyi gösterir.
    /// 4. Diğer hâllerde kurulum onayı sorulur.
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
        let installed =
            self.store_status(&entry.id) == crate::detection::InstalledStatus::Installed;

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
        } else if installed {
            // Kurulu: yeniden kurma yok — kaldır veya güncelleme denetle.
            self.modal = modal::Modal::StoreInstalledMenu {
                id: entry.id.clone(),
                name: entry.name.clone(),
                index: 0,
            };
        } else if !entry.install_method.is_installable() {
            // curl|bash vb. manual kayıtlarda "Kur" sunulmaz; talimat gösterilir.
            let note = entry
                .install_notes
                .as_deref()
                .unwrap_or("resmî kurulum yönergelerini takip edin");
            self.status_message = format!("'{}' için {note}", entry.name);
        } else {
            if let Some(arch) = crate::store::CpuArch::host() {
                if !entry.supports_arch(arch) {
                    self.status_message = format!(
                        "'{}' bu mimaride ({}) desteklenmiyor.",
                        entry.name,
                        arch.as_str()
                    );
                    return;
                }
            }
            let confirm = crate::store::install::prepare_confirmation(entry);
            self.modal = modal::Modal::Confirm {
                question: confirm.question_text(),
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

    /// İlerleme modalını açar ve arka plan işi kaydını hazırlar.
    fn begin_job(
        &mut self,
        kind: crate::progress::JobKind,
        title: String,
        command: String,
        app_id: String,
        app_name: String,
        is_uninstall: bool,
    ) -> (
        mpsc::Sender<JobResult>,
        mpsc::Sender<crate::progress::ProgressEvent>,
    ) {
        let (result_tx, result_rx) = mpsc::channel();
        let (progress_tx, progress_rx) = crate::progress::progress_channel();
        self.modal = modal::Modal::JobProgress {
            kind,
            title: title.clone(),
            command: command.clone(),
            phase: "Başlıyor".into(),
            status_line: "hazırlanıyor…".into(),
            log: vec![format!("$ {command}")],
            log_scroll: 0,
            spinner: 0,
            done: None,
        };
        self.status_message = format!("{title}…");
        self.install_job = Some(InstallJob {
            kind,
            app_id,
            app_name,
            command,
            is_uninstall,
            receiver: result_rx,
            progress_rx,
        });
        (result_tx, progress_tx)
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
        if let Some(arch) = crate::store::CpuArch::host() {
            if !entry.supports_arch(arch) {
                self.status_message = format!(
                    "'{}' bu mimaride ({}) desteklenmiyor.",
                    entry.name,
                    arch.as_str()
                );
                return;
            }
        }

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
                    operation: crate::store::install::PmOperation::Install,
                });
                self.status_message =
                    format!("'{app_name}' kuruluyor ({method_label}, sudo TTY): {cmd}...");
                return;
            }
            let plan_bg = plan.clone();
            let (tx, progress_tx) = self.begin_job(
                crate::progress::JobKind::StoreInstall,
                format!("'{app_name}' kuruluyor"),
                cmd,
                id.to_string(),
                app_name,
                false,
            );
            std::thread::spawn(move || {
                let _ = progress_tx.send(crate::progress::ProgressEvent::Phase(format!(
                    "{method_label} çalışıyor"
                )));
                let mut on_line = |line: &str| crate::progress::emit_line(&progress_tx, line);
                let result = crate::store::install::run_package_manager_argv_with_progress(
                    &entry,
                    &plan_bg,
                    crate::store::install::PmOperation::Install,
                    crate::store::install::PmIoMode::Captured,
                    &mut on_line,
                )
                .map_err(|e| e.to_string());
                let _ = tx.send(JobResult::Store(result));
            });
            return;
        }

        let Some(dest_dir) = crate::store::store_app_dir(&entry.id) else {
            self.status_message = "Kurulum dizini belirlenemedi ($HOME bulunamadı).".into();
            return;
        };

        let (tx, progress_tx) = self.begin_job(
            crate::progress::JobKind::StoreInstall,
            format!("'{app_name}' kuruluyor"),
            format!("archive → {}", dest_dir.display()),
            id.to_string(),
            app_name,
            false,
        );
        std::thread::spawn(move || {
            let _ = progress_tx.send(crate::progress::ProgressEvent::Phase("İndiriliyor".into()));
            let _ = progress_tx.send(crate::progress::ProgressEvent::Status(
                "SHA-256 doğrulamasıyla arşiv kurulumu…".into(),
            ));
            let downloader = crate::store::http::HttpDownloader::new();
            let result = crate::store::install::install_app(&entry, &dest_dir, &downloader)
                .map_err(|e| e.to_string());
            let _ = tx.send(JobResult::Store(result));
        });
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
            let app_name = entry.name.clone();
            let cmd = pm.rendered();
            let method_label = pm.method.short_label().to_string();
            let plan = pm.as_install_plan();
            if plan.needs_tty_suspend() {
                self.pending_tty_pm = Some(PendingTtyPm {
                    entry,
                    plan,
                    operation: crate::store::install::PmOperation::Upgrade,
                });
                self.status_message =
                    format!("'{app_name}' güncelleniyor ({method_label}, sudo TTY): {cmd}...");
                return;
            }
            let (tx, progress_tx) = self.begin_job(
                crate::progress::JobKind::StoreUpdate,
                format!("'{app_name}' güncelleniyor"),
                cmd,
                id.to_string(),
                app_name,
                false,
            );
            std::thread::spawn(move || {
                let _ = progress_tx.send(crate::progress::ProgressEvent::Phase(format!(
                    "{method_label} güncellemesi"
                )));
                let mut on_line = |line: &str| crate::progress::emit_line(&progress_tx, line);
                let result = crate::store::install::run_package_manager_argv_with_progress(
                    &entry,
                    &plan,
                    crate::store::install::PmOperation::Upgrade,
                    crate::store::install::PmIoMode::Captured,
                    &mut on_line,
                )
                .map_err(|e| e.to_string());
                let _ = tx.send(JobResult::Store(result));
            });
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

        let app_name = entry.name.clone();
        let version = release.version.clone();
        let (tx, progress_tx) = self.begin_job(
            crate::progress::JobKind::StoreUpdate,
            format!("'{app_name}' → {version}"),
            format!("release → {}", dest_dir.display()),
            id.to_string(),
            app_name,
            false,
        );
        std::thread::spawn(move || {
            let _ = progress_tx.send(crate::progress::ProgressEvent::Phase("İndiriliyor".into()));
            let _ = progress_tx.send(crate::progress::ProgressEvent::Status(format!(
                "{version} sürümü SHA-256 ile kuruluyor…"
            )));
            let downloader = crate::store::http::HttpDownloader::new();
            let result =
                crate::store::install::install_release(&entry, &dest_dir, &release, &downloader)
                    .map_err(|e| e.to_string());
            let _ = tx.send(JobResult::Store(result));
        });
    }

    /// Nova self-update başarısını işler: ikiliyi doğrular, durumu kaydeder,
    /// geçerliyse temiz çıkış + yeniden başlatmayı planlar.
    ///
    /// Doğrulama başarısızsa TUI açık kalır; kullanıcı elle yeniden açabilir.
    fn apply_nova_update_success(&mut self, outcome: crate::update::UpdateOutcome) {
        let summary = outcome.summary.clone();
        match crate::update::validate_restart_binary(&outcome.binary_path) {
            Ok(binary) => {
                let _ = self.persist_state();
                let restart_msg = format!("{summary} Yeniden başlatılıyor…");
                self.status_message = format!("✓ {restart_msg}");
                if let modal::Modal::JobProgress {
                    phase,
                    status_line,
                    done,
                    ..
                } = &mut self.modal
                {
                    *phase = "Yeniden başlatılıyor".into();
                    *status_line = restart_msg.clone();
                    *done = Some(Ok(restart_msg));
                }
                self.pending_self_restart = Some(binary);
                self.running = false;
            }
            Err(err) => {
                let msg = format!("{summary} Otomatik yeniden başlatma yapılamadı: {err}");
                self.status_message = format!("✓ {msg}");
                if let modal::Modal::JobProgress {
                    phase,
                    status_line,
                    done,
                    ..
                } = &mut self.modal
                {
                    *phase = "Tamamlandı".into();
                    *status_line = msg.clone();
                    *done = Some(Ok(msg));
                }
                self.pending_self_restart = None;
            }
        }
    }

    /// Arka plandaki kurulum işinin durumunu kontrol eder.
    ///
    /// İş tamamlandıysa sonucu durum satırına yazar ve katalogdaki tespit
    /// durumlarını tazeler (kurulan uygulama artık "kurulu" görünür).
    pub fn poll_install(&mut self) {
        let Some(job) = self.install_job.take() else {
            return;
        };

        // Canlı ilerleme olaylarını modağa aktar.
        while let Ok(ev) = job.progress_rx.try_recv() {
            if let modal::Modal::JobProgress {
                phase,
                status_line,
                log,
                spinner,
                ..
            } = &mut self.modal
            {
                match ev {
                    crate::progress::ProgressEvent::Phase(p) => *phase = p,
                    crate::progress::ProgressEvent::Status(s) => *status_line = s,
                    crate::progress::ProgressEvent::Log(line) => {
                        log.push(line);
                        if log.len() > crate::progress::JOB_LOG_CAP {
                            let drop_n = log.len() - crate::progress::JOB_LOG_CAP;
                            log.drain(0..drop_n);
                        }
                    }
                }
                *spinner = spinner.wrapping_add(1);
            }
        }

        // Spinner tick (olay olmasa da).
        if let modal::Modal::JobProgress {
            spinner,
            done: None,
            ..
        } = &mut self.modal
        {
            *spinner = spinner.wrapping_add(1);
        }

        match job.receiver.try_recv() {
            Ok(JobResult::Store(Ok(outcome))) => {
                let summary = outcome.summary.clone();
                if job.is_uninstall {
                    self.apply_uninstall_outcome(outcome);
                } else {
                    self.apply_install_outcome(outcome);
                }
                if let modal::Modal::JobProgress {
                    phase,
                    status_line,
                    done,
                    ..
                } = &mut self.modal
                {
                    *phase = "Tamamlandı".into();
                    *status_line = summary.clone();
                    *done = Some(Ok(summary));
                }
            }
            Ok(JobResult::Store(Err(message))) => {
                self.status_message = format!("Hata: {message}");
                if let modal::Modal::JobProgress {
                    phase,
                    status_line,
                    done,
                    ..
                } = &mut self.modal
                {
                    *phase = "Hata".into();
                    *status_line = message.clone();
                    *done = Some(Err(message));
                }
            }
            Ok(JobResult::Nova(Ok(outcome))) => {
                crate::store::path::ensure_local_bin_on_process_path();
                self.apply_nova_update_success(outcome);
            }
            Ok(JobResult::Nova(Err(message))) => {
                self.status_message = format!("Hata: {message}");
                if let modal::Modal::JobProgress {
                    phase,
                    status_line,
                    done,
                    ..
                } = &mut self.modal
                {
                    *phase = "Hata".into();
                    *status_line = message.clone();
                    *done = Some(Err(message));
                }
            }
            Err(TryRecvError::Empty) => {
                // İş hâlâ sürüyor.
                self.install_job.replace(job);
            }
            Err(TryRecvError::Disconnected) => {
                self.status_message = "Hata: kurulum işi beklenmedik şekilde sonlandı.".into();
                if let modal::Modal::JobProgress {
                    phase,
                    status_line,
                    done,
                    ..
                } = &mut self.modal
                {
                    *phase = "Hata".into();
                    *status_line = "iş beklenmedik şekilde sonlandı".into();
                    *done = Some(Err("iş beklenmedik şekilde sonlandı".into()));
                }
            }
        }
    }

    /// Kurulu mağaza uygulamasını kaldırır (arşiv dizini veya PM uninstall).
    pub fn start_store_uninstall(&mut self, id: &str) {
        if self.install_job.is_some() || self.pending_tty_pm.is_some() {
            self.status_message = "Zaten süren bir işlem var; önce onun bitmesini bekle.".into();
            return;
        }
        let Some(entry) = self.store.get_by_id(id).cloned() else {
            self.status_message = format!("Mağazada '{}' bulunamadı.", id);
            return;
        };
        let app_name = entry.name.clone();

        if entry.install_method.is_archive() {
            let (tx, progress_tx) = self.begin_job(
                crate::progress::JobKind::StoreUninstall,
                format!("'{app_name}' kaldırılıyor"),
                format!("archive uninstall → {id}"),
                id.to_string(),
                app_name.clone(),
                true,
            );
            std::thread::spawn(move || {
                let _ =
                    progress_tx.send(crate::progress::ProgressEvent::Phase("Kaldırılıyor".into()));
                let result =
                    crate::store::install::uninstall_archive_app(&entry).map_err(|e| e.to_string());
                let _ = tx.send(JobResult::Store(result));
            });
            return;
        }

        if entry.install_method.is_package_manager() {
            let host = self
                .store_host_tools_override
                .unwrap_or_else(host::HostTools::detect);
            let os = host::detect_host_os();
            let Some(plan) = host::resolve_uninstall_plan(&entry, &host, os) else {
                self.status_message =
                    format!("'{app_name}' için kaldırma komutu üretilemedi; elle kaldırın.");
                return;
            };
            let cmd = plan.rendered();
            let method_label = plan.method.short_label().to_string();
            if plan.needs_tty_suspend() {
                self.pending_tty_pm = Some(PendingTtyPm {
                    entry,
                    plan,
                    operation: crate::store::install::PmOperation::Uninstall,
                });
                self.status_message =
                    format!("'{app_name}' kaldırılıyor ({method_label}, sudo TTY): {cmd}...");
                return;
            }
            let plan_bg = plan.clone();
            let (tx, progress_tx) = self.begin_job(
                crate::progress::JobKind::StoreUninstall,
                format!("'{app_name}' kaldırılıyor"),
                cmd.clone(),
                id.to_string(),
                app_name,
                true,
            );
            std::thread::spawn(move || {
                let _ = progress_tx.send(crate::progress::ProgressEvent::Phase(format!(
                    "{method_label} kaldırma"
                )));
                let mut on_line = |line: &str| crate::progress::emit_line(&progress_tx, line);
                let result = crate::store::install::run_package_manager_argv_with_progress(
                    &entry,
                    &plan_bg,
                    crate::store::install::PmOperation::Uninstall,
                    crate::store::install::PmIoMode::Captured,
                    &mut on_line,
                )
                .map_err(|e| e.to_string());
                let _ = tx.send(JobResult::Store(result));
            });
            return;
        }

        self.status_message =
            format!("'{app_name}' Nova ile kaldırılamaz (manual kayıt); resmi yönergeyi izleyin.");
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

    /// Kaldırma sonucunu mağaza / katalog durumuna işler.
    fn apply_uninstall_outcome(&mut self, outcome: crate::store::install::InstallOutcome) {
        self.store_updates.remove(&outcome.app_id);
        self.store_pm_updates.remove(&outcome.app_id);
        self.store_check_errors.remove(&outcome.app_id);
        // Mağazadan senkron eklenen özel araç kaydını temizle.
        let was_custom = self
            .state
            .file
            .custom_tools
            .iter()
            .any(|t| t.id == outcome.app_id);
        if was_custom {
            self.state
                .file
                .custom_tools
                .retain(|t| t.id != outcome.app_id);
            let _ = self.catalog.remove_entry(&outcome.app_id);
            let _ = self.persist_state();
        }
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
        let verb = match pending.operation {
            crate::store::install::PmOperation::Upgrade => "güncelleniyor",
            crate::store::install::PmOperation::Uninstall => "kaldırılıyor",
            crate::store::install::PmOperation::Install => "kuruluyor",
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
            pending.operation,
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
            Ok(outcome) => {
                if pending.operation == crate::store::install::PmOperation::Uninstall {
                    self.apply_uninstall_outcome(outcome);
                } else {
                    self.apply_install_outcome(outcome);
                }
            }
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
        let host_arch = crate::store::CpuArch::host();
        let sources: Vec<(String, ReleaseSource)> = self
            .store
            .entries()
            .iter()
            .filter(|e| host_arch.is_none_or(|a| e.supports_arch(a)))
            .filter_map(|e| {
                let src = match host_arch {
                    Some(arch) => e.release_source_for(arch),
                    None => e.release.clone(),
                }?;
                Some((e.id.clone(), src))
            })
            .collect();
        let pm_entries: Vec<StoreEntry> = self
            .store
            .entries()
            .iter()
            .filter(|e| e.install_method.is_package_manager())
            .filter(|e| host_arch.is_none_or(|a| e.supports_arch(a)))
            .cloned()
            .collect();
        let installed_ids: std::collections::BTreeSet<String> = self
            .store_status
            .iter()
            .filter(|(_, s)| **s == crate::detection::InstalledStatus::Installed)
            .map(|(id, _)| id.clone())
            .collect();
        let (tx, rx) = mpsc::channel();
        let provider = Arc::clone(&self.store_release_provider);
        let cache_path = self.store_cache_path.clone().or_else(manifest::cache_path);
        let outdated_override = self.store_outdated_override.clone();
        let host_override = self.store_host_tools_override;
        std::thread::spawn(move || {
            // İmzalı uzak katalog: doğrulanırsa uygulanır; aksi hâlde yok sayılır.
            let (signed_catalog, catalog_note) =
                match crate::store::remote::try_refresh_from_remote(None) {
                    Ok((store, crate::store::remote::CatalogRefresh::Updated { entries })) => (
                        Some(store),
                        Some(format!(
                            "Nova-Store kataloğu imzalı güncellendi ({entries} kayıt)."
                        )),
                    ),
                    Ok((_, crate::store::remote::CatalogRefresh::KeptTrusted { reason })) => {
                        (None, Some(format!("Katalog korundu: {reason}")))
                    }
                    Err(reason) => (None, Some(format!("Katalog: uzak yok/imza yok — {reason}"))),
                };

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
                signed_catalog,
                catalog_note,
            });
        });

        self.store_check_job = Some(StoreCheckJob { receiver: rx });
        self.status_message =
            "Mağaza kataloğu (imza), sürümler ve PM güncellemeleri denetleniyor...".into();
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
                if let Some(store) = outcome.signed_catalog {
                    self.store = store;
                    self.refresh_store_status();
                    self.sync_installed_store_apps_to_catalog();
                    // Seçim indekslerini güvenli aralığa çek.
                    let cats = STORE_CATEGORIES.len();
                    if cats > 0 {
                        self.store_category_index = self.store_category_index.min(cats - 1);
                    }
                    let apps = self.filtered_store_apps().len();
                    if apps > 0 {
                        self.store_app_index = self.store_app_index.min(apps - 1);
                    } else {
                        self.store_app_index = 0;
                    }
                }
                self.store_updates = updates;
                self.store_pm_updates = outcome.pm_updates;
                self.store_check_errors = errors;
                let mut status = match (newer, pm_count) {
                    (0, 0) if self.store_check_errors.is_empty() => {
                        "Mağaza uygulamalarının tümü güncel.".into()
                    }
                    (0, 0) => format!(
                        "{} uygulama denetlenemedi; diğerleri güncel.",
                        self.store_check_errors.len()
                    ),
                    (n, 0) => format!("{n} mağaza uygulaması için yeni sürüm bulundu."),
                    (0, p) => format!("{p} paket yöneticisi güncellemesi bulundu."),
                    (n, p) => {
                        format!("{n} yeni sürüm, {p} paket yöneticisi güncellemesi bulundu.")
                    }
                };
                if let Some(note) = outcome.catalog_note {
                    // Uzak katalog henüz yokken (404) gürültüyü kıs: yalnızca
                    // başarılı güncellemede veya açık hata dışı kısa not.
                    if note.contains("imzalı güncellendi") {
                        status = format!("{note} {status}");
                    }
                }
                self.status_message = status;
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
                    // `begin_job` ilerleme modalını `self.modal`e yazar; onu koru.
                    if matches!(self.modal, modal::Modal::JobProgress { .. }) {
                        take(std::mem::replace(&mut self.modal, modal::Modal::None));
                    } else {
                        take(modal::Modal::None);
                    }
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
                language,
                focus,
                error,
            } => match code {
                KeyCode::Esc => {
                    self.status_message =
                        self.ui_lang().t(crate::i18n::Msg::SettingsUnchanged).into();
                    take(modal::Modal::None);
                }
                KeyCode::Tab | KeyCode::Down => *focus = (*focus + 1) % 7,
                KeyCode::BackTab | KeyCode::Up => *focus = (*focus + 6) % 7,
                KeyCode::Left if *focus == 6 => {
                    *language = crate::i18n::Language::parse(language).prev().id().into();
                    *error = None;
                }
                KeyCode::Right if *focus == 6 => {
                    *language = crate::i18n::Language::parse(language).next().id().into();
                    *error = None;
                }
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
                        6 => {
                            language.pop();
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
                    language,
                ) {
                    Ok(()) => {
                        self.status_message =
                            self.ui_lang().t(crate::i18n::Msg::SettingsSaved).into();
                        take(modal::Modal::None);
                    }
                    Err(message) => {
                        self.status_message = format!(
                            "{} {message}",
                            self.ui_lang().t(crate::i18n::Msg::ErrorPrefix)
                        );
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
                        6 => language.push(c),
                        _ => {}
                    }
                    *error = None;
                }
                _ => {}
            },
            modal::Modal::UpdateCheck { check, .. } => match code {
                KeyCode::Esc => {
                    self.status_message = "Güncelleme denetimi kapatıldı.".into();
                    take(modal::Modal::None);
                }
                KeyCode::Enter => {
                    // `next` alınmış durumda; sonucu `check` üzerinden oku
                    // (`self.modal` şu an None).
                    let available = match check {
                        Some(UpdateCheck::UpdateAvailable {
                            current,
                            latest,
                            release,
                        }) => Some((current.clone(), latest.clone(), release.as_ref().clone())),
                        _ => None,
                    };
                    if let Some((current, latest, release)) = available {
                        take(modal::Modal::UpdateConfirm {
                            current,
                            latest,
                            release,
                        });
                    } else {
                        // Idle / güncel / yayın yok / hata: Enter yeniden denetler.
                        self.run_nova_update_check();
                        take(std::mem::replace(&mut self.modal, modal::Modal::None));
                    }
                }
                KeyCode::Char('g') | KeyCode::Char('G') => {
                    let plan = crate::update::RepoUpdatePlan::for_nova_main();
                    take(modal::Modal::Confirm {
                        question: format!(
                            "Nova kaynak güncellemesi (repo {})?\n{}\nAğ + derleme gerekir. Enter onay, Esc vazgeç.",
                            plan.branch, plan.command_display
                        ),
                        action: modal::ConfirmAction::InstallRepoUpdate { plan },
                    });
                }
                KeyCode::Char('u') | KeyCode::Char('U') => {
                    self.run_nova_update_check();
                    take(std::mem::replace(&mut self.modal, modal::Modal::None));
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
            modal::Modal::StoreInstalledMenu { id, name, index } => match code {
                KeyCode::Esc => take(modal::Modal::None),
                KeyCode::Up | KeyCode::Char('k') => *index = index.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => *index = (*index + 1).min(1),
                KeyCode::Enter => {
                    if *index == 0 {
                        take(modal::Modal::Confirm {
                            question: format!(
                                "'{name}' kaldırılsın mı? Enter onaylar, Esc vazgeçer."
                            ),
                            action: modal::ConfirmAction::UninstallStoreApp {
                                id: id.clone(),
                                name: name.clone(),
                            },
                        });
                    } else {
                        take(modal::Modal::None);
                        self.check_store_updates();
                    }
                }
                _ => {}
            },
            modal::Modal::ToolActionsMenu { tool_id, index, .. } => {
                let max = self
                    .catalog
                    .get_by_id(tool_id)
                    .map(|e| e.actions.len().saturating_sub(1))
                    .unwrap_or(0);
                match code {
                    KeyCode::Esc => take(modal::Modal::None),
                    KeyCode::Up | KeyCode::Char('k') => *index = index.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => *index = (*index + 1).min(max),
                    KeyCode::Enter => {
                        let id = tool_id.clone();
                        let idx = *index;
                        take(modal::Modal::None);
                        self.launch_tool_action_by_id(&id, idx);
                    }
                    _ => {}
                }
            }
            modal::Modal::JobProgress {
                log_scroll,
                done,
                log,
                ..
            } => match code {
                KeyCode::Esc | KeyCode::Enter if done.is_some() => {
                    take(modal::Modal::None);
                }
                KeyCode::Esc if done.is_none() => {
                    // İş sürerken Esc paneli gizlemez; durum satırında sürer.
                    self.status_message =
                        "Güncelleme arka planda sürüyor — bitince durum satırına yazar.".into();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let max = log.len().saturating_sub(1) as u16;
                    *log_scroll = (*log_scroll + 1).min(max);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    *log_scroll = log_scroll.saturating_sub(1);
                }
                KeyCode::PageUp => {
                    let max = log.len().saturating_sub(1) as u16;
                    *log_scroll = (*log_scroll + 8).min(max);
                }
                KeyCode::PageDown => {
                    *log_scroll = log_scroll.saturating_sub(8);
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
        language: &str,
    ) -> Result<(), String> {
        let terminal = preferred_terminal.trim();
        let preferred_terminal = if terminal.is_empty() {
            None
        } else if let Some(adapter) = crate::launcher::find_adapter_by_name(terminal) {
            Some(adapter.executable().to_string())
        } else {
            return Err(format!(
                "Desteklenmeyen terminal '{terminal}' (desteklenen: {}; boş = otomatik).",
                crate::launcher::supported_executables_list()
            ));
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

        let language = crate::i18n::Language::parse(language).id().to_string();

        let old = self.state.file.prefs.clone();
        self.state.file.prefs.preferred_terminal = preferred_terminal;
        self.state.file.prefs.default_working_dir = default_working_dir;
        self.state.file.prefs.theme = Some(theme.trim().to_string());
        self.state.file.prefs.default_ai_tool = default_ai_tool;
        self.state.file.prefs.default_image_app = default_image_app;
        self.state.file.prefs.default_video_app = default_video_app;
        self.state.file.prefs.language = Some(language);
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
            modal::ConfirmAction::UninstallStoreApp { id, name } => {
                self.status_message = format!("'{name}' kaldırılıyor...");
                self.start_store_uninstall(&id);
            }
            modal::ConfirmAction::InstallUpdate { version, release } => {
                match (
                    release.download_url.as_deref(),
                    release.asset_name.as_deref(),
                ) {
                    (Some(url), Some(asset)) => {
                        if self.install_job.is_some() || self.pending_tty_pm.is_some() {
                            self.status_message =
                                "Zaten süren bir kurulum var; önce onun bitmesini bekle.".into();
                            return;
                        }
                        let ver = release.version.clone();
                        let url = url.to_string();
                        let asset = asset.to_string();
                        let (tx, progress_tx) = self.begin_job(
                            crate::progress::JobKind::NovaRelease,
                            format!("Nova v{version} kuruluyor"),
                            format!("download {asset}"),
                            "nova".into(),
                            "Nova".into(),
                            false,
                        );
                        std::thread::spawn(move || {
                            let _ = progress_tx
                                .send(crate::progress::ProgressEvent::Phase("İndiriliyor".into()));
                            let _ = progress_tx.send(crate::progress::ProgressEvent::Log(format!(
                                "İndiriliyor: {url}"
                            )));
                            let result = crate::update::install_nova_from_url(&ver, &url, &asset);
                            let _ = tx.send(JobResult::Nova(result));
                        });
                    }
                    _ => {
                        self.status_message = format!(
                            "Hata: Nova v{version} yayında indirilebilir Linux ikilisi yok.                              G ile repo main’den cargo install deneyin."
                        );
                    }
                }
            }
            modal::ConfirmAction::InstallRepoUpdate { plan } => {
                if self.install_job.is_some() || self.pending_tty_pm.is_some() {
                    self.status_message =
                        "Zaten süren bir kurulum var; önce onun bitmesini bekle.".into();
                    return;
                }
                let cmd = plan.command_display.clone();
                let (tx, progress_tx) = self.begin_job(
                    crate::progress::JobKind::NovaRepo,
                    "Nova kaynak güncellemesi".into(),
                    cmd,
                    "nova".into(),
                    "Nova".into(),
                    false,
                );
                std::thread::spawn(move || {
                    let _ = progress_tx.send(crate::progress::ProgressEvent::Phase(
                        "Kaynak alınıyor".into(),
                    ));
                    let mut on_line = |line: &str| crate::progress::emit_line(&progress_tx, line);
                    let result =
                        crate::update::install_nova_from_git_with_progress(&plan, &mut on_line);
                    let _ = tx.send(JobResult::Nova(result));
                });
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

    /// Test Nova self-update sağlayıcısı (ağ yok).
    struct FakeNovaUpdateProvider {
        current: crate::update::Version,
        latest: Result<Option<crate::update::GitHubRelease>, String>,
    }

    impl crate::update::UpdateProvider for FakeNovaUpdateProvider {
        fn current_version(&self) -> crate::update::Version {
            self.current.clone()
        }
        fn fetch_latest_release(&self) -> Result<Option<crate::update::GitHubRelease>, String> {
            self.latest.clone()
        }
    }

    fn fake_nova_provider_no_release() -> Box<dyn crate::update::UpdateProvider + Send + Sync> {
        Box::new(FakeNovaUpdateProvider {
            current: crate::update::Version::parse("0.1.0").unwrap(),
            latest: Ok(None),
        })
    }

    /// Gerçek gömülü katalogla oluşturulmuş uygulama.
    ///
    /// Kullanıcının kendi `config.toml` dosyası okunmaz; testler makineye
    /// göre değişmemeli. Nova self-update ağına çıkmaz (sahte sağlayıcı).
    fn sample_app() -> App {
        sample_app_with_category("Sistem ve Kaynak Kullanımı")
    }

    fn sample_app_with_category(category: &str) -> App {
        let catalog = crate::catalog::loader::builtin_catalog().expect("gömülü katalog okunmalı");
        let catalog = Catalog::from_entries(catalog).expect("katalog oluşturulamadı");
        let mut app = App::new(catalog);
        app.category_index = category_index(category);
        app.update_provider = fake_nova_provider_no_release();
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
        // Borders::ALL: içerik_top=2 üst kenarlık; ilk öğe y=3 → index 0.
        // İkinci kategori: y = 4.
        app.handle_mouse(mouse_down(5, 4)).unwrap();
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
        // Araç paneli: x >= pane_secondary (varsayılan 28); Borders::ALL sonrası ilk araç y = 3.
        app.handle_mouse(mouse_down(40, 3)).unwrap();
        assert_eq!(app.focus, PanelFocus::Tool);
        assert!(!app.filtered_tools().is_empty());
        assert_eq!(app.tool_index, 0);
    }

    #[test]
    fn mouse_click_on_list_border_is_ignored() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.terminal_size = Some((100, 30));
        app.focus = PanelFocus::Tool;
        let before = app.category_index;
        // y=2 üst kenarlık; seçim değişmemeli.
        app.handle_mouse(mouse_down(5, 2)).unwrap();
        assert_eq!(app.category_index, before);
        assert_eq!(app.focus, PanelFocus::Tool);
    }

    #[test]
    fn mouse_click_accounts_for_scrolled_tool_list() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.terminal_size = Some((100, 30));
        app.focus = PanelFocus::Tool;
        app.category_index = category_index("Sistem ve Kaynak Kullanımı");
        let tool_count = app.filtered_tools().len();
        assert!(tool_count > 5, "kaydırma için yeterli araç olmalı");
        // Seçimi alta indir; ratatui penceresi kayar. Görünür ilk satır
        // artık tool_index - (inner_h - 1) olmalı.
        app.tool_index = tool_count - 1;
        let areas = crate::ui::click_areas((100, 30), false, false, false);
        let inner_h = areas.content_height.saturating_sub(2);
        let offset = crate::ui::list_scroll_offset(app.tool_index, inner_h, tool_count);
        // Görünür ilk satıra tıkla → offset indeksini seçer.
        app.handle_mouse(mouse_down(40, areas.content_top + 1))
            .unwrap();
        assert_eq!(app.tool_index, offset);
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
        // Dar modda kategori paneli yok; kenarlık sonrası ilk araç y=3.
        app.handle_mouse(mouse_down(39, 3)).unwrap();
        assert_eq!(app.focus, PanelFocus::Tool);
    }

    #[test]
    fn mouse_click_ignores_open_modal() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.terminal_size = Some((100, 30));
        app.focus = PanelFocus::Tool;
        app.modal = modal::Modal::History;
        let before = app.category_index;
        app.handle_mouse(mouse_down(5, 4)).unwrap();
        assert_eq!(app.category_index, before);
        assert_eq!(app.focus, PanelFocus::Tool);
    }

    #[test]
    fn mouse_click_settings_first_row_is_update_not_theme() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.terminal_size = Some((100, 30));
        app.select_settings_tab();
        // İlk Ayarlar satırı (Güncelle) y=3; eski kod kenarlık yüzünden Tema açıyordu.
        app.handle_mouse(mouse_down(40, 3)).unwrap();
        assert_eq!(app.settings_action_index, 0);
        assert!(!app.theme_picker_open);
        assert_eq!(app.focus, PanelFocus::Tool);
    }

    #[test]
    fn mouse_click_narrow_theme_picker_selects_theme() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.terminal_size = Some((50, 30));
        app.narrow_mode = true;
        app.select_settings_tab();
        app.open_theme_picker();
        let areas = crate::ui::click_areas((50, 30), false, false, false);
        let half = (areas.content_height / 2).max(3);
        // Alt yarıda tema paneli; kenarlık sonrası ilk tema.
        let y = areas.content_top + half + 1;
        app.handle_mouse(mouse_down(10, y)).unwrap();
        assert_eq!(app.focus, PanelFocus::ThemePicker);
        assert_eq!(app.theme_picker_index, 0);
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
        app.store_status
            .insert("aider".into(), crate::detection::InstalledStatus::Missing);
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
        // Host’ta ripgrep kurulu olsa bile “kurulmamış” varsay — Install onayı.
        app.store_status
            .insert("ripgrep".into(), crate::detection::InstalledStatus::Missing);
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
    fn store_enter_on_installed_opens_menu_not_reinstall() {
        let mut app = sample_app();
        app.select_store_tab();
        if let Some(idx) = STORE_CATEGORIES.iter().position(|c| *c == "Dosya ve Arama") {
            app.store_category_index = idx;
        }
        let apps = app.filtered_store_apps();
        let pos = apps
            .iter()
            .position(|e| e.id == "ripgrep")
            .expect("ripgrep");
        app.store_app_index = pos;
        app.store_status.insert(
            "ripgrep".into(),
            crate::detection::InstalledStatus::Installed,
        );
        app.select_store_app();
        match &app.modal {
            modal::Modal::StoreInstalledMenu { id, index, .. } => {
                assert_eq!(id, "ripgrep");
                assert_eq!(*index, 0);
            }
            other => panic!("kurulu menü beklenirdi: {other:?}"),
        }
        // Enter → kaldırma onayı
        app.handle_key(key(crossterm::event::KeyCode::Enter))
            .unwrap();
        match &app.modal {
            modal::Modal::Confirm {
                action: modal::ConfirmAction::UninstallStoreApp { id, .. },
                ..
            } => assert_eq!(id, "ripgrep"),
            other => panic!("kaldırma onayı beklenirdi: {other:?}"),
        }
    }

    #[test]
    fn theme_enter_opens_side_panel_instead_of_cycling() {
        let mut app = sample_app();
        let before = app.ui_theme();
        app.select_settings_tab();
        app.settings_action_index = 4;
        app.focus = PanelFocus::Tool;
        app.activate_settings_action();
        assert!(app.theme_picker_open);
        assert_eq!(app.focus, PanelFocus::ThemePicker);
        assert_eq!(app.ui_theme(), before, "Enter temayı hemen değiştirmemeli");
        // Seçici satırını sonraki temaya alıp uygula.
        app.theme_picker_index = before.next().index();
        app.apply_theme_at_picker_index().unwrap();
        assert_eq!(app.ui_theme(), before.next());
    }

    #[test]
    fn language_picker_persists_and_switches_ui_strings() {
        let mut app = sample_app();
        assert_eq!(app.ui_lang(), crate::i18n::Language::Tr);
        app.select_settings_tab();
        app.settings_action_index = 5;
        app.focus = PanelFocus::Tool;
        app.activate_settings_action();
        assert!(app.language_picker_open);
        assert!(!app.theme_picker_open);
        assert_eq!(app.focus, PanelFocus::LanguagePicker);
        app.language_picker_index = crate::i18n::Language::En.index();
        app.apply_language_at_picker_index().unwrap();
        assert_eq!(app.ui_lang(), crate::i18n::Language::En);
        assert_eq!(app.state.file.prefs.language.as_deref(), Some("en"));
        assert_eq!(app.ui_lang().t(crate::i18n::Msg::Settings), "Settings");
        // Dil paneli açıkken tema paneli açılırsa dil kapanır.
        app.open_theme_picker();
        assert!(app.theme_picker_open);
        assert!(!app.language_picker_open);
    }

    #[test]
    fn focused_panel_resize_is_per_screen_without_bleed() {
        let mut app = sample_app();
        assert_eq!(
            app.pane_widths().tabs,
            crate::ui::SECONDARY_PANEL_WIDTH_DEFAULT
        );
        assert_eq!(app.pane_widths().list, crate::ui::LIST_PANEL_WIDTH_DEFAULT);

        // Mağaza: uygulama listesine odaklanıp listeyi büyüt.
        app.select_store_tab();
        app.focus = PanelFocus::StoreApp;
        let before_list = app.pane_widths().list;
        app.handle_key(typed('=')).unwrap();
        assert_eq!(
            app.state.file.prefs.pane_sizes.store.list,
            Some(before_list + 1)
        );
        assert!(
            app.state.file.prefs.pane_sizes.tools.list.is_none(),
            "Mağaza boyutu Kurulu'ya sızmamalı"
        );

        // Kurulu: sekme sütununu küçült — Mağaza listesi değişmez.
        app.set_tab_index(0);
        app.focus = PanelFocus::Category;
        let tools_tabs = app.pane_widths().tabs;
        app.handle_key(typed('-')).unwrap();
        assert_eq!(
            app.state.file.prefs.pane_sizes.tools.tabs,
            Some(tools_tabs - 1)
        );
        assert_eq!(
            app.state.file.prefs.pane_sizes.store.list,
            Some(before_list + 1)
        );

        // `[` / `]` artık boyutlandırmaz.
        let tabs_now = app.pane_widths().tabs;
        app.handle_key(typed(']')).unwrap();
        assert_eq!(app.pane_widths().tabs, tabs_now);
    }

    #[test]
    fn tab_and_shift_tab_wrap_focus_without_resetting_selection() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.focus = PanelFocus::Category;
        app.tool_index = 2;
        let selected = app.selected_tool().map(|e| e.id.clone());

        app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap();
        assert_eq!(app.focus, PanelFocus::Tool);
        assert_eq!(app.selected_tool().map(|e| e.id.clone()), selected);

        // Tool'dan Tab → (aksiyon yoksa) başa sarar: Category.
        app.handle_key(key(crossterm::event::KeyCode::Tab)).unwrap();
        assert_eq!(app.focus, PanelFocus::Category);
        assert_eq!(app.tool_index, 2);

        app.handle_key(key(crossterm::event::KeyCode::BackTab))
            .unwrap();
        assert_eq!(app.focus, PanelFocus::Tool);

        // Sınırda geri bildirim.
        app.focus = PanelFocus::Category;
        for _ in 0..40 {
            app.handle_key(typed('-')).unwrap();
        }
        assert!(
            app.status_message.contains("en küçük") || app.status_message.contains("genişlik"),
            "{}",
            app.status_message
        );
        assert_eq!(app.pane_widths().tabs, crate::ui::TABS_PANEL_WIDTH_MIN);
    }

    #[test]
    fn store_tab_is_followed_by_settings_in_the_category_list() {
        let mut app = sample_app();
        mark_all_installed(&mut app);
        app.focus = PanelFocus::Category;
        let visible = app.visible_category_indices();
        app.category_index = *visible.last().expect("görünür kategori");
        app.handle_key(typed('j')).unwrap();
        assert!(app.commands_tab, "önce Komut Rehberi");
        assert_eq!(app.selected_tab_index(), visible.len());
        app.handle_key(typed('j')).unwrap();
        assert!(app.store_tab);
        assert!(!app.commands_tab);
        assert_eq!(app.selected_tab_index(), visible.len() + 1);
        app.handle_key(typed('j')).unwrap();
        assert!(app.settings_tab);
        assert_eq!(app.selected_tab_index(), visible.len() + 2);
        app.handle_key(typed('k')).unwrap();
        assert!(app.store_tab);
        assert!(!app.settings_tab);
    }

    #[test]
    fn commands_tab_keeps_bank_and_local_kinds_distinct() {
        let mut app = sample_app();
        app.select_commands_tab();
        assert!(app.commands_tab);
        assert!(!app.store_tab);
        assert_eq!(app.commands_bank.kind, crate::commands::BANK_KIND);
        assert_eq!(app.commands_local.kind, crate::commands::LOCAL_KIND);
        assert_ne!(app.commands_bank.kind, app.commands_local.kind);
        assert!(!app.commands_bank.commands.is_empty());
        // PATH varlığı bank verification yükseltmez.
        for ent in &app.commands_local.entries {
            if let Some(id) = &ent.bank_id {
                let bank = app.commands_bank.find_by_id_or_name(id).unwrap();
                assert_ne!(
                    format!("{:?}", bank.verification.status),
                    "inherited-from-path"
                );
            }
        }
        app.set_commands_mode(crate::commands::CommandsBrowseMode::SystemPath);
        app.show_selected_command_info();
        assert!(
            app.status_message.contains("nova-local-commands")
                || app.status_message.contains("PATH")
                || app.status_message.contains("Cihazda")
                || app.status_message.contains("eşleşen"),
            "local status: {}",
            app.status_message
        );
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
        // Mağaza → Komut Rehberi (araç kategorilerinden hemen önce).
        assert!(app.commands_tab);
        assert!(!app.store_tab);
        app.handle_key(typed('k')).unwrap();
        assert!(!app.commands_tab);
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
        // Artık Idle'da kalmaz; denetim hemen çalışır ve durum satırı dolar.
        match &app.modal {
            modal::Modal::UpdateCheck {
                status: modal::UpdateStatus::Done,
                check: Some(UpdateCheck::NoPublishedRelease { .. }),
                ..
            } => {}
            other => panic!("beklenen Done+NoPublishedRelease, geldi: {other:?}"),
        }
        assert!(
            app.status_message.contains("ikili yayını yok")
                || app.status_message.contains("cargo rebuild"),
            "mesaj: {}",
            app.status_message
        );
    }

    #[test]
    fn settings_commands_bank_refresh_keeps_embedded_when_remote_skipped() {
        let mut app = sample_app();
        let before_sha = app.commands_bank.content_sha256.clone();
        let before_n = app.commands_bank.commands.len();
        app.select_settings_tab();
        app.settings_action_index = 2;
        app.focus = PanelFocus::Tool;
        app.activate_settings_action();
        assert_eq!(app.commands_bank.kind, crate::commands::BANK_KIND);
        assert_eq!(app.commands_bank.content_sha256, before_sha);
        assert_eq!(app.commands_bank.commands.len(), before_n);
        assert!(
            app.status_message.contains("Komut bankası")
                || app.status_message.contains("yenilenemedi")
                || app.status_message.contains("test:"),
            "mesaj: {}",
            app.status_message
        );
        // Mağaza kind sızmamalı.
        assert!(!app.commands_bank.kind.contains("store"));
    }

    #[test]
    fn ayarlar_guncelle_reports_no_release_and_status() {
        let mut app = sample_app();
        app.select_settings_tab();
        app.settings_action_index = 0;
        app.focus = PanelFocus::Tool;
        app.activate_settings_action();
        match &app.modal {
            modal::Modal::UpdateCheck {
                status: modal::UpdateStatus::Done,
                check: Some(UpdateCheck::NoPublishedRelease { current }),
                error: None,
            } => {
                assert_eq!(current.to_string(), "0.1.0");
            }
            other => panic!("beklenen yayın-yok sonucu, geldi: {other:?}"),
        }
        assert!(
            app.status_message.contains("ikili yayını yok"),
            "mesaj: {}",
            app.status_message
        );
    }

    #[test]
    fn nova_update_check_reports_available_and_up_to_date() {
        let release = crate::update::GitHubRelease {
            tag_name: "v0.2.0".into(),
            version: crate::update::Version::parse("0.2.0").unwrap(),
            body: "tema".into(),
            html_url: "https://github.com/Teknoloji-Filozoflari/Nova/releases/tag/v0.2.0".into(),
            draft: false,
            prerelease: false,
            asset_name: Some("nova-x86_64-unknown-linux-gnu.tar.gz".into()),
            download_url: Some(
                "https://github.com/Teknoloji-Filozoflari/Nova/releases/download/v0.2.0/nova.tar.gz"
                    .into(),
            ),
        };

        let mut app = sample_app();
        app.update_provider = Box::new(FakeNovaUpdateProvider {
            current: crate::update::Version::parse("0.1.0").unwrap(),
            latest: Ok(Some(release.clone())),
        });
        app.start_nova_update_check();
        match &app.modal {
            modal::Modal::UpdateCheck {
                check: Some(UpdateCheck::UpdateAvailable { latest, .. }),
                ..
            } => assert_eq!(latest.to_string(), "0.2.0"),
            other => panic!("beklenen UpdateAvailable, geldi: {other:?}"),
        }
        assert!(
            app.status_message.contains("Yeni Nova sürümü"),
            "mesaj: {}",
            app.status_message
        );

        app.update_provider = Box::new(FakeNovaUpdateProvider {
            current: crate::update::Version::parse("0.2.0").unwrap(),
            latest: Ok(Some(release)),
        });
        app.run_nova_update_check();
        match &app.modal {
            modal::Modal::UpdateCheck {
                check: Some(UpdateCheck::UpToDate { current }),
                ..
            } => assert_eq!(current.to_string(), "0.2.0"),
            other => panic!("beklenen UpToDate, geldi: {other:?}"),
        }
        assert!(
            app.status_message.contains("güncel"),
            "mesaj: {}",
            app.status_message
        );
    }

    #[test]
    fn nova_update_check_reports_error_on_provider_failure() {
        let mut app = sample_app();
        app.update_provider = Box::new(FakeNovaUpdateProvider {
            current: crate::update::Version::parse("0.1.0").unwrap(),
            latest: Err("ağ yok".into()),
        });
        app.start_nova_update_check();
        match &app.modal {
            modal::Modal::UpdateCheck {
                status: modal::UpdateStatus::Done,
                check: Some(UpdateCheck::Error(msg)),
                error: Some(err),
            } => {
                assert!(msg.contains("ağ yok"));
                assert!(err.contains("ağ yok"));
            }
            other => panic!("beklenen Error, geldi: {other:?}"),
        }
        assert!(
            app.status_message.contains("başarısız"),
            "mesaj: {}",
            app.status_message
        );
    }

    fn write_fake_nova_binary(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "nova-app-restart-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("nova");
        std::fs::write(&bin, b"#!/bin/sh\necho nova\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin, perms).unwrap();
        }
        (dir, bin)
    }

    #[test]
    fn nova_update_success_schedules_self_restart() {
        let mut app = sample_app();
        let (dir, bin) = write_fake_nova_binary("ok");
        let (tx, _progress_tx) = app.begin_job(
            crate::progress::JobKind::NovaRelease,
            "Nova test".into(),
            "test".into(),
            "nova".into(),
            "Nova".into(),
            false,
        );
        tx.send(JobResult::Nova(Ok(crate::update::UpdateOutcome {
            version: crate::update::Version::parse("0.2.0").unwrap(),
            install_dir: dir.clone(),
            binary_path: bin.clone(),
            summary: "kuruldu".into(),
        })))
        .unwrap();
        app.poll_install();
        assert!(!app.running, "güncelleme sonrası döngü kapanmalı");
        assert_eq!(app.pending_self_restart.as_ref(), Some(&bin));
        assert!(
            app.status_message.contains("Yeniden başlatılıyor"),
            "mesaj: {}",
            app.status_message
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nova_update_success_keeps_session_when_restart_binary_missing() {
        let mut app = sample_app();
        let missing = std::env::temp_dir().join(format!(
            "nova-missing-restart-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let (tx, _progress_tx) = app.begin_job(
            crate::progress::JobKind::NovaRelease,
            "Nova test".into(),
            "test".into(),
            "nova".into(),
            "Nova".into(),
            false,
        );
        tx.send(JobResult::Nova(Ok(crate::update::UpdateOutcome {
            version: crate::update::Version::parse("0.2.0").unwrap(),
            install_dir: missing.parent().unwrap().to_path_buf(),
            binary_path: missing,
            summary: "kuruldu".into(),
        })))
        .unwrap();
        app.poll_install();
        assert!(app.running, "bozuk restart oturumu kapatmamalı");
        assert!(app.pending_self_restart.is_none());
        assert!(
            app.status_message.contains("Otomatik yeniden başlatma"),
            "mesaj: {}",
            app.status_message
        );
    }

    #[test]
    fn update_modal_enter_on_idle_like_state_retries_check() {
        let mut app = sample_app();
        app.modal = modal::Modal::UpdateCheck {
            status: modal::UpdateStatus::Idle,
            check: None,
            error: None,
        };
        app.handle_key(key(crossterm::event::KeyCode::Enter))
            .unwrap();
        match &app.modal {
            modal::Modal::UpdateCheck {
                status: modal::UpdateStatus::Done,
                check: Some(UpdateCheck::NoPublishedRelease { .. }),
                ..
            } => {}
            other => panic!("Enter denetimi çalıştırmalıydı, geldi: {other:?}"),
        }
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
        app.store_status
            .insert("aider".into(), crate::detection::InstalledStatus::Missing);
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
    fn kurulu_detail_description_follows_store_catalog_match() {
        use crate::store::sync::{detail_description_for_tool, find_store_for_catalog_tool};

        let app = sample_app();
        let btm = app
            .catalog
            .entries()
            .iter()
            .find(|e| e.id == "btm")
            .expect("btm katalogda olmalı");
        let store =
            find_store_for_catalog_tool(app.store.entries(), &btm.id, &btm.executable, &btm.args)
                .expect("btm → Mağaza bottom eşleşmeli");
        assert_eq!(store.id, "bottom");
        let desc = detail_description_for_tool(&btm.description, Some(store));
        assert_eq!(desc, store.description.as_str());
        assert_ne!(
            desc,
            btm.description.as_str(),
            "Kurulu, Mağaza açıklamasını kullanmalı (id/ikili eşleşmesi)"
        );

        let speed = app
            .catalog
            .entries()
            .iter()
            .find(|e| e.id == "cloudflare-speed-test")
            .expect("cloudflare-speed-test katalogda olmalı");
        assert!(
            find_store_for_catalog_tool(
                app.store.entries(),
                &speed.id,
                &speed.executable,
                &speed.args,
            )
            .is_none(),
            "özel argv’li hız testi curl Mağaza kaydına bağlanmamalı"
        );
        assert_eq!(
            detail_description_for_tool(&speed.description, None),
            speed.description.as_str()
        );
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
