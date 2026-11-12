//! Komut Rehberi: **konu rehberi** ve **komut detay** görünümleri.
//!
//! Tek duvar metin değil — terminale özgü krom:
//! - kenarlıklı kod blokları
//! - ipucu / risk callout’ları
//! - dar genişlikte tablo → kart
//! - sarma + kaydırma (çağıran Panel scroll’u)
//!
//! Boş bölüm uydurulmaz. Panel yan/alt yalnızca genişliğe göre (`commands_detail_side`).

use crate::app::App;
use crate::commands::{
    command_os_fit, package_lines_for_filter, select_command_examples, sort_examples_by_difficulty,
    CommandEntry, CommandExample, CommandsFilters, ConceptEntry, LearningLevel, OsFit, RiskLevel,
    TopicLearningStep, VerificationStatus,
};
use crate::i18n::Msg;
use crate::ui::theme::{heading_caps, ThemePalette};
use crate::ui::wrap_text;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Kart düzenine düşülecek iç genişlik eşiği (tablo sütunları sığmaz).
pub const NARROW_TABLE_WIDTH: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutKind {
    Tip,
    Risk,
    Warn,
    Draft,
}

/// Detay paneli başlığı — konu vs komut ayrımı.
pub fn detail_panel_title(app: &App) -> String {
    let lang = app.ui_lang();
    if app.commands_list_shows_concepts() {
        format!(" {} ", lang.t(Msg::CommandsTopicGuide))
    } else {
        format!(" {} ", lang.t(Msg::CommandsDetail))
    }
}

fn push_wrapped(lines: &mut Vec<Line<'static>>, text: &str, width: usize, style: Style) {
    for part in wrap_text(text, width) {
        lines.push(Line::from(Span::styled(part, style)));
    }
}

fn push_heading(lines: &mut Vec<Line<'static>>, title: &str, pal: ThemePalette) {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{}:", heading_caps(title)),
        pal.heading(),
    )));
    // Kısa nefes — başlık ile içerik arası sıkışmasın.
    lines.push(Line::from(""));
}

/// Paragraf boşluklarını koruyarak sar; `**…**` işaretlerini TUI’de yut (düzen).
fn push_learning_paragraphs(
    lines: &mut Vec<Line<'static>>,
    body: &str,
    wrap_w: usize,
    text: Style,
    accent: Style,
) {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }
    let mut first = true;
    for raw in trimmed.split("\n\n") {
        let para = raw
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if para.is_empty() {
            continue;
        }
        if !first {
            lines.push(Line::from(""));
        }
        first = false;
        // `**Başlık:** kalan…` → başlık satırı + gövde
        if let Some((label, rest)) = split_leading_md_bold_label(&para) {
            lines.push(Line::from(Span::styled(
                label,
                accent.add_modifier(Modifier::BOLD),
            )));
            if !rest.is_empty() {
                push_wrapped(lines, &strip_md_bold(rest), wrap_w, text);
            }
        } else {
            push_wrapped(lines, &strip_md_bold(&para), wrap_w, text);
        }
    }
}

fn strip_md_bold(s: &str) -> String {
    s.replace("**", "")
}

/// `**Etiket:**` veya `**Etiket**` ile başlayan paragrafı ayır.
fn split_leading_md_bold_label(para: &str) -> Option<(String, &str)> {
    let rest = para.strip_prefix("**")?;
    let end = rest.find("**")?;
    let label = rest[..end].trim();
    if label.is_empty() {
        return None;
    }
    let after = rest[end + 2..].trim_start();
    Some((label.to_string(), after))
}

/// Sol cyan şeritli kod satırı (`$ cmd` + argüman rengi).
pub fn push_code_block(
    lines: &mut Vec<Line<'static>>,
    code: &str,
    wrap_w: usize,
    accent: Style,
    muted: Style,
) {
    push_code_block_themed(lines, code, wrap_w, accent, muted, None);
}

/// Palet biliniyorsa argüman rengi + sol accent şerit.
pub fn push_code_block_themed(
    lines: &mut Vec<Line<'static>>,
    code: &str,
    wrap_w: usize,
    accent: Style,
    muted: Style,
    pal: Option<ThemePalette>,
) {
    let border = pal
        .map(|p| Style::default().fg(p.code_border))
        .unwrap_or(accent);
    let arg = pal.map(|p| Style::default().fg(p.arg)).unwrap_or(accent);
    let panel = pal
        .map(|p| Style::default().bg(p.bg_panel))
        .unwrap_or_default();
    let inner = wrap_w.saturating_sub(4).max(8);
    let cmd = if code.trim_start().starts_with('$') {
        code.trim().to_string()
    } else {
        format!("$ {code}")
    };
    // İnce üst/alt çerçeve + sol cyan şerit (mock: kod kartı).
    let top = format!("┌{}", "─".repeat(inner.saturating_sub(1).min(48)));
    lines.push(Line::from(Span::styled(top, muted.patch(panel))));
    for part in wrap_text(&cmd, inner) {
        let mut spans = vec![Span::styled("│ ".to_string(), border.patch(panel))];
        spans.extend(style_shell_line(&part, accent, muted, arg, panel));
        lines.push(Line::from(spans));
    }
    let bot = format!("└{}", "─".repeat(inner.saturating_sub(1).min(48)));
    lines.push(Line::from(Span::styled(bot, muted.patch(panel))));
}

