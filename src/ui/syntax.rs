//! Terminal komut satırı sözdizimi renklendirme.
//!
//! Mock `$` kuralları:
//! - **Girdi** satırında `$` muted (yalnızca istem); ardından cyan cmd · gold flag · green path
//! - **Çıktı** satırında `$` yok — tüm satır muted
//! - **Sözdizimi** kutusunda `$` yok — çıplak `ls [seçenekler] [dizin]`
//! - **İpucu** gövdesinde satır-içi snippet: `$` yok (prompt satırı değilse); cmd/flag/path boyanır

use crate::ui::theme::ThemePalette;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

/// Tek bir kabuk satırını boyar. Satırda zaten `$` varsa muted prompt olarak bırakır;
/// yoksa **eklemez** (sözdizimi / satır-içi snippet).
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

    // Çıktı satırı (ls listesi, ağaç, …): `$` yok, tamamen muted.
    if looks_like_output(trimmed) {
        spans.push(Span::styled(trimmed.to_string(), prompt));
        return spans;
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
        if i > 0 && !tight_operator_join(&tokens[i - 1], tok) {
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

/// Örnek **girdi** satırı: `$` yoksa ekler, sonra boyar.
pub fn highlight_prompt_line(line: &str, pal: ThemePalette) -> Vec<Span<'static>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    // Çıktıya yanlışlıkla prompt basma.
    if looks_like_output(trimmed) && !trimmed.starts_with('$') {
        return highlight_output_line(trimmed, pal);
    }
    let cmd = if trimmed.starts_with("$ ") || trimmed == "$" {
        trimmed.to_string()
    } else if let Some(rest) = trimmed.strip_prefix('$') {
        // `$ls` → `$ ls`
        format!("$ {}", rest.trim_start())
    } else {
        format!("$ {trimmed}")
    };
    highlight_shell(&cmd, pal)
}

/// Geriye dönük ad — [`highlight_prompt_line`].
pub fn highlight_command_line(line: &str, pal: ThemePalette) -> Vec<Span<'static>> {
    highlight_prompt_line(line, pal)
}

/// Örnek **çıktı** satırı: `$` yok, muted.
pub fn highlight_output_line(line: &str, pal: ThemePalette) -> Vec<Span<'static>> {
    vec![Span::styled(
        line.to_string(),
        Style::default().fg(pal.muted),
    )]
}

/// Satır başına önek + prompt girdi satırı (`$` eklenir).
pub fn highlight_shell_prefixed(prefix: &str, line: &str, pal: ThemePalette) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if !prefix.is_empty() {
        spans.push(Span::styled(
            prefix.to_string(),
            Style::default().fg(pal.muted),
        ));
    }
    spans.extend(highlight_prompt_line(line, pal));
    spans
}

/// İpucu / düzyazı: satır-içi cmd/flag/path boyaması.
/// `$` yalnızca gerçek istem gibi durduğunda (sonraki token komut/bayrak) muted kalır;
/// aksi halde eklenmez ve düz metin gibi işlenir.
pub fn highlight_tip_text(text: &str, pal: ThemePalette, tip: Style) -> Vec<Span<'static>> {
    let prompt = Style::default().fg(pal.muted);
    let command = Style::default().fg(pal.accent).add_modifier(Modifier::BOLD);
    let flag = Style::default().fg(pal.flag);
    let path = Style::default().fg(pal.path);

    let mut spans = Vec::new();
    let parts = split_ws_keep(text);
    let mut i = 0usize;
    while i < parts.len() {
        let part = &parts[i];
        if part.chars().all(char::is_whitespace) {
            spans.push(Span::styled(part.clone(), tip));
            i += 1;
            continue;
        }
        // `$` + komut → gerçek prompt
        if part.as_str() == "$" {
            let next = parts[i + 1..]
                .iter()
                .find(|p| !p.chars().all(char::is_whitespace));
            if next.is_some_and(|n| is_flag_like(n) || is_command_word(n) || looks_like_value(n)) {
                spans.push(Span::styled("$".to_string(), prompt));
                i += 1;
                // sonraki boşluktan sonra komut beklentisi: bir sonraki non-ws'i komut gibi boya
                continue;
            }
            spans.push(Span::styled(part.clone(), tip));
            i += 1;
            continue;
        }
        if is_flag_like(part) {
            spans.push(Span::styled(part.clone(), flag));
        } else if looks_like_value(part) || is_bracket_placeholder(part) {
            let style = if is_bracket_placeholder(part) && bracket_is_option(part) {
                flag
            } else {
                path
            };
            spans.push(Span::styled(part.clone(), style));
        } else if is_command_word(part) || next_non_ws_is_flag(&parts, i) {
            spans.push(Span::styled(part.clone(), command));
        } else {
            // Noktalama yapışık olabilir: `ls,` / `kullanın;`
            let (core, trail) = split_trailing_punct(part);
            if !core.is_empty()
                && (is_command_word(core)
                    || is_flag_like(core)
                    || looks_like_value(core)
                    || next_non_ws_is_flag(&parts, i))
            {
                if is_flag_like(core) {
                    spans.push(Span::styled(core.to_string(), flag));
                } else if looks_like_value(core) {
                    spans.push(Span::styled(core.to_string(), path));
                } else {
                    spans.push(Span::styled(core.to_string(), command));
                }
                if !trail.is_empty() {
                    spans.push(Span::styled(trail.to_string(), tip));
                }
            } else {
                spans.push(Span::styled(part.clone(), tip));
            }
        }
        i += 1;
    }
    spans
}

