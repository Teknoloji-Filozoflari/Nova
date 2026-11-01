//! Ekran üstü modallar: onay, özel araç formu/gözden geçirme, proje seçici,
//! geçmiş.
//!
//! Modal açıkken ana ekranın kısayolları **çalışmaz** (bkz.
//! [`crate::app::App::handle_key`]); böylece yıkıcı bir işlem yanlışlıkla
//! tetiklenmez. Modallar dosya sistemi değiştirmez; etki yalnızca
//! [`crate::app::App`]'in onaylanmış eylemlerinden doğar.

use super::form::{ToolForm, ValidTool};
use crate::file_picker::FilePickerState;
use crate::update::{GitHubRelease, UpdateCheck, Version};
use std::path::PathBuf;

/// Onay ekranının neyi onaylattığı.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    /// Bir özel aracı sil / gizle.
    DeleteTool { id: String, name: String },
    /// Geçmişi temizle.
    ClearRecents,
    /// Listedeki proje dizinini çıkar.
    RemoveProject { path: PathBuf },
    /// Var olmayan çalışma dizinini oluşturup başlatmayı yeniden dene.
    CreateDir { path: PathBuf, tool_id: String },
    /// Mağaza uygulamasını kur (kullanıcı alanına).
    InstallStoreApp { id: String, name: String },
    /// Nova güncellemesini kur (kullanıcı alanına — GitHub Releases ikilisi).
    InstallUpdate {
        version: String,
        release: Box<crate::update::GitHubRelease>,
    },
    /// Nova’yı GitHub repo `main` (veya dal) üzerinden `cargo install --git` ile kur.
    /// Ağ + derleme gerekir; Releases etiketine bağlı değildir.
    InstallRepoUpdate { plan: crate::update::RepoUpdatePlan },
    /// Mağaza uygulamasını canlı çözümlenmiş yeni sürümle güncelle.
    UpdateStoreApp {
        id: String,
        name: String,
        version: String,
    },
    /// Kurulu mağaza uygulamasını kaldır (arşiv dizini veya PM uninstall).
    UninstallStoreApp { id: String, name: String },
}

/// Açık olan modal.
#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    /// Modal yok.
    None,
    /// Evet/hayır onayı.
    Confirm {
        /// Sorulacak soru.
        question: String,
        /// Onaylanınca yapılacak eylem.
        action: ConfirmAction,
    },
    /// Özel araç formu.
    ToolForm(Box<ToolForm>),
    /// Form doğrulandıktan sonraki gözden geçirme adımı.
    ToolReview {
        /// Form (Esc ile geri dönülünce korunur).
        form: Box<ToolForm>,
        /// Doğrulanmış değerler.
        valid: Box<ValidTool>,
    },
    /// Proje dizini listesi.
    ProjectPicker {
        /// Seçili satır.
        index: usize,
    },
    /// Proje dizini ekleme (tek satır).
    ProjectAdd {
        /// Girilen metin.
        input: String,
        /// Son hatadan kalan mesaj.
        error: Option<String>,
    },
    /// Geçmiş görünümü (salt okunur; `X` onaylı temizleme).
    History,
    /// Kullanıcı tercihleri: terminal, çalışma dizini, tema, dil ve varsayılan
    /// uygulamalar.
    Settings {
        /// Tercih edilen terminal adı.
        preferred_terminal: String,
        /// Varsayılan çalışma dizini.
        default_working_dir: String,
        /// Arayüz teması.
        theme: String,
        /// Varsayılan AI komut satırı aracının katalog kimliği.
        default_ai_tool: String,
        /// Resim dosyaları için varsayılan uygulama.
        default_image_app: String,
        /// Video dosyaları için varsayılan uygulama.
        default_video_app: String,
        /// Arayüz dili kodu (`tr`/`en`/`ru`/`az`/`uz`/`kk`).
        language: String,
        /// Odaklı alan (0 terminal, 1 çalışma dizini, 2 tema, 3 AI, 4 resim,
        /// 5 video, 6 dil).
        focus: usize,
        /// Son doğrulama/kaydetme hatası.
        error: Option<String>,
    },
    /// Dosya seçici: dizin gezinme ve dosya seçme.
    FilePicker {
        /// Dosya seçici durumu.
        state: FilePickerState,
    },
    /// Güncelleme denetimi ve onay.
    UpdateCheck {
        /// Denetim durumu.
        status: UpdateStatus,
        /// Denetim sonucu.
        check: Option<UpdateCheck>,
        /// Hata mesajı.
        error: Option<String>,
    },
    /// Güncelleme onayı.
    UpdateConfirm {
        /// Mevcut sürüm.
        current: Version,
        /// Yeni sürüm.
        latest: Version,
        /// Yayın kaydı.
        release: GitHubRelease,
    },
    /// Kurulu mağaza uygulaması için eylem menüsü (kaldır / güncelleme denetle).
    StoreInstalledMenu {
        /// Mağaza kimliği.
        id: String,
        /// Görünen ad.
        name: String,
        /// Seçili satır: 0 = Kaldır, 1 = Güncelleme denetle.
        index: usize,
    },
}

/// Güncelleme denetim durumu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// Henüz denetim yapılmadı.
    Idle,
    /// Denetim devam ediyor.
    Checking,
    /// Denetim tamamlandı.
    Done,
}

impl Modal {
    /// Modal açık mı?
    pub fn is_open(&self) -> bool {
        !matches!(self, Modal::None)
    }

    /// Ana ekran tuşlarının engellenip engellenmeyeceği.
    pub fn blocks_main_keys(&self) -> bool {
        self.is_open()
    }
}
