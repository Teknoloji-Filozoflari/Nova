//! Kullanıcı arayüzü modülü.
//! Ratatui tabanlı terminal arayüzünün çizim işlevlerini içerir.
//!
//! Düzen terminal boyutuna göre üç kademede uyarlanır:
//! - **Geniş (≥ 70 sütun):** kategori paneli solda, araç listesi ortada.
//!   Yeterince genişken (`≥ TOOL_DETAIL_SIDE_MIN_WIDTH`) Detay sağda —
//!   Mağaza yan-detay düzeniyle aynı. AI aracı Enter ile Mağaza onay
//!   modalı boyutunda ortalanmış **aksiyon popup** açar (alt şerit yok).
//! - **Orta (45–69 sütun):** tek sütun; kategori başlık satırında gösterilir,
//!   araç listesi ve alt detay alanı görünür; AI aksiyonları yine modal.
//! - **Dar (< 45 sütun):** yalnızca araç listesi; detay ve büyük yardım
//!   paneli gizlenir, tek satırlık kısa yardım gösterilir.
//!
//! Hiçbir metin bayt indeksiyle kesilmez; `truncate` karakter sayarak çalıştığı
//! için Türkçe karakterlerde panik oluşmaz.

pub mod modal;
pub mod theme;

pub use theme::{ThemePalette, UiTheme};

use crate::app::{App, PanelFocus};
use crate::catalog::CATEGORIES;
use crate::detection::InstalledStatus;
use crate::i18n::Msg;
use crate::store::STORE_CATEGORIES;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

/// Uygulamanın etkin tema paleti.
fn palette(app: &App) -> ThemePalette {
    app.ui_theme().palette()
}

/// Bu genişliğin altında iki panel yan yana sığmaz.
pub const WIDE_MIN_WIDTH: u16 = 70;
/// Bu genişliğin altında detay alanı gizlenir.
pub const MEDIUM_MIN_WIDTH: u16 = 45;
/// Bu yüksekliğin altında yardım paneli tek satıra iner.
pub const TALL_MIN_HEIGHT: u16 = 14;
/// Bu genişliğin altında uzun yardım ipuçları sığmaz.
const TIGHT_HINT_WIDTH: u16 = 52;
/// Yardım açıkken araç listesine bırakılan en az satır.
pub const CONTENT_MIN_HEIGHT: u16 = 5;
/// Yardım panelinin tam tuş tablosu için ayırabileceği en fazla satır.
pub const HELP_HELP_MAX_HEIGHT: u16 = 10;
/// Sol sekme listesi varsayılan genişliği (Mağaza / Komut Rehberi).
///
/// Odaklı panelde `-` / `+` ile ekran başına ayarlanır (`prefs.pane_sizes`).
pub const CATEGORY_PANEL_WIDTH: u16 = 26;
/// Sekme / sol sütun alt sınırı.
pub const TABS_PANEL_WIDTH_MIN: u16 = 16;
/// Sekme / sol sütun üst sınırı.
pub const TABS_PANEL_WIDTH_MAX: u16 = 48;
/// İkincil sütun varsayılanı (Mağaza kategorileri, Komut kaynak/banka,
/// Ayarlar ayar paneli). Kurulu / Ayarlar sol sütunu `tabs` alanındadır.
pub const SECONDARY_PANEL_WIDTH_DEFAULT: u16 = 28;
/// İkincil sütun alt sınırı.
pub const SECONDARY_PANEL_WIDTH_MIN: u16 = 16;
/// İkincil sütun üst sınırı.
pub const SECONDARY_PANEL_WIDTH_MAX: u16 = 48;
/// Geriye dönük ad: ikincil sütun varsayılanı (`SECONDARY_PANEL_WIDTH_DEFAULT`).
pub const STORE_CATEGORY_PANEL_WIDTH: u16 = SECONDARY_PANEL_WIDTH_DEFAULT;
/// Liste sütunu tercih edilen genişlik (yan detaylı düzenler).
/// Odaklı listede `-` / `+`; fazla genişlik detaya gidebilir.
pub const LIST_PANEL_WIDTH_DEFAULT: u16 = 32;
/// Liste sütunu üst sınırı (tercih).
pub const LIST_PANEL_WIDTH_MAX: u16 = 64;
/// Mağaza sağ detay panelinin en az / varsayılan genişliği (geniş terminalde).
pub const STORE_DETAIL_PANEL_WIDTH: u16 = 72;
/// Yan detay genişliği üst sınırı.
pub const DETAIL_PANEL_WIDTH_MAX: u16 = 120;
/// Uygulama / araç listesinin dört sütunlu düzende bırakacağı en az genişlik.
pub const LIST_PANEL_MIN_WIDTH: u16 = 14;
/// Liste sütunu alt sınırı (= [`LIST_PANEL_MIN_WIDTH`]).
pub const LIST_PANEL_WIDTH_MIN: u16 = LIST_PANEL_MIN_WIDTH;
/// Tek ikincil sütunlu yan-detay düzeni (Mağaza / yerel Komut) için en az
/// genişlik — varsayılan ikincil genişlikle.
pub const STORE_DETAIL_SIDE_MIN_WIDTH: u16 = CATEGORY_PANEL_WIDTH
    + SECONDARY_PANEL_WIDTH_DEFAULT
    + LIST_PANEL_MIN_WIDTH
    + STORE_DETAIL_PANEL_WIDTH;
/// Araç detay panelinin satır sayısı (üst kenarlık dâhil; varsayılan).
pub const DETAIL_ROWS: u16 = 10;
/// Alt detay şeridi minimum yüksekliği.
pub const DETAIL_HEIGHT_MIN: u16 = 4;
/// Alt detay şeridi maksimum yüksekliği.
pub const DETAIL_HEIGHT_MAX: u16 = 24;
/// AI aksiyon paneli kenarlık satırları (üst+alt Borders::ALL).
pub const ACTIONS_PANEL_BORDER_ROWS: u16 = 2;
/// Geriye dönük ad: varsayılan 3 satırlık aksiyon paneli (1 seçenek + kenarlık).
pub const ACTIONS_STRIP_ROWS: u16 = ACTIONS_PANEL_BORDER_ROWS + 1;
/// AI aksiyon paneli maksimum yüksekliği.
pub const ACTIONS_HEIGHT_MAX: u16 = 16;
/// Araç detay paneli genişliği (mağaza detayıyla aynı).
pub const TOOL_DETAIL_PANEL_WIDTH: u16 = 72;
/// Kurulu araçlar: sekme | liste | yan detay için en az genişlik
/// (sol sütun varsayılanı + `LIST_MIN` + `TOOL_DETAIL`).
pub const TOOL_DETAIL_SIDE_MIN_WIDTH: u16 =
    SECONDARY_PANEL_WIDTH_DEFAULT + LIST_PANEL_MIN_WIDTH + TOOL_DETAIL_PANEL_WIDTH;
/// Mağaza detay panelinin satır sayısı (üst kenarlık dâhil; dar/orta düzen).
///
/// Geniş terminalde detay sağ panele taşınır; bu yükseklik yalnızca
/// dar/orta düzende alt şerit için kullanılır.
pub const STORE_DETAIL_ROWS: u16 = 16;
/// Dar Mağaza üst kategori paneli varsayılan / min / max yüksekliği.
pub const STORE_CAT_HEIGHT_DEFAULT: u16 = 8;
/// Dar Mağaza kategori paneli alt sınırı.
pub const STORE_CAT_HEIGHT_MIN: u16 = 3;
/// Dar Mağaza kategori paneli üst sınırı.
pub const STORE_CAT_HEIGHT_MAX: u16 = 20;

/// Tek ekranın çözülmüş sütun / yükseklik tercihleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneWidths {
    /// Sol sekme / kategori sütunu.
    pub tabs: u16,
    /// İkincil sütun (Mağaza kategori / Komut kaynak…).
    pub secondary: u16,
    /// Liste sütunu tercihi.
    pub list: u16,
    /// Yan detay tercih genişliği.
    pub detail_width: u16,
    /// Alt detay şeridi yüksekliği.
    pub detail_height: u16,
    /// AI aksiyon paneli tercih yüksekliği (`None` çözümde 0 = içerik kadar).
    pub actions_height: u16,
    /// Dar Mağaza üst kategori yüksekliği.
    pub secondary_height: u16,
}

impl Default for PaneWidths {
    fn default() -> Self {
        // Kurulu varsayılanı: sol sütun ikincil genişlikte.
        // Mağaza/Komut `for_screen` ile `tabs = CATEGORY_PANEL_WIDTH` alır.
        Self {
            tabs: SECONDARY_PANEL_WIDTH_DEFAULT,
            secondary: SECONDARY_PANEL_WIDTH_DEFAULT,
            list: LIST_PANEL_WIDTH_DEFAULT,
            detail_width: STORE_DETAIL_PANEL_WIDTH,
            detail_height: DETAIL_ROWS,
            actions_height: 0,
            secondary_height: STORE_CAT_HEIGHT_DEFAULT,
        }
    }
}

impl PaneWidths {
    /// Eski paylaşımlı tercihler (yalnızca geriye dönük testler).
    pub fn from_prefs(prefs: &crate::config::state::Prefs) -> Self {
        Self::for_screen(crate::config::state::UiScreen::Tools, prefs)
    }

    /// Ekrana özel boyutlar; eski global alanlar yalnızca o ekranda yedek.
    pub fn for_screen(
        screen: crate::config::state::UiScreen,
        prefs: &crate::config::state::Prefs,
    ) -> Self {
        use crate::config::state::UiScreen;
        let s = prefs.pane_sizes.get(screen);
        let legacy_sec = prefs.pane_secondary_width;
        let legacy_list = prefs.pane_list_width;
        let default_tabs = match screen {
            UiScreen::Tools | UiScreen::Settings => SECONDARY_PANEL_WIDTH_DEFAULT,
            UiScreen::Store | UiScreen::Commands => CATEGORY_PANEL_WIDTH,
        };
        let default_detail_h = match screen {
            UiScreen::Store | UiScreen::Commands => STORE_DETAIL_ROWS,
            _ => DETAIL_ROWS,
        };
        let default_detail_w = match screen {
            UiScreen::Tools => TOOL_DETAIL_PANEL_WIDTH,
            _ => STORE_DETAIL_PANEL_WIDTH,
        };
        // Kurulu / Ayarlar sol sütunu eskiden `pane_secondary_width` idi.
        let tabs_raw = s.tabs.or(match screen {
            UiScreen::Tools | UiScreen::Settings => legacy_sec,
            UiScreen::Store | UiScreen::Commands => None,
        });
        let secondary_raw = s.secondary.or(match screen {
            UiScreen::Store | UiScreen::Commands => legacy_sec,
            UiScreen::Tools | UiScreen::Settings => None,
        });
        Self {
            tabs: clamp_tabs_width(tabs_raw.unwrap_or(default_tabs)),
            secondary: clamp_secondary_width(
                secondary_raw.unwrap_or(SECONDARY_PANEL_WIDTH_DEFAULT),
            ),
            list: clamp_list_width(s.list.or(legacy_list).unwrap_or(LIST_PANEL_WIDTH_DEFAULT)),
            detail_width: clamp_detail_width(s.detail_width.unwrap_or(default_detail_w)),
            detail_height: clamp_detail_height(s.detail_height.unwrap_or(default_detail_h)),
            actions_height: s.actions_height.unwrap_or(0),
            secondary_height: clamp_secondary_height(
                s.secondary_height.unwrap_or(STORE_CAT_HEIGHT_DEFAULT),
            ),
        }
    }
}

/// Sekme / sol sütun genişliğini min/max aralığına kırpar.
pub fn clamp_tabs_width(w: u16) -> u16 {
    w.clamp(TABS_PANEL_WIDTH_MIN, TABS_PANEL_WIDTH_MAX)
}

/// İkincil sütun genişliğini min/max aralığına kırpar.
pub fn clamp_secondary_width(w: u16) -> u16 {
    w.clamp(SECONDARY_PANEL_WIDTH_MIN, SECONDARY_PANEL_WIDTH_MAX)
}

/// Liste sütunu tercih genişliğini min/max aralığına kırpar.
pub fn clamp_list_width(w: u16) -> u16 {
    w.clamp(LIST_PANEL_WIDTH_MIN, LIST_PANEL_WIDTH_MAX)
}

/// Yan detay genişliğini min/max aralığına kırpar.
pub fn clamp_detail_width(w: u16) -> u16 {
    w.clamp(
        STORE_DETAIL_PANEL_WIDTH.min(TOOL_DETAIL_PANEL_WIDTH),
        DETAIL_PANEL_WIDTH_MAX,
    )
    .max(LIST_PANEL_MIN_WIDTH)
}

/// Alt detay yüksekliğini min/max aralığına kırpar.
pub fn clamp_detail_height(h: u16) -> u16 {
    h.clamp(DETAIL_HEIGHT_MIN, DETAIL_HEIGHT_MAX)
}

/// Dar Mağaza kategori yüksekliğini kırpar.
pub fn clamp_secondary_height(h: u16) -> u16 {
    h.clamp(STORE_CAT_HEIGHT_MIN, STORE_CAT_HEIGHT_MAX)
}

/// Yan detay düzeninin açılabileceği en az terminal genişliği.
///
/// `tabs`: sol sekme sütunu. `secondary_cols`: Mağaza / yerel Komut = 1;
/// banka Komut Rehberi = 2 (kaynak + banka kategori).
pub fn detail_side_min_width(tabs: u16, secondary: u16, secondary_cols: u16) -> u16 {
    tabs.saturating_add(secondary.saturating_mul(secondary_cols.max(1)))
        .saturating_add(LIST_PANEL_MIN_WIDTH)
        .saturating_add(STORE_DETAIL_PANEL_WIDTH)
}

/// Liste + detaya kalan alandan etkin liste genişliğini seçer.
///
/// Tercih edilen liste genişliği kullanılır; detay için en az
/// [`STORE_DETAIL_PANEL_WIDTH`] bırakılır.
pub fn fit_list_width(available_for_list_and_detail: u16, list_pref: u16) -> u16 {
    let max_list = available_for_list_and_detail.saturating_sub(STORE_DETAIL_PANEL_WIDTH);
    if max_list == 0 {
        return LIST_PANEL_MIN_WIDTH;
    }
    clamp_list_width(list_pref)
        .min(max_list)
        .max(LIST_PANEL_MIN_WIDTH.min(max_list))
}

/// Yan detay yokken liste sütunu genişliği (`-` / `=` görünür etki).
///
/// Kalan alan `Min(0)` boşluğa gider; tercihin altında/üstünde kırpılır.
pub fn fit_list_width_alone(available: u16, list_pref: u16) -> u16 {
    if available == 0 {
        return 0;
    }
    clamp_list_width(list_pref)
        .min(available)
        .max(LIST_PANEL_MIN_WIDTH.min(available))
}

/// AI aksiyon paneli yüksekliği (kenarlık + seçenek satırları).
pub fn actions_panel_rows(action_count: usize) -> u16 {
    if action_count == 0 {
        0
    } else {
        ACTIONS_PANEL_BORDER_ROWS.saturating_add(action_count as u16)
    }
}

