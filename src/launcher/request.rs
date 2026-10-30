//! Başlatma isteği: "Ne başlatılacak?" sorusunun cevabı.
//!
//! İstek oluşturma **yan etkisizdir**: süreç başlatmaz veya dizin oluşturmaz.
//! Yalnızca açıkça çağrılan `write_stdin_file()` geçici dosya oluşturur.
//!
//! Terminal emülatörü katmanı bunu alır ve kendi argümanlarına çevirir.

use crate::catalog::CatalogEntry;
use crate::error::NovaError;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Removes a Nova help file and the private directory created for it.
pub(crate) fn remove_private_help_file(path: &Path) -> io::Result<()> {
    fs::remove_file(path)?;
    if let Some(parent) = path.parent() {
        fs::remove_dir(parent)?;
    }
    Ok(())
}

/// Verifies that a help file belongs to Nova's private temporary layout.
pub(crate) fn validate_private_help_file(path: &Path) -> io::Result<()> {
    let temp_root = fs::canonicalize(std::env::temp_dir())?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "geçici dosyanın dizini yok"))?;
    let canonical_parent = fs::canonicalize(parent)?;
    let parent_name = parent
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if canonical_parent.parent() != Some(temp_root.as_path())
        || !parent_name.starts_with("nova-help-")
        || path.file_name() != Some(std::ffi::OsStr::new("help.txt"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Nova geçici yardım yolu bekleniyordu",
        ));
    }

    let dir_metadata = fs::symlink_metadata(parent)?;
    let file_metadata = fs::symlink_metadata(path)?;
    if !dir_metadata.file_type().is_dir() || !file_metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "yardım yolu normal dizin ve dosya olmalı",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if dir_metadata.permissions().mode() & 0o077 != 0
            || file_metadata.permissions().mode() & 0o077 != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "yardım dosyası veya dizininin izinleri güvenli değil",
            ));
        }
    }
    Ok(())
}

/// Başlatılacak programı ve nerede çalışacağını tanımlayan doğrulanmış istek.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    /// Çalıştırılacak programın **tam yolu** (PATH çözümlemesi yapılmış).
    pub program: PathBuf,
    /// Programa geçirilecek argümanlar (ayrı ayrı, kabuk yok).
    pub args: Vec<OsString>,
    /// Yeni sekmenin açılacağı dizin (göreli yol değil, tam yol).
    pub working_directory: PathBuf,
    /// Programa standart girişten verilecek veri (genellikle yok).
    ///
    /// Sabit kullanım metnini bir sayfalayıcıya beslemek için kullanılır.
    /// Konsole, başlattığı programın stdin'ini yönlendiremediği için veri
    /// geçici bir dosyaya yazılır ve sekmede sayfalayıcıya **dosya adı**
    /// olarak verilir (bkz. [`LaunchRequest::stdin_path`]).
    pub stdin_data: Option<Vec<u8>>,
    /// `stdin_data` için güvenli biçimde oluşturulan dosya. Konsole asenkron
    /// başladığından dosya pager kapatana kadar korunur.
    pub stdin_file_path: Option<PathBuf>,
}

impl LaunchRequest {
    /// Programa standart girişten verilecek veriyi ayarlar.
    pub fn set_stdin(&mut self, data: Vec<u8>) {
        self.stdin_data = Some(data);
        self.stdin_file_path = None;
    }

    /// Önceden oluşturulmuş geçici dosyanın yolu.
    pub fn stdin_path(&self) -> Option<&Path> {
        self.stdin_file_path.as_deref()
    }

    /// `stdin_data` varsa özel izinli, çakışmayan bir geçici dosyaya yazar.
    pub fn write_stdin_file(&mut self) -> Result<Option<PathBuf>, NovaError> {
        let Some(data) = &self.stdin_data else {
            return Ok(None);
        };
        if let Some(path) = &self.stdin_file_path {
            return Ok(Some(path.clone()));
        }

        let (path, mut file) = create_private_temp_file().map_err(|e| {
            NovaError::Launcher(format!(
                "kullanım metni için güvenli geçici dosya açılamadı: {e}"
            ))
        })?;
        if let Err(e) = file.write_all(data).and_then(|()| file.sync_all()) {
            let _ = remove_private_help_file(&path);
            return Err(NovaError::Launcher(format!(
                "kullanım metni geçici dosyaya yazılamadı ('{}'): {e}",
                path.display()
            )));
        }
        self.stdin_file_path = Some(path.clone());
        Ok(Some(path))
    }

