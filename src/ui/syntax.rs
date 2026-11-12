//! Terminal komut satırı sözdizimi renklendirme.
//!
//! Mock dili:
//! - `$` → muted
//! - komut → cyan (`accent`)
//! - bayrak / alt-komut / `[seçenekler]` → sarı (`flag`)
//! - yol / değer / tırnaklı / `[dizin]` → yeşil (`path`)

use crate::ui::theme::ThemePalette;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

/// Tek bir kabuk satırını (`$ cmd -f arg`) Span listesine boyar.
pub fn highlight_shell(line: &str, pal: ThemePalette) -> Vec<Span<'static>> {
    highlight_shell_styled(line, pal, Style::default())
}

/// Panel arka planı (kod kartı) ile birleştirerek boyar.
pub fn highlight_shell_styled(line: &str, pal: ThemePalette, panel: Style) -> Vec<Span<'static>> {
    let prompt = Style::default().fg(pal.muted).patch(panel);
    let command = Style::default()
        .fg(pal.accent)
        .add_modifier(Modifier::BOLD)
        .patch(panel);
    let flag = Style::default().fg(pal.flag).patch(panel);
    let path = Style::default().fg(pal.path).patch(panel);
    let plain = Style::default().fg(pal.text).patch(panel);

    let trimmed = line.trim_start();
    let lead = line.len().saturating_sub(trimmed.len());
    let mut spans = Vec::new();
    if lead > 0 {
        spans.push(Span::styled(" ".repeat(lead), prompt));
    }

    let mut body = trimmed;
    if let Some(rest) = body.strip_prefix("$ ") {
        spans.push(Span::styled("$ ".to_string(), prompt));
        body = rest;
    } else if body == "$" {
        spans.push(Span::styled("$".to_string(), prompt));
        return spans;
    }

    let tokens = tokenize(body);
    let mut expect_command = true;
    for (i, tok) in tokens.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ".to_string(), prompt));
        }
        let style = classify_token(tok, expect_command, command, flag, path, plain, prompt);
        if is_command_reset(tok) {
            expect_command = true;
        } else if expect_command && !is_flag_like(tok) && !is_bracket_placeholder(tok) {
            expect_command = false;
        }
        spans.push(Span::styled(tok.clone(), style));
    }
    spans
}

/// `$ cmd …` önekli boyalı satır (liste ÖRNEK / detay örnekleri).
pub fn highlight_command_line(line: &str, pal: ThemePalette) -> Vec<Span<'static>> {
    let cmd = if line.trim_start().starts_with('$') {
        line.trim().to_string()
    } else {
        format!("$ {line}")
    };
    highlight_shell(&cmd, pal)
}

/// Satır başına önek (ör. `"  "`) ile boyalı kabuk satırı.
pub fn highlight_shell_prefixed(prefix: &str, line: &str, pal: ThemePalette) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if !prefix.is_empty() {
        spans.push(Span::styled(
            prefix.to_string(),
            Style::default().fg(pal.muted),
        ));
    }
    spans.extend(highlight_command_line(line, pal));
    spans
}

fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if matches!(c, '|' | '&' | ';' | '>' | '<') {
            let mut op = String::new();
            while let Some(&n) = chars.peek() {
                if matches!(n, '|' | '&' | ';' | '>' | '<') {
                    op.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(op);
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            let mut tok = String::new();
            tok.push(chars.next().unwrap());
            while let Some(n) = chars.next() {
                tok.push(n);
                if n == quote {
                    break;
                }
            }
            out.push(tok);
            continue;
        }
        let mut tok = String::new();
        while let Some(&n) = chars.peek() {
            if n.is_whitespace() || matches!(n, '|' | '&' | ';' | '>' | '<') {
                break;
            }
            if n == '[' {
                tok.push(chars.next().unwrap());
                while let Some(m) = chars.next() {
                    tok.push(m);
                    if m == ']' {
                        break;
                    }
                }
                continue;
            }
            tok.push(chars.next().unwrap());
        }
        if !tok.is_empty() {
            out.push(tok);
        }
    }
    out
}

fn is_command_reset(tok: &str) -> bool {
    matches!(tok, "|" | "||" | "&&" | ";" | ">" | ">>" | "<")
}

fn is_flag_like(tok: &str) -> bool {
    if tok == "-" || tok == "--" {
        return true;
    }
    tok.starts_with("--") || (tok.starts_with('-') && tok.len() > 1)
}

fn is_bracket_placeholder(tok: &str) -> bool {
    tok.contains('[') && tok.contains(']')
}

fn bracket_body(tok: &str) -> &str {
    if let (Some(s), Some(e)) = (tok.find('['), tok.find(']')) {
        if e > s {
            return &tok[s + 1..e];
        }
    }
    tok.trim_start_matches('[').trim_end_matches(']')
}