/// Liste panellerindeki `Borders::ALL` üst/alt kenarlık kalınlığı (satır).
pub const LIST_BORDER: u16 = 1;

/// Fare tıklama testi için hesaplanan düzen alanları. `ui::draw` ile aynı
/// eşikleri kullandığı iki kod yolu senkron kalır.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClickAreas {
    /// İçerik alanının ilk satırı (başlık + arama satırından sonra).
    pub content_top: u16,
    /// İçerik alanının yüksekliği (satır).
    pub content_height: u16,
    /// Geniş mod (iki panel) mı?
    pub wide: bool,
    /// Kategori panelinin bitiş sütunu (hariç); dar modda tüm genişlik.
    pub category_end_x: u16,
    /// Paylaşılan ikincil sütun genişliği (Mağaza kategori / Komut kaynak…).
    pub secondary_width: u16,
    /// Liste sütunu etkin/tercih genişliği (yan detay hesaplarında).
    pub list_width: u16,
    /// Mağaza kategori panelinin bitiş sütunu (hariç). Yalnızca mağaza
    /// sekmesinde ve geniş modda anlamlıdır.
    pub store_category_end_x: u16,
    /// Mağaza uygulama listesinin bitiş sütunu (hariç). Sağ detay paneli
    /// açıkken uygulamalar bu sütuna kadar uzar; aksi hâlde ekran sonudur.
    pub store_apps_end_x: u16,
    /// Araç listesinin bitiş sütunu (hariç). AI detay sağdayken liste bu
    /// sütuna kadar uzar; aksi hâlde ekran sonudur.
    pub tools_end_x: u16,
    /// Mağaza kategori panelinin bittiği satır (hariç). Yalnızca mağaza
    /// sekmesinde ve dar modda anlamlıdır (kategoriler üstte).
    pub store_category_end_y: u16,
    /// Geniş mağazada detay sağda mı? (değilse alt şeritte veya gizli).
    pub store_detail_side: bool,
    /// Geniş Komut Rehberi'nde detay sağda mı?
    pub commands_detail_side: bool,
    /// Komut listesinin bitiş sütunu (hariç); yan detay açıkken daralır.
    pub commands_list_end_x: u16,
    /// Geniş Kurulu sekmesinde detay sağda mı? (Mağaza yan-detay düzeni).
    pub tool_detail_side: bool,
    /// AI aksiyon şeridinin üst satırı (yoksa `u16::MAX`).
    pub actions_top: u16,
    /// AI aksiyon şeridi yüksekliği (yoksa 0).
    pub actions_height: u16,
}

/// Kurulu araçlarda Mağaza ile aynı yan-detay düzeni açılsın mı?
///
/// Geniş terminalde detay sağ sütunda; eşik canlı sol sütun (`tabs`)
/// genişliğine bağlıdır. AI aksiyonları Enter ile modal popup'ta.
/// Dar/orta düzende detay alt şeritte kalır.
pub fn tool_detail_side_layout(wide: bool, width: u16, tabs: u16) -> bool {
    wide && width
        >= tabs
            .saturating_add(LIST_PANEL_MIN_WIDTH)
            .saturating_add(TOOL_DETAIL_PANEL_WIDTH)
}

/// Kurulu araç listesinde görünen satır (yalnızca araç; aksiyonlar şeritte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolListHit {
    /// `filtered_tools` indeksi.
    Tool(usize),
}

/// Araç listesi görsel satırları (aksiyonlar artık şeritte; iç içe yok).
pub fn tool_list_hits(app: &App) -> Vec<ToolListHit> {
    (0..app.filtered_tools().len())
        .map(ToolListHit::Tool)
        .collect()
}

/// Odak ve seçime göre vurgulanacak görsel satır indeksi.
pub fn tool_list_selected_visual(app: &App) -> Option<usize> {
    let hits = tool_list_hits(app);
    if hits.is_empty() {
        return None;
    }
    Some(app.tool_index.min(hits.len() - 1))
}

/// Dikey aksiyon panelinde `y` satırına denk gelen aksiyon indeksi.
pub fn action_index_at_y(app: &App, y: u16, panel: Rect) -> Option<usize> {
    let entry = app.selected_tool()?;
    if !entry.has_ai_actions() || panel.height == 0 {
        return None;
    }
    list_index_at_y(
        y,
        panel.y,
        panel.height,
        app.tool_action_index,
        entry.actions.len(),
    )
}

/// `click_areas_ex` girdi parametreleri (clippy `too_many_arguments` kaçınır).
#[derive(Debug, Clone, Copy)]
pub struct ClickAreasInput {
    /// Terminal boyutu `(genişlik, yükseklik)`.
    pub size: (u16, u16),
    /// Yardım paneli açık mı?
    pub show_help: bool,
    /// Mağaza sekmesi mi?
    pub store_tab: bool,
    /// Komut Rehberi sekmesi mi?
    pub commands_tab: bool,
    /// Kurulu yan-detay düzeni (geniş terminal).
    pub tool_detail_side: bool,
    /// Seçili AI aracının aksiyon sayısı (0 = panel yok).
    pub actions_count: usize,
    /// Çözülmüş sütun genişlikleri.
    pub panes: PaneWidths,
    /// Yan-detay eşiği için ikincil sütun sayısı (Mağaza/yerel=1, banka=2).
    pub secondary_cols: u16,
}

/// Terminal boyutuna göre fare tıklama alanlarını hesaplar.
pub fn click_areas(
    size: (u16, u16),
    show_help: bool,
    store_tab: bool,
    tool_detail_side: bool,
) -> ClickAreas {
    click_areas_ex(ClickAreasInput {
        size,
        show_help,
        store_tab,
        commands_tab: false,
        tool_detail_side,
        actions_count: 0,
        panes: PaneWidths::default(),
        secondary_cols: 1,
    })
}

/// Genişletilmiş tıklama alanı hesabı (Mağaza / Komut / AI panel / sütun prefs).
pub fn click_areas_ex(input: ClickAreasInput) -> ClickAreas {
    let ClickAreasInput {
        size,
        show_help,
        store_tab,
        commands_tab,
        tool_detail_side,
        actions_count,
        panes,
        secondary_cols,
    } = input;
    let (w, h) = size;
    let wide = w >= WIDE_MIN_WIDTH;
    let secondary_cols = secondary_cols.max(1);
    let store_detail_side =
        store_tab && wide && w >= detail_side_min_width(panes.tabs, panes.secondary, 1);
    let commands_detail_side = commands_tab
        && wide
        && w >= detail_side_min_width(panes.tabs, panes.secondary, secondary_cols);
    let show_detail = w >= MEDIUM_MIN_WIDTH && h >= 10;
    let tall = h >= TALL_MIN_HEIGHT;
    // AI aksiyonları modal popup; alt panel yüksekliği her zaman 0.
    let _ = (actions_count, panes.actions_height);
    let actions_h = 0u16;
    // Geniş mağaza / Komut Rehberi: detay sağ panel; araçta alt şerit.
    let detail_h = if show_detail {
        if store_tab {
            if store_detail_side {
                0
            } else {
                panes
                    .detail_height
                    .min(h.saturating_sub(CONTENT_MIN_HEIGHT + 3 + actions_h))
            }
        } else if commands_tab {
            if commands_detail_side {
                0
            } else {
                panes
                    .detail_height
                    .min(h.saturating_sub(CONTENT_MIN_HEIGHT + 3 + actions_h))
            }
        } else if tool_detail_side {
            0
        } else {
            panes.detail_height
        }
    } else {
        0
    };
    let help_height = if show_help {
        let reserved = 1 + 1 + CONTENT_MIN_HEIGHT + if show_detail { 2 } else { 0 } + 1;
        let reserved = if store_tab || commands_tab || tool_detail_side || actions_h > 0 {
            1 + 1 + CONTENT_MIN_HEIGHT + actions_h + detail_h + 1
        } else {
            reserved
        };
        h.saturating_sub(reserved).clamp(1, HELP_HELP_MAX_HEIGHT)
    } else if tall {
        3
    } else {
        1
    };
    let content_top = 2; // başlık(1) + arama(1)
    let content_bottom = h.saturating_sub(actions_h + detail_h + 1 + help_height);
    let content_height = content_bottom.saturating_sub(content_top);
    let actions_top = if actions_h > 0 {
        content_top.saturating_add(content_height)
    } else {
        u16::MAX
    };
    // Tüm sekmelerde sol sütun `panes.tabs` (ekran başına kalıcı).
    let tabs_w = panes.tabs;
    let category_end_x = if wide { tabs_w } else { w };
    let store_category_end_x = tabs_w.saturating_add(panes.secondary);
    let store_list_w = if store_detail_side {
        fit_list_width(
            w.saturating_sub(tabs_w).saturating_sub(panes.secondary),
            panes.list,
        )
    } else {
        panes.list
    };
    let store_apps_end_x = if store_detail_side {
        store_category_end_x.saturating_add(store_list_w)
    } else {
        w
    };
    let commands_list_w = if commands_detail_side {
        fit_list_width(
            w.saturating_sub(tabs_w)
                .saturating_sub(panes.secondary.saturating_mul(secondary_cols)),
            panes.list,
        )
    } else {
        panes.list
    };
    let commands_list_end_x = if commands_detail_side {
        tabs_w
            .saturating_add(panes.secondary.saturating_mul(secondary_cols))
            .saturating_add(commands_list_w)
    } else {
        w
    };
    // Kurulu sol sütun `panes.tabs`; liste tercihi yan detayda Length.
    let tools_list_w = if tool_detail_side {
        fit_list_width(w.saturating_sub(tabs_w), panes.list)
    } else if wide && !store_tab && !commands_tab {
        fit_list_width_alone(w.saturating_sub(tabs_w), panes.list)
    } else {
        panes.list
    };
    let tools_end_x = if tool_detail_side || (wide && !store_tab && !commands_tab) {
        tabs_w.saturating_add(tools_list_w)
    } else {
        w
    };
    // Dar modda mağaza kategorileri içerik alanının üstünde durur (+2 kenarlık).
    // `draw_store_content` ile aynı: uygulama listesine en az 3 satır bırak.
    let store_category_rows = panes
        .secondary_height
        .min(content_height.saturating_sub(3))
        .max(STORE_CAT_HEIGHT_MIN);
    ClickAreas {
        content_top,
        content_height,
        wide,
        category_end_x,
        secondary_width: panes.secondary,
        list_width: if store_detail_side {
            store_list_w
        } else if commands_detail_side {
            commands_list_w
        } else if tool_detail_side || (wide && !store_tab && !commands_tab) {
            tools_list_w
        } else {
            panes.list
        },
        store_category_end_x,
        store_apps_end_x,
        tools_end_x,
        store_category_end_y: content_top + store_category_rows,
        store_detail_side,
        commands_detail_side,
        commands_list_end_x,
        tool_detail_side,
        actions_top,
        actions_height: actions_h,
    }
}

/// Nova her karede `ListState`i `offset = 0` ile yeniden kurar; ratatui
/// tek satırlık öğelerde seçili satırı görünür tutmak için kaydırmayı
/// şöyle seçer. Fare isabet hesabı aynı formülü kullanmalıdır.
pub fn list_scroll_offset(selected: usize, inner_height: u16, len: usize) -> usize {
    let h = inner_height as usize;
    if h == 0 || len == 0 {
        return 0;
    }
    let max_off = len.saturating_sub(h);
    if selected >= h {
        (selected + 1).saturating_sub(h).min(max_off)
    } else {
        0
    }
}

/// Kenarlıklı liste panelinde tıklanan satırın öğe indeksini döner.
///
/// Üst/alt kenarlık ve listenin seçime göre kaydırılmış penceresi hesaba
/// katılır. Kenarlığa veya boş satıra tıklanınca `None`.
pub fn list_index_at_y(
    y: u16,
    panel_top: u16,
    panel_height: u16,
    selected: usize,
    len: usize,
) -> Option<usize> {
    if panel_height <= 2 || len == 0 {
        return None;
    }
    let inner_top = panel_top.saturating_add(LIST_BORDER);
    let inner_h = panel_height.saturating_sub(2 * LIST_BORDER);
    if y < inner_top || y >= inner_top.saturating_add(inner_h) {
        return None;
    }
    let visual = (y - inner_top) as usize;
    let offset = list_scroll_offset(selected, inner_h, len);
    let idx = offset + visual;
    (idx < len).then_some(idx)
}

// Düzen eşikleri birbirini mantıksal olarak izlemeli; ihlal derleme hatasıdır.
const _: () = assert!(WIDE_MIN_WIDTH > MEDIUM_MIN_WIDTH);
const _: () = assert!(TIGHT_HINT_WIDTH < WIDE_MIN_WIDTH);
const _: () = assert!(TIGHT_HINT_WIDTH > 20);
const _: () = assert!(TOOL_DETAIL_SIDE_MIN_WIDTH >= WIDE_MIN_WIDTH);
const _: () = assert!(
    TOOL_DETAIL_SIDE_MIN_WIDTH
        == SECONDARY_PANEL_WIDTH_DEFAULT + LIST_PANEL_MIN_WIDTH + TOOL_DETAIL_PANEL_WIDTH
);
const _: () = assert!(
    STORE_DETAIL_SIDE_MIN_WIDTH
        == CATEGORY_PANEL_WIDTH
            + SECONDARY_PANEL_WIDTH_DEFAULT
            + LIST_PANEL_MIN_WIDTH
            + STORE_DETAIL_PANEL_WIDTH
);
const _: () = assert!(STORE_CATEGORY_PANEL_WIDTH == SECONDARY_PANEL_WIDTH_DEFAULT);
const _: () = assert!(STORE_DETAIL_PANEL_WIDTH == TOOL_DETAIL_PANEL_WIDTH);
const _: () = assert!(SECONDARY_PANEL_WIDTH_DEFAULT > SECONDARY_PANEL_WIDTH_MIN);
const _: () = assert!(LIST_PANEL_WIDTH_DEFAULT >= LIST_PANEL_WIDTH_MIN);

/// Metni en fazla `max` karakter olacak şekilde keser (UTF-8 güvenli).
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// Metni kelime sınırlarında `width` sütuna sarar (UTF-8 güvenli).
///
/// Detay panelinde açıklamaların tek satıra sıkışıp `…` ile kaybolmasını
/// önlemek için kullanılır. Boş metinde boş vektör döner.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if word.chars().count() > width {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            let mut chunk = String::new();
            for ch in word.chars() {
                if chunk.chars().count() >= width {
                    lines.push(std::mem::take(&mut chunk));
                }
                chunk.push(ch);
            }
            if !chunk.is_empty() {
                current = chunk;
            }
            continue;
        }
        if current.is_empty() {
            current = word.to_string();
        } else if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Mağaza detayındaki tıklanabilir / Enter ile açılan bağlantı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreDetailLink {
    /// Kısa etiket (`Kaynak` / `Depo`).
    pub label: &'static str,
    /// Açılacak `https://` URL.
    pub url: String,
}