    /// Bir katalog girdisinden, verilen ev dizinini kullanarak istek oluşturur.
    ///
    /// Doğrulamalar burada ve **süreç başlatılmadan önce** yapılır:
    ///
    /// - Araç kurulu değilse (`Missing`) veya durumu bilinmiyorsa istek
    ///   oluşturulmaz.
    /// - Çalışma dizini yoksa, bir dizin değilse veya çözümlenemiyorsa hata
    ///   döner.
    ///
    /// Bu doğrulama zorunludur: Konsole, geçersiz bir `--workdir` verildiğinde
    /// hata vermez, sessizce ev dizinine döner. Bu davranış gerçek Konsole
    /// 26.08 üzerinde doğrulandı; bu yüzden kontrol Nova'da yapılmalıdır.
    pub fn from_entry(entry: &CatalogEntry, home: &Path) -> Result<Self, NovaError> {
        Self::from_entry_with_project(entry, home, None)
    }

    /// `from_entry` ile aynıdır; ancak seçili proje dizini verildiğinde,
    /// `working_directory = "project"` girdisinin **kendi** (örneğin bayatlamış)
    /// dizini doğrulanmadan etkin proje dizini kullanılır.
    ///
    /// Seçili proje yoksa girdinin kendi dizini doğrulanır ve geçersizse hata
    /// döner (sessiz geri düşme yok).
    pub fn from_entry_with_project(
        entry: &CatalogEntry,
        home: &Path,
        project: Option<&Path>,
    ) -> Result<Self, NovaError> {
        use crate::detection::InstalledStatus;

        match entry.status {
            InstalledStatus::Installed => {}
            InstalledStatus::Missing => {
                return Err(NovaError::Launcher(format!(
                    "'{}' kurulu değil; '{}' komutu PATH üzerinde bulunamadı. \
                     Kurulumu senin onayınla yapılmalı.",
                    entry.name, entry.executable
                )));
            }
            InstalledStatus::Unknown => {
                return Err(NovaError::Launcher(format!(
                    "'{}' için kurulum durumu kontrol edilemedi; başlatılmadı.",
                    entry.name
                )));
            }
        }

        let program = entry.resolved_path.clone().ok_or_else(|| {
            NovaError::Launcher(format!(
                "'{}' çalıştırılabilir dosyası bulunamadı",
                entry.name
            ))
        })?;

        // Seçili proje varsa girdinin kendi dizini hiç doğrulanmaz: bayat ad
        // artık kullanılmayacak, etkin proje geçerlidir.
        let selection = match (&entry.working_directory, project) {
            (crate::catalog::WorkingDirectory::Project(_), Some(p)) => {
                crate::catalog::WorkingDirectory::Project(p.to_path_buf())
            }
            _ => entry.working_directory.clone(),
        };

        let working_directory = selection.resolve(home, &entry.name)?;

        Ok(Self {
            program,
            args: entry.args.iter().map(OsString::from).collect(),
            working_directory,
            stdin_data: None,
            stdin_file_path: None,
        })
    }
}

fn create_private_temp_file() -> io::Result<(PathBuf, File)> {
    let temp_root = fs::canonicalize(std::env::temp_dir())?;
    let root_metadata = fs::metadata(&temp_root)?;
    if !root_metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "geçici yol bir dizin değil",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = root_metadata.permissions().mode();
        if mode & 0o022 != 0 && mode & 0o1000 == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "geçici dizin ortak yazmaya açık ve sticky değil",
            ));
        }
    }

    for _ in 0..100 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let private_dir = temp_root.join(format!(
            "nova-help-{}-{nanos}-{counter}",
            std::process::id()
        ));
        let mut dir_builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            dir_builder.mode(0o700);
        }
        match dir_builder.create(&private_dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
        let path = private_dir.join("help.txt");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) => {
                let _ = fs::remove_dir(&private_dir);
                return Err(error);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "benzersiz geçici dosya adı oluşturulamadı",
    ))
}

/// Bir yolun daha önce var olup olmadığını kontrol edilebilir kılan doğrulayıcı.
pub trait DirectoryValidator {
    /// Dizin geçerliyse tam yolunu döner.
    fn validate(&self, path: &Path) -> Option<PathBuf>;
}

/// Gerçek dosya sistemini kullanan doğrulayıcı.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsValidator;

