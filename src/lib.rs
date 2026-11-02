//! Nova — TUI terminal uygulama başlatıcısı.
//!
//! Modül yapısı:
//! - `app`: uygulama durumu ve ana döngü
//! - `ui`: Ratatui tabanlı arayüz çizimi
//! - `event`: klavye ve terminal olayları
//! - `error`: hata türleri ve raporlama
//! - `config`: TOML tabanlı kullanıcı yapılandırması
//! - `catalog`: Araç kataloğu (UI'dan ayrı)
//! - `launcher`: Seçilen uygulamayı terminalde başlatma
//! - `detection`: Kurulu komutları tespit etme
//! - `terminal_guard`: Çıkışta ve panikte terminalin geri yüklenmesi

pub mod app;
pub mod browser;
pub mod catalog;
pub mod commands;
pub mod config;
pub mod detection;
pub mod error;
pub mod event;
pub mod file_picker;
pub mod i18n;
pub mod launcher;
pub mod media_launcher;
pub mod store;
pub mod terminal_guard;
pub mod ui;
pub mod update;