/// Seçili mağaza uygulaması için tarayıcıda açılabilir bağlantılar.
///
/// Sıra: önce proje kaynağı, varsa canlı takip deposu.
pub fn store_detail_links(app: &App) -> Vec<StoreDetailLink> {
    let Some(entry) = app.selected_store_app() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let source = entry.source.trim();
    if source.starts_with("https://") {
        out.push(StoreDetailLink {
            label: "Kaynak",
            url: source.to_string(),
        });
    }
    if let Some(release) = &entry.release {
        if let Some(url) = crate::browser::github_repo_https_url(&release.repo) {
            if out.iter().all(|l| l.url != url) {
                out.push(StoreDetailLink { label: "Depo", url });
            }
        }
    }
    out
}

/// Durum satırındaki mesajın rengi.
///
/// (Genel API: entegrasyon testleri renk ayrımını doğrudan sınar.)
///
/// Nova'nın ürettiği mesajlar sabit biçimdedir: başarı `✓ ` ile, hata
/// `Hata: ` ile başlar. Diğer her şey (nötr bilgi, onay istemi) sarıdır.
pub fn message_color(message: &str) -> Color {
    if message.starts_with('✓') {
        Color::Green
    } else if crate::i18n::Language::is_error_message(message) {
        Color::Red
    } else {
        Color::Yellow
    }
}

/// Mesajın simgesi (varsa).
fn message_symbol(message: &str) -> Option<&'static str> {
    if message.starts_with('✓') {
        Some("✓")
    } else if crate::i18n::Language::is_error_message(message) {
        Some("✗")
    } else {
        None
    }
}

/// Kurulu durumuna göre renk.
fn status_color(status: InstalledStatus) -> Color {
    match status {
        InstalledStatus::Installed => Color::Green,
        InstalledStatus::Missing => Color::Red,
        InstalledStatus::Unknown => Color::Yellow,
    }
}

/// TUI ekranını çizer.
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Açık temada yüzey boyanır; aksi hâlde kategori etiketleri (siyah metin)
    // koyu emülatör arka planında kaybolur.
    let pal = palette(app);
    if let Some(bg) = pal.surface {
        f.render_widget(Clear, area);
        f.render_widget(
            Block::default().style(Style::default().bg(bg).fg(pal.text)),
            area,
        );
    }

    let wide = area.width >= WIDE_MIN_WIDTH;
    let panes = app.pane_widths();
    let store_detail_side = app.store_tab
        && wide
        && area.width >= detail_side_min_width(panes.tabs, panes.secondary, 1);
    let commands_detail_side = app.commands_tab
        && wide
        && area.width
            >= detail_side_min_width(panes.tabs, panes.secondary, app.pane_secondary_cols());
    let tools_tab = !app.store_tab && !app.settings_tab && !app.commands_tab;
    let tool_detail_side = tools_tab && tool_detail_side_layout(wide, area.width, panes.tabs);
    let show_detail = area.width >= MEDIUM_MIN_WIDTH && area.height >= 10;
    let tall = area.height >= TALL_MIN_HEIGHT;
    // AI aksiyonları artık Mağaza onay modalı gibi ortalanmış popup'ta;
    // alt şerit / satır içi panel yok.
    let actions_h = 0u16;

    // Detay: geniş mağaza / Komut Rehberi / Kurulu sağ sütun; dar/orta alt şerit.
    let detail_h = if show_detail {
        if app.store_tab {
            if store_detail_side {
                0
            } else {
                panes.detail_height.min(
                    area.height
                        .saturating_sub(CONTENT_MIN_HEIGHT + 3 + actions_h),
                )
            }
        } else if app.commands_tab {
            if commands_detail_side {
                0
            } else {
                panes.detail_height.min(
                    area.height
                        .saturating_sub(CONTENT_MIN_HEIGHT + 3 + actions_h),
                )
            }
        } else if app.settings_tab || tool_detail_side {
            0
        } else {
            panes.detail_height
        }
    } else {
        0
    };

    // Yardım açıkken panel tuş tablosuna, dosya yollarına ve etkin çalışma
    // dizinine yer açar. Yükseklik ekranla birlikte büyür ama araç listesine
    // en az `CONTENT_MIN_HEIGHT` satır bırakır; küçük terminallerde ise
    // satırlar kırpılır (bkz. `draw_help`).
    let help_height = if app.show_help {
        let reserved = 1 + 1 + CONTENT_MIN_HEIGHT + if show_detail { 2 } else { 0 } + 1;
        let reserved = if app.store_tab || app.commands_tab || tool_detail_side || actions_h > 0 {
            1 + 1 + CONTENT_MIN_HEIGHT + actions_h + detail_h + 1
        } else {
            reserved
        };
        area.height
            .saturating_sub(reserved)
            .clamp(1, HELP_HELP_MAX_HEIGHT)
    } else if tall {
        3
    } else {
        1
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),           // başlık
            Constraint::Length(1),           // arama satırı
            Constraint::Min(3),              // içerik (kategori | liste)
            Constraint::Length(actions_h),   // (eski AI şerit yeri; artık 0)
            Constraint::Length(detail_h),    // detay
            Constraint::Length(1),           // durum
            Constraint::Length(help_height), // yardım
        ])
        .split(area);

    draw_header(f, app, chunks[0]);
    draw_search(f, app, chunks[1]);
    draw_content(f, app, chunks[2], wide);
    if show_detail {
        draw_detail(f, app, chunks[4]);
    }
    draw_status(f, app, chunks[5]);
    draw_help(f, app, chunks[6], tall);

    // Modal en son çizilir: altındaki içeriği örter, ana ekran tuşları da
    // modal açıkken işlenmez (bkz. `App::handle_key`).
    modal::draw(f, app);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let accent = palette(app).accent;
    let title = Paragraph::new(Line::from(Span::styled(
        "Nova",
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center);
    f.render_widget(title, area);
}

fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let lang = app.ui_lang();
    let (text, style) = if app.search_active || !app.search_query.is_empty() {
        let prefix = if app.commands_tab {
            match app.commands_mode {
                crate::commands::CommandsBrowseMode::SystemPath => "PATH / ",
                crate::commands::CommandsBrowseMode::ByTask => "görev / ",
                crate::commands::CommandsBrowseMode::Context => "bağlam / ",
                crate::commands::CommandsBrowseMode::Start => "başlangıç / ",
                crate::commands::CommandsBrowseMode::AllCommands => "komut / ",
            }
        } else if app.store_tab {
            "mağaza / "
        } else {
            "/ "
        };
        (
            format!("{prefix}{}", app.search_query),
            Style::default().fg(Color::Yellow),
        )
    } else if app.commands_tab {
        let hint = if area.width < TIGHT_HINT_WIDTH {
            lang.t(Msg::CommandsSearchHintTight)
        } else {
            lang.t(Msg::CommandsSearchHintWide)
        };
        (hint.to_string(), Style::default().fg(Color::DarkGray))
    } else if area.width < TIGHT_HINT_WIDTH {
        (
            lang.t(Msg::SearchHintTight).to_string(),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (
            lang.t(Msg::SearchHintWide).to_string(),
            Style::default().fg(Color::DarkGray),
        )
    };
    let text = truncate(&text, area.width.saturating_sub(1) as usize);
    f.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
}

fn draw_content(f: &mut Frame, app: &App, area: Rect, wide: bool) {
    let panes = app.pane_widths();
    if app.commands_tab {
        draw_commands_content(f, app, area, wide, panes);
    } else if app.store_tab {
        draw_store_content(f, app, area, wide, panes);
    } else if app.settings_tab {
        if wide {
            if app.settings_side_picker_open() {
                // Sekme (`tabs`) + ayar (`secondary`) + seçici (`list`).
                let after_tabs = area
                    .width
                    .saturating_sub(panes.tabs.saturating_add(panes.secondary));
                let list_w = fit_list_width_alone(after_tabs, panes.list);
                let panels = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(panes.tabs),
                        Constraint::Length(panes.secondary),
                        Constraint::Length(list_w),
                        Constraint::Min(0),
                    ])
                    .split(area);
                draw_categories(f, app, panels[0]);
                draw_settings_panel(f, app, panels[1]);
                if app.language_picker_open {
                    draw_language_picker(f, app, panels[2]);
                } else {
                    draw_theme_picker(f, app, panels[2]);
                }
            } else {
                // Sol sekme `tabs`; ayar paneli `list`.
                let list_w =
                    fit_list_width_alone(area.width.saturating_sub(panes.tabs), panes.list);
                let panels = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(panes.tabs),
                        Constraint::Length(list_w),
                        Constraint::Min(0),
                    ])
                    .split(area);
                draw_categories(f, app, panels[0]);
                draw_settings_panel(f, app, panels[1]);
            }
        } else if app.settings_side_picker_open() {
            let half = (area.height / 2).max(3);
            let panels = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(half), Constraint::Min(3)])
                .split(area);
            draw_settings_panel(f, app, panels[0]);
            if app.language_picker_open {
                draw_language_picker(f, app, panels[1]);
            } else {
                draw_theme_picker(f, app, panels[1]);
            }
        } else {
            draw_settings_panel(f, app, area);
        }
    } else if wide {
        // Kurulu yan-detay: sekme | liste | detay. Odaklı panelde `-`/`+`.
        // AI aksiyonları modal; alt şerit yok.
        if tool_detail_side_layout(true, area.width, panes.tabs) {
            let list_w = fit_list_width(area.width.saturating_sub(panes.tabs), panes.list);
            let panels = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(panes.tabs),
                    Constraint::Length(list_w),
                    Constraint::Min(panes.detail_width.min(TOOL_DETAIL_PANEL_WIDTH)),
                ])
                .split(area);
            draw_categories(f, app, panels[0]);
            draw_tools(f, app, panels[1]);
            draw_tool_detail_info(f, app, panels[2], true);
        } else {
            let list_w = fit_list_width_alone(area.width.saturating_sub(panes.tabs), panes.list);
            let panels = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(panes.tabs),
                    Constraint::Length(list_w),
                    Constraint::Min(0),
                ])
                .split(area);
            draw_categories(f, app, panels[0]);
            draw_tools(f, app, panels[1]);
        }
    } else {
        // Tek sütun: kategori seçimi başlıkta özetlenir.
        draw_tools(f, app, area);
    }
}

/// Komut Rehberi içerik alanı.
///
/// Görev odaklı IA: seçim (mod) → [grup] → liste → detay. PATH ve katalog
/// listeleri birleştirilmez. Mağaza bu sekmede yok. Sütun genişlikleri bu
/// ekranın [`PaneWidths`] tercihleridir (diğer ekranlar değişmez).
fn draw_commands_content(f: &mut Frame, app: &App, area: Rect, wide: bool, panes: PaneWidths) {
    let tabs = panes.tabs;
    let secondary = panes.secondary;
    let cols = app.pane_secondary_cols();
    let side = wide && area.width >= detail_side_min_width(tabs, secondary, cols);
    let grouped = app.commands_mode.needs_group_column();
    if wide {
        if grouped {
            if side {
                let after_fixed = area
                    .width
                    .saturating_sub(tabs)
                    .saturating_sub(secondary.saturating_mul(2));
                let list_w = fit_list_width(after_fixed, panes.list);
                let panels = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(tabs),
                        Constraint::Length(secondary),
                        Constraint::Length(secondary),
                        Constraint::Length(list_w),
                        Constraint::Min(panes.detail_width.min(STORE_DETAIL_PANEL_WIDTH)),
                    ])
                    .split(area);
                draw_categories(f, app, panels[0]);
                draw_commands_mode(f, app, panels[1]);
                draw_commands_group(f, app, panels[2]);
                draw_commands_list(f, app, panels[3]);
                draw_commands_detail_panel(f, app, panels[4]);
            } else {
                let after_fixed = area
                    .width
                    .saturating_sub(tabs)
                    .saturating_sub(secondary.saturating_mul(2));
                let list_w = fit_list_width_alone(after_fixed, panes.list);
                let panels = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(tabs),
                        Constraint::Length(secondary),
                        Constraint::Length(secondary),
                        Constraint::Length(list_w),
                        Constraint::Min(0),
                    ])
                    .split(area);
                draw_categories(f, app, panels[0]);
                draw_commands_mode(f, app, panels[1]);
                draw_commands_group(f, app, panels[2]);
                draw_commands_list(f, app, panels[3]);
            }
        } else if side {
            let after_fixed = area.width.saturating_sub(tabs).saturating_sub(secondary);
            let list_w = fit_list_width(after_fixed, panes.list);
            let panels = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(tabs),
                    Constraint::Length(secondary),
                    Constraint::Length(list_w),
                    Constraint::Min(panes.detail_width.min(STORE_DETAIL_PANEL_WIDTH)),
                ])
                .split(area);
            draw_categories(f, app, panels[0]);
            draw_commands_mode(f, app, panels[1]);
            draw_commands_list(f, app, panels[2]);
            draw_commands_detail_panel(f, app, panels[3]);
        } else {
            let after_fixed = area.width.saturating_sub(tabs).saturating_sub(secondary);
            let list_w = fit_list_width_alone(after_fixed, panes.list);
            let panels = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(tabs),
                    Constraint::Length(secondary),
                    Constraint::Length(list_w),
                    Constraint::Min(0),
                ])
                .split(area);
            draw_categories(f, app, panels[0]);
            draw_commands_mode(f, app, panels[1]);
            draw_commands_list(f, app, panels[2]);
        }
    } else {
        let src_h = panes
            .secondary_height
            .min(area.height.saturating_sub(3))
            .max(3);
        let panels = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(src_h), Constraint::Min(3)])
            .split(area);
        draw_commands_mode(f, app, panels[0]);
        draw_commands_list(f, app, panels[1]);
    }
}

fn commands_mode_label(mode: crate::commands::CommandsBrowseMode, en: bool) -> &'static str {
    use crate::commands::CommandsBrowseMode::*;
    match (mode, en) {
        (Start, false) => "1 Başlangıç",
        (Start, true) => "1 Start",
        (ByTask, false) => "2 Göreve göre",
        (ByTask, true) => "2 By task",
        (Context, false) => "3 Kullanım bağlamı",
        (Context, true) => "3 Usage context",
        (AllCommands, false) => "4 Tüm komutlar",
        (AllCommands, true) => "4 All commands",
        (SystemPath, false) => "5 Sistemde bulunanlar",
        (SystemPath, true) => "5 On this system",
    }
}

