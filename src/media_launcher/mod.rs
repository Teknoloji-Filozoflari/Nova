//! Medya başlatıcı modülü.
//!
//! Seçilen bir dosyayı ayarlarda belirlenmiş uygulama ile açar. Açma işlemi
//! **kabuk kullanılmadan**, executable ve dosya yolu ayrı argümanlar olarak
//! yürütülür. Nova, sadece sürecin başlatıldığını doğrular; dosyanın
//! gerçekten açıldığı/iddaa edilmez. Terminal dışı medya uygulamaları ayrı
//! ve küçük bir başlatma yolu olarak ele alınır; Nova'nın Konsole sekme
//! başlatıcısı bozulmaz.
//!
//! Dosya silme, taşıma veya yeniden adlandırma yapılmaz.

use crate::error::NovaError;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

/// Dosya türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    /// Resim dosyası.
    Image,
    /// Video dosyası.
    Video,
    /// Türü bilinmeyen / desteklenmeyen dosya.
    Unknown,
}

/// Medya başlatıcısı.
///
/// `default_image_app` ve `default_video_app` ayarlarından gelir. Her ikisi
/// de `None` ise ilgili dosya türü için hata döner.
#[derive(Debug, Clone, Default)]
pub struct MediaLauncher {
    /// Resim dosyaları için varsayılan uygulama (katalog `id` veya executable adı).
    pub default_image_app: Option<String>,
    /// Video dosyaları için varsayılan uygulama (katalog `id` veya executable adı).
    pub default_video_app: Option<String>,
}

impl MediaLauncher {
    /// Yeni bir medya başlatıcısı oluşturur.
    pub fn new(default_image_app: Option<String>, default_video_app: Option<String>) -> Self {
        Self {
            default_image_app,
            default_video_app,
        }
    }

    /// Dosya uzantısına göre medya türünü tespit eder.
    pub fn detect_media_type(path: &Path) -> MediaType {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" | "ico" | "tiff" | "tif" => {
                MediaType::Image
            }
            "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "mpeg" | "mpg" => {
                MediaType::Video
            }
            _ => MediaType::Unknown,
        }
    }

    /// `path` dosyasını uygun uygulama ile açar.
    ///
    /// # Güvenlik
    /// - Kabuk (`sh -c`) çalıştırılmaz; executable ve dosya yolu ayrı ayrı
    ///   `std::process::Command`'a verilir.
    /// - `sudo`/root kullanılmaz, sistem dizinlerine yazılmaz.
    /// - Sadece sürecin başlatılması doğrulanır (PID döner); dosyanın
    ///   gerçekten açıldığı iddiası edilmez.
    pub fn launch_media(&self, path: &Path) -> Result<u32, NovaError> {
        let media_type = Self::detect_media_type(path);
        let app =
            match media_type {
                MediaType::Image => self.default_image_app.clone(),
                MediaType::Video => self.default_video_app.clone(),
                MediaType::Unknown => return Err(NovaError::Launcher(
                    "Bu dosya türü için varsayılan uygulama ayarlanmamış. Ayarlardan resim veya \
                     video uygulaması seçebilirsiniz."
                        .into(),
                )),
            };
        let app = app.ok_or_else(|| {
            NovaError::Launcher(
                "Bu dosya türü için varsayılan uygulama ayarlanmamış. Ayarlardan resim veya \
                 video uygulaması seçebilirsiniz."
                    .into(),
            )
        })?;

        // Program ve dosya yolu ayrı ayrı geçirilir; kabuk yok.
        let mut cmd = Command::new(&app);
        cmd.arg(path);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd
            .spawn()
            .map_err(|e| NovaError::Launcher(format!("'{app}' çalıştırılamadı: {e}")))?;
        let pid = child.id();
        drop(child);
        Ok(pid)
    }

    /// `path` dosyasını belirtilen spawner ile açar (testlerde gerçek süreç
    /// başlatılmadan davranış sınanabilir).
    pub fn launch_media_with<S: ProcessSpawner>(
        &self,
        path: &Path,
        spawner: &S,
    ) -> Result<u32, NovaError> {
        let media_type = Self::detect_media_type(path);
        let app =
            match media_type {
                MediaType::Image => self.default_image_app.clone(),
                MediaType::Video => self.default_video_app.clone(),
                MediaType::Unknown => return Err(NovaError::Launcher(
                    "Bu dosya türü için varsayılan uygulama ayarlanmamış. Ayarlardan resim veya \
                     video uygulaması seçebilirsiniz."
                        .into(),
                )),
            };
        let app = app.ok_or_else(|| {
            NovaError::Launcher(
                "Bu dosya türü için varsayılan uygulama ayarlanmamış. Ayarlardan resim veya \
                 video uygulaması seçebilirsiniz."
                    .into(),
            )
        })?;

        let args: Vec<OsString> = vec![path.as_os_str().to_owned()];
        spawner.spawn(Path::new(&app), &args)
    }
}

