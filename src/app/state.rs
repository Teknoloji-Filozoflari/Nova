//! Kullanıcı durumu üzerinde çalışan mantık: favoriler, geçmiş, proje
//! dizinleri ve tercihler.
//!
//! Bu modül **saf mantığa** odaklanır; dosya yollarını ve yazma anını
//! [`crate::app::App`] yönetir. Böylece geçmiş sınırı, tekrar birleştirme ve
//! dizin doğrulama kuralları dosya sistemi olmadan test edilebilir.

use crate::config::state::{AttemptStatus, Prefs, RecentEntry, StateFile, RECENT_LIMIT_DEFAULT};
use std::path::PathBuf;

/// Uygulamanın kalıcı durumu (bellek içi model).
#[derive(Debug, Clone, Default)]
pub struct AppState {
    /// Kalıcı verinin gideceği dosya; `None` ise kalıcılık yoktur
    /// (`$HOME` yok ya da testler bilinçli olarak kapatmış).
    pub path: Option<PathBuf>,
    /// `state.toml` içeriğinin son okunan/yazılan hâli.
    pub file: StateFile,
    /// O an seçili proje dizini (başlatmalara uygulanır).
    pub active_project: Option<PathBuf>,
    /// Geri yüklenemeyen özel araçlar için uyarılar.
    ///
    /// Bozuk bir özel araç kaydı (ör. geçersiz kategori) diğer araçların ve
    /// dosyanın tamamının kaybına yol açmamalı: yalnızca bu araç atlanır ve
    /// kullanıcıya tek tek söylenir.
    pub restore_warnings: Vec<String>,
}

impl AppState {
    /// Dosyasız (kalıcı olmayan) durum. Testler ve `$HOME` olmayan
    /// ortamlar için.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// Geçmişte tutulacak kayıt sayısı.
    pub fn recent_limit(&self) -> usize {
        self.file.recent_limit()
    }

    /// Başlatma denemesini geçmişe ekler.
    ///
    /// Aynı id daha önce varsa eski kaydı kaldırır (tekrarlar birleşir) ve
    /// en başa ekler; sınır aşılınca en eski atılır.
    pub fn record_attempt(&mut self, id: &str, name: &str, at: u64, status: AttemptStatus) {
        self.file.recents.retain(|r| r.id != id);
        self.file.recents.insert(
            0,
            RecentEntry {
                id: id.to_string(),
                name: name.to_string(),
                at,
                status,
            },
        );
        let limit = self.recent_limit();
        self.file.recents.truncate(limit);
    }

    /// Geçmişi temizler. Geriye döndürülen değer silinen kayıt sayısıdır.
    pub fn clear_recents(&mut self) -> usize {
        let n = self.file.recents.len();
        self.file.recents.clear();
        n
    }

    /// Favori kimlikleri (sıralı).
    /// Yardım ekranında gösterilecek durum dosyası yolu.
    ///
    /// Bellekte çalışan (test) oturumda yol yoktur; bunu açıkça söyler,
    /// var olmayan bir dosya adı uydurmaz.
    pub fn path_display(&self) -> String {
        match &self.path {
            Some(p) => p.display().to_string(),
            None => "yok (bu oturumda kalıcı değil)".to_string(),
        }
    }

    pub fn favorites(&self) -> &std::collections::BTreeSet<String> {
        &self.file.favorites
    }

    /// Kalıcı verinin hiç yazılmadığı ilk çalıştırma mı?
    pub fn is_first_run(&self) -> bool {
        self.file == StateFile::default()
    }

    /// Tercihler.
    pub fn prefs(&self) -> &Prefs {
        &self.file.prefs
    }

    /// Geçmiş sınırını günceller ve listeyi hemen kırpar.
    pub fn set_recent_limit(&mut self, limit: usize) {
        let limit = if limit == 0 {
            RECENT_LIMIT_DEFAULT
        } else {
            limit.min(crate::config::state::RECENT_LIMIT_MAX)
        };
        self.file.prefs.recent_limit_value = Some(limit);
        self.file.recents.truncate(limit);
    }

    /// Proje dizinini listeye ekler (tekrarsız); sıra korunur.
    ///
    /// Dizin var mı diye **bakılmaz**: kullanıcı sonradan oluşturabilir.
    /// Yol boşsa veya göreliyse reddedilir.
    pub fn add_project_dir(&mut self, raw: &str) -> Result<PathBuf, String> {
        let path = normalize_project_path(raw)?;
        let as_str = path.to_string_lossy().to_string();
        if !self.file.project_dirs.iter().any(|p| p == &as_str) {
            self.file.project_dirs.push(as_str);
        }
        Ok(path)
    }

    /// Proje dizinini listeden çıkarır; etkin seçim aynı dizinse temizlenir.
    pub fn remove_project_dir(&mut self, index: usize) -> Option<PathBuf> {
        if index >= self.file.project_dirs.len() {
            return None;
        }
        let removed = PathBuf::from(self.file.project_dirs.remove(index));
        if self.active_project.as_ref() == Some(&removed) {
            self.active_project = None;
        }
        Some(removed)
    }