fn draw_commands_mode(f: &mut Frame, app: &App, area: Rect) {
    use crate::commands::CommandsBrowseMode;
    let pal = palette(app);
    let lang = app.ui_lang();
    let en = lang.id().to_ascii_lowercase().starts_with("en");
    let fav_n = app.state.file.command_favorites.len();
    let items: Vec<ListItem> = CommandsBrowseMode::ALL
        .iter()
        .map(|mode| {
            let selected = app.commands_mode == *mode;
            let focused = selected && app.focus == PanelFocus::CommandsSource;
            let style = if focused {
                Style::default()
                    .fg(pal.accent_on)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(pal.accent)
            } else {
                Style::default().fg(pal.text)
            };
            let label = truncate(
                commands_mode_label(*mode, en),
                area.width.saturating_sub(6) as usize,
            );
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "▶ " } else { "  " },
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(label, style),
            ]))
        })
        .collect();
    let border = if app.focus == PanelFocus::CommandsSource {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    let title = if fav_n > 0 {
        format!(" {} · ★{fav_n} ", lang.t(Msg::CommandsSourceTitle))
    } else {
        format!(" {} ", lang.t(Msg::CommandsSourceTitle))
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(title),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.commands_mode.index()));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_commands_group(f: &mut Frame, app: &App, area: Rect) {
    use crate::commands::{CommandsBrowseMode, VerificationStatus};
    let pal = palette(app);
    let lang = app.ui_lang();
    let lang_id = lang.id();
    let en = lang_id.to_ascii_lowercase().starts_with("en");
    let (title, items): (String, Vec<ListItem>) = match app.commands_mode {
        CommandsBrowseMode::ByTask => {
            let cats = app.commands_bank.task_categories();
            let items = cats
                .iter()
                .enumerate()
                .map(|(i, cat)| {
                    let count = app.commands_bank.tasks_in_category(cat).len();
                    let selected = i == app.commands_category_index;
                    let focused = selected && app.focus == PanelFocus::CommandsCategory;
                    let style = row_style(pal, selected, focused);
                    let label = truncate(
                        &format!("{} ({count})", lang.bank_category(cat)),
                        area.width.saturating_sub(6) as usize,
                    );
                    list_row(selected, label, style)
                })
                .collect();
            let title = if en {
                " Task groups "
            } else {
                " Görev grupları "
            };
            (title.into(), items)
        }
        CommandsBrowseMode::Context => {
            let items = app
                .commands_bank
                .profiles
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let selected = i == app.commands_category_index;
                    let focused = selected && app.focus == PanelFocus::CommandsCategory;
                    let style = row_style(pal, selected, focused);
                    let checked = if app.commands_selected_profiles.contains(&p.id) {
                        "[x]"
                    } else {
                        "[ ]"
                    };
                    let draft = if p.verification.status == VerificationStatus::Draft {
                        " ·draft"
                    } else {
                        ""
                    };
                    let label = truncate(
                        &format!("{checked} {}{draft}", p.name.pick(lang_id)),
                        area.width.saturating_sub(6) as usize,
                    );
                    list_row(selected, label, style)
                })
                .collect();
            let title = if en { " Profiles " } else { " Profiller " };
            (title.into(), items)
        }
        CommandsBrowseMode::AllCommands => {
            let cats = app.commands_bank_categories();
            let items = cats
                .iter()
                .enumerate()
                .map(|(i, cat)| {
                    let count = app.commands_bank.by_category(cat).len();
                    let selected = i == app.commands_category_index;
                    let focused = selected && app.focus == PanelFocus::CommandsCategory;
                    let style = row_style(pal, selected, focused);
                    let label = truncate(
                        &format!("{} ({count})", lang.bank_category(cat)),
                        area.width.saturating_sub(6) as usize,
                    );
                    list_row(selected, label, style)
                })
                .collect();
            (format!(" {} ", lang.t(Msg::CommandsCategories)), items)
        }
        _ => (String::new(), Vec::new()),
    };
    let border = if app.focus == PanelFocus::CommandsCategory {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(title),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.commands_category_index));
    f.render_stateful_widget(list, area, &mut state);
}

fn row_style(pal: ThemePalette, selected: bool, focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(pal.accent_on)
            .bg(pal.accent)
            .add_modifier(Modifier::BOLD)
    } else if selected {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.text)
    }
}

fn list_row(selected: bool, label: String, style: Style) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(
            if selected { "▶ " } else { "  " },
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(label, style),
    ]))
}

fn draw_commands_list(f: &mut Frame, app: &App, area: Rect) {
    use crate::commands::{CommandsBrowseMode, VerificationStatus};
    let pal = palette(app);
    let lang = app.ui_lang();
    let lang_id = lang.id();
    let searching = !app.search_query.is_empty();
    let (title, items): (String, Vec<ListItem>) =
        if app.commands_mode == CommandsBrowseMode::SystemPath {
            let ents = app.filtered_local_commands();
            let items = if ents.is_empty() {
                vec![ListItem::new(Span::styled(
                    lang.t(Msg::EmptyLocalCommands),
                    Style::default().fg(pal.muted),
                ))]
            } else {
                ents.iter()
                    .enumerate()
                    .map(|(i, ent)| {
                        let selected = i == app.commands_entry_index;
                        let focused = selected && app.focus == PanelFocus::CommandsList;
                        let style = row_style(pal, selected, focused);
                        let bank = if ent.bank_id.is_some() {
                            " ·katalog?"
                        } else {
                            " ·yalnız PATH"
                        };
                        let label = truncate(
                            &format!("{}{bank}", ent.name),
                            area.width.saturating_sub(6) as usize,
                        );
                        list_row(selected, label, style)
                    })
                    .collect()
            };
            let title = if searching {
                format!(
                    " {} /{} ({}) ",
                    lang.t(Msg::CommandsLocalList),
                    truncate(&app.search_query, 12),
                    ents.len()
                )
            } else {
                format!(" {} ({}) ", lang.t(Msg::CommandsLocalList), ents.len())
            };
            (title, items)
        } else if app.commands_list_shows_tasks() {
            let tasks = app.filtered_tasks();
            let items = if tasks.is_empty() {
                vec![ListItem::new(Span::styled(
                    if searching {
                        lang.t(Msg::EmptyCommandsSearch)
                    } else {
                        "Bu grupta görev yok."
                    },
                    Style::default().fg(pal.muted),
                ))]
            } else {
                tasks
                    .iter()
                    .enumerate()
                    .map(|(i, task)| {
                        let selected = i == app.commands_entry_index;
                        let focused = selected && app.focus == PanelFocus::CommandsList;
                        let style = row_style(pal, selected, focused);
                        let draft = if task.verification.status == VerificationStatus::Draft {
                            " ·draft"
                        } else {
                            ""
                        };
                        let label = truncate(
                            &format!("{}{draft}", task.name.pick(lang_id)),
                            area.width.saturating_sub(6) as usize,
                        );
                        list_row(selected, label, style)
                    })
                    .collect()
            };
            let title = if searching {
                format!(
                    " Görevler /{} ({}) ",
                    truncate(&app.search_query, 12),
                    tasks.len()
                )
            } else {
                format!(" Görevler ({}) ", tasks.len())
            };
            (title, items)
        } else {
            let cmds = app.filtered_bank_commands();
            let items = if cmds.is_empty() {
                let empty = if searching {
                    lang.t(Msg::EmptyCommandsSearch)
                } else if app.command_favorite_only {
                    lang.t(Msg::EmptyCommandsFavorites)
                } else if app.commands_mode == CommandsBrowseMode::Context
                    && app.commands_selected_profiles.is_empty()
                {
                    "Profil seçin (Space) — draft/doğrulanmamış işaretli."
                } else if app.commands_task_drill.is_some() {
                    "Bu görevin ilgili komutu yok."
                } else {
                    lang.t(Msg::EmptyCommandsCategory)
                };
                vec![ListItem::new(Span::styled(
                    empty,
                    Style::default().fg(pal.muted),
                ))]
            } else {
                cmds.iter()
                    .enumerate()
                    .map(|(i, cmd)| {
                        let selected = i == app.commands_entry_index;
                        let focused = selected && app.focus == PanelFocus::CommandsList;
                        let style = row_style(pal, selected, focused);
                        let fav = if app.is_command_favorite(&cmd.id) {
                            "★"
                        } else {
                            " "
                        };
                        let draft = if cmd.verification.status == VerificationStatus::Draft {
                            "·draft"
                        } else {
                            ""
                        };
                        let risk = cmd.risk.marker();
                        let cat = if searching
                            || matches!(
                                app.commands_mode,
                                CommandsBrowseMode::Start | CommandsBrowseMode::Context
                            )
                            || app.commands_task_drill.is_some()
                        {
                            format!(" [{}]", lang.bank_category(&cmd.category))
                        } else {
                            String::new()
                        };
                        let label = truncate(
                            &format!("{fav}{}{risk}{draft}{cat}", cmd.name),
                            area.width.saturating_sub(6) as usize,
                        );
                        list_row(selected, label, style)
                    })
                    .collect()
            };
            let list_title = match app.commands_mode {
                CommandsBrowseMode::Start => "Öğrenme",
                CommandsBrowseMode::Context => "Bağlam komutları",
                CommandsBrowseMode::ByTask => "İlgili komutlar",
                _ => lang.t(Msg::CommandsList),
            };
            let mut title = format!(" {list_title} ({}) ", cmds.len());
            if app.command_favorite_only {
                title = format!(" {list_title} ★ ");
            }
            if searching {
                title = format!(
                    " {list_title} /{} ({}) ",
                    truncate(&app.search_query, 12),
                    cmds.len()
                );
            }
            (title, items)
        };
    let border = if app.focus == PanelFocus::CommandsList {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(title),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.commands_entry_index));
    f.render_stateful_widget(list, area, &mut state);
}

/// Mağaza sekmesinin içerik alanı.
///
/// Yeterince geniş terminalde **dört sütun**: sekme listesi, mağaza
/// kategorileri, uygulamalar ve sağda zengin detay. Daha dar geniş
/// modda üç sütun (detay alt şeritte kalır). Dar terminalde yatay
/// sütunlar sığmaz; kategoriler üste, uygulamalar alta yerleşir.
///
/// İkincil (kategori) ve liste genişlikleri bu ekranın [`PaneWidths`] ile
/// ayarlanır; yan detayda fazla alan detaya gider.
fn draw_store_content(f: &mut Frame, app: &App, area: Rect, wide: bool, panes: PaneWidths) {
    let tabs = panes.tabs;
    let secondary = panes.secondary;
    if wide && area.width >= detail_side_min_width(tabs, secondary, 1) {
        // Liste tercih genişliğinde; artan alan detaya (`Min`) gider.
        let after_fixed = area.width.saturating_sub(tabs).saturating_sub(secondary);
        let list_w = fit_list_width(after_fixed, panes.list);
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(tabs),
                Constraint::Length(secondary),
                Constraint::Length(list_w),
                Constraint::Min(panes.detail_width.min(STORE_DETAIL_PANEL_WIDTH)),
            ])
            .split(area);
        draw_categories(f, app, panels[0]);
        draw_store_categories(f, app, panels[1]);
        draw_store_apps(f, app, panels[2]);
        draw_store_detail_panel(f, app, panels[3]);
    } else if wide {
        let after_fixed = area.width.saturating_sub(tabs).saturating_sub(secondary);
        let list_w = fit_list_width_alone(after_fixed, panes.list);
        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(tabs),
                Constraint::Length(secondary),
                Constraint::Length(list_w),
                Constraint::Min(0),
            ])
            .split(area);
        draw_categories(f, app, panels[0]);
        draw_store_categories(f, app, panels[1]);
        draw_store_apps(f, app, panels[2]);
    } else {
        // Uygulama listesine en az üç satır bırakılır.
        let cat_height = panes
            .secondary_height
            .min(area.height.saturating_sub(3))
            .max(STORE_CAT_HEIGHT_MIN);
        let panels = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(cat_height), Constraint::Min(3)])
            .split(area);
        draw_store_categories(f, app, panels[0]);
        draw_store_apps(f, app, panels[1]);
    }
}

fn draw_categories(f: &mut Frame, app: &App, area: Rect) {
    let pal = palette(app);
    // Yalnızca kurulu araç içeren kategoriler gösterilir (boşlar gizlenir).
    let mut items: Vec<ListItem> = app
        .visible_category_indices()
        .into_iter()
        .map(|i| {
            let cat = CATEGORIES[i];
            let count = app
                .catalog
                .by_category(cat)
                .into_iter()
                .filter(|e| {
                    e.status == InstalledStatus::Installed
                        && app.matches_search(e)
                        && (!app.favorite_only || e.favorite)
                })
                .count();
            let selected =
                i == app.category_index && !app.store_tab && !app.settings_tab && !app.commands_tab;
            let focused = selected && app.focus == PanelFocus::Category;
            let style = if focused {
                Style::default()
                    .fg(pal.accent_on)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(pal.accent)
            } else if count == 0 {
                Style::default().fg(pal.muted)
            } else {
                Style::default().fg(pal.text)
            };
            let prefix = if selected { "▶ " } else { "  " };
            let lang = app.ui_lang();
            let label = truncate(
                &format!("{} ({count})", lang.catalog_short(cat)),
                area.width.saturating_sub(6) as usize,
            );
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(label, style),
            ]))
        })
        .collect();

    // Komut Rehberi, ardından Mağaza, ardından Ayarlar.
    let lang = app.ui_lang();
    let commands_count = app.commands_bank.commands.len();
    let commands_selected = app.commands_tab;
    let commands_focused = commands_selected && app.focus == PanelFocus::Category;
    let commands_style = if commands_focused {
        Style::default()
            .fg(pal.accent_on)
            .bg(pal.accent)
            .add_modifier(Modifier::BOLD)
    } else if commands_selected {
        Style::default().fg(pal.accent)
    } else if commands_count == 0 {
        Style::default().fg(pal.muted)
    } else {
        Style::default().fg(pal.text)
    };
    let commands_label = truncate(
        &format!("{} ({commands_count})", lang.t(Msg::CommandsGuide)),
        area.width.saturating_sub(6) as usize,
    );
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            if commands_selected { "▶ " } else { "  " },
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(commands_label, commands_style),
    ])));

    let store_count = app.store.entries().len();
    let store_selected = app.store_tab;
    let store_focused = store_selected && app.focus == PanelFocus::Category;
    let store_style = if store_focused {
        Style::default()
            .fg(pal.accent_on)
            .bg(pal.accent)
            .add_modifier(Modifier::BOLD)
    } else if store_selected {
        Style::default().fg(pal.accent)
    } else if store_count == 0 {
        Style::default().fg(pal.muted)
    } else {
        Style::default().fg(pal.text)
    };
    let store_label = truncate(
        &format!("{} ({store_count})", lang.t(Msg::Store)),
        area.width.saturating_sub(6) as usize,
    );
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            if store_selected { "▶ " } else { "  " },
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(store_label, store_style),
    ])));

    let settings_selected = app.settings_tab;
    let settings_focused = settings_selected && app.focus == PanelFocus::Category;
    let settings_style = if settings_focused {
        Style::default()
            .fg(pal.accent_on)
            .bg(pal.accent)
            .add_modifier(Modifier::BOLD)
    } else if settings_selected {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.text)
    };
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            if settings_selected { "▶ " } else { "  " },
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate(lang.t(Msg::Settings), area.width.saturating_sub(6) as usize),
            settings_style,
        ),
    ])));

    let border = if app.focus == PanelFocus::Category {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    // Satır stilleri zaten seçimi gösteriyor; List’in varsayılan REVERSED
    // vurgusu açık temada kategori adlarını siler — kapatılır.
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(format!(" {} ", lang.t(Msg::Categories))),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.selected_tab_index()));
    f.render_stateful_widget(list, area, &mut state);
}

