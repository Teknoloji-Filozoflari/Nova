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
    VerificationStatus,
};
use crate::i18n::Msg;
use crate::ui::theme::ThemePalette;
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

fn push_heading(lines: &mut Vec<Line<'static>>, title: &str) {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{title}:"),
        Style::default().add_modifier(Modifier::BOLD),
    )));
}

/// Kenarlıklı kod satırı (`$ cmd`).
pub fn push_code_block(
    lines: &mut Vec<Line<'static>>,
    code: &str,
    wrap_w: usize,
    accent: Style,
    muted: Style,
) {
    let inner = wrap_w.saturating_sub(4).max(8);
    let top = format!("┌ {}", "─".repeat(inner.saturating_sub(2).min(40)));
    lines.push(Line::from(Span::styled(top, muted)));
    let cmd = if code.starts_with('$') {
        code.to_string()
    } else {
        format!("$ {code}")
    };
    for part in wrap_text(&cmd, inner) {
        lines.push(Line::from(vec![
            Span::styled("│ ".to_string(), muted),
            Span::styled(part, accent.add_modifier(Modifier::BOLD)),
        ]));
    }
    let bot = format!("└ {}", "─".repeat(inner.saturating_sub(2).min(40)));
    lines.push(Line::from(Span::styled(bot, muted)));
}

/// İpucu / risk / uyarı / draft callout.
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
    let marker = match kind {
        CalloutKind::Tip => "·",
        CalloutKind::Risk | CalloutKind::Warn | CalloutKind::Draft => "!",
    };
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