impl DirectoryValidator for FsValidator {
    fn validate(&self, path: &Path) -> Option<PathBuf> {
        if !path.is_dir() {
            return None;
        }
        // `canonicalize` hem göreli parçaları hem sembolik bağlantıları
        // çözer; Konsole'ye tek ve kesin bir yol vermek için kullanılır.
        std::fs::canonicalize(path).ok()
    }
}

/// Çalışma dizini seçimini **doğrulayarak** tam yola çevirir.
///
/// Yalnızca `FsValidator` kullanır; testler kendi doğrulayıcılarını geçirerek
/// dosya sistemi gerektirmeden davranışı sınayabilir.
pub fn resolve_directory<V: DirectoryValidator>(
    selection: &crate::catalog::WorkingDirectory,
    home: &Path,
    validator: &V,
    tool_name: &str,
) -> Result<PathBuf, NovaError> {
    use crate::catalog::WorkingDirectory;

    let (candidate, origin) = match selection {
        WorkingDirectory::Home | WorkingDirectory::Current => (home.to_path_buf(), "ev dizini"),
        WorkingDirectory::Custom(path) => (path.clone(), "araç için tanımlı dizin"),
        WorkingDirectory::Project(path) => (path.clone(), "proje dizini"),
    };

    if let Some(resolved) = validator.validate(&candidate) {
        return Ok(resolved);
    }

    if home.as_os_str().is_empty() {
        return Err(NovaError::Launcher(format!(
            "'{tool_name}' için çalışma dizini belirlenemedi: ev dizini bulunamadı \
             ve {origin} '{}' geçerli değil.",
            candidate.display()
        )));
    }

    // Araç özelinde bir dizin tanımlandıysa ve geçersizse sessizce ev
    // dizinine düşmek yanıltıcı olur: kullanıcı yanlış dizinde açılmış bir
    // araç görür. Bu yüzden açıkça hata verilir.
    Err(NovaError::Launcher(format!(
        "'{tool_name}' için {origin} geçerli değil: '{}' bulunamadı veya bir dizin değil.",
        candidate.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{LaunchMode, WorkingDirectory};
    use std::collections::HashSet;

    /// Dosya sistemi gerektirmeyen doğrulayıcı: yalnızca listedeki yollar geçerli.
    struct FakeValidator(HashSet<PathBuf>);

    impl DirectoryValidator for FakeValidator {
        fn validate(&self, path: &Path) -> Option<PathBuf> {
            if self.0.contains(path) {
                Some(path.to_path_buf())
            } else {
                None
            }
        }
    }

    fn validator(paths: &[&str]) -> FakeValidator {
        FakeValidator(paths.iter().map(PathBuf::from).collect())
    }

    fn entry_with(executable: &str, args: Vec<String>, wd: WorkingDirectory) -> CatalogEntry {
        CatalogEntry::new(
            "test",
            "Test Aracı",
            "test",
            "Test",
            executable,
            args,
            LaunchMode::Terminal,
            wd,
            false,
        )
    }

    #[test]
    fn home_selection_resolves_to_home() {
        let home = PathBuf::from("/home/kullanici");
        let v = validator(&["/home/kullanici"]);
        let got = resolve_directory(&WorkingDirectory::Home, &home, &v, "test").unwrap();
        assert_eq!(got, PathBuf::from("/home/kullanici"));
    }

    #[test]
    fn current_selection_falls_back_to_home_not_process_cwd() {
        // Nova `cargo run` ile çalıştırılsa bile bir sekmenin depo dizininde
        // açılması beklenmez; güvenli varsayılan ev dizinidir.
        let home = PathBuf::from("/home/kullanici");
        let v = validator(&["/home/kullanici"]);
        let got = resolve_directory(&WorkingDirectory::Current, &home, &v, "test").unwrap();
        assert_eq!(got, PathBuf::from("/home/kullanici"));
    }

    #[test]
    fn custom_directory_is_used_verbatim() {
        let home = PathBuf::from("/home/kullanici");
        let custom = WorkingDirectory::Custom(PathBuf::from("/srv/proje"));
        let v = validator(&["/home/kullanici", "/srv/proje"]);
        let got = resolve_directory(&custom, &home, &v, "test").unwrap();
        assert_eq!(got, PathBuf::from("/srv/proje"));
    }

    #[test]
    fn project_directory_is_supported_for_future_projects() {
        let home = PathBuf::from("/home/kullanici");
        let project = WorkingDirectory::Project(PathBuf::from("/home/kullanici/git/nova"));
        let v = validator(&["/home/kullanici/git/nova"]);
        let got = resolve_directory(&project, &home, &v, "test").unwrap();
        assert_eq!(got, PathBuf::from("/home/kullanici/git/nova"));
    }

    #[test]
    fn nonexistent_custom_directory_is_an_error_not_a_fallback() {
        let home = PathBuf::from("/home/kullanici");
        let custom = WorkingDirectory::Custom(PathBuf::from("/yok/boyle/bir/dizin"));
        let v = validator(&["/home/kullanici"]);
        let err = resolve_directory(&custom, &home, &v, "lazygit").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("lazygit"), "mesaj aracı anmalı: {msg}");
        assert!(
            msg.contains("/yok/boyle/bir/dizin"),
            "mesaj yolu anmalı: {msg}"
        );
    }

    #[test]
    fn path_with_spaces_and_special_characters_is_preserved() {
        let home = PathBuf::from("/home/kullanici");
        let odd = PathBuf::from("/tmp/bir dizin/özel & $dizin 'tırnak'");
        let custom = WorkingDirectory::Custom(odd.clone());
        let v = validator(&["/tmp/bir dizin/özel & $dizin 'tırnak'"]);
        let got = resolve_directory(&custom, &home, &v, "test").unwrap();
        assert_eq!(got, odd);
    }

    #[test]
    fn missing_executable_never_produces_a_request() {
        let entry = entry_with(
            "nova-yok-boyle-bir-komut-xyz",
            vec![],
            WorkingDirectory::Home,
        );
        let err = LaunchRequest::from_entry(&entry, Path::new("/home/kullanici")).unwrap_err();
        assert!(err.to_string().contains("kurulu değil"), "hata: {err}");
    }

    /// Gerçek ev dizini: bu testte istek `from_entry` ile üretildiği için
    /// dosya sistemi doğrulaması gerçek dizine ihtiyaç duyar.
    fn real_home() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .expect("testler için HOME gerekli")
    }

    #[test]
    fn args_are_kept_separate_and_untouched() {
        let entry = entry_with(
            "sh",
            vec![
                "-c".into(),
                "echo 'tek argüman'".into(),
                "a b".into(),
                "$HOME".into(),
            ],
            WorkingDirectory::Home,
        );
        if entry.status != crate::detection::InstalledStatus::Installed {
            return; // sh yoksa anlamlı değil
        }
        let home = real_home();
        let req = LaunchRequest::from_entry(&entry, &home).unwrap();
        assert_eq!(req.args.len(), 4);
        assert_eq!(req.args[1], OsString::from("echo 'tek argüman'"));
        assert_eq!(req.args[2], OsString::from("a b"));
        // `$HOME` genişletilmez: kabuk yok.
        assert_eq!(req.args[3], OsString::from("$HOME"));
    }

    #[test]
    fn active_project_overrides_stale_project_path() {
        // Girdinin kendi `project` dizini bayat (yok); seçili proje dizini
        // geçerli. Eski davranış bayat yolu çözmeye çalışıp hata veriyordu.
        let dir = std::env::temp_dir().join(format!("nova-proje-testi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let home = real_home();
        let entry = entry_with(
            "sh",
            vec![],
            WorkingDirectory::Project("/yok/boyle/bir/dizin".into()),
        );
        if entry.status != crate::detection::InstalledStatus::Installed {
            let _ = std::fs::remove_dir_all(&dir);
            return; // sh yoksa anlamlı değil
        }
        let req = LaunchRequest::from_entry_with_project(&entry, &home, Some(&dir)).unwrap();
        let expected = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(req.working_directory, expected);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_project_path_is_an_error_without_active_project() {
        // Seçili proje yokken girdinin kendi `project` dizini geçersizse
        // sessizce ev dizinine düşülmez: açık hata döner.
        let home = real_home();
        let entry = entry_with(
            "sh",
            vec![],
            WorkingDirectory::Project("/yok/boyle/bir/dizin".into()),
        );
        if entry.status != crate::detection::InstalledStatus::Installed {
            return; // sh yoksa anlamlı değil
        }
        let err = LaunchRequest::from_entry(&entry, &home).unwrap_err();
        assert!(err.to_string().contains("/yok/boyle/bir/dizin"), "{err}");
    }
}