/// Süreç başlatmayı soyutlar. Testler gerçek süreç açmadan aynı yolu sınayabilir.
pub trait ProcessSpawner {
    /// Verilen programı argümanlarla başlatır ve bu çalıştırılabilir dosyanın
    /// PID'ini döner.
    fn spawn(&self, program: &Path, args: &[OsString]) -> Result<u32, NovaError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;

    /// Sahte spawner: çağrıları kaydeder, gerçek süreç başlatmaz.
    struct FakeSpawner {
        calls: RefCell<Vec<(PathBuf, Vec<OsString>)>>,
        fail: bool,
        pid: u32,
    }

    impl FakeSpawner {
        fn ok() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                fail: false,
                pid: 1234,
            }
        }
        fn failing() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                fail: true,
                pid: 0,
            }
        }
    }

    impl ProcessSpawner for FakeSpawner {
        fn spawn(&self, program: &Path, args: &[OsString]) -> Result<u32, NovaError> {
            self.calls
                .borrow_mut()
                .push((program.to_path_buf(), args.to_vec()));
            if self.fail {
                return Err(NovaError::Launcher("spawn başarısız".into()));
            }
            Ok(self.pid)
        }
    }

    #[test]
    fn detect_media_type_recognizes_images_and_videos() {
        assert_eq!(
            MediaLauncher::detect_media_type(Path::new("resim.jpg")),
            MediaType::Image
        );
        assert_eq!(
            MediaLauncher::detect_media_type(Path::new("resim.PNG")),
            MediaType::Image
        );
        assert_eq!(
            MediaLauncher::detect_media_type(Path::new("video.mp4")),
            MediaType::Video
        );
        assert_eq!(
            MediaLauncher::detect_media_type(Path::new("belge.pdf")),
            MediaType::Unknown
        );
        assert_eq!(
            MediaLauncher::detect_media_type(Path::new("dosya")),
            MediaType::Unknown
        );
    }

    #[test]
    fn unknown_media_type_returns_error() {
        let launcher = MediaLauncher::new(Some("feh".into()), Some("mpv".into()));
        let err = launcher
            .launch_media_with(Path::new("belge.pdf"), &FakeSpawner::ok())
            .unwrap_err();
        assert!(err.to_string().contains("ayarlanmamış"), "hata: {err}");
    }

    #[test]
    fn missing_app_for_media_type_returns_error() {
        let launcher = MediaLauncher::new(None, None);
        let err = launcher
            .launch_media_with(Path::new("resim.jpg"), &FakeSpawner::ok())
            .unwrap_err();
        assert!(err.to_string().contains("ayarlanmamış"), "hata: {err}");
    }

    #[test]
    fn image_launches_with_image_app_and_separate_args() {
        let launcher = MediaLauncher::new(Some("feh".into()), None);
        let spawner = FakeSpawner::ok();
        let path = Path::new("/home/kullanici/resim.jpg");
        let pid = launcher
            .launch_media_with(path, &spawner)
            .expect("başlatma başarılı");
        assert_eq!(pid, 1234);
        let calls = spawner.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, Path::new("feh"));
        assert_eq!(calls[0].1.len(), 1);
        assert_eq!(calls[0].1[0], OsString::from("/home/kullanici/resim.jpg"));
    }

    #[test]
    fn video_launches_with_video_app_and_separate_args() {
        let launcher = MediaLauncher::new(None, Some("mpv".into()));
        let spawner = FakeSpawner::ok();
        let path = Path::new("/home/kullanici/video.mp4");
        let pid = launcher
            .launch_media_with(path, &spawner)
            .expect("başlatma başarılı");
        assert_eq!(pid, 1234);
        let calls = spawner.calls.borrow();
        assert_eq!(calls[0].0, Path::new("mpv"));
        assert_eq!(calls[0].1[0], OsString::from("/home/kullanici/video.mp4"));
    }

    #[test]
    fn spawn_failure_is_reported() {
        let launcher = MediaLauncher::new(Some("feh".into()), None);
        let spawner = FakeSpawner::failing();
        let err = launcher
            .launch_media_with(Path::new("resim.jpg"), &spawner)
            .unwrap_err();
        assert!(err.to_string().contains("spawn başarısız"), "hata: {err}");
    }

    #[test]
    fn no_shell_is_inserted_into_args() {
        let launcher = MediaLauncher::new(Some("feh".into()), None);
        let spawner = FakeSpawner::ok();
        let path = Path::new("/home/kullanici/resim;rm -rf / .jpg");
        let _ = launcher.launch_media_with(path, &spawner);
        let calls = spawner.calls.borrow();
        // Tüm metin tek bir argüman olarak geçer; kabuk yok.
        assert_eq!(calls[0].1.len(), 1);
        assert!(!calls[0].1.iter().any(|a| a == "sh" || a == "-c"));
    }
}