/// `$ cmd -flags args` — prompt muted, komut cyan, bayrak/arg amber.
fn style_shell_line(
    line: &str,
    accent: Style,
    muted: Style,
    arg: Style,
    panel: Style,
) -> Vec<Span<'static>> {
    let accent = accent.add_modifier(Modifier::BOLD).patch(panel);
    let muted = muted.patch(panel);
    let arg = arg.patch(panel);
    let trimmed = line.trim_start();
    let lead = line.len().saturating_sub(trimmed.len());
    let mut spans = Vec::new();
    if lead > 0 {
        spans.push(Span::styled(" ".repeat(lead), muted));
    }
    let body = trimmed;
    let (prompt, rest) = if let Some(r) = body.strip_prefix("$ ") {
        spans.push(Span::styled("$ ".to_string(), muted));
        (true, r)
    } else if body == "$" {
        spans.push(Span::styled("$".to_string(), muted));
        return spans;
    } else {
        (false, body)
    };
    let _ = prompt;
    let mut parts = rest.split_whitespace();
    if let Some(cmd) = parts.next() {
        spans.push(Span::styled(cmd.to_string(), accent));
    }
    for p in parts {
        spans.push(Span::styled(" ".to_string(), muted));
        spans.push(Span::styled(p.to_string(), arg));
    }
    spans
}

/// İpucu / risk / uyarı / draft callout — tip için sol amber şerit (mock İPUCU).
pub fn push_callout(
    lines: &mut Vec<Line<'static>>,
    kind: CalloutKind,
    label: &str,
    body: &str,
    wrap_w: usize,
    style: Style,
    muted: Style,
) {
    if body.trim().is_empty() {
        return;
    }
    // Tip: sol amber şerit + etiket (mock callout).
    if matches!(kind, CalloutKind::Tip) {
        let tag = if label.is_empty() {
            "İPUCU".to_string()
        } else {
            heading_caps(label)
        };
        let head = format!("▍ {tag} ");
        let budget = wrap_w.saturating_sub(head.chars().count()).max(8);
        let mut parts = wrap_text(body, budget).into_iter();
        if let Some(first) = parts.next() {
            lines.push(Line::from(vec![
                Span::styled(head, style.add_modifier(Modifier::BOLD)),
                Span::styled(first, style),
            ]));
        }
        for part in parts {
            lines.push(Line::from(vec![
                Span::styled("│ ".to_string(), style),
                Span::styled(part, style),
            ]));
        }
        return;
    }
    let marker = "!";
    let head = format!("╭ {marker} {label}");
    lines.push(Line::from(Span::styled(
        head,
        style.add_modifier(Modifier::BOLD),
    )));
    let inner = wrap_w.saturating_sub(2).max(8);
    for part in wrap_text(body, inner) {
        lines.push(Line::from(vec![
            Span::styled("│ ".to_string(), muted),
            Span::styled(part, style),
        ]));
    }
    lines.push(Line::from(Span::styled("╰".to_string(), muted)));
}

/// Okunur yığın: başlık + satır-içi rozet + saran açıklama.
/// Dar (< [`NARROW_TABLE_WIDTH`]): kenarlıklı kart.
/// Geniş: esnek yığın — sabit dar id/risk sütunu yok, ellipsis yok (yer yettiğince).
/// `headers` geriye dönük API için tutulur; geniş düzende kullanılmaz.
pub fn push_table_or_cards(
    lines: &mut Vec<Line<'static>>,
    _headers: &[&str],
    rows: &[(String, String, String)],
    wrap_w: usize,
    text: Style,
    muted: Style,
    accent: Style,
) {
    if rows.is_empty() {
        return;
    }
    let narrow = wrap_w < NARROW_TABLE_WIDTH;
    for (i, (title, detail, badge)) in rows.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        if narrow {
            let head = if badge.is_empty() {
                title.clone()
            } else {
                format!("{title}  · {badge}")
            };
            lines.push(Line::from(Span::styled(
                format!("┌ {head}"),
                accent.add_modifier(Modifier::BOLD),
            )));
            if !detail.is_empty() {
                let inner = wrap_w.saturating_sub(2).max(8);
                for part in wrap_text(detail, inner) {
                    lines.push(Line::from(vec![
                        Span::styled("│ ".to_string(), muted),
                        Span::styled(part, text),
                    ]));
                }
            }
            lines.push(Line::from(Span::styled("└".to_string(), muted)));
            continue;
        }
        // Geniş: cyan komut + outlined rozet; açıklama alt satırda.
        let pill_color = if badge.is_empty() {
            None
        } else {
            Some(accent.fg.unwrap_or(ratatui::style::Color::Cyan))
        };
        push_title_with_badge_color(lines, title, badge, wrap_w, accent, muted, pill_color);
        if !detail.is_empty() {
            let inner = wrap_w.saturating_sub(2).max(8);
            for part in wrap_text(detail, inner) {
                lines.push(Line::from(Span::styled(format!("  {part}"), text)));
            }
        }
    }
}

fn push_title_with_badge_color(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    badge: &str,
    wrap_w: usize,
    accent: Style,
    muted: Style,
    badge_color: Option<ratatui::style::Color>,
) {
    let title = title.trim();
    let badge = badge.trim();
    if badge.is_empty() {
        push_wrapped(lines, title, wrap_w, accent.add_modifier(Modifier::BOLD));
        return;
    }
    let pill = if badge_color.is_some() {
        format!("  [{badge}]")
    } else {
        format!("  · {badge}")
    };
    let pill_style = if let Some(c) = badge_color {
        Style::default().fg(c).add_modifier(Modifier::BOLD)
    } else {
        muted
    };
    let budget = wrap_w.saturating_sub(pill.chars().count()).max(8);
    let mut title_parts = wrap_text(title, budget).into_iter();
    if let Some(first) = title_parts.next() {
        lines.push(Line::from(vec![
            Span::styled(first, accent.add_modifier(Modifier::BOLD)),
            Span::styled(pill, pill_style),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            if badge_color.is_some() {
                format!("[{badge}]")
            } else {
                format!("· {badge}")
            },
            pill_style,
        )));
    }
    for part in title_parts {
        lines.push(Line::from(Span::styled(
            part,
            accent.add_modifier(Modifier::BOLD),
        )));
    }
}

