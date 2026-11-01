//! Dosya seçici modülü.
//!
//! Nova'ya tam kapsamlı bir dosya yöneticisi **eklemez**; yalnızca terminal
//! içinde temel bir dosya seçici sunar. Kullanıcı dizinler arasında gezinir ve
//! bir dosya seçer. Seçilen dosya, ayarlarda belirlenmiş medya uygulamasıyla
//! açılabilir.
//!
//! Bu modül dosya silme, taşıma veya yeniden adlandırma **yapmaz**. Dosya
//! seçimi salt okunurdur; düzenleme [`crate::media_launcher`] modülü ile
//! ayrı bir başlatma yolu üzerinden yürütülür.

use std::path::{Path, PathBuf};

/// Tek bir dizin girişi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// Girişin adı.
    pub name: String,
    /// Dizin mi, dosya mı?
    pub is_dir: bool,
    /// Girişin tam yolu.
    pub path: PathBuf,
}

/// Dosya seçici durumu.
#[derive(Debug, Clone, PartialEq)]
pub struct FilePickerState {
    /// Şu anda görüntülenen dizin.
    pub current_dir: PathBuf,
    /// Dizin içeriği (klasörler önce, ardından dosyalar; her iki grup da
    /// alfabetik sıralıdır).
    pub entries: Vec<DirEntry>,
    /// Seçili girişin indeksi (`entries` üzerinden).
    pub selected: usize,
    /// Son hatadan kalan mesaj.
    pub error: Option<String>,
    /// Kullanıcının seçtiği dosya (Enter sonrası `Some`).
    pub selected_file: Option<PathBuf>,
}

impl FilePickerState {
    /// `home` dizininden başlayan dosya seçici oluşturur.
    pub fn new(home: &Path) -> Self {
        let mut state = Self {
            current_dir: home.to_path_buf(),
            entries: Vec::new(),
            selected: 0,
            error: None,
            selected_file: None,
        };
        state.reload();
        state
    }

    /// Mevcut dizinin içeriğini yeniden okur.
    ///
    /// Dosya sistemi erişimi gerçek dizin gerektirir; testlerde geçici bir
    /// dizin ile çağrılabilir.
    pub fn reload(&mut self) {
        self.error = None;
        self.entries = match std::fs::read_dir(&self.current_dir) {
            Ok(read_dir) => {
                let mut entries: Vec<DirEntry> = read_dir
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let path = e.path();
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let is_dir = path.is_dir();
                        DirEntry { name, is_dir, path }
                    })
                    .collect();
                // Klasörler önce, ardından dosyalar; her iki grup alfabetik.
                entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.cmp(&b.name),
                });
                entries
            }
            Err(e) => {
                self.error = Some(format!("Dizin okunamadı: {e}"));
                Vec::new()
            }
        };
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
    }

    /// Ebeveyn dizinine çıkar. Zaten kök dizindeyse hiçbir şey yapmaz.
    pub fn navigate_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            if parent != self.current_dir.as_path() {
                self.current_dir = parent.to_path_buf();
                self.reload();
            }
        }
    }

    /// Seçili girişi bir dizin ise oraya girer.
    pub fn enter_dir(&mut self) {
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        if !entry.is_dir {
            return;
        }
        self.current_dir = entry.path.clone();
        self.selected = 0;
        self.reload();
    }

    /// Seçimi bir satır yukarı taşır.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Seçimi bir satır aşağı taşır.
    pub fn move_down(&mut self) {
        let max = self.entries.len().saturating_sub(1);
        self.selected = (self.selected + 1).min(max);
    }

    /// Seçili girişi bir dosya olarak döner. Dizin ise `None` döner.
    pub fn selected_file(&self) -> Option<&PathBuf> {
        let entry = self.entries.get(self.selected)?;
        if entry.is_dir {
            return None;
        }
        Some(&entry.path)
    }

    /// Seçili girişin adı.
    pub fn selected_name(&self) -> Option<&str> {
        self.entries.get(self.selected).map(|e| e.name.as_str())
    }

    /// Seçili girişin dizin olup olmadığı.
    pub fn selected_is_dir(&self) -> bool {
        self.entries
            .get(self.selected)
            .map(|e| e.is_dir)
            .unwrap_or(false)
    }

    /// Seçili dosyayı kalır ve `selected_file` olarak işaretler.
    pub fn confirm_selection(&mut self) -> Option<PathBuf> {
        let path = self.selected_file().cloned();
        self.selected_file = path.clone();
        path
    }
}

impl Default for FilePickerState {
    fn default() -> Self {
        Self::new(Path::new("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_picker(tag: &str) -> (FilePickerState, PathBuf) {
        let dir = env::temp_dir().join(format!(
            "nova-filepicker-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join("b.txt"), "b").unwrap();
        std::fs::create_dir_all(dir.join("subdir")).unwrap();
        let state = FilePickerState::new(&dir);
        (state, dir)
    }

    #[test]
    fn new_starts_at_home_and_loads_entries() {
        let (state, dir) = temp_picker("start");
        assert_eq!(state.current_dir, dir);
        assert_eq!(state.entries.len(), 3); // a.txt, b.txt, subdir
        assert!(state.error.is_none());
    }

    #[test]
    fn entries_are_sorted_dirs_first_then_alphabetically() {
        let (state, _dir) = temp_picker("sort");
        let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["subdir", "a.txt", "b.txt"]);
        assert!(state.entries[0].is_dir);
        assert!(!state.entries[1].is_dir);
        assert!(!state.entries[2].is_dir);
    }

    #[test]
    fn navigate_up_goes_to_parent() {
        let (mut state, dir) = temp_picker("up");
        state.enter_dir();
        assert_eq!(state.current_dir, dir.join("subdir"));
        state.navigate_up();
        assert_eq!(state.current_dir, dir);
    }

    #[test]
    fn enter_dir_changes_directory() {
        let (mut state, dir) = temp_picker("enter");
        // subdir zaten index 0 (klasörler önce)
        state.enter_dir();
        assert_eq!(state.current_dir, dir.join("subdir"));
        assert_eq!(state.selected, 0);
        assert!(state.entries.is_empty());
    }

    #[test]
    fn move_up_and_down_are_bounded() {
        let (mut state, _dir) = temp_picker("bounds");
        for _ in 0..10 {
            state.move_up();
        }
        assert_eq!(state.selected, 0);
        for _ in 0..10 {
            state.move_down();
        }
        assert_eq!(state.selected, state.entries.len() - 1);
    }

    #[test]
    fn selected_file_returns_file_not_dir() {
        let (mut state, dir) = temp_picker("file");
        // subdir index 0, a.txt index 1
        state.move_down();
        let path = state.selected_file().unwrap().clone();
        assert_eq!(path, dir.join("a.txt"));

        state.selected = 0; // subdir
        assert!(state.selected_file().is_none());
    }

    #[test]
    fn confirm_selection_sets_selected_file() {
        let (mut state, dir) = temp_picker("confirm");
        state.move_down();
        let path = state.confirm_selection().unwrap();
        assert_eq!(path, dir.join("a.txt"));
        assert_eq!(state.selected_file, Some(path));
    }

    #[test]
    fn reload_on_missing_directory_sets_error() {
        let (mut state, _dir) = temp_picker("missing");
        state.current_dir = env::temp_dir().join("nova-yok-boyle-dizin-xyz");
        state.reload();
        assert!(state.error.is_some());
        assert!(state.entries.is_empty());
    }
}