    /// Proje dizinleri.
    pub fn project_dirs(&self) -> Vec<PathBuf> {
        self.file.project_dirs.iter().map(PathBuf::from).collect()
    }

    /// Etkin proje dizinini ayarlar; `None` temizler.
    pub fn set_active_project(&mut self, path: Option<PathBuf>) {
        self.active_project = path;
    }
}

/// Kullanıcının yazdığı proje yolunu doğrular ve tam yola çevirir.
///
/// `~` ev dizinine, göreli yol ev dizinine göre çözülür. Boş girdi
/// reddedilir. Dosya sistemi **değiştirilmez**; yalnızca `exists` sorulur.
pub fn normalize_project_path(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Dizin yolu boş olamaz.".into());
    }
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .ok_or("$HOME tanımlı değil; göreli yol çözülemez.")?;
    let expanded = if let Some(rest) = trimmed.strip_prefix("~/") {
        home.join(rest)
    } else if trimmed == "~" {
        home.clone()
    } else {
        PathBuf::from(trimmed)
    };
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        home.join(expanded)
    };
    // `..` gibi bileşenleri kabaca temizle (dosya sistemi erişimi olmadan).
    let mut clean = PathBuf::new();
    for comp in absolute.components() {
        use std::path::Component;
        match comp {
            Component::ParentDir => {
                clean.pop();
            }
            Component::CurDir => {}
            other => clean.push(other.as_os_str()),
        }
    }
    Ok(clean)
}

/// Zaman damgalarını göreli biçime çevirir: "az önce", "12 dk önce"…
///
/// Gelecek bir zaman (saat kayması) "az önce" sayılır; ekranda eksi değer
/// görünmez.
pub fn relative_time(now: u64, at: u64) -> String {
    let secs = now.saturating_sub(at);
    if secs < 60 {
        "az önce".into()
    } else if secs < 3600 {
        format!("{} dk önce", secs / 60)
    } else if secs < 86_400 {
        format!("{} sa önce", secs / 3600)
    } else if secs < 86_400 * 30 {
        format!("{} gün önce", secs / 86_400)
    } else {
        format!("{} ay önce", secs / (86_400 * 30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recents_keep_newest_first_and_merge_duplicates() {
        let mut s = AppState::in_memory();
        s.record_attempt("a", "A", 10, AttemptStatus::RequestSent);
        s.record_attempt("b", "B", 20, AttemptStatus::Rejected);
        s.record_attempt("a", "A", 30, AttemptStatus::RequestSent);
        let ids: Vec<&str> = s.file.recents.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
        assert_eq!(s.file.recents[0].at, 30);
    }

    #[test]
    fn recents_are_truncated_to_limit() {
        let mut s = AppState::in_memory();
        s.set_recent_limit(3);
        for i in 0..5 {
            s.record_attempt(&format!("t{i}"), "T", i as u64, AttemptStatus::RequestSent);
        }
        assert_eq!(s.file.recents.len(), 3);
        assert_eq!(s.file.recents[0].id, "t4");
        assert_eq!(s.file.recents[2].id, "t2");
    }

    #[test]
    fn clearing_recents_reports_count() {
        let mut s = AppState::in_memory();
        s.record_attempt("a", "A", 1, AttemptStatus::RequestSent);
        s.record_attempt("b", "B", 2, AttemptStatus::Rejected);
        assert_eq!(s.clear_recents(), 2);
        assert!(s.file.recents.is_empty());
    }

    #[test]
    fn project_dirs_are_deduplicated() {
        let mut s = AppState::in_memory();
        s.add_project_dir("/home/kullanici/proje").unwrap();
        s.add_project_dir("/home/kullanici/proje/").unwrap();
        assert_eq!(s.project_dirs().len(), 1);
    }

    #[test]
    fn blank_project_dir_is_rejected_without_touching_disk() {
        let mut s = AppState::in_memory();
        assert!(s.add_project_dir("   ").is_err());
        assert!(s.project_dirs().is_empty());
    }

    #[test]
    fn removing_active_project_clears_selection() {
        let mut s = AppState::in_memory();
        let p = s.add_project_dir("/tmp/nova-ornek-proje").unwrap();
        s.set_active_project(Some(p));
        s.remove_project_dir(0);
        assert!(s.active_project.is_none());
    }

    #[test]
    fn relative_time_has_no_negative_values() {
        assert_eq!(relative_time(100, 200), "az önce");
        assert_eq!(relative_time(1000, 1000 - 120), "2 dk önce");
        assert_eq!(relative_time(10_000, 10_000 - 7200), "2 sa önce");
        assert_eq!(relative_time(1_000_000, 1_000_000 - 172_800), "2 gün önce");
    }
}
