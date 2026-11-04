//! Arka plan işleri için TUI içi ilerleme olayları.
//!
//! `cargo install`, paket yöneticisi kurulum/güncelleme ve Nova self-update
//! çıktısı terminali doldurmak yerine satır satır yakalanır; ana döngü
//! [`ProgressEvent`] ile modalı günceller.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

/// İlerleme modalında görünen iş türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// Nova `cargo install --git` (repo main).
    NovaRepo,
    /// Nova GitHub Releases ikili kurulumu.
    NovaRelease,
    /// Mağaza kurulum.
    StoreInstall,
    /// Mağaza güncelleme.
    StoreUpdate,
    /// Mağaza kaldırma.
    StoreUninstall,
}

impl JobKind {
    /// Modal başlığı için kısa etiket.
    pub fn title_prefix(self) -> &'static str {
        match self {
            Self::NovaRepo => "Nova kaynak güncellemesi",
            Self::NovaRelease => "Nova Releases güncellemesi",
            Self::StoreInstall => "Kurulum",
            Self::StoreUpdate => "Güncelleme",
            Self::StoreUninstall => "Kaldırma",
        }
    }
}

/// İş parçacığından TUI’ye akan olay.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// Üst durum / faz (ör. "Derleniyor").
    Phase(String),
    /// Kısa durum satırı (spinner yanında).
    Status(String),
    /// Kaydırılabilir iş günlüğüne bir satır.
    Log(String),
}

/// Günlükte tutulacak en fazla satır (eski satırlar düşer).
pub const JOB_LOG_CAP: usize = 400;

/// `cargo` / PM çıktısından insan okunur faz etiketi üretir.
pub fn classify_build_line(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    let t = lower.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains("updating git repository")
        || t.contains("cloning")
        || t.starts_with("fetching")
        || t.contains("downloaded")
    {
        return Some("Kaynak alınıyor");
    }
    if t.starts_with("compiling") || t.starts_with("building") || t.contains("running rustc") {
        return Some("Derleniyor");
    }
    if t.starts_with("checking") {
        return Some("Denetleniyor");
    }
    if t.starts_with("installing") || t.contains("replacing ") || t.contains("installed package") {
        return Some("Kuruluyor");
    }
    if t.starts_with("finished") || t.contains("finished `release`") {
        return Some("Tamamlanıyor");
    }
    if t.starts_with("error") || t.contains("error:") {
        return Some("Hata");
    }
    None
}

/// Gürültülü / boş satırları günlükten ele.
pub fn should_keep_log_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    // ANSI / progress bar kırıntıları
    if t.chars()
        .all(|c| c == '#' || c == '=' || c == '-' || c == ' ')
    {
        return false;
    }
    true
}

/// stdout+stderr’i satır satır okuyarak komut çalıştırır (TUI’ye spam yok).
///
/// `on_line` her satır için çağrılır (birleşik akış). Başarısız çıkışta `Err`
/// içinde son birleşik çıktı özeti (kırpılmış) döner.
pub fn run_command_streaming(
    program: &str,
    args: &[String],
    envs: &[(&str, &str)],
    on_line: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TERM_PROGRESS_WHEN", "never");
    for (k, v) in envs {
        cmd.env(k, v);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("'{program}' başlatılamadı: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "stdout bağlanamadı".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "stderr bağlanamadı".to_string())?;

    let (tx, rx) = mpsc::channel::<String>();
    let tx_err = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx_err.send(line);
        }
    });

    let mut tail: Vec<String> = Vec::new();
    while let Ok(line) = rx.recv() {
        if should_keep_log_line(&line) {
            on_line(&line);
            tail.push(line);
            if tail.len() > 40 {
                tail.remove(0);
            }
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("'{program}' beklendi: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        let detail = if tail.is_empty() {
            format!("çıkış kodu {:?}", status.code())
        } else {
            tail.join(" | ")
        };
        Err(format!(
            "'{program} {}' başarısız: {detail}",
            args.join(" ")
        ))
    }
}

/// İlerleme kanalına faz + satır gönderen yardımcı.
pub fn emit_line(tx: &Sender<ProgressEvent>, line: &str) {
    if let Some(phase) = classify_build_line(line) {
        let _ = tx.send(ProgressEvent::Phase(phase.to_string()));
    }
    // Derleme satırlarından kısa durum: "Compiling foo v1" → foo
    let status = summarize_build_line(line);
    if let Some(s) = status {
        let _ = tx.send(ProgressEvent::Status(s));
    }
    let _ = tx.send(ProgressEvent::Log(line.to_string()));
}

/// Uzun cargo satırını durum şeridi için kısaltır.
pub fn summarize_build_line(line: &str) -> Option<String> {
    let t = line.trim();
    if t.is_empty() {
        return None;
    }
    let mapped = [
        ("Compiling ", "Derleniyor:"),
        ("Checking ", "Denetleniyor:"),
        ("Installing ", "Kuruluyor:"),
        ("Downloading ", "İndiriliyor:"),
        ("Fetched ", "Alındı:"),
        ("Updating git repository", "Git güncelleniyor"),
    ];
    for (prefix, label) in mapped {
        if let Some(rest) = t.strip_prefix(prefix) {
            let short = rest
                .trim()
                .trim_matches('`')
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join(" ");
            if short.is_empty() {
                return Some(label.to_string());
            }
            return Some(format!("{label} {short}"));
        }
        if t.contains(prefix) && prefix.starts_with("Updating") {
            return Some(label.to_string());
        }
    }
    if t.chars().count() > 64 {
        Some(format!("{}…", t.chars().take(61).collect::<String>()))
    } else {
        Some(t.to_string())
    }
}

/// İlerleme olaylarını bir [`Receiver`] üzerinden tüketmek için kanal çifti.
pub fn progress_channel() -> (Sender<ProgressEvent>, Receiver<ProgressEvent>) {
    mpsc::channel()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_cargo_phases() {
        assert_eq!(
            classify_build_line("    Updating git repository `https://github.com/x`"),
            Some("Kaynak alınıyor")
        );
        assert_eq!(
            classify_build_line("   Compiling nova v0.1.0"),
            Some("Derleniyor")
        );
        assert_eq!(
            classify_build_line("    Finished `release` profile [optimized] target(s)"),
            Some("Tamamlanıyor")
        );
        assert_eq!(
            classify_build_line("  Installing /home/x/.cargo/bin/nova"),
            Some("Kuruluyor")
        );
    }

    #[test]
    fn drops_empty_log_noise() {
        assert!(!should_keep_log_line("   "));
        assert!(should_keep_log_line("   Compiling foo"));
    }
}