fn risk_color(pal: ThemePalette, risk: RiskLevel) -> ratatui::style::Color {
    pal.risk_color(risk)
}

fn bank_cmd<'a>(app: &'a App, id: &str) -> Option<&'a CommandEntry> {
    app.commands_bank.commands.iter().find(|c| c.id == id)
}

fn bank_concept<'a>(app: &'a App, id: &str) -> Option<&'a ConceptEntry> {
    app.commands_bank.concepts.iter().find(|c| c.id == id)
}

/// `learning_path` ref → komut adı veya konu başlığı (id ham gösterilmez).
fn resolve_learning_ref(app: &App, step: &TopicLearningStep, lang_id: &str) -> String {
    let kind = step.kind.as_deref().unwrap_or("").to_ascii_lowercase();
    if kind == "concept" || kind == "topic" {
        if let Some(c) = bank_concept(app, &step.r#ref) {
            let t = c.title.pick(lang_id);
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    if let Some(cmd) = bank_cmd(app, &step.r#ref) {
        return cmd.name.clone();
    }
    if let Some(c) = bank_concept(app, &step.r#ref) {
        let t = c.title.pick(lang_id);
        if !t.is_empty() {
            return t.to_string();
        }
    }
    step.r#ref.clone()
}

fn beginner_example<'a>(
    examples: &'a [CommandExample],
    filters: &CommandsFilters,
) -> Option<&'a CommandExample> {
    let filtered = select_command_examples(examples, filters);
    if filtered.is_empty() {
        return None;
    }
    let mut owned: Vec<&CommandExample> = filtered;
    owned.sort_by_key(|ex| {
        ex.difficulty
            .unwrap_or(LearningLevel::Unspecified)
            .sort_key()
            .min(ex.risk as u8)
    });
    owned.into_iter().next()
}