/// Geniş: tablo satırları; dar: kartlar. Boş `rows` → hiçbir şey.
pub fn push_table_or_cards(
    lines: &mut Vec<Line<'static>>,
    headers: &[&str],
    rows: &[(String, String, String)],
    wrap_w: usize,
    text: Style,
    muted: Style,
    accent: Style,
) {
    if rows.is_empty() {
        return;
    }
    if wrap_w < NARROW_TABLE_WIDTH {
        for (a, b, c) in rows {
            lines.push(Line::from(Span::styled(
                format!("┌ {a}"),
                accent.add_modifier(Modifier::BOLD),
            )));
            if !b.is_empty() {
                push_wrapped(lines, &format!("│ {b}"), wrap_w, text);
            }
            if !c.is_empty() {
                lines.push(Line::from(Span::styled(format!("│ {c}"), muted)));
            }
            lines.push(Line::from(Span::styled("└".to_string(), muted)));
        }
        return;
    }
    // Basit sabit sütun tablosu
    let w0 = headers
        .first()
        .map(|h| h.chars().count().max(10))
        .unwrap_or(10)
        .min(18);
    let w2 = headers
        .get(2)
        .map(|h| h.chars().count().max(6))
        .unwrap_or(8)
        .min(12);
    let w1 = wrap_w.saturating_sub(w0 + w2 + 4).max(12);
    let hdr = format!(
        "{:<w0$}  {:<w1$}  {}",
        headers.first().copied().unwrap_or(""),
        headers.get(1).copied().unwrap_or(""),
        headers.get(2).copied().unwrap_or(""),
        w0 = w0,
        w1 = w1
    );
    lines.push(Line::from(Span::styled(
        hdr,
        muted.add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled("─".repeat(wrap_w.min(64)), muted)));
    for (a, b, c) in rows {
        let a_t = truncate_chars(a, w0);
        let b_t = truncate_chars(b, w1);
        let c_t = truncate_chars(c, w2);
        lines.push(Line::from(format!(
            "{a_t:<w0$}  {b_t:<w1$}  {c_t}",
            w0 = w0,
            w1 = w1
        )));
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    let take = max.saturating_sub(1);
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}

fn risk_color(pal: ThemePalette, risk: RiskLevel) -> ratatui::style::Color {
    match risk {
        RiskLevel::Safe => pal.success,
        RiskLevel::Caution => pal.warn,
        RiskLevel::Dangerous | RiskLevel::Destructive => pal.error,
    }
}

fn bank_cmd<'a>(app: &'a App, id: &str) -> Option<&'a CommandEntry> {
    app.commands_bank.commands.iter().find(|c| c.id == id)
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
        Style::default()
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            .fg(pal.accent),
    )));
    if let Some(ref diff) = concept.difficulty {
        lines.push(Line::from(Span::styled(
            format!("[{diff}] · {}", concept.verification.status.as_str()),
            Style::default().fg(pal.search),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            concept.verification.status.as_str().to_string(),
            muted,
        )));
    }
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
        push_heading(&mut lines, lang.t(Msg::CommandsTopicIntro));
        push_wrapped(&mut lines, intro_body, wrap_w, text);
    }

    // Order — `learning_path`
    if !concept.learning_path.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicOrder));
        for (i, step) in concept.learning_path.iter().enumerate() {
            let note = step.note.pick(lang_id);
            let line = if note.is_empty() {
                format!("  {}. {}", i + 1, step.r#ref)
            } else {
                format!("  {}. {} — {note}", i + 1, step.r#ref)
            };
            push_wrapped(&mut lines, &line, wrap_w, text);
        }
    }

    // Subtopics — structured; fallback related_concepts ids
    if !concept.subtopics.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicSubtopics));
        let rows: Vec<(String, String, String)> = concept
            .subtopics
            .iter()
            .map(|s| {
                (
                    s.title.pick(lang_id).to_string(),
                    s.summary.pick(lang_id).to_string(),
                    s.id.clone(),
                )
            })
            .collect();
        push_table_or_cards(
            &mut lines,
            &[
                lang.t(Msg::CommandsTopicSubtopics),
                lang.t(Msg::CommandsTopicIntro),
                "id",
            ],
            &rows,
            wrap_w,
            text,
            muted,
            accent,
        );
    } else if !concept.related_concepts.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicSubtopics));
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
                (title, summ, id.clone())
            })
            .collect();
        push_table_or_cards(
            &mut lines,
            &[
                lang.t(Msg::CommandsTopicSubtopics),
                lang.t(Msg::CommandsTopicIntro),
                "id",
            ],
            &rows,
            wrap_w,
            text,
            muted,
            accent,
        );
    }

    // Command table — bank `command_table` preferred; else related_commands
    if !concept.command_table.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicCmdTable));
        let rows: Vec<(String, String, String)> = concept
            .command_table
            .iter()
            .map(|row| {
                let name = bank_cmd(app, &row.id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| row.id.clone());
                let purpose = row.purpose.pick(lang_id).to_string();
                let level = if row.learning_level.is_specified() {
                    row.learning_level.label(lang_id).to_string()
                } else {
                    String::new()
                };
                (name, purpose, level)
            })
            .collect();
        push_table_or_cards(
            &mut lines,
            &[
                lang.t(Msg::CommandsList),
                lang.t(Msg::CommandsPurpose),
                "lvl",
            ],
            &rows,
            wrap_w,
            text,
            muted,
            accent,
        );
    } else if !concept.related_commands.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicCmdTable));
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

    // Learning body — intro’dan ayrı, doluysa
    let body = concept.body.pick(lang_id);
    if !body.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicLearning));
        push_wrapped(&mut lines, body, wrap_w, text);
    }

    // Examples — graded_examples, else related beginner
    let mut shown_examples = 0usize;
    if !concept.graded_examples.is_empty() {
        let filtered = select_command_examples(&concept.graded_examples, &app.commands_filters);
        if filtered.is_empty() {
            push_heading(&mut lines, lang.t(Msg::CommandsExamples));
            lines.push(Line::from(Span::styled(
                format!("  {}", lang.t(Msg::CommandsFilterNoMatch)),
                warn,
            )));
            shown_examples = 1; // section shown; skip fallback
        } else {
            push_heading(&mut lines, lang.t(Msg::CommandsExamples));
            let mut exs: Vec<_> = filtered.into_iter().cloned().collect();
            sort_examples_by_difficulty(&mut exs);
            for ex in exs.iter().take(6) {
                let title = ex.title.pick(lang_id);
                if !title.is_empty() {
                    lines.push(Line::from(format!(
                        "  · {title} [{}]",
                        ex.risk.label(lang_id)
                    )));
                }
                push_code_block(&mut lines, &ex.command, wrap_w, accent, muted);
                let expl = ex.explanation.pick(lang_id);
                if !expl.is_empty() {
                    push_wrapped(&mut lines, &format!("  {expl}"), wrap_w, muted);
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
            push_heading(&mut lines, lang.t(Msg::CommandsExamples));
            for (name, ex) in collected {
                lines.push(Line::from(Span::styled(format!("  · {name}"), muted)));
                push_code_block(&mut lines, &ex.command, wrap_w, accent, muted);
                let expl = ex.explanation.pick(lang_id);
                if !expl.is_empty() {
                    push_wrapped(&mut lines, &format!("  {expl}"), wrap_w, muted);
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
        push_heading(&mut lines, lang.t(Msg::CommandsTopicTips));
        for tip in tips {
            let t = tip.text.pick(lang_id);
            if !t.is_empty() {
                push_callout(
                    &mut lines,
                    CalloutKind::Tip,
                    lang.t(Msg::CommandsTopicTips),
                    t,
                    wrap_w,
                    Style::default().fg(pal.success),
                    muted,
                );
            }
        }
    }
    if !warns.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicWarnings));
        for w in warns {
            let wt = w.text.pick(lang_id);
            if !wt.is_empty() {
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
    }

    // Exercises
    if !concept.safe_exercises.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicExercises));
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
        push_heading(&mut lines, lang.t(Msg::CommandsTopicRelatedTasks));
        let names: Vec<String> = related_tasks
            .iter()
            .take(8)
            .map(|t| t.name.pick(lang_id).to_string())
            .collect();
        push_wrapped(&mut lines, &format!("  {}", names.join(", ")), wrap_w, text);
    }

    // Next steps
    if !concept.next_steps.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsTopicNext));
        let rows: Vec<(String, String, String)> = concept
            .next_steps
            .iter()
            .map(|n| {
                let title = app
                    .commands_bank
                    .concepts
                    .iter()
                    .find(|c| c.id == n.id)
                    .map(|c| c.title.pick(lang_id).to_string())
                    .unwrap_or_else(|| n.id.clone());
                (title, n.note.pick(lang_id).to_string(), n.id.clone())
            })
            .collect();
        push_table_or_cards(
            &mut lines,
            &[
                lang.t(Msg::CommandsTopicNext),
                lang.t(Msg::CommandsTopicIntro),
                "id",
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

    // Name
    lines.push(Line::from(Span::styled(
        format!("{fav_prefix}{}", cmd.name),
        Style::default()
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            .fg(pal.accent),
    )));
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
        push_heading(&mut lines, lang.t(Msg::CommandsWhenUseful));
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
    push_heading(&mut lines, lang.t(Msg::CommandsSyntax));
    push_code_block(&mut lines, &cmd.name, wrap_w, accent, muted);
    if !cmd.aliases.is_empty() {
        lines.push(Line::from(format!(
            "  {}: {}",
            lang.t(Msg::CommandsAliases),
            cmd.aliases.join(", ")
        )));
    }

    // Beginner example
    if let Some(ex) = beginner_example(&cmd.examples, &app.commands_filters) {
        push_heading(&mut lines, lang.t(Msg::CommandsBeginnerExample));
        let title = ex.title.pick(lang_id);
        if !title.is_empty() {
            lines.push(Line::from(format!("  · {title}")));
        }
        push_code_block(&mut lines, &ex.command, wrap_w, accent, muted);
        let expl = ex.explanation.pick(lang_id);
        if !expl.is_empty() {
            push_heading(&mut lines, lang.t(Msg::CommandsExpectedEffect));
            push_wrapped(&mut lines, &format!("  {expl}"), wrap_w, muted);
        }
    } else if !cmd.examples.is_empty() {
        // Filtre eşleşmesi yok — dürüst boş
        push_heading(&mut lines, lang.t(Msg::CommandsBeginnerExample));
        lines.push(Line::from(Span::styled(
            format!("  {}", lang.t(Msg::CommandsFilterNoMatch)),
            warn,
        )));
    }

    // Flags
    if !cmd.flags.is_empty() {
        push_heading(&mut lines, lang.t(Msg::CommandsFlags));
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
        push_heading(&mut lines, lang.t(Msg::CommandsExamplesEasyAdv));
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
                if !title.is_empty() {
                    lines.push(Line::from(format!(
                        "  · {title} [{}{}]",
                        ex.risk.label(lang_id),
                        elev
                    )));
                }
                push_code_block(&mut lines, &ex.command, wrap_w, accent, muted);
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
    push_heading(&mut lines, lang.t(Msg::CommandsPlatforms));
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
    push_heading(&mut lines, lang.t(Msg::CommandsSideEffects));
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
        push_heading(&mut lines, lang.t(Msg::CommandsMistakes));
        for m in mistakes.iter().take(6) {
            push_callout(&mut lines, CalloutKind::Warn, "!", m, wrap_w, warn, muted);
        }
    }

    // Packages — honest distro
    let is_arch = crate::commands::host_is_arch();
    if cmd.has_package_hints() {
        push_heading(&mut lines, lang.t(Msg::CommandsPackages));
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
        push_heading(&mut lines, lang.t(Msg::CommandsRelated));
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
    push_heading(&mut lines, lang.t(Msg::CommandsSources));
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

    #[test]
    fn table_becomes_cards_when_narrow() {
        let rows = vec![
            ("restic".into(), "backup".into(), "caution".into()),
            ("borg".into(), "archive".into(), "caution".into()),
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
        let wide_t: String = wide
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(wide_t.contains("cmd"), "{wide_t}");
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
        let narrow_t: String = narrow
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(narrow_t.contains('┌'), "{narrow_t}");
        assert!(narrow_t.contains("restic"), "{narrow_t}");
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
}
