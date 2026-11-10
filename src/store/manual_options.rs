//! Manuel / trust-D mağaza kayıtları için kurulum seçenekleri.
//!
//! `install_notes` metnini Enter modalında seçilebilir satırlara çevirir.
//! Nova bu komutları **çalıştırmaz**; yalnızca okunabilir seçenek sunar.

use crate::store::StoreEntry;

/// Seçenek türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualInstallKind {
    /// Upstream komut satırı — Nova yürütmez.
    Command,
    /// `https://` belge / kaynak sayfası.
    OpenUrl,
}

/// Manuel kurulum modalında bir satır.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualInstallOption {
    /// Kısa etiket (Bun, Homebrew, Resmî…).
    pub label: String,
    /// Komut veya URL.
    pub detail: String,
    /// Eylem türü.
    pub kind: ManualInstallKind,
}

/// `install_notes` satırlarından seçenek listesi üretir.
///
/// Tanınan biçimler:
/// - `Etiket: komut…`
/// - `Resmî: https://…` / `Official: https://…`
/// - Tek başına `https://…`
/// - İki nokta olmayan komut satırları (`make && …`)
///
/// `Not:` / `Note:` satırları ve boş etiketler atlanır.
pub fn parse_manual_install_options(notes: &str) -> Vec<ManualInstallOption> {
    let mut out = Vec::new();
    for raw in notes.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("not:")
            || lower.starts_with("note:")
            || lower.starts_with("not :")
            || lower.starts_with("note :")
        {
            continue;
        }

        if let Some(url) = https_url_token(line) {
            if line == url || line.starts_with("https://") {
                push_unique(
                    &mut out,
                    ManualInstallOption {
                        label: "Resmî".into(),
                        detail: url,
                        kind: ManualInstallKind::OpenUrl,
                    },
                );
                continue;
            }
        }

        if let Some((label, rest)) = split_label_rest(line) {
            let rest = rest.trim();
            if rest.is_empty() {
                // "Resmî kurulumlar (upstream README):" gibi başlık — atla.
                continue;
            }
            if let Some(url) = https_url_token(rest) {
                push_unique(
                    &mut out,
                    ManualInstallOption {
                        label: shorten_label(label),
                        detail: url,
                        kind: ManualInstallKind::OpenUrl,
                    },
                );
                continue;
            }
            push_unique(
                &mut out,
                ManualInstallOption {
                    label: shorten_label(label),
                    detail: rest.to_string(),
                    kind: ManualInstallKind::Command,
                },
            );
            continue;
        }

        // İki noktasız satır: ham komut.
        push_unique(
            &mut out,
            ManualInstallOption {
                label: "Komut".into(),
                detail: line.to_string(),
                kind: ManualInstallKind::Command,
            },
        );
    }
    out
}

/// Mağaza kaydı için modal seçenekleri (`install_notes` + kaynak URL).
///
/// `open_source_label` yerelleştirilmiş "Kaynak sayfasını aç" metnidir.
pub fn options_for_entry(entry: &StoreEntry, open_source_label: &str) -> Vec<ManualInstallOption> {
    let mut opts = entry
        .install_notes
        .as_deref()
        .map(parse_manual_install_options)
        .unwrap_or_default();

    if let Some(url) = https_url_token(entry.source.trim()) {
        let already = opts
            .iter()
            .any(|o| o.kind == ManualInstallKind::OpenUrl && o.detail == url);
        if !already {
            opts.push(ManualInstallOption {
                label: open_source_label.to_string(),
                detail: url,
                kind: ManualInstallKind::OpenUrl,
            });
        }
    }

    if opts.is_empty() {
        opts.push(ManualInstallOption {
            label: open_source_label.to_string(),
            detail: entry.source.clone(),
            kind: if entry.source.trim().starts_with("https://") {
                ManualInstallKind::OpenUrl
            } else {
                ManualInstallKind::Command
            },
        });
    }
    opts
}

fn split_label_rest(line: &str) -> Option<(&str, &str)> {
    // İlk `:` — URL şemasındaki `https:` ile karışmasın diye satır
    // `https://` ile başlıyorsa etiket yok sayılır (üstte yakalandı).
    if line.starts_with("https://") {
        return None;
    }
    line.split_once(':')
}