/// Ayarlar sekmesi: Güncelle + Tema + Dil.
fn draw_settings_panel(f: &mut Frame, app: &App, area: Rect) {
    let pal = palette(app);
    let lang = app.ui_lang();
    let theme = app.ui_theme();
    let theme_hint = if app.theme_picker_open {
        format!("{} — {}", theme.label_for(lang), lang.t(Msg::ThemeHintOpen))
    } else {
        format!(
            "{} ({}) — {}",
            theme.label_for(lang),
            theme.id(),
            lang.t(Msg::ThemeHintClosed)
        )
    };
    let language = app.ui_lang();
    let language_hint = if app.language_picker_open {
        format!(
            "{} — {}",
            language.native_name(),
            lang.t(Msg::LanguageHintOpen)
        )
    } else {
        format!(
            "{} ({}) — {}",
            language.native_name(),
            language.id(),
            lang.t(Msg::LanguageHintClosed)
        )
    };
    let actions = [
        (lang.t(Msg::Update), lang.t(Msg::UpdateHint)),
        (lang.t(Msg::UpdateCatalog), lang.t(Msg::UpdateCatalogHint)),
        (
            lang.t(Msg::UpdateCommandsBank),
            lang.t(Msg::UpdateCommandsBankHint),
        ),
        (lang.t(Msg::AddCustomTool), lang.t(Msg::AddCustomToolHint)),
        (lang.t(Msg::Theme), theme_hint.as_str()),
        (lang.t(Msg::Language), language_hint.as_str()),
    ];
    let items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, (title, hint))| {
            let selected = i == app.settings_action_index;
            let focused = selected && app.focus == PanelFocus::Tool;
            let style = if focused {
                Style::default()
                    .fg(pal.accent_on)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(pal.accent)
            } else {
                Style::default().fg(pal.text)
            };
            let prefix = if selected { "▶ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(format!("[{title}]  "), style),
                Span::styled((*hint).to_string(), Style::default().fg(pal.muted)),
            ]))
        })
        .collect();
    let border = if app.focus == PanelFocus::Tool {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(format!(" {} ", lang.t(Msg::Settings))),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.settings_action_index));
    f.render_stateful_widget(list, area, &mut state);
}

/// Tema seçici yan paneli (Ayarlar → Tema).
fn draw_theme_picker(f: &mut Frame, app: &App, area: Rect) {
    let pal = palette(app);
    let lang = app.ui_lang();
    let current = app.ui_theme();
    let items: Vec<ListItem> = UiTheme::all()
        .iter()
        .enumerate()
        .map(|(i, theme)| {
            let selected = i == app.theme_picker_index;
            let focused = selected && app.focus == PanelFocus::ThemePicker;
            let style = if focused {
                Style::default()
                    .fg(pal.accent_on)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(pal.accent)
            } else {
                Style::default().fg(pal.text)
            };
            let mark = if *theme == current { "● " } else { "○ " };
            let prefix = if selected { "▶ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(mark, Style::default().fg(pal.accent)),
                Span::styled(format!("{} ({})", theme.label_for(lang), theme.id()), style),
            ]))
        })
        .collect();
    let border = if app.focus == PanelFocus::ThemePicker {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(format!(" {} ", lang.t(Msg::ThemePickTitle))),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.theme_picker_index));
    f.render_stateful_widget(list, area, &mut state);
}

/// Dil seçici yan paneli (Ayarlar → Dil).
fn draw_language_picker(f: &mut Frame, app: &App, area: Rect) {
    let pal = palette(app);
    let lang = app.ui_lang();
    let current = lang;
    let items: Vec<ListItem> = crate::i18n::Language::all()
        .iter()
        .enumerate()
        .map(|(i, language)| {
            let selected = i == app.language_picker_index;
            let focused = selected && app.focus == PanelFocus::LanguagePicker;
            let style = if focused {
                Style::default()
                    .fg(pal.accent_on)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(pal.accent)
            } else {
                Style::default().fg(pal.text)
            };
            let mark = if *language == current { "● " } else { "○ " };
            let prefix = if selected { "▶ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(mark, Style::default().fg(pal.accent)),
                Span::styled(
                    format!("{} ({})", language.native_name(), language.id()),
                    style,
                ),
            ]))
        })
        .collect();
    let border = if app.focus == PanelFocus::LanguagePicker {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(format!(" {} ", lang.t(Msg::LanguagePickTitle))),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.language_picker_index));
    f.render_stateful_widget(list, area, &mut state);
}

/// Mağaza kategorileri panelini çizer.
fn draw_store_categories(f: &mut Frame, app: &App, area: Rect) {
    let pal = palette(app);
    let items: Vec<ListItem> = STORE_CATEGORIES
        .iter()
        .enumerate()
        .map(|(i, cat)| {
            let count = app.store.by_category(cat).len();
            let selected = i == app.store_category_index;
            let focused = selected && app.focus == PanelFocus::StoreCategory;
            let style = if focused {
                Style::default()
                    .fg(pal.accent_on)
                    .bg(pal.accent)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(pal.accent)
            } else if count == 0 {
                Style::default().fg(pal.muted)
            } else {
                Style::default().fg(pal.text)
            };
            let prefix = if selected { "▶ " } else { "  " };
            // Panelin iç genişliği (kenarlıklar düşülmüş) ve "▶ " öneki
            // hesaba katılır; kategori adı ile sayı birlikte sığmalıdır.
            let label = truncate(
                &format!("{} ({count})", app.ui_lang().store_category(cat)),
                area.width.saturating_sub(4) as usize,
            );
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(label, style),
            ]))
        })
        .collect();

    let border = if app.focus == PanelFocus::StoreCategory {
        Style::default().fg(pal.accent)
    } else {
        Style::default().fg(pal.muted)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(format!(" {} ", app.ui_lang().t(Msg::StoreCategories))),
        )
        .highlight_style(Style::default());
    let mut state = ListState::default().with_selected(Some(app.store_category_index));
    f.render_stateful_widget(list, area, &mut state);
}

/// Mağaza uygulama listesi panelini çizer.
fn draw_store_apps(f: &mut Frame, app: &App, area: Rect) {
    let apps = app.filtered_store_apps();
    let inner_width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = apps
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let selected = i == app.store_app_index;
            let focused = selected && app.focus == PanelFocus::StoreApp;
            let style = if focused {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            let prefix = if selected { "▶ " } else { "  " };

            // Durum işareti: yeni sürüm / PM güncellemesi varsa ↑, kuruluysa ✓.
            let has_update = app.store_has_pending_update(&entry.id);
            let (marker, marker_style) = if has_update {
                ("↑ ", Style::default().fg(Color::Yellow))
            } else if app.store_status(&entry.id) == crate::detection::InstalledStatus::Installed {
                ("✓ ", Style::default().fg(Color::Green))
            } else {
                ("· ", Style::default().fg(Color::DarkGray))
            };

            // Güven rozeti (manual/hash) liste satırında gösterilmez; detay panelinde kalır.
            let name = truncate(&entry.name, inner_width.saturating_sub(6));
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(marker, marker_style),
                Span::styled(name, style),
            ]))
        })
        .collect();

    let title = if !app.search_query.is_empty() {
        format!(
            " 🔍 '{}' ({}) ",
            truncate(&app.search_query, 20),
            apps.len()
        )
    } else {
        format!(
            " {} ({}) ",
            truncate(
                app.ui_lang().store_category(app.selected_store_category()),
                20
            ),
            apps.len()
        )
    };

    let border = if app.focus == PanelFocus::StoreApp {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(title),
    );
    let mut state = ListState::default();
    if !apps.is_empty() {
        state.select(Some(app.store_app_index.min(apps.len() - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);
}

/// Bir araç satırındaki sabit sütun sayısı
/// (vurgu işareti 2 + favori 1 + boşluk 1 + durum 1 + boşluk 1).
/// Testlerin toplamı yeniden hesaplarken kullandığı doğrulama değeridir.
pub const TOOL_ROW_FIXED_WIDTH: usize = 6;

/// Liste satırında ada ayrılan genişlik (açıklama liste satırında yok;
/// detay paneli taşır). `inner` kenarlıklar hariç sütun sayısıdır.
pub fn tool_row_name_width(inner: usize, max_width: usize) -> usize {
    if inner < TOOL_ROW_FIXED_WIDTH {
        return 0;
    }
    let free = inner - TOOL_ROW_FIXED_WIDTH;
    free.min(max_width)
}

/// Geriye dönük test uyumu: açıklama sütunu her zaman 0 (liste satırında yok).
pub fn tool_row_widths(inner: usize, max_width: usize) -> (usize, usize) {
    (tool_row_name_width(inner, max_width), 0)
}

fn draw_tools(f: &mut Frame, app: &App, area: Rect) {
    let tools = app.filtered_tools();
    // Kenarlıklar (2 sütun) satır içeriğine dahil değildir.
    let inner_width = area.width.saturating_sub(2) as usize;
    // highlight_symbol ("▶ ") 2 sütun kaplar; ada kalan yer.
    let name_width = tool_row_name_width(
        inner_width,
        inner_width.saturating_sub(TOOL_ROW_FIXED_WIDTH),
    );
    let hits = tool_list_hits(app);

    let items: Vec<ListItem> = hits
        .iter()
        .map(|hit| {
            let ToolListHit::Tool(idx) = *hit;
            let entry = tools[idx];
            let fav = if entry.favorite { "★" } else { "☆" };
            let name = truncate(&entry.name, name_width.max(1));
            ListItem::new(Line::from(vec![
                Span::styled(fav, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(
                    entry.status.symbol(),
                    Style::default().fg(status_color(entry.status)),
                ),
                Span::raw(" "),
                Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    if let Some(sel) = tool_list_selected_visual(app) {
        state.select(Some(sel));
    }

    let lang = app.ui_lang();
    let category_label = app
        .selected_category()
        .map(|c| lang.catalog_category(c))
        .unwrap_or("");
    let title = if !app.search_query.is_empty() {
        format!(
            " 🔍 '{}' ({}) ",
            truncate(&app.search_query, 20),
            tools.len()
        )
    } else if app.favorite_only {
        format!(" ★ {} ({}) ", lang.t(Msg::Favorites), tools.len())
    } else {
        format!(
            " {} ({}) ",
            truncate(category_label, area.width.saturating_sub(26) as usize),
            tools.len()
        )
    };

    let border = if app.focus == PanelFocus::Tool {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(truncate(&title, area.width.saturating_sub(2) as usize));

    if tools.is_empty() {
        let empty = if app.favorite_only {
            "Bu görünümde favori araç yok.  F ile favorilere ekleyebilirsin."
        } else if !app.search_query.is_empty() {
            "Aramayla eşleşen araç yok.  Esc ile aramayı temizle."
        } else {
            "Bu kategoride araç yok. Özel araç eklemek için ? yardımındaki yapılandırma yoluna bak."
        };
        let para = Paragraph::new(Line::from(Span::styled(
            empty,
            Style::default().fg(Color::DarkGray),
        )))
        .wrap(Wrap { trim: true })
        .block(block);
        f.render_widget(para, area);
        return;
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, area, &mut state);
}

/// Mağaza sağ detay paneli (geniş dört sütun düzeni).
fn draw_store_detail_panel(f: &mut Frame, app: &App, area: Rect) {
    let lang = app.ui_lang();
    let pal = palette(app);
    let focused = app.focus == PanelFocus::StoreDetail;
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(pal.muted)
    };
    let lines = store_detail_lines(app, area);
    let detail = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(format!(" {} ", lang.t(Msg::StoreDetail))),
    );
    f.render_widget(detail, area);
}

fn draw_commands_detail_panel(f: &mut Frame, app: &App, area: Rect) {
    let lang = app.ui_lang();
    let pal = palette(app);
    let focused = app.focus == PanelFocus::CommandsDetail;
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(pal.muted)
    };
    let lines = commands_detail_lines(app);
    let detail = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(format!(" {} ", lang.t(Msg::CommandsDetail))),
    );
    f.render_widget(detail, area);
}

fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    // Geniş mağaza / Komut Rehberi sağ-detayında alt şerit boş bırakılır.
    if area.height == 0 {
        return;
    }
    let lang = app.ui_lang();
    if app.commands_tab {
        let lines = commands_detail_lines(app);
        let detail = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::TOP)
                .title(format!(" {} ", lang.t(Msg::CommandsDetail))),
        );
        f.render_widget(detail, area);
        return;
    }
    if app.store_tab {
        let lines = store_detail_lines(app, area);
        let detail = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::TOP)
                .title(format!(" {} ", lang.t(Msg::StoreDetail))),
        );
        f.render_widget(detail, area);
        return;
    }

    // AI aksiyonları modal popup'ta; burası yalnızca Detay.
    draw_tool_detail_info(f, app, area, false);
}