/// Konu rehberi görünümü (Başlangıç kavramları).
pub fn topic_guide_lines(app: &App, concept: &ConceptEntry, wrap_w: usize) -> Vec<Line<'static>> {
    let lang = app.ui_lang();
    let lang_id = lang.id();
    let pal = app.ui_theme().palette();
    let wrap_w = wrap_w.max(12);
    let mut lines = Vec::new();
    let text = Style::default().fg(pal.text);
    let muted = Style::default().fg(pal.muted);
    let accent = Style::default().fg(pal.accent);
    let warn = Style::default().fg(pal.warn);

    // Başlık
    lines.push(Line::from(Span::styled(
        concept.title.pick(lang_id).to_string(),
        Style::default().add_modifier(Modifier::BOLD).fg(pal.accent),
    )));
    let mut badge_row = Vec::new();
    if let Some(ref diff) = concept.difficulty {
        badge_row.push(pal.pill(diff, pal.accent));
        badge_row.push(Span::raw(" "));
    }
    let ver = concept.verification.status.as_str();
    badge_row.push(pal.pill(ver, pal.muted));
    lines.push(Line::from(badge_row));
    if concept.verification.status == VerificationStatus::Draft {
        push_callout(
            &mut lines,
            CalloutKind::Draft,
            "draft",
            lang.t(Msg::CommandsDraftWarn),
            wrap_w,
            warn,
            muted,
        );
    }

    // Intro — additive intro yoksa summary (kısa); body ayrı öğrenme bölümü
    let intro = concept.intro.pick(lang_id);
    let summary = concept.summary.pick(lang_id);
    let intro_body = if !intro.is_empty() { intro } else { summary };
    if !intro_body.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicIntro), pal);
        push_learning_paragraphs(&mut lines, intro_body, wrap_w, text, accent);
    }

    // Order — `learning_path` (ref → komut/konu başlığı)
    if !concept.learning_path.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicOrder), pal);
        for (i, step) in concept.learning_path.iter().enumerate() {
            let label = resolve_learning_ref(app, step, lang_id);
            let note = step.note.pick(lang_id);
            if note.is_empty() {
                push_wrapped(&mut lines, &format!("{}. {label}", i + 1), wrap_w, text);
            } else {
                lines.push(Line::from(Span::styled(
                    format!("{}. {label}", i + 1),
                    accent.add_modifier(Modifier::BOLD),
                )));
                push_wrapped(&mut lines, &format!("   {note}"), wrap_w, text);
            }
        }
    }

    // Subtopics — tam başlık + açıklama; id sütunu yok (ellipsis tuzağı)
    if !concept.subtopics.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicSubtopics), pal);
        let rows: Vec<(String, String, String)> = concept
            .subtopics
            .iter()
            .map(|s| {
                (
                    s.title.pick(lang_id).to_string(),
                    s.summary.pick(lang_id).to_string(),
                    String::new(),
                )
            })
            .collect();
        push_table_or_cards(
            &mut lines,
            &[
                lang.t(Msg::CommandsTopicSubtopics),
                lang.t(Msg::CommandsTopicIntro),
                "",
            ],
            &rows,
            wrap_w,
            text,
            muted,
            accent,
        );
    } else if !concept.related_concepts.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicSubtopics), pal);
        let rows: Vec<(String, String, String)> = concept
            .related_concepts
            .iter()
            .map(|id| {
                let title = app
                    .commands_bank
                    .concepts
                    .iter()
                    .find(|c| c.id == *id)
                    .map(|c| c.title.pick(lang_id).to_string())
                    .unwrap_or_else(|| id.clone());
                let summ = app
                    .commands_bank
                    .concepts
                    .iter()
                    .find(|c| c.id == *id)
                    .map(|c| c.summary.pick(lang_id).to_string())
                    .unwrap_or_default();
                (title, summ, String::new())
            })
            .collect();
        push_table_or_cards(
            &mut lines,
            &[
                lang.t(Msg::CommandsTopicSubtopics),
                lang.t(Msg::CommandsTopicIntro),
                "",
            ],
            &rows,
            wrap_w,
            text,
            muted,
            accent,
        );
    }

    // Command table — risk/seviye satır-içi rozet; sabit id/risk sütunu yok
    if !concept.command_table.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicCmdTable), pal);
        let rows: Vec<(String, String, String)> = concept
            .command_table
            .iter()
            .map(|row| {
                let cmd = bank_cmd(app, &row.id);
                let name = cmd
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| row.id.clone());
                let purpose = {
                    let p = row.purpose.pick(lang_id);
                    if !p.is_empty() {
                        p.to_string()
                    } else {
                        cmd.map(|c| c.summary.pick(lang_id).to_string())
                            .unwrap_or_default()
                    }
                };
                let badge = if row.learning_level.is_specified() {
                    row.learning_level.label(lang_id).to_string()
                } else {
                    cmd.map(|c| c.risk.label(lang_id).to_string())
                        .unwrap_or_default()
                };
                (name, purpose, badge)
            })
            .collect();
        push_table_or_cards(
            &mut lines,
            &[lang.t(Msg::CommandsList), lang.t(Msg::CommandsPurpose), ""],
            &rows,
            wrap_w,
            text,
            muted,
            accent,
        );
    } else if !concept.related_commands.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicCmdTable), pal);
        let rows: Vec<(String, String, String)> = concept
            .related_commands
            .iter()
            .filter_map(|id| {
                let cmd = bank_cmd(app, id)?;
                Some((
                    cmd.name.clone(),
                    cmd.summary.pick(lang_id).to_string(),
                    cmd.risk.label(lang_id).to_string(),
                ))
            })
            .collect();
        if rows.is_empty() {
            let fallback: Vec<(String, String, String)> = concept
                .related_commands
                .iter()
                .map(|id| (id.clone(), String::new(), String::new()))
                .collect();
            push_table_or_cards(
                &mut lines,
                &["id", "", ""],
                &fallback,
                wrap_w,
                text,
                muted,
                accent,
            );
        } else {
            push_table_or_cards(
                &mut lines,
                &[
                    lang.t(Msg::CommandsList),
                    lang.t(Msg::CommandsTopicIntro),
                    lang.t(Msg::CommandsRisk),
                ],
                &rows,
                wrap_w,
                text,
                muted,
                accent,
            );
        }
    }

    // Learning body — paragraflı, sıkışık duvar değil
    let body = concept.body.pick(lang_id);
    if !body.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicLearning), pal);
        push_learning_paragraphs(&mut lines, body, wrap_w, text, accent);
    }

    // Examples — graded_examples, else related beginner
    let mut shown_examples = 0usize;
    if !concept.graded_examples.is_empty() {
        let filtered = select_command_examples(&concept.graded_examples, &app.commands_filters);
        if filtered.is_empty() {
            push_heading(&mut lines, lang.t(Msg::CommandsExamples), pal);
            lines.push(Line::from(Span::styled(
                format!("  {}", lang.t(Msg::CommandsFilterNoMatch)),
                warn,
            )));
            shown_examples = 1; // section shown; skip fallback
        } else {
            push_heading(&mut lines, lang.t(Msg::CommandsExamples), pal);
            let mut exs: Vec<_> = filtered.into_iter().cloned().collect();
            sort_examples_by_difficulty(&mut exs);
            for (i, ex) in exs.iter().take(6).enumerate() {
                if i > 0 {
                    lines.push(Line::from(""));
                }
                let title = ex.title.pick(lang_id);
                if !title.is_empty() {
                    lines.push(Line::from(format!(
                        "· {title}  · {}",
                        ex.risk.label(lang_id)
                    )));
                }
                push_code_block_themed(&mut lines, &ex.command, wrap_w, accent, muted, Some(pal));
                let expl = ex.explanation.pick(lang_id);
                if !expl.is_empty() {
                    push_wrapped(&mut lines, expl, wrap_w, muted);
                }
                shown_examples += 1;
            }
        }
    }
    if shown_examples == 0 {
        let mut collected: Vec<(&str, &CommandExample)> = Vec::new();
        for id in &concept.related_commands {
            if let Some(cmd) = bank_cmd(app, id) {
                if let Some(ex) = beginner_example(&cmd.examples, &app.commands_filters) {
                    collected.push((cmd.name.as_str(), ex));
                }
            }
            if collected.len() >= 4 {
                break;
            }
        }
        if !collected.is_empty() {
            push_heading(&mut lines, lang.t(Msg::CommandsExamples), pal);
            for (i, (name, ex)) in collected.into_iter().enumerate() {
                if i > 0 {
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(format!("· {name}"), muted)));
                push_code_block_themed(&mut lines, &ex.command, wrap_w, accent, muted, Some(pal));
                let expl = ex.explanation.pick(lang_id);
                if !expl.is_empty() {
                    push_wrapped(&mut lines, expl, wrap_w, muted);
                }
            }
        }
    }

    // Callouts → tip / warning / risk
    let tips: Vec<_> = concept
        .callouts
        .iter()
        .filter(|c| c.kind.eq_ignore_ascii_case("tip"))
        .collect();
    let warns: Vec<_> = concept
        .callouts
        .iter()
        .filter(|c| {
            let k = c.kind.to_ascii_lowercase();
            k == "warning" || k == "warn" || k == "risk"
        })
        .collect();
    if !tips.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicTips), pal);
        for (i, tip) in tips.into_iter().enumerate() {
            let t = tip.text.pick(lang_id);
            if t.is_empty() {
                continue;
            }
            if i > 0 {
                lines.push(Line::from(""));
            }
            // Bölüm başlığı zaten var — tekrar "İpuçları:" etiketi yok.
            push_callout(
                &mut lines,
                CalloutKind::Tip,
                "",
                t,
                wrap_w,
                Style::default().fg(pal.tip),
                muted,
            );
        }
    }
    if !warns.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicWarnings), pal);
        for (i, w) in warns.into_iter().enumerate() {
            let wt = w.text.pick(lang_id);
            if wt.is_empty() {
                continue;
            }
            if i > 0 {
                lines.push(Line::from(""));
            }
            let kind = if w.kind.eq_ignore_ascii_case("risk") {
                CalloutKind::Risk
            } else {
                CalloutKind::Warn
            };
            push_callout(
                &mut lines,
                kind,
                lang.t(Msg::CommandsTopicWarnings),
                wt,
                wrap_w,
                warn,
                muted,
            );
        }
    }

    // Exercises
    if !concept.safe_exercises.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicExercises), pal);
        for (i, ex) in concept.safe_exercises.iter().enumerate() {
            let title = ex.title.pick(lang_id);
            let prompt = ex.prompt.pick(lang_id);
            let hint = ex.hint.pick(lang_id);
            if title.is_empty() && prompt.is_empty() {
                continue;
            }
            let head = if title.is_empty() {
                format!("  {}. {prompt}", i + 1)
            } else {
                format!("  {}. {title}", i + 1)
            };
            push_wrapped(&mut lines, &head, wrap_w, text);
            if !title.is_empty() && !prompt.is_empty() {
                push_wrapped(&mut lines, &format!("     {prompt}"), wrap_w, muted);
            }
            if !hint.is_empty() {
                push_callout(
                    &mut lines,
                    CalloutKind::Tip,
                    "hint",
                    hint,
                    wrap_w,
                    muted,
                    muted,
                );
            }
        }
    }

    // Related tasks
    let related_tasks = app.commands_bank.related_tasks_for_concept(concept);
    if !related_tasks.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicRelatedTasks), pal);
        let names: Vec<String> = related_tasks
            .iter()
            .take(8)
            .map(|t| t.name.pick(lang_id).to_string())
            .collect();
        push_wrapped(&mut lines, &format!("  {}", names.join(", ")), wrap_w, text);
    }

    // Next steps — başlık + not; id sütunu yok
    if !concept.next_steps.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicNext), pal);
        let rows: Vec<(String, String, String)> = concept
            .next_steps
            .iter()
            .map(|n| {
                let title = bank_concept(app, &n.id)
                    .map(|c| c.title.pick(lang_id).to_string())
                    .unwrap_or_else(|| n.id.clone());
                (title, n.note.pick(lang_id).to_string(), String::new())
            })
            .collect();
        push_table_or_cards(
            &mut lines,
            &[
                lang.t(Msg::CommandsTopicNext),
                lang.t(Msg::CommandsTopicIntro),
                "",
            ],
            &rows,
            wrap_w,
            text,
            muted,
            accent,
        );
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        lang.t(Msg::CommandsNeverAutorun).to_string(),
        muted,
    )));
    lines
}