fn https_url_token(text: &str) -> Option<String> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("https://") {
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '|')
            .unwrap_or(rest.len());
        let url = format!("https://{}", &rest[..end]);
        if url.len() > "https://".len() {
            return Some(url);
        }
    }
    // Metin içinde gömülü URL (nadiren).
    if let Some(idx) = text.find("https://") {
        return https_url_token(&text[idx..]);
    }
    None
}

fn shorten_label(label: &str) -> String {
    let label = label.trim();
    // "Bun (önerilen, Bun >= 1.4.4)" → ilk kelimeyi koru; parantezli uzun
    // etiketleri olduğu gibi bırak ama aşırı uzunsa kısalt.
    let chars: Vec<char> = label.chars().collect();
    if chars.len() <= 42 {
        return label.to_string();
    }
    let kept: String = chars.into_iter().take(41).collect();
    format!("{kept}…")
}

fn push_unique(out: &mut Vec<ManualInstallOption>, opt: ManualInstallOption) {
    if out
        .iter()
        .any(|e| e.kind == opt.kind && e.detail == opt.detail)
    {
        return;
    }
    out.push(opt);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{InstallMethod, StoreEntry};

    fn entry_with_notes(notes: &str) -> StoreEntry {
        StoreEntry {
            id: "oh-my-pi".into(),
            name: "oh-my-pi".into(),
            description: "t".into(),
            category: "AI".into(),
            source: "https://github.com/can1357/oh-my-pi".into(),
            license: "MIT".into(),
            executable: "omp".into(),
            install_method: InstallMethod::Manual,
            supported_platforms: vec!["linux-x64".into()],
            checksum: None,
            checksum_type: None,
            version: None,
            download_url: None,
            artifacts: vec![],
            install_command: None,
            install_notes: Some(notes.into()),
            release: None,
            repository_url: None,
            signature: None,
        }
    }

    #[test]
    fn parses_oh_my_pi_notes_into_structured_options() {
        let notes = r#"
Resmî kurulumlar (upstream README):
Bun (önerilen, Bun >= 1.4.4): bun install -g @oh-my-pi/pi-coding-agent
Homebrew: brew install can1357/tap/omp
Kurulum betiği (ön derleme ikilisini SHA-256 ile doğrular): curl -fsSL https://omp.sh/install | sh
Nix: nix profile add github:can1357/oh-my-pi
mise: mise use -g github:can1357/oh-my-pi
Not: Nova uzak curl|sh / Bun yollarını otomatik çalıştırmaz.
Resmî: https://github.com/can1357/oh-my-pi
"#;
        let opts = parse_manual_install_options(notes);
        assert!(
            opts.iter()
                .any(|o| o.label.starts_with("Bun") && o.detail.contains("bun install")),
            "{opts:?}"
        );
        assert!(
            opts.iter()
                .any(|o| o.label == "Homebrew" && o.detail.contains("brew install")),
            "{opts:?}"
        );
        assert!(
            opts.iter().any(|o| {
                o.label.starts_with("Kurulum betiği") && o.detail.contains("omp.sh/install")
            }),
            "{opts:?}"
        );
        assert!(
            opts.iter().any(|o| o.kind == ManualInstallKind::OpenUrl
                && o.detail == "https://github.com/can1357/oh-my-pi"),
            "{opts:?}"
        );
        assert!(
            opts.iter().all(|o| !o.detail.contains("Nova uzak")),
            "Not: satırı seçenek olmamalı: {opts:?}"
        );
    }

    #[test]
    fn options_for_entry_adds_source_when_missing() {
        let entry = entry_with_notes("go: go install example.com/x@latest\n");
        let opts = options_for_entry(&entry, "Kaynak sayfasını aç");
        assert!(
            opts.iter()
                .any(|o| o.label == "go" && o.kind == ManualInstallKind::Command),
            "{opts:?}"
        );
        assert!(
            opts.iter().any(|o| {
                o.kind == ManualInstallKind::OpenUrl
                    && o.detail == "https://github.com/can1357/oh-my-pi"
                    && o.label == "Kaynak sayfasını aç"
            }),
            "{opts:?}"
        );
    }

    #[test]
    fn skips_header_only_and_parses_bare_command() {
        let opts = parse_manual_install_options(
            "Resmî: https://example.com/x\nmake && sudo make install\n",
        );
        assert_eq!(opts.len(), 2);
        assert_eq!(opts[0].kind, ManualInstallKind::OpenUrl);
        assert_eq!(opts[1].kind, ManualInstallKind::Command);
        assert_eq!(opts[1].detail, "make && sudo make install");
    }
}