fn commands_detail_lines(app: &App) -> Vec<Line<'static>> {
    use crate::commands::{CommandsSource, RiskLevel, VerificationStatus};
    let lang = app.ui_lang();
    let lang_id = lang.id();
    let mut lines = Vec::new();

    if app.commands_list_shows_tasks() {
        let tasks = app.filtered_tasks();
        let Some(task) = tasks
            .get(app.commands_entry_index.min(tasks.len().saturating_sub(1)))
            .copied()
        else {
            lines.push(Line::from("Seçili görev yok.".to_string()));
            return lines;
        };
        lines.push(Line::from(Span::styled(
            task.name.pick(lang_id).to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(format!(
            "{} · {}",
            lang.bank_category(&task.category),
            task.verification.status.as_str()
        )));
        if task.verification.status == VerificationStatus::Draft {
            lines.push(Line::from(Span::styled(
                "draft / doğrulanmamış — örnekler rehber amaçlı; otomatik çalışmaz.".to_string(),
                Style::default().fg(Color::Yellow),
            )));
        }
        lines.push(Line::from(Span::styled(
            "Amaç:".to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        let summary = task.summary.pick(lang_id);
        if !summary.is_empty() {
            lines.push(Line::from(summary.to_string()));
        }
        // Güvenli örnek: implementations içinden en düşük riskli.
        let mut best: Option<(&str, RiskLevel, &str, bool)> = None;
        for impl_ in &task.implementations {
            for ex in &impl_.examples {
                let better = match best {
                    None => true,
                    Some((_, r, _, _)) => {
                        (ex.risk as u8) < (r as u8)
                            || (r != RiskLevel::Safe && ex.risk == RiskLevel::Safe)
                    }
                };
                if better {
                    let expl = ex.explanation.pick(lang_id);
                    best = Some((
                        ex.command.as_str(),
                        ex.risk,
                        expl,
                        ex.requires_elevated || ex.privilege.as_deref() == Some("elevated"),
                    ));
                }
            }
        }
        if let Some((cmd, risk, expl, elev)) = best {
            lines.push(Line::from(Span::styled(
                "Güvenli örnek:".to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                format!("  {cmd}"),
                Style::default().fg(Color::Cyan),
            )));
            lines.push(Line::from(format!(
                "  {} · {}",
                lang.t(Msg::CommandsRisk),
                risk.label(lang_id)
            )));
            if elev {
                lines.push(Line::from(lang.t(Msg::CommandsElevated).to_string()));
            }
            if !expl.is_empty() {
                lines.push(Line::from(Span::styled(
                    "Beklenen etki:".to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(format!("  {expl}")));
            }
        }
        if !task.related_commands.is_empty() {
            lines.push(Line::from(format!(
                "{}: {}",
                lang.t(Msg::CommandsRelated),
                task.related_commands.join(", ")
            )));
        }
        lines.push(Line::from(Span::styled(
            "Enter: ilgili komutlar · Esc: geri · çalıştırılmaz.".to_string(),
            Style::default().fg(Color::DarkGray),
        )));
        return lines;
    }

    match app.commands_source() {
        CommandsSource::Bank => {
            let cmds = app.filtered_bank_commands();
            let Some(cmd) = cmds
                .get(app.commands_entry_index.min(cmds.len().saturating_sub(1)))
                .copied()
            else {
                lines.push(Line::from(lang.t(Msg::NoCommandSelected).to_string()));
                return lines;
            };
            let fav = if app.is_command_favorite(&cmd.id) {
                "★ "
            } else {
                ""
            };
            lines.push(Line::from(Span::styled(
                format!("{fav}{}", cmd.name),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(format!(
                "{} · {} · {} · {}",
                lang.bank_category(&cmd.category),
                lang.t(Msg::CommandsRisk),
                cmd.risk.label(lang_id),
                cmd.verification.status.as_str()
            )));
            if cmd.verification.status == VerificationStatus::Draft {
                lines.push(Line::from(Span::styled(
                    lang.t(Msg::CommandsDraftWarn).to_string(),
                    Style::default().fg(Color::Yellow),
                )));
            }
            lines.push(Line::from(Span::styled(
                "Amaç:".to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            let summary = cmd.summary.pick(lang_id);
            if !summary.is_empty() {
                lines.push(Line::from(summary.to_string()));
            }
            let desc = cmd.description.pick(lang_id);
            if !desc.is_empty() {
                lines.push(Line::from(desc.to_string()));
            }
            // Risk + neden
            lines.push(Line::from(Span::styled(
                format!("{} + neden:", lang.t(Msg::CommandsRisk)),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            let why = match cmd.risk {
                RiskLevel::Safe => "Salt okuma / yerel bilgi; sistem durumunu değiştirmez.",
                RiskLevel::Caution => "Yan etkileri olabilir; örnekleri dikkatle kullanın.",
                RiskLevel::Dangerous => {
                    "Sistem veya uzak durumu değiştirebilir; doğrulamadan çalıştırmayın."
                }
                RiskLevel::Destructive => {
                    "Veri veya sistem kaybı riski yüksek; üretimde kullanmayın."
                }
            };
            lines.push(Line::from(format!("  {} — {why}", cmd.risk.label(lang_id))));
            if cmd.requires_elevated {
                lines.push(Line::from(Span::styled(
                    format!("sudo: {}", lang.t(Msg::CommandsElevated)),
                    Style::default().fg(Color::Yellow),
                )));
            } else {
                lines.push(Line::from(
                    "sudo: gerekmez (kayıt yükseltme istemiyor).".to_string(),
                ));
            }
            // Güvenli örnek tercih et
            let safe_ex = cmd
                .examples
                .iter()
                .find(|ex| ex.risk == RiskLevel::Safe)
                .or_else(|| cmd.examples.first());
            if let Some(ex) = safe_ex {
                lines.push(Line::from(Span::styled(
                    "Güvenli örnek:".to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                let elev = if ex.requires_elevated { " · sudo" } else { "" };
                let title = ex.title.pick(lang_id);
                if !title.is_empty() {
                    lines.push(Line::from(format!(
                        "  · {title} [{}{}]",
                        ex.risk.label(lang_id),
                        elev
                    )));
                }
                lines.push(Line::from(Span::styled(
                    format!("    {}", ex.command),
                    Style::default().fg(Color::Cyan),
                )));
                let expl = ex.explanation.pick(lang_id);
                lines.push(Line::from(Span::styled(
                    "Beklenen etki:".to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                if !expl.is_empty() {
                    lines.push(Line::from(format!("  {expl}")));
                } else if !summary.is_empty() {
                    lines.push(Line::from(format!("  {summary}")));
                }
            }
            let is_arch = crate::commands::host_is_arch();
            if cmd.has_package_hints() {
                lines.push(Line::from(Span::styled(
                    format!("{}:", lang.t(Msg::CommandsPackages)),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                for (label, pkg) in cmd.package_labeled_pairs(is_arch) {
                    let mark = if is_arch && label.starts_with("arch") {
                        " ←"
                    } else {
                        ""
                    };
                    lines.push(Line::from(format!("  {label}: {pkg}{mark}")));
                }
            }
            let notes = cmd.install_notes.pick(lang_id);
            if !notes.is_empty() {
                lines.push(Line::from(format!(
                    "{}: {notes}",
                    lang.t(Msg::CommandsInstallNotes)
                )));
            }
            if !cmd.related.is_empty() {
                lines.push(Line::from(format!(
                    "{}: {}",
                    lang.t(Msg::CommandsRelated),
                    cmd.related.join(", ")
                )));
            }
            if !cmd.nova.concepts.is_empty() {
                lines.push(Line::from(format!(
                    "{}: {}",
                    lang.t(Msg::CommandsConcepts),
                    cmd.nova.concepts.join(", ")
                )));
            }
            lines.push(Line::from(Span::styled(
                "Otomatik çalıştırılmaz — yalnız açık eylem / kopyalama.".to_string(),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(Span::styled(
                lang.t(Msg::CommandsBankFooter).to_string(),
                Style::default().fg(Color::DarkGray),
            )));
        }
        CommandsSource::Local => {
            let ents = app.filtered_local_commands();
            let Some(ent) = ents
                .get(app.commands_entry_index.min(ents.len().saturating_sub(1)))
                .copied()
            else {
                lines.push(Line::from(lang.t(Msg::EmptyLocalCommands).to_string()));
                return lines;
            };
            lines.push(Line::from(Span::styled(
                ent.name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(ent.path.clone()));
            lines.push(Line::from(Span::styled(
                "Amaç:".to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(
                "Bu cihazda PATH üzerinde bulunan yürütülebilir (katalog kaydı değil).".to_string(),
            ));
            lines.push(Line::from(format!(
                "source={} · kind={}",
                ent.source.as_str(),
                crate::commands::LOCAL_KIND
            )));
            match &ent.bank_id {
                Some(id) => {
                    lines.push(Line::from(format!(
                        "katalog eşleşmesi bank_id={id} ({})",
                        ent.bank_match.as_deref().unwrap_or("match")
                    )));
                    lines.push(Line::from(Span::styled(
                        lang.t(Msg::CommandsLocalNoVerify).to_string(),
                        Style::default().fg(Color::Yellow),
                    )));
                }
                None => {
                    lines.push(Line::from(
                        "katalogda karşılık yok — yalnız sistem PATH.".to_string(),
                    ));
                    lines.push(Line::from(lang.t(Msg::CommandsLocalNoBank).to_string()));
                }
            }
            lines.push(Line::from(Span::styled(
                "PATH ≠ katalog · otomatik çalıştırılmaz.".to_string(),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(Span::styled(
                lang.t(Msg::CommandsLocalFooter).to_string(),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    lines
}

fn draw_tool_detail_info(f: &mut Frame, app: &App, area: Rect, side_panel: bool) {
    let lang = app.ui_lang();
    let pal = palette(app);
    let focused = app.focus == PanelFocus::ToolDetail;
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(pal.muted)
    };
    let lines = tool_detail_info_lines(app, area);
    let block = if side_panel {
        Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(format!(" {} ", lang.t(Msg::ToolDetail)))
    } else {
        Block::default()
            .borders(Borders::TOP)
            .border_style(border)
            .title(format!(" {} ", lang.t(Msg::ToolDetail)))
    };
    let detail = Paragraph::new(lines).wrap(Wrap { trim: true }).block(block);
    f.render_widget(detail, area);
}

fn tool_detail_info_lines<'a>(app: &'a App, area: Rect) -> Vec<Line<'a>> {
    let Some(entry) = app.selected_tool() else {
        return vec![Line::from(Span::styled(
            "Henüz araç seçilmedi.",
            Style::default().fg(Color::DarkGray),
        ))];
    };
    let mut lines: Vec<Line> = Vec::new();
    let inner_w = area.width.saturating_sub(4) as usize;

    // Mağaza detayıyla aynı metin: eşleşen store kaydının açıklaması (id / ikili).
    let store_match = crate::store::sync::find_store_for_catalog_tool(
        app.store.entries(),
        &entry.id,
        &entry.executable,
        &entry.args,
    );
    let description =
        crate::store::sync::detail_description_for_tool(&entry.description, store_match);

    lines.push(Line::from(Span::styled(
        entry.name.clone(),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    let lang = app.ui_lang();
    lines.push(Line::from(vec![
        Span::styled(
            format!("{}: ", lang.t(Msg::CategoryLabel)),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            lang.catalog_category(&entry.category).to_string(),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    // Açıklama: satır kırarak tam göster (küçük şerit / `…` yok) — Mağaza ile aynı kaynak.
    let desc = description.trim();
    if desc.is_empty() {
        lines.push(Line::from(Span::styled(
            "Bu uygulama için katalog açıklaması yok.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for part in wrap_text(desc, inner_w) {
            lines.push(Line::from(Span::styled(
                part,
                Style::default().fg(Color::Gray),
            )));
        }
    }

    let status_line = match &entry.resolved_path {
        Some(path) => format!("{} Kurulu — {}", entry.status.symbol(), path.display()),
        None => format!(
            "{} {} — program: {}",
            entry.status.symbol(),
            entry.status.label(),
            entry.executable
        ),
    };
    lines.push(Line::from(Span::styled(
        truncate(&status_line, inner_w.max(1)),
        Style::default().fg(status_color(entry.status)),
    )));

    let cmd = if entry.args.is_empty() {
        entry.executable.clone()
    } else {
        format!("{} {}", entry.executable, entry.args.join(" "))
    };
    lines.push(Line::from(vec![
        Span::styled("Komut: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate(&cmd, inner_w.saturating_sub(7).max(1)),
            Style::default().fg(Color::White),
        ),
    ]));

    // Mağaza’daki kurulum notları (gerçek katalog metni; uydurma yok).
    if let Some(notes) = store_match
        .and_then(|e| e.install_notes.as_deref())
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Notlar:",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        for part in wrap_text(notes, inner_w) {
            lines.push(Line::from(Span::styled(
                part,
                Style::default().fg(Color::Yellow),
            )));
        }
    }

    let examples = enriched_tool_examples(entry);
    if !examples.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Kullanım örnekleri",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        for (i, example) in examples.iter().enumerate() {
            let bullet = if i % 2 == 0 { "▸ " } else { "• " };
            for (li, part) in wrap_text(example, inner_w.saturating_sub(2).max(1))
                .into_iter()
                .enumerate()
            {
                let prefix = if li == 0 { bullet } else { "  " };
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Color::DarkGray)),
                    Span::styled(part, Style::default().fg(Color::White)),
                ]));
            }
        }
    }

    if let Some(info) = &entry.setup_info {
        lines.push(Line::from(""));
        for part in wrap_text(info, inner_w) {
            lines.push(Line::from(Span::styled(
                part,
                Style::default().fg(Color::Yellow),
            )));
        }
    }

    lines
}

/// Katalog örneklerini zenginleştirir; seyrek listeleri çeşitlendirir.
fn enriched_tool_examples(entry: &crate::catalog::CatalogEntry) -> Vec<String> {
    let mut out: Vec<String> = entry
        .examples
        .iter()
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect();
    // Tek satırlık "yalnızca binary adı" örneklerini çeşitlendir.
    if out.len() <= 1 {
        let exe = entry.executable.as_str();
        let only_exe = out.len() == 1 && out[0] == exe;
        if out.is_empty() || only_exe {
            out.clear();
            out.extend(default_usage_examples(exe, &entry.category, &entry.name));
        }
    }
    // Çok kısa listelere yardım / tipik bayrak ekle (yinelenmesin).
    if out.len() < 3 {
        let help = format!("{exe} --help", exe = entry.executable);
        if !out
            .iter()
            .any(|e| e.contains("--help") || e.contains("help"))
        {
            out.push(format!("{help}   ← seçenekleri listeler"));
        }
    }
    out
}

/// Kurulu araç detayında boşluğu dolduran Türkçe kullanım satırları.
fn default_usage_examples(exe: &str, category: &str, name: &str) -> Vec<String> {
    let mut rows = vec![
        format!("{exe}   ← {name} arayüzünü açar"),
        format!("{exe} --help   ← yardım ve bayraklar"),
    ];
    let cat = category.to_lowercase();
    if cat.contains("ai") || cat.contains("kodlama") {
        rows.push(format!("{exe}   ← proje dizininde etkileşimli oturum"));
        rows.push("İpucu: Enter ile aksiyon modalından Devam et / Yardım seçilebilir.".into());
    } else if cat.contains("git") {
        rows.push(format!("{exe}   ← depo kökünde durum / commit TUI"));
        rows.push(format!("cd proje && {exe}   ← belirli depoda aç"));
    } else if cat.contains("dosya") || cat.contains("arama") {
        rows.push(format!("{exe} .   ← geçerli dizinde gez"));
        rows.push(format!("{exe} ~/Belgeler   ← klasör yolunu vererek aç"));
    } else if cat.contains("sistem") || cat.contains("kaynak") {
        rows.push(format!("{exe}   ← canlı kaynak grafikleri"));
        rows.push("İpucu: çıkmak için çoğu izleyicide q tuşu kullanılır.".into());
    } else if cat.contains("ağ") || cat.contains("network") {
        rows.push(format!("{exe}   ← canlı ağ görünümü"));
        rows.push(format!(
            "{exe} örnek.com   ← hedef vererek çalıştır (destekliyorsa)"
        ));
    } else if cat.contains("docker") || cat.contains("konteyner") {
        rows.push(format!("{exe}   ← konteyner / servis panosu"));
    } else {
        rows.push(format!(
            "Terminalde `{exe}` yazarak veya Nova’da Enter ile başlatın."
        ));
    }
    rows
}

/// Mağaza uygulamasının detay satırlarını üretir.
///
/// Sıralama bilinçlidir: ad, kategori ve **satır kırılmış açıklama** üstte;
/// kurulum / güven / kaynak bağlantıları hemen ardından gelir. Kaynak ve depo
/// satırları tıklanabilir / Enter ile tarayıcıda açılır.
fn store_detail_lines<'a>(app: &'a App, area: Rect) -> Vec<Line<'a>> {
    let Some(entry) = app.selected_store_app() else {
        return vec![Line::from(Span::styled(
            app.ui_lang().t(Msg::NoStoreApp),
            Style::default().fg(Color::DarkGray),
        ))];
    };

    let inner_w = area.width.saturating_sub(4) as usize;
    let status = app.store_status(&entry.id);
    let has_update = app.store_has_pending_update(&entry.id);
    let pm_update = app.store_pm_update_for(&entry.id);
    let resolved_plan = if entry.install_method.is_package_manager() {
        crate::store::install::resolve_entry_install_plan(entry)
    } else {
        None
    };
    let links = store_detail_links(app);
    let link_focus = app.focus == crate::app::PanelFocus::StoreDetail;

    let mut lines: Vec<Line> = Vec::new();

    // 1. Ad (bold) + kategori ayrı satır — ne olduğu hemen okunsun.
    lines.push(Line::from(Span::styled(
        entry.name.clone(),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    let lang = app.ui_lang();
    lines.push(Line::from(vec![
        Span::styled(
            format!("{}: ", lang.t(Msg::CategoryLabel)),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            lang.store_category(&entry.category).to_string(),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    // 2. Açıklama — satır kırarak; tek satırlık `…` şeridi yok.
    let desc = entry.description.trim();
    if desc.is_empty() {
        lines.push(Line::from(Span::styled(
            "Bu uygulama için katalog açıklaması yok.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for part in wrap_text(desc, inner_w) {
            lines.push(Line::from(Span::styled(
                part,
                Style::default().fg(Color::Gray),
            )));
        }
    }

    // 3. Sürüm.
    lines.push(Line::from(vec![
        Span::styled("Sürüm: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            entry.version.as_deref().unwrap_or("belirtilmemiş"),
            Style::default().fg(Color::White),
        ),
    ]));

    // 4. Kurulum durumu (önbellekten; her karede `which` çağrısı yapılmaz).
    lines.push(Line::from(vec![
        Span::styled("Durum: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} {}", status.symbol(), status.label()),
            Style::default().fg(status_color(status)),
        ),
    ]));

    // 5. Canlı sürüm takibi / PM güncelleme.
    let follow = if let Some(pm) = pm_update {
        format!(
            "PM güncelleme ({}) → {}",
            pm.method.short_label(),
            pm.rendered()
        )
    } else {
        match app.store_update_for(&entry.id) {
            Some(release) => {
                let current = entry.version.clone().unwrap_or_default();
                if crate::store::manifest::is_newer(&release.version, &current) {
                    format!("yeni sürüm {} → {}", current, release.version)
                } else {
                    format!("güncel ({})", release.version)
                }
            }
            None => match app.store_check_errors.get(&entry.id) {
                Some(_) => "denetlenemedi".to_string(),
                None => match entry.release {
                    Some(_) => "denetlenmedi (U)".to_string(),
                    None if entry.install_method.is_package_manager() => {
                        "PM: denetlenmedi (U)".to_string()
                    }
                    None => "takip edilmiyor".to_string(),
                },
            },
        }
    };
    lines.push(Line::from(vec![
        Span::styled("Canlı takip: ", Style::default().fg(Color::DarkGray)),
        Span::styled(follow, Style::default().fg(Color::Cyan)),
    ]));

    // 6. Kurulum yöntemi — kısa kod değil, anlaşılır etiket.
    let display_method = resolved_plan
        .as_ref()
        .map(|p| p.method)
        .unwrap_or(entry.install_method);
    let method_line = if entry.install_method.is_installable() {
        let state = if status == crate::detection::InstalledStatus::Installed {
            "kurulu"
        } else {
            "kurulabilir"
        };
        let mut label = format!("{state} · {}", display_method.label());
        if display_method != entry.install_method {
            label.push_str(&format!(
                " (katalog: {})",
                entry.install_method.short_label()
            ));
        }
        label
    } else {
        entry.install_method.label().to_string()
    };
    for part in wrap_text(&format!("Kurulum: {method_line}"), inner_w) {
        lines.push(Line::from(Span::styled(
            part,
            Style::default().fg(Color::Yellow),
        )));
    }

    // Güven sınıfı: A/B/C/D (distro / proje+imza / kayıt defteri / manuel).
    let trust_class = entry.trust_class();
    let trust_color = match trust_class {
        crate::store::trust::TrustClass::A => Color::Green,
        crate::store::trust::TrustClass::B => Color::Cyan,
        crate::store::trust::TrustClass::C => Color::Yellow,
        crate::store::trust::TrustClass::D => Color::DarkGray,
    };
    lines.push(Line::from(vec![
        Span::styled("Güven: ", Style::default().fg(Color::DarkGray)),
        Span::styled(entry.trust_display(), Style::default().fg(trust_color)),
    ]));
    if let Some(note) = trust_class.risk_note() {
        for part in wrap_text(&format!("Risk: {note}"), inner_w) {
            lines.push(Line::from(Span::styled(
                part,
                Style::default().fg(Color::Yellow),
            )));
        }
    }

    if entry.install_method.is_package_manager() {
        let cmd = resolved_plan
            .as_ref()
            .map(|p| p.rendered())
            .or_else(|| {
                entry
                    .install_command
                    .as_ref()
                    .map(|a| crate::store::format_install_command(a))
            })
            .unwrap_or_default();
        if !cmd.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Komut: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    truncate(&cmd, inner_w.saturating_sub(8)),
                    Style::default().fg(Color::White),
                ),
            ]));
        }
    }

    // 7. Kaynak / depo bağlantıları (tıkla veya detayda Enter → tarayıcı).
    if links.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Kaynak: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                truncate(&entry.source, inner_w.saturating_sub(10).max(1)),
                Style::default().fg(Color::White),
            ),
        ]));
    } else {
        for (i, link) in links.iter().enumerate() {
            let selected = link_focus && i == app.store_detail_link_index;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::UNDERLINED)
            };
            let mark = if selected { "▶ " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(mark, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("{}: ", link.label),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    truncate(
                        &link.url,
                        inner_w.saturating_sub(link.label.len() + 4).max(1),
                    ),
                    style,
                ),
            ]));
        }
        lines.push(Line::from(Span::styled(
            "Bağlantı: tıkla veya detayda Enter → tarayıcı (xdg-open)",
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(vec![
        Span::styled("Lisans: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate(&entry.license, inner_w.saturating_sub(10).max(1)),
            Style::default().fg(Color::White),
        ),
    ]));

    // 8. Çalıştırılabilir dosya.
    lines.push(Line::from(vec![
        Span::styled("Executable: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate(&entry.executable, inner_w.saturating_sub(12)),
            Style::default().fg(Color::White),
        ),
    ]));

    // 9. Mimari / platform.
    if let Some(arch) = crate::store::CpuArch::host() {
        let ok = entry.supports_arch(arch);
        lines.push(Line::from(vec![
            Span::styled("Mimari: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{} {}",
                    arch.as_str(),
                    if ok { "✓" } else { "desteklenmiyor" }
                ),
                Style::default().fg(if ok { Color::Green } else { Color::Red }),
            ),
        ]));
    }
    if !entry.supported_platforms.is_empty() {
        let plats = entry.supported_platforms.join(", ");
        lines.push(Line::from(vec![
            Span::styled("Platform: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                truncate(&plats, inner_w.saturating_sub(10)),
                Style::default().fg(Color::White),
            ),
        ]));
    }

    // 10. İmza / checksum (güven ayrıntısı).
    if let Some(sig) = &entry.signature {
        lines.push(Line::from(vec![
            Span::styled("İmza: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                truncate(sig.kind.label(), inner_w.saturating_sub(6)),
                Style::default().fg(Color::Green),
            ),
        ]));
    }
    if let Some(sum) = entry.checksum.as_deref().filter(|s| !s.is_empty()) {
        let kind = entry.checksum_type.as_deref().unwrap_or("SHA-256");
        lines.push(Line::from(vec![
            Span::styled(format!("{kind}: "), Style::default().fg(Color::DarkGray)),
            Span::styled(
                truncate(sum, inner_w.saturating_sub(kind.chars().count() + 2)),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    // 11. Kurulum notları (özellikle manual) — satır kırarak.
    if let Some(notes) = entry.install_notes.as_deref().filter(|n| !n.is_empty()) {
        lines.push(Line::from(Span::styled(
            "Notlar:",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        for part in wrap_text(notes, inner_w) {
            lines.push(Line::from(Span::styled(
                part,
                Style::default().fg(Color::Yellow),
            )));
        }
    }

    // 12. Eylem ipuçları.
    let installed = status == crate::detection::InstalledStatus::Installed;
    lines.push(Line::from(""));
    let hint = if link_focus && !links.is_empty() {
        "Enter: seçili bağlantıyı tarayıcıda aç   Esc/←: listeye dön"
    } else if has_update {
        "Enter: güncelle   → detay: bağlantı   U: sürümleri denetle"
    } else if installed {
        "Enter: kaldır / güncelle   → detay: bağlantı   U: denetle"
    } else if entry.install_method.is_package_manager() {
        "Enter: kur   → detayda Enter/tık: kaynak   U: denetle"
    } else if entry.install_method.is_archive() {
        "Enter: kur (arşiv)   → detay: kaynak aç   U: denetle"
    } else {
        "Enter: resmî talimat   → detay: kaynak aç   U: denetle"
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )));

    lines
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let tight = area.width < TIGHT_HINT_WIDTH;

    // Anlık geri bildirim (favori eklendi, Enter sonucu, reddedilen başlatma)
    // her zaman önceliklidir; aksi hâlde kullanıcı eyleminin sonucunu göremez.
    // Mesaj yokken odaklı panel adı + −/+ boyut kısayolu gösterilir.
    let line = if !app.status_message.is_empty() {
        let color = message_color(&app.status_message);
        let mut spans = Vec::new();
        // Başarı ve hata yalnızca renkle değil simgeyle de ayrılır: renk
        // körlüğü olan kullanıcı da sonucu ayırt edebilmelidir.
        if let Some(symbol) = message_symbol(&app.status_message) {
            spans.push(Span::styled(
                format!("{symbol} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        }
        spans.push(Span::styled(
            truncate(&app.status_message, area.width as usize),
            Style::default().fg(color),
        ));
        Line::from(spans)
    } else if tight {
        Line::from(Span::styled(
            truncate(
                &format!("▶ {}  −/+", app.focus_panel_name()),
                area.width as usize,
            ),
            Style::default().fg(Color::Cyan),
        ))
    } else {
        Line::from(Span::styled(
            truncate(
                &format!(
                    "▶ {}  −/+ boyut   Tab/Shift+Tab panel",
                    app.focus_panel_name()
                ),
                area.width as usize,
            ),
            Style::default().fg(Color::Cyan),
        ))
    };
    f.render_widget(Paragraph::new(line), area);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect, tall: bool) {
    let tight = area.width < TIGHT_HINT_WIDTH;
    // Kısa yardım satırı: dar terminalde tek satır, gezinme sırasında ipucu.
    // Çok dar terminallerde yalnızca en gerekli tuşlar söylenir; uzun metni
    // tek satıra sıkıştırmak okunaksız olur.
    let short = if app.search_active {
        if tight {
            "yaz · Esc çık"
        } else {
            "yaz: ara   Enter/Esc: aramadan çık"
        }
    } else if app.commands_tab {
        if tight {
            "↑↓ · Tab · −/+ · ?"
        } else {
            "↑↓ gezinme/liste   Tab panel   Space profil   / ara   F favori   ?"
        }
    } else if app.store_tab {
        match app.focus {
            PanelFocus::Category => {
                if tight {
                    "↑↓ · Tab · ?"
                } else {
                    "↑↓ sekme   Tab/→ mağaza   −/+ boyut   / ara   ?"
                }
            }
            PanelFocus::StoreCategory => {
                if tight {
                    "↑↓ · Tab · ?"
                } else {
                    "↑↓ kategori   Tab/→ uygulamalar   −/+ boyut   / ara   ?"
                }
            }
            _ => {
                if tight {
                    "↑↓ · Enter · ?"
                } else {
                    "↑↓ uygulama   Tab panel   Enter Install   −/+ boyut   U · ?"
                }
            }
        }
    } else if app.settings_tab {
        if tight {
            "↑↓ · −/+ · ?"
        } else {
            "↑↓ ayar   Tab panel   −/+ boyut   Enter   ? yardım"
        }
    } else if app.narrow_mode && tight {
        "↑↓ · / · ?"
    } else if app.narrow_mode {
        "↑↓ araç   / ara   Enter   F favori   ? yardım"
    } else if app.focus == PanelFocus::Category {
        "↑↓ kategori   Tab/→ araçlar   −/+ boyut   / ara   ? yardım"
    } else if app.selected_tool_has_ai_actions() {
        "↑↓ araç   Tab panel   Enter aksiyonlar   −/+ boyut   F favori   ? yardım"
    } else {
        "↑↓ araç   Tab/← kategori   Enter başlat   −/+ boyut   F favori   ? yardım"
    };

    let narrow = area.width < WIDE_MIN_WIDTH;
    if !tall && !app.show_help {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                short,
                Style::default().fg(Color::DarkGray),
            ))),
            area,
        );
        return;
    }

    // Yardım paneli terminal yüksekliğine göre kısalır; satırlar alana sığmazsa
    // taşmak yerine kırpılır.
    let lang = app.ui_lang();
    let full = help_full_lines(app);
    let compact = vec![help_line(&[
        ("↑↓", lang.t(Msg::HelpNavigate)),
        ("←→", lang.t(Msg::HelpPanel)),
        ("/", lang.t(Msg::HelpSearch)),
        ("Enter", lang.t(Msg::HelpLaunch)),
        ("F", lang.t(Msg::HelpFavorite)),
        ("N", lang.t(Msg::HelpNewTool)),
        ("R", lang.t(Msg::HelpRecents)),
        ("P", lang.t(Msg::HelpProject)),
        ("M", lang.t(Msg::Store)),
        ("?", lang.t(Msg::HelpHelp)),
    ])];

    let mut lines = if app.show_help && narrow {
        // Dar terminalde yardım açıkken tam tablo sığmaz ve yükseklik de
        // arttıkça ilk satırlarda kalır; en sık kullanılan tuşlar öne alınır
        // ki kırpma anlamlı bir şeyi kessin.
        help_full_lines_narrow(app)
    } else if app.show_help {
        full
    } else {
        compact
    };

    // Satır sarması yerine sığdığı kadarını göster: ayrılmış yükseklik sabit.
    // Açık yardım ekranı tek satıra indirilmez; kullanıcı tam tabloyu istedi.
    if tight && !app.show_help {
        lines = vec![short_line(short)];
    }
    let capacity = area.height.saturating_sub(1) as usize;
    lines.truncate(capacity.max(1));

    let block = Block::default()
        .borders(Borders::TOP)
        .title(format!(" {} ", lang.t(Msg::Keyboard)));
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left);
    f.render_widget(para, area);
}

/// Yardım ekranının tam tuş tablosu.
///
/// Tablo, `App::handle_key`'deki gerçek davranışı yansıtır: burada listelenen
/// her tuş uygulamada çalışır, uygulamadaki her ana ekran tuşu burada
/// listelenir. Dosya yolları da burada gösterilir; kullanıcı yapılandırmayı
/// aramadan bulabilir.
fn help_full_lines(app: &App) -> Vec<Line<'static>> {
    let lang = app.ui_lang();
    let mut lines = vec![
        help_line(&[
            ("↑↓/jk", lang.t(Msg::HelpNavigate)),
            ("←→/hl", lang.t(Msg::HelpPanel)),
            ("Tab/S-Tab", lang.t(Msg::HelpSwitchPanel)),
            ("/", lang.t(Msg::HelpSearch)),
        ]),
        help_line(&[
            ("Enter", lang.t(Msg::HelpLaunchInfo)),
            ("Esc", lang.t(Msg::HelpCloseSearch)),
            ("?", lang.t(Msg::HelpHelp)),
        ]),
        help_line(&[
            ("F", lang.t(Msg::HelpFavorite)),
            ("C", lang.t(Msg::HelpFavOnly)),
        ]),
        help_line(&[
            ("N", lang.t(Msg::HelpNewTool)),
            ("D", lang.t(Msg::HelpDeleteTool)),
            ("R", lang.t(Msg::HelpRecents)),
            ("P", lang.t(Msg::HelpProjects)),
            ("S", lang.t(Msg::HelpSettings)),
        ]),
        help_line(&[
            ("G", lang.t(Msg::HelpCommandsTab)),
            ("M", lang.t(Msg::HelpStoreTab)),
            ("U", lang.t(Msg::HelpUpdateCheck)),
            ("O", lang.t(Msg::HelpFilePicker)),
            ("A", lang.t(Msg::HelpDefaultAi)),
            ("−/+", "odaklı panel boyutu"),
        ]),
        help_line(&[
            ("Q", lang.t(Msg::HelpQuit)),
            ("Ctrl+C", lang.t(Msg::HelpQuit)),
        ]),
    ];
    if app.commands_tab {
        lines.push(help_line(&[(
            lang.t(Msg::CommandsGuide),
            "Başlangıç→Görev→Bağlam→Tümü→PATH · katalog ≠ PATH ≠ Mağaza",
        )]));
        lines.push(help_line(&[
            ("F", lang.t(Msg::HelpCommandFavorite)),
            ("C", lang.t(Msg::HelpCommandFavOnly)),
            ("Space", "profil seç"),
            ("/", lang.t(Msg::HelpSearch)),
        ]));
        lines.push(help_line(&[
            ("Enter", "bilgi / görev aç"),
            ("Esc", "görevden geri"),
        ]));
    }
    // Mağaza sekmesinin kendi tuşları ayrı satırda anlatılır; sekme
    // görünümü farklı bir gezinme düzeni sunduğu için tek satırlık tablo
    // yetersiz kalır.
    if app.store_tab {
        lines.push(help_line(&[(
            lang.t(Msg::Store),
            lang.t(Msg::HelpStoreNav),
        )]));
        lines.push(help_line(&[("U", lang.t(Msg::HelpStoreUpdate))]));
    }
    let (config_line, state_line, workdir_line) = help_info_lines(app);
    lines.push(config_line);
    lines.push(state_line);
    lines.push(workdir_line);
    lines
}

/// Dar terminalde yardım: tuşlar tek sütun, önce en sık kullanılanlar.
///
/// Yükseklik sınırlı olduğu için kırpma kaçınılmaz; bu sıralama kırpılan
/// kısmın en az kritik tuşlar olmasını sağlar.
fn help_full_lines_narrow(app: &App) -> Vec<Line<'static>> {
    let lang = app.ui_lang();
    let entries: [(&'static str, &'static str); 17] = [
        ("↑↓", lang.t(Msg::HelpNavigate)),
        ("←→", lang.t(Msg::HelpPanel)),
        ("/", lang.t(Msg::HelpSearch)),
        ("Enter", lang.t(Msg::HelpLaunch)),
        ("?", lang.t(Msg::HelpHelp)),
        ("F", lang.t(Msg::HelpFavorite)),
        ("P", lang.t(Msg::HelpProject)),
        ("S", lang.t(Msg::HelpSettings)),
        ("R", lang.t(Msg::HelpRecents)),
        ("N", lang.t(Msg::HelpNewTool)),
        ("D", lang.t(Msg::HelpDeleteTool)),
        ("M", lang.t(Msg::Store)),
        ("U", lang.t(Msg::HelpUpdateCheck)),
        ("O", lang.t(Msg::HelpFilePicker)),
        ("A", lang.t(Msg::HelpDefaultAi)),
        ("C", lang.t(Msg::HelpFavOnly)),
        ("Q", lang.t(Msg::HelpQuit)),
    ];
    let mut lines: Vec<Line<'static>> = entries
        .iter()
        .map(|(key, desc)| help_line(&[(*key, *desc)]))
        .collect();
    let (config_line, state_line, workdir_line) = help_info_lines(app);
    lines.push(config_line);
    lines.push(state_line);
    lines.push(workdir_line);
    lines
}

/// Yardım ekranının bilgi satırları: dosya yolları ve etkin çalışma dizini.
///
/// Uzun yollar karakter sayılarak yalnızca görsel sığdırma için kesilir; bilgi
/// kaybı yalnızca ekrandadır, uygulama tam yolu kullanmaya devam eder.
fn help_info_lines(app: &App) -> (Line<'static>, Line<'static>, Line<'static>) {
    // Dinamik satırlar: yollar ve etkin yol. Uzun yollar karakter sayılarak
    // yalnızca görsel sığdırma için kesilir; bilgi kaybı yalnızca ekrandadır.
    let lang = app.ui_lang();
    let mut info = Vec::new();
    for (key, value) in [
        ("config", &app.config_path_display),
        ("state", &app.state_path_display),
    ] {
        info.push(Line::from(vec![
            Span::styled(format!("{key}: "), Style::default().fg(Color::DarkGray)),
            Span::styled(value.clone(), Style::default().fg(Color::Gray)),
        ]));
    }
    let workdir = match (
        &app.state.active_project,
        &app.state.prefs().default_working_dir,
    ) {
        (Some(p), _) => format!("{} ({})", p.display(), lang.t(Msg::ActiveProject)),
        (None, Some(d)) => format!("{d} ({})", lang.t(Msg::DefaultLabel)),
        (None, None) => app.home_dir.display().to_string(),
    };
    info.push(Line::from(vec![
        Span::styled(
            format!("{}: ", lang.t(Msg::WorkingDir)),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(workdir, Style::default().fg(Color::Gray)),
    ]));
    let mut it = info.into_iter();
    (
        it.next().expect("config satırı"),
        it.next().expect("state satırı"),
        it.next().expect("çalışma dizini satırı"),
    )
}

/// Düz metinden tek satırlık yardım satırı üretir.
fn short_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(Color::DarkGray),
    ))
}

/// `(tuş, açıklama)` çiftlerinden tek bir yardım satırı üretir.
fn help_line(items: &[(&'static str, &'static str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (key, desc)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(
            (*key).to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(" {desc}")));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_text() {
        assert_eq!(truncate("htop", 10), "htop");
    }

    #[test]
    fn wrap_text_breaks_on_words() {
        let lines = wrap_text("kısa bir açıklama metni burada", 12);
        assert!(lines.len() >= 2, "{lines:?}");
        assert!(lines.iter().all(|l| l.chars().count() <= 12), "{lines:?}");
    }

    #[test]
    fn installed_detail_uses_side_panel_like_store_when_wide() {
        // Geniş Kurulu: yan detay; AI aksiyonları modal (alt panel yok).
        let secondary = SECONDARY_PANEL_WIDTH_DEFAULT;
        assert!(tool_detail_side_layout(true, 200, secondary));
        assert!(tool_detail_side_layout(
            true,
            TOOL_DETAIL_SIDE_MIN_WIDTH,
            secondary
        ));
        assert!(!tool_detail_side_layout(
            true,
            TOOL_DETAIL_SIDE_MIN_WIDTH.saturating_sub(1),
            secondary
        ));
        // Canlı secondary eşiği: daraltılmış sol sütun daha erken yan-detay açar.
        let min_side = SECONDARY_PANEL_WIDTH_MIN + LIST_PANEL_MIN_WIDTH + TOOL_DETAIL_PANEL_WIDTH;
        assert!(tool_detail_side_layout(
            true,
            min_side,
            SECONDARY_PANEL_WIDTH_MIN
        ));
        assert!(!tool_detail_side_layout(
            true,
            min_side.saturating_sub(1),
            SECONDARY_PANEL_WIDTH_MIN
        ));
        assert!(min_side < TOOL_DETAIL_SIDE_MIN_WIDTH);
        let plain = click_areas_ex(ClickAreasInput {
            size: (120, 30),
            show_help: false,
            store_tab: false,
            commands_tab: false,
            tool_detail_side: true,
            actions_count: 0,
            panes: PaneWidths::default(),
            secondary_cols: 1,
        });
        let with_actions = click_areas_ex(ClickAreasInput {
            size: (120, 30),
            show_help: false,
            store_tab: false,
            commands_tab: false,
            tool_detail_side: true,
            actions_count: 4,
            panes: PaneWidths::default(),
            secondary_cols: 1,
        });
        assert!(plain.tool_detail_side);
        assert!(with_actions.tool_detail_side);
        // AI aksiyonları modal; alt panel yüksekliği her zaman 0.
        assert_eq!(plain.actions_height, 0);
        assert_eq!(with_actions.actions_height, 0);
        assert_eq!(with_actions.content_height, plain.content_height);
        let panes = PaneWidths::default();
        assert!(plain.tools_end_x < 120);
        assert_eq!(
            plain.tools_end_x,
            panes.tabs + fit_list_width(120 - panes.tabs, panes.list)
        );
        // Kurulu sol sütun `tabs` (varsayılan ikincil genişlik).
        assert_eq!(plain.category_end_x, panes.tabs);
        assert_eq!(STORE_DETAIL_PANEL_WIDTH, TOOL_DETAIL_PANEL_WIDTH);
        assert_eq!(
            TOOL_DETAIL_SIDE_MIN_WIDTH,
            SECONDARY_PANEL_WIDTH_DEFAULT + LIST_PANEL_MIN_WIDTH + TOOL_DETAIL_PANEL_WIDTH
        );
        // Orta genişlikte (yan detay yok) liste pref Length ile uygulanır.
        let mid = click_areas_ex(ClickAreasInput {
            size: (100, 30),
            show_help: false,
            store_tab: false,
            commands_tab: false,
            tool_detail_side: false,
            actions_count: 0,
            panes: PaneWidths::default(),
            secondary_cols: 1,
        });
        assert_eq!(
            mid.tools_end_x,
            panes.tabs + fit_list_width_alone(100 - panes.tabs, panes.list)
        );
        let store_panes = PaneWidths::for_screen(
            crate::config::state::UiScreen::Store,
            &crate::config::state::Prefs::default(),
        );
        let store = click_areas_ex(ClickAreasInput {
            size: (120, 30),
            show_help: false,
            store_tab: true,
            commands_tab: false,
            tool_detail_side: false,
            actions_count: 0,
            panes: store_panes,
            secondary_cols: 1,
        });
        assert_eq!(store.category_end_x, CATEGORY_PANEL_WIDTH);
    }

    #[test]
    fn pane_widths_defaults_widen_secondary_and_cap_list() {
        let p = PaneWidths::default();
        assert_eq!(p.tabs, 28);
        assert_eq!(p.secondary, 28);
        assert_eq!(p.list, 32);
        assert!(p.secondary > 22, "eski 22'den daha geniş kategori");
        // Yan detayda fazla alan detaya: list Length, detail Min.
        let store = PaneWidths::for_screen(
            crate::config::state::UiScreen::Store,
            &crate::config::state::Prefs::default(),
        );
        let avail = 200u16 - store.tabs - store.secondary;
        let list_w = fit_list_width(avail, store.list);
        assert_eq!(list_w, 32);
        assert!(avail - list_w >= STORE_DETAIL_PANEL_WIDTH);
        assert_eq!(clamp_secondary_width(3), SECONDARY_PANEL_WIDTH_MIN);
        assert_eq!(clamp_list_width(200), LIST_PANEL_WIDTH_MAX);
        assert_eq!(
            detail_side_min_width(store.tabs, store.secondary, 1),
            STORE_DETAIL_SIDE_MIN_WIDTH
        );
        assert!(
            detail_side_min_width(store.tabs, store.secondary, 2) > STORE_DETAIL_SIDE_MIN_WIDTH
        );
    }

    #[test]
    fn list_scroll_offset_keeps_selection_visible() {
        assert_eq!(list_scroll_offset(0, 5, 20), 0);
        assert_eq!(list_scroll_offset(4, 5, 20), 0);
        assert_eq!(list_scroll_offset(5, 5, 20), 1);
        assert_eq!(list_scroll_offset(19, 5, 20), 15);
        assert_eq!(list_scroll_offset(0, 5, 3), 0);
        assert_eq!(list_scroll_offset(2, 5, 3), 0);
    }

    #[test]
    fn list_index_at_y_skips_border_and_uses_scroll() {
        // panel_top=2, height=10 → inner 3..=10 (8 satır)
        assert_eq!(list_index_at_y(2, 2, 10, 0, 20), None); // üst kenarlık
        assert_eq!(list_index_at_y(3, 2, 10, 0, 20), Some(0));
        assert_eq!(list_index_at_y(4, 2, 10, 0, 20), Some(1));
        assert_eq!(list_index_at_y(11, 2, 10, 0, 20), None); // alt kenarlık
                                                             // Seçili 10 → offset 3; görünür ilk satır index 3
        assert_eq!(list_index_at_y(3, 2, 10, 10, 20), Some(3));
        assert_eq!(list_index_at_y(10, 2, 10, 10, 20), Some(10));
    }

    #[test]
    fn truncate_is_utf8_safe() {
        // Eskiden `&text[..35]` bayt indeksiyle kesildiği için panik oluşuyordu.
        let text = "Geliştirici araçları için çok uzun bir açıklama metni";
        let cut = truncate(text, 10);
        assert_eq!(cut.chars().count(), 10);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn truncate_does_not_panic_on_turkish_chars() {
        let text = "İşlem görüntüleyici ş ç ğ ü ö İ";
        let _ = truncate(text, 1);
        let _ = truncate(text, 0);
    }

    #[test]
    fn status_colors_differ_and_cover_all_states() {
        assert_eq!(
            status_color(InstalledStatus::Installed),
            Color::Green,
            "kurulu yeşil olmalı"
        );
        assert_ne!(
            status_color(InstalledStatus::Missing),
            status_color(InstalledStatus::Unknown),
            "kurulu değil ile kontrol edilemedi ayırt edilmeli"
        );
    }

    #[test]
    fn truncate_respects_budget_for_every_width() {
        // 1..=60 sütun için üretilen başlık hiçbir zaman alanı aşmamalı.
        let sample = "Günlük Terminal Yardımcıları ◆ ★ ✓ çok uzun bir başlık";
        for width in 1..=60u16 {
            let budget = width.saturating_sub(2) as usize;
            assert!(
                truncate(sample, budget).chars().count() <= budget.max(1),
                "genişlik {width} için metin alanı aşıyor"
            );
        }
    }
}