fn bracket_is_option(tok: &str) -> bool {
    let inner = bracket_body(tok).trim().to_ascii_lowercase();
    // Türkçe 'İ'/'ı' için ekstra kontroller
    let folded = inner
        .replace('ı', "i")
        .replace('İ', "i")
        .replace('ş', "s")
        .replace('ö', "o")
        .replace('ü', "u")
        .replace('ç', "c")
        .replace('ğ', "g");
    folded.contains("option")
        || folded.contains("secenek")
        || folded.contains("flag")
        || folded.contains("opt")
        || inner.contains("seçenek")
}

fn looks_like_value(tok: &str) -> bool {
    if tok.is_empty() {
        return false;
    }
    if (tok.starts_with('"') && tok.ends_with('"'))
        || (tok.starts_with('\'') && tok.ends_with('\''))
    {
        return true;
    }
    if tok.starts_with('/')
        || tok.starts_with("./")
        || tok.starts_with("../")
        || tok.starts_with('~')
    {
        return true;
    }
    if tok.contains('/') || tok.ends_with('/') {
        return true;
    }
    if tok == "." || tok == ".." {
        return true;
    }
    if tok.contains('*') || tok.contains('?') {
        return true;
    }
    if tok.contains('.') {
        if let Some(ext) = tok.rsplit('.').next() {
            if !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
                return true;
            }
        }
    }
    if tok.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}

fn classify_token(
    tok: &str,
    expect_command: bool,
    command: Style,
    flag: Style,
    path: Style,
    _plain: Style,
    prompt: Style,
) -> Style {
    if is_command_reset(tok) {
        return prompt;
    }
    if is_bracket_placeholder(tok) {
        return if bracket_is_option(tok) { flag } else { path };
    }
    if is_flag_like(tok) {
        return flag;
    }
    if expect_command {
        return command;
    }
    if looks_like_value(tok) {
        return path;
    }
    if is_likely_subcommand(tok) {
        flag
    } else {
        path
    }
}

fn is_likely_subcommand(tok: &str) -> bool {
    const SUBS: &[&str] = &[
        "status", "start", "stop", "restart", "enable", "disable", "reload", "install", "remove",
        "update", "upgrade", "search", "list", "show", "get", "set", "add", "commit", "push",
        "pull", "clone", "checkout", "branch", "merge", "rebase", "log", "diff", "init", "run",
        "exec", "build", "test", "help", "version", "info", "create", "delete", "apply", "edit",
        "open", "close", "attach", "detach", "kill", "wait",
    ];
    let lower = tok.to_ascii_lowercase();
    SUBS.contains(&lower.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::UiTheme;

    fn text(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn highlights_prompt_command_flag_path() {
        let pal = UiTheme::Default.palette();
        let spans = highlight_shell("$ ls -lh /var/log", pal);
        assert_eq!(text(&spans), "$ ls -lh /var/log");
        let ls = spans.iter().find(|s| s.content == "ls").unwrap();
        let flag = spans.iter().find(|s| s.content == "-lh").unwrap();
        let path = spans.iter().find(|s| s.content == "/var/log").unwrap();
        assert_eq!(ls.style.fg, Some(pal.accent));
        assert_eq!(flag.style.fg, Some(pal.flag));
        assert_eq!(path.style.fg, Some(pal.path));
        assert_eq!(spans[0].style.fg, Some(pal.muted));
    }

    #[test]
    fn highlights_quoted_and_subcommand() {
        let pal = UiTheme::Default.palette();
        let spans = highlight_shell("$ grep -rn \"TODO\" proje/", pal);
        assert!(text(&spans).contains("\"TODO\""));
        let q = spans.iter().find(|s| s.content == "\"TODO\"").unwrap();
        assert_eq!(q.style.fg, Some(pal.path));
        let spans2 = highlight_shell("$ systemctl status ssh", pal);
        assert_eq!(text(&spans2), "$ systemctl status ssh");
        let status = spans2.iter().find(|s| s.content == "status").unwrap();
        let ssh = spans2.iter().find(|s| s.content == "ssh").unwrap();
        assert_eq!(status.style.fg, Some(pal.flag));
        assert_eq!(ssh.style.fg, Some(pal.path));
    }

    #[test]
    fn highlights_syntax_placeholders() {
        let pal = UiTheme::Default.palette();
        let spans = highlight_shell("ls [seçenekler] [dizin]", pal);
        let opt = spans
            .iter()
            .find(|s| s.content.as_ref() == "[seçenekler]")
            .unwrap();
        let dir = spans
            .iter()
            .find(|s| s.content.as_ref() == "[dizin]")
            .unwrap();
        assert_eq!(opt.style.fg, Some(pal.flag));
        assert_eq!(dir.style.fg, Some(pal.path));
        let spans2 = highlight_shell("ls [OPTION]... [FILE]...", pal);
        let opt2 = spans2
            .iter()
            .find(|s| s.content.contains("OPTION"))
            .unwrap();
        let file = spans2.iter().find(|s| s.content.contains("FILE")).unwrap();
        assert_eq!(opt2.style.fg, Some(pal.flag));
        assert_eq!(file.style.fg, Some(pal.path));
    }
}