/// Komut detay görünümü (banka kaydı).
pub fn command_detail_lines(
    app: &App,
    cmd: &CommandEntry,
    wrap_w: usize,
    fav_prefix: &str,
) -> Vec<Line<'static>> {
    let lang = app.ui_lang();
    let lang_id = lang.id();
    let pal = app.ui_theme().palette();
    let wrap_w = wrap_w.max(12);
    let mut lines = Vec::new();
    let text = Style::default().fg(pal.text);
    let muted = Style::default().fg(pal.muted);
    let accent = Style::default().fg(pal.accent);
    let warn = Style::default().fg(pal.warn);

    // Name + outlined seviye/risk/doğrulama pill'leri (mock detay başlığı).
    let mut title_spans = vec![Span::styled(
        format!("{fav_prefix}{}", cmd.name),
        Style::default().add_modifier(Modifier::BOLD).fg(pal.accent),
    )];
    if cmd.learning_level.is_specified() {
        title_spans.push(Span::raw("  "));
        title_spans.push(pal.pill(
            cmd.learning_level.label(lang_id),
            pal.level_color(cmd.learning_level),
        ));
    }
    title_spans.push(Span::raw(" "));
    title_spans.push(pal.pill(cmd.risk.label(lang_id), pal.risk_color(cmd.risk)));
    let ver_label = match cmd.verification.status {
        VerificationStatus::Verified => {
            if lang_id.to_ascii_lowercase().starts_with("en") {
                "verified"
            } else {
                "doğrulandı"
            }
        }
        VerificationStatus::Reviewed => {
            if lang_id.to_ascii_lowercase().starts_with("en") {
                "reviewed"
            } else {
                "incelendi"
            }
        }
        VerificationStatus::Draft => "draft",
    };
    title_spans.push(Span::raw(" "));
    title_spans.push(pal.pill(ver_label, pal.muted));
    lines.push(Line::from(title_spans));
    let summary = cmd.summary.pick(lang_id);
    if !summary.is_empty() {
        push_wrapped(&mut lines, summary, wrap_w, text);
    }
    if cmd.verification.status == VerificationStatus::Draft {
        push_callout(
            &mut lines,
            CalloutKind::Draft,
            "draft",
            lang.t(Msg::CommandsDraftWarn),
            wrap_w,
            warn,
            muted,
        );
    }

    // When
    let purpose = cmd.purpose.pick(lang_id);
    let mut when_bits: Vec<String> = cmd.tags.clone();
    when_bits.extend(cmd.intent.all_for_lang(lang_id).iter().cloned());
    when_bits.sort();
    when_bits.dedup();
    let desc = cmd.description.pick(lang_id);
    if !purpose.is_empty() || !when_bits.is_empty() || (!desc.is_empty() && desc != summary) {
        push_heading(&mut lines, lang.t(Msg::CommandsWhenUseful), pal);
        if !purpose.is_empty() {
            push_wrapped(&mut lines, purpose, wrap_w, text);
        }
        if !when_bits.is_empty() {
            push_wrapped(
                &mut lines,
                &format!("  {}", when_bits.join(", ")),
                wrap_w,
                muted,
            );
        }
        if !desc.is_empty() && desc != summary && desc != purpose {
            push_wrapped(&mut lines, desc, wrap_w, text);
        }
    }

    // Syntax
    push_heading(&mut lines, lang.t(Msg::CommandsSyntax), pal);
    push_code_block_themed(&mut lines, &cmd.name, wrap_w, accent, muted, Some(pal));
    if !cmd.aliases.is_empty() {
        lines.push(Line::from(format!(
            "  {}: {}",
            lang.t(Msg::CommandsAliases),
            cmd.aliases.join(", ")
        )));
    }

    // Beginner example
    if let Some(ex) = beginner_example(&cmd.examples, &app.commands_filters) {
        push_heading(&mut lines, lang.t(Msg::CommandsBeginnerExample), pal);
        let title = ex.title.pick(lang_id);
        let mut head = Vec::new();
        if !title.is_empty() {
            head.push(Span::styled(format!("· {title}"), muted));
        }
        let diff = ex.difficulty.unwrap_or(LearningLevel::Unspecified);
        if diff.is_specified() {
            head.push(Span::raw("  "));
            head.push(pal.pill(diff.label(lang_id), pal.level_color(diff)));
        }
        if !head.is_empty() {
            lines.push(Line::from(head));
        }
        push_code_block_themed(&mut lines, &ex.command, wrap_w, accent, muted, Some(pal));
        let expl = ex.explanation.pick(lang_id);
        if !expl.is_empty() {
            push_heading(&mut lines, lang.t(Msg::CommandsExpectedEffect), pal);
            push_wrapped(&mut lines, &format!("  {expl}"), wrap_w, muted);
        }
    } else if !cmd.examples.is_empty() {
        // Filtre eşleşmesi yok — dürüst boş
        push_heading(&mut lines, lang.t(Msg::CommandsBeginnerExample), pal);
        lines.push(Line::from(Span::styled(
            format!("  {}", lang.t(Msg::CommandsFilterNoMatch)),
            warn,
        )));
    }

    // Flags
    if !cmd.flags.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsFlags), pal);
        let rows: Vec<(String, String, String)> = cmd
            .flags
            .iter()
            .take(12)
            .map(|f| {
                let ver = if f.verified { "✓" } else { "" };
                (
                    format!("{}{ver}", f.flag),
                    f.meaning.pick(lang_id).to_string(),
                    String::new(),
                )
            })
            .collect();
        push_table_or_cards(
            &mut lines,
            &[
                lang.t(Msg::CommandsFlags),
                lang.t(Msg::CommandsTopicIntro),
                "",
            ],
            &rows,
            wrap_w,
            text,
            muted,
            accent,
        );
        if cmd.flags.len() > 12 {
            lines.push(Line::from(Span::styled(
                format!("  … +{}", cmd.flags.len() - 12),
                muted,
            )));
        }
    }

    // More examples (easy→adv), beginner zaten üstte
    if cmd.examples.len() > 1 {
        push_heading(&mut lines, lang.t(Msg::CommandsExamplesEasyAdv), pal);
        let filtered = select_command_examples(&cmd.examples, &app.commands_filters);
        if filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("  {}", lang.t(Msg::CommandsFilterNoMatch)),
                warn,
            )));
        } else {
            let mut examples: Vec<_> = filtered.into_iter().cloned().collect();
            sort_examples_by_difficulty(&mut examples);
            // Skip the beginner example already shown above (once).
            let skip_cmd =
                beginner_example(&cmd.examples, &app.commands_filters).map(|b| b.command.clone());
            let mut skipped = false;
            let mut n = 0usize;
            for ex in examples.iter() {
                if !skipped && skip_cmd.as_deref() == Some(ex.command.as_str()) {
                    skipped = true;
                    continue;
                }
                if n >= 6 {
                    break;
                }
                let title = ex.title.pick(lang_id);
                let elev = if ex.requires_elevated || ex.privilege.as_deref() == Some("elevated") {
                    " · sudo"
                } else {
                    ""
                };
                let diff = ex.difficulty.unwrap_or(LearningLevel::Unspecified);
                let mut head = Vec::new();
                if !title.is_empty() {
                    head.push(Span::styled(format!("· {title}"), muted));
                }
                if diff.is_specified() {
                    head.push(Span::raw("  "));
                    head.push(pal.pill(diff.label(lang_id), pal.level_color(diff)));
                }
                head.push(Span::raw(" "));
                head.push(pal.pill(
                    &format!("{}{elev}", ex.risk.label(lang_id)),
                    pal.risk_color(ex.risk),
                ));
                if !head.is_empty() {
                    lines.push(Line::from(head));
                }
                push_code_block_themed(&mut lines, &ex.command, wrap_w, accent, muted, Some(pal));
                let expl = ex.explanation.pick(lang_id);
                if !expl.is_empty() {
                    push_wrapped(&mut lines, &format!("  {expl}"), wrap_w, muted);
                }
                for w in &ex.warnings {
                    let wt = w.pick(lang_id);
                    if !wt.is_empty() {
                        push_callout(&mut lines, CalloutKind::Warn, "!", wt, wrap_w, warn, muted);
                    }
                }
                n += 1;
            }
        }
    }

    // OS / shell
    push_heading(&mut lines, lang.t(Msg::CommandsPlatforms), pal);
    let os_label = if !cmd.operating_systems.is_empty() {
        cmd.operating_systems.join("/")
    } else if !cmd.platforms.is_empty() {
        cmd.platforms.join("/")
    } else {
        "general".into()
    };
    lines.push(Line::from(format!(
        "  OS: {os_label} · {} · {}",
        cmd.learning_level.label(lang_id),
        lang.bank_category(&cmd.category)
    )));
    if !cmd.shells.is_empty() {
        lines.push(Line::from(format!(
            "  {}: {}",
            lang.t(Msg::CommandsShells),
            cmd.shells.join(", ")
        )));
    }
    let distro_bits: Vec<String> = cmd
        .package_refs
        .iter()
        .flat_map(|r| r.distributions.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if !distro_bits.is_empty() {
        lines.push(Line::from(format!("  distro: {}", distro_bits.join(", "))));
    }
    let fit = command_os_fit(cmd);
    let fit_msg = match fit {
        OsFit::Native => lang.t(Msg::CommandsOsFitNative),
        OsFit::General => lang.t(Msg::CommandsOsFitGeneral),
        OsFit::OtherPlatform => lang.t(Msg::CommandsOsFitOther),
        OsFit::NoEquivalent => lang.t(Msg::CommandsOsFitNone),
    };
    lines.push(Line::from(Span::styled(
        fit_msg.to_string(),
        Style::default().fg(match fit {
            OsFit::Native => pal.success,
            OsFit::General => pal.muted,
            OsFit::OtherPlatform | OsFit::NoEquivalent => pal.warn,
        }),
    )));

    // Risk callout
    push_heading(&mut lines, lang.t(Msg::CommandsSideEffects), pal);
    let risk_why = match cmd.risk {
        RiskLevel::Safe => lang.t(Msg::CommandsRiskWhySafe),
        RiskLevel::Caution => lang.t(Msg::CommandsRiskWhyCaution),
        RiskLevel::Dangerous => lang.t(Msg::CommandsRiskWhyDangerous),
        RiskLevel::Destructive => lang.t(Msg::CommandsRiskWhyDestructive),
    };
    push_callout(
        &mut lines,
        CalloutKind::Risk,
        cmd.risk.label(lang_id),
        risk_why,
        wrap_w,
        Style::default().fg(risk_color(pal, cmd.risk)),
        muted,
    );
    if cmd.requires_elevated {
        lines.push(Line::from(Span::styled(
            format!("  sudo: {}", lang.t(Msg::CommandsElevated)),
            warn,
        )));
    } else {
        lines.push(Line::from(format!(
            "  {}",
            lang.t(Msg::CommandsSudoNotNeeded)
        )));
    }
    let sec = cmd.security.notes.pick(lang_id);
    if !sec.is_empty() {
        push_callout(
            &mut lines,
            CalloutKind::Warn,
            "security",
            sec,
            wrap_w,
            warn,
            muted,
        );
    }
    if cmd.security.requires_admin || cmd.security.uac_prompt {
        lines.push(Line::from(Span::styled("  admin/UAC", warn)));
    }

    // Mistakes
    let mut mistakes: Vec<String> = cmd
        .common_mistakes
        .iter()
        .map(|m| m.pick(lang_id).to_string())
        .filter(|s| !s.is_empty())
        .collect();
    mistakes.extend(cmd.examples.iter().flat_map(|ex| {
        ex.warnings
            .iter()
            .map(|w| w.pick(lang_id).to_string())
            .filter(|s| !s.is_empty())
    }));
    mistakes.sort();
    mistakes.dedup();
    if !mistakes.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsMistakes), pal);
        for m in mistakes.iter().take(6) {
            push_callout(&mut lines, CalloutKind::Warn, "!", m, wrap_w, warn, muted);
        }
    }

    // Packages — honest distro
    let is_arch = crate::commands::host_is_arch();
    if cmd.has_package_hints() {
        push_heading(&mut lines, lang.t(Msg::CommandsPackages), pal);
        let pkgs = package_lines_for_filter(cmd, &app.commands_filters, is_arch);
        if pkgs.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("  {}", lang.t(Msg::CommandsNoPackageForDistro)),
                warn,
            )));
        } else {
            let rows: Vec<(String, String, String)> = pkgs
                .into_iter()
                .map(|(label, pkg)| (label, pkg, String::new()))
                .collect();
            push_table_or_cards(
                &mut lines,
                &["pm", "pkg", ""],
                &rows,
                wrap_w,
                text,
                muted,
                accent,
            );
        }
    }
    let notes = cmd.install_notes.pick(lang_id);
    if !notes.is_empty() {
        lines.push(Line::from(format!(
            "{}: {notes}",
            lang.t(Msg::CommandsInstallNotes)
        )));
    }

    // Related topic / cmds
    let mut related = cmd.related.clone();
    related.extend(cmd.see_also.iter().cloned());
    related.sort();
    related.dedup();
    if !related.is_empty() || !cmd.nova.concepts.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsRelated), pal);
        if !related.is_empty() {
            push_wrapped(
                &mut lines,
                &format!(
                    "  {}: {}",
                    lang.t(Msg::CommandsTopicRelatedCmds),
                    related.join(", ")
                ),
                wrap_w,
                accent,
            );
        }
        if !cmd.nova.concepts.is_empty() {
            push_wrapped(
                &mut lines,
                &format!(
                    "  {}: {}",
                    lang.t(Msg::CommandsConcepts),
                    cmd.nova.concepts.join(", ")
                ),
                wrap_w,
                muted,
            );
        }
    }

    // Sources
    push_heading(&mut lines, lang.t(Msg::CommandsSources), pal);
    if cmd.verification.sources.is_empty() {
        lines.push(Line::from(Span::styled("  —", muted)));
    } else {
        for src in cmd.verification.sources.iter().take(4) {
            lines.push(Line::from(Span::styled(
                format!("  {src}"),
                Style::default().fg(pal.search),
            )));
        }
    }
    lines.push(Line::from(format!(
        "  {}: {}",
        lang.t(Msg::CommandsVerification),
        cmd.verification.status.as_str()
    )));
    if let Some(ref reviewed) = cmd.verification.last_reviewed {
        lines.push(Line::from(format!(
            "  {}: {reviewed}",
            lang.t(Msg::CommandsReviewDate)
        )));
    }
    if let Some(ref note) = cmd.verification.notes {
        if !note.is_empty() {
            push_wrapped(&mut lines, &format!("  {note}"), wrap_w, muted);
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        lang.t(Msg::CommandsNeverAutorun).to_string(),
        muted,
    )));
    lines.push(Line::from(Span::styled(
        lang.t(Msg::CommandsBankFooter).to_string(),
        muted,
    )));
    lines.push(Line::from(Span::styled(
        lang.t(Msg::CommandsPathPresenceNote).to_string(),
        muted,
    )));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Style;

    #[test]
    fn code_block_has_borders() {
        let mut lines = Vec::new();
        push_code_block(&mut lines, "ls -la", 40, Style::default(), Style::default());
        let text: String = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains('┌'), "{text}");
        assert!(text.contains('│'), "{text}");
        assert!(text.contains('└'), "{text}");
        assert!(text.contains("$ ls -la"), "{text}");
    }

    fn lines_text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn table_stacks_when_wide_and_cards_when_narrow() {
        let rows = vec![
            (
                "PATH ve konum".into(),
                "Ortam değişkeni ve arama yolu.".into(),
                String::new(),
            ),
            (
                "shell-builtin".into(),
                "Kabuk yerleşikleri.".into(),
                "güvenli".into(),
            ),
        ];
        let mut wide = Vec::new();
        push_table_or_cards(
            &mut wide,
            &["cmd", "sum", "risk"],
            &rows,
            80,
            Style::default(),
            Style::default(),
            Style::default(),
        );
        let wide_t = lines_text(&wide);
        assert!(wide_t.contains("PATH ve konum"), "{wide_t}");
        assert!(
            wide_t.contains("Ortam değişkeni ve arama yolu."),
            "{wide_t}"
        );
        assert!(wide_t.contains("güvenli"), "rozet kırpılmamalı: {wide_t}");
        assert!(
            !wide_t.contains('…') && !wide_t.contains("güven..") && !wide_t.contains("shell.."),
            "geniş panelde ellipsis olmamalı: {wide_t}"
        );
        assert!(!wide_t.contains('┌'), "wide should not card: {wide_t}");

        let mut narrow = Vec::new();
        push_table_or_cards(
            &mut narrow,
            &["cmd", "sum", "risk"],
            &rows,
            30,
            Style::default(),
            Style::default(),
            Style::default(),
        );
        let narrow_t = lines_text(&narrow);
        assert!(narrow_t.contains('┌'), "{narrow_t}");
        assert!(narrow_t.contains("PATH ve konum"), "{narrow_t}");
        assert!(narrow_t.contains("güvenli"), "{narrow_t}");
    }

    #[test]
    fn wide_table_does_not_pad_tiny_id_risk_columns() {
        let rows = vec![(
            "pwd".into(),
            "Çalışma dizinini yazdırır.".into(),
            "güvenli".into(),
        )];
        let mut lines = Vec::new();
        push_table_or_cards(
            &mut lines,
            &["Bank Komutları", "Giriş", "risk"],
            &rows,
            100,
            Style::default(),
            Style::default(),
            Style::default(),
        );
        let text = lines_text(&lines);
        // Eski hata: sağda `güven…` + ortada boşluk dolgusu.
        assert!(text.contains("güvenli"), "{text}");
        assert!(
            !text.contains("güven…") && !text.contains("güven.."),
            "{text}"
        );
        let longest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
        assert!(
            longest < 60,
            "sabit sütun dolgusu olmamalı (satır {longest}): {text}"
        );
    }

    #[test]
    fn empty_rows_push_nothing() {
        let mut lines = Vec::new();
        push_table_or_cards(
            &mut lines,
            &["a", "b", "c"],
            &[],
            80,
            Style::default(),
            Style::default(),
            Style::default(),
        );
        assert!(lines.is_empty());
    }

    #[test]
    fn learning_paragraphs_keep_gaps_and_strip_md() {
        let mut lines = Vec::new();
        push_learning_paragraphs(
            &mut lines,
            "**İstem (prompt):** Kabuk hazır.\n\n**Çıkış kodu:** 0 başarı.",
            40,
            Style::default(),
            Style::default(),
        );
        let text = lines_text(&lines);
        assert!(text.contains("İstem (prompt)"), "{text}");
        assert!(text.contains("Çıkış kodu"), "{text}");
        assert!(!text.contains("**"), "md işaretleri yutulmalı: {text}");
        assert!(
            text.contains("\n\n"),
            "paragraflar arası boşluk beklenir: {text:?}"
        );
    }
}