fn split_ws_keep(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_ws = false;
    let mut first = true;
    for c in s.chars() {
        let ws = c.is_whitespace();
        if first {
            in_ws = ws;
            first = false;
            cur.push(c);
            continue;
        }
        if ws == in_ws {
            cur.push(c);
        } else {
            out.push(std::mem::take(&mut cur));
            in_ws = ws;
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn next_non_ws_is_flag(parts: &[String], i: usize) -> bool {
    parts[i + 1..]
        .iter()
        .find(|p| !p.chars().all(char::is_whitespace))
        .is_some_and(|p| {
            let (core, _) = split_trailing_punct(p);
            is_flag_like(core)
        })
}

fn split_trailing_punct(tok: &str) -> (&str, &str) {
    let trim_at = tok
        .char_indices()
        .rev()
        .take_while(|(_, c)| matches!(c, ',' | ';' | '.' | ':' | '!' | '?' | ')' | ']' | '»' | '"'))
        .last()
        .map(|(i, _)| i);
    match trim_at {
        Some(i) if i > 0 => (&tok[..i], &tok[i..]),
        _ => (tok, ""),
    }
}

fn looks_like_output(line: &str) -> bool {
    let t = line.trim_start();
    if t.is_empty() || t.starts_with('$') {
        return false;
    }
    t.starts_with("total ")
        || t.starts_with("-rw")
        || t.starts_with("-r-")
        || t.starts_with("drw")
        || t.starts_with("lrw")
        || t.starts_with("crw")
        || t.starts_with("brw")
        || t.starts_with('├')
        || t.starts_with('└')
        || t.starts_with('│')
        || t.starts_with("├──")
        || t.starts_with("└──")
}

fn is_command_word(tok: &str) -> bool {
    const CMDS: &[&str] = &[
        "ls",
        "cd",
        "pwd",
        "cp",
        "mv",
        "rm",
        "mkdir",
        "rmdir",
        "touch",
        "cat",
        "head",
        "tail",
        "grep",
        "find",
        "chmod",
        "chown",
        "chgrp",
        "ln",
        "tree",
        "tar",
        "gzip",
        "curl",
        "wget",
        "ssh",
        "scp",
        "systemctl",
        "journalctl",
        "docker",
        "git",
        "pacman",
        "apt",
        "dnf",
        "brew",
        "echo",
        "printf",
        "which",
        "whereis",
        "whoami",
        "id",
        "ps",
        "kill",
        "top",
        "htop",
        "df",
        "du",
        "mount",
        "umount",
        "lsblk",
        "ip",
        "ping",
        "dig",
        "host",
        "nmap",
        "sudo",
        "su",
        "chmod",
        "tee",
        "xargs",
        "sort",
        "uniq",
        "wc",
        "cut",
        "sed",
        "awk",
        "jq",
        "vim",
        "nano",
        "less",
        "more",
        "man",
        "file",
        "stat",
        "realpath",
        "basename",
        "dirname",
        "env",
        "export",
        "source",
        "bash",
        "zsh",
        "sh",
        "python",
        "python3",
        "node",
        "npm",
        "cargo",
        "rustc",
        "make",
        "cmake",
        "gcc",
        "clang",
        "nginx",
        "systemctl",
        "service",
        "crontab",
        "uname",
        "hostname",
        "free",
        "uptime",
        "date",
        "cal",
        "history",
        "clear",
        "reset",
        "alias",
        "unalias",
        "type",
        "command",
        "builtin",
        "hash",
        "help",
        "true",
        "false",
        "test",
        "restic",
        "aws",
        "terraform",
        "kubectl",
        "virsh",
        "strace",
        "nft",
        "mysql",
        "nix",
    ];
    let lower = tok
        .trim_matches(|c: char| matches!(c, '`' | '"' | '\''))
        .to_ascii_lowercase();
    CMDS.contains(&lower.as_str())
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
            for n in chars.by_ref() {
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
                for m in chars.by_ref() {
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

/// `2>&1` gibi yönlendirmelerde operatörler arasına boşluk koyma.
fn tight_operator_join(prev: &str, cur: &str) -> bool {
    let op = |t: &str| t.chars().all(|c| matches!(c, '|' | '&' | ';' | '>' | '<'));
    let digit = |t: &str| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit());
    if op(prev) && op(cur) {
        return true;
    }
    if digit(prev) && op(cur) {
        return true;
    }
    if op(prev) && digit(cur) {
        return true;
    }
    false
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
    let folded = inner
        .replace(['ı', 'İ'], "i")
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
    // gizli dosya / uzantı
    if tok.starts_with('.') && tok.len() > 1 && !tok[1..].chars().all(|c| c == '.') {
        return true;
    }
    if tok.contains('.') {
        if let Some(ext) = tok.rsplit('.').next() {
            if !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
                return true;
            }
        }
    }
    tok.chars().all(|c| c.is_ascii_digit())
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
    fn prompt_line_adds_dollar_input_only() {
        let pal = UiTheme::Default.palette();
        let spans = highlight_prompt_line("ls -lh /var/log", pal);
        assert_eq!(text(&spans), "$ ls -lh /var/log");
        assert_eq!(spans[0].style.fg, Some(pal.muted));
        assert_eq!(
            spans.iter().find(|s| s.content == "ls").unwrap().style.fg,
            Some(pal.accent)
        );
        assert_eq!(
            spans.iter().find(|s| s.content == "-lh").unwrap().style.fg,
            Some(pal.flag)
        );
        assert_eq!(
            spans
                .iter()
                .find(|s| s.content == "/var/log")
                .unwrap()
                .style
                .fg,
            Some(pal.path)
        );
    }

    #[test]
    fn syntax_line_has_no_dollar() {
        let pal = UiTheme::Default.palette();
        let spans = highlight_shell("ls [seçenekler] [dizin]", pal);
        assert!(!text(&spans).starts_with('$'));
        assert_eq!(
            spans
                .iter()
                .find(|s| s.content.as_ref() == "[seçenekler]")
                .unwrap()
                .style
                .fg,
            Some(pal.flag)
        );
        assert_eq!(
            spans
                .iter()
                .find(|s| s.content.as_ref() == "[dizin]")
                .unwrap()
                .style
                .fg,
            Some(pal.path)
        );
    }

    #[test]
    fn output_line_never_gets_dollar() {
        let pal = UiTheme::Default.palette();
        let out = "-rw-r--r--  1 ahmet dev  4.2K not.txt";
        let spans = highlight_prompt_line(out, pal);
        assert!(!text(&spans).contains('$'));
        assert!(spans.iter().all(|s| s.style.fg == Some(pal.muted)));
        let tree = highlight_shell("├── proje/", pal);
        assert!(!text(&tree).starts_with('$'));
        assert_eq!(tree[0].style.fg, Some(pal.muted));
    }

    #[test]
    fn tip_inline_has_no_dollar_but_colors_cmd() {
        let pal = UiTheme::Default.palette();
        let tip = Style::default().fg(pal.tip);
        let spans = highlight_tip_text(
            "Gizli dosyalar için ls -a kullanın; .bashrc görünür.",
            pal,
            tip,
        );
        assert!(!text(&spans).contains('$'));
        assert_eq!(
            spans.iter().find(|s| s.content == "ls").unwrap().style.fg,
            Some(pal.accent)
        );
        assert_eq!(
            spans.iter().find(|s| s.content == "-a").unwrap().style.fg,
            Some(pal.flag)
        );
        assert_eq!(
            spans
                .iter()
                .find(|s| s.content == ".bashrc")
                .unwrap()
                .style
                .fg,
            Some(pal.path)
        );
    }

    #[test]
    fn highlights_quoted_and_subcommand() {
        let pal = UiTheme::Default.palette();
        let spans = highlight_prompt_line("grep -rn \"TODO\" proje/", pal);
        assert!(text(&spans).starts_with("$ "));
        let q = spans.iter().find(|s| s.content == "\"TODO\"").unwrap();
        assert_eq!(q.style.fg, Some(pal.path));
        let spans2 = highlight_prompt_line("systemctl status ssh", pal);
        assert_eq!(
            spans2
                .iter()
                .find(|s| s.content == "status")
                .unwrap()
                .style
                .fg,
            Some(pal.flag)
        );
        assert_eq!(
            spans2.iter().find(|s| s.content == "ssh").unwrap().style.fg,
            Some(pal.path)
        );
    }
}
