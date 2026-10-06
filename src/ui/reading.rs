use crate::{App, Entry, theme::Theme};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
pub(super) struct ReadingLine {
    pub(super) text: String,
    pub(super) style: Style,
}

// Screen regions and word positions from the most recent frame, filled during
// rendering because ratatui performs no hit testing. Kept out of `App` so the
// application state stays independent of the terminal backend.

fn append_entry(
    lines: &mut Vec<ReadingLine>,
    entry: &Entry,
    related: bool,
    theme: Theme,
    expand_ipa: bool,
    expand_examples: bool,
) {
    if related {
        lines.push(ReadingLine {
            text: format!("→ {}", entry.headword),
            style: theme.heading(),
        });
    }
    // Partition across groups as well as within each group: historical-only groups come last.
    for historical in [false, true] {
        for group in &entry.groups {
            let senses: Vec<_> = group
                .senses
                .iter()
                .filter(|s| s.historical() == historical)
                .collect();
            if senses.is_empty() {
                continue;
            }
            let ipa = if !expand_ipa && group.ipa.len() > 2 {
                format!("{} …", group.ipa[..2].join(" · "))
            } else {
                group.ipa.join(" · ")
            };
            lines.push(ReadingLine {
                text: format!(
                    "{}  {}  {}{}",
                    group.headword,
                    group.pos,
                    ipa,
                    if historical {
                        "  [archaic / obsolete]"
                    } else {
                        ""
                    }
                ),
                style: theme.heading(),
            });
            for (i, sense) in senses.iter().enumerate() {
                let tags = if sense.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", sense.tags.join(", "))
                };
                lines.push(ReadingLine {
                    text: format!("{}. {}{}", i + 1, sense.glosses.join(" › "), tags),
                    style: Style::default(),
                });
                if expand_examples {
                    for example in &sense.examples {
                        lines.push(ReadingLine {
                            text: format!("   • {}", example.text),
                            style: theme.example(),
                        });
                        if !example.reference.is_empty() {
                            lines.push(ReadingLine {
                                text: format!("     — {}", example.reference),
                                style: theme.dim(),
                            });
                        }
                    }
                } else if let Some(example) = sense.examples.first() {
                    lines.push(ReadingLine {
                        text: format!("   • {}", example.text),
                        style: theme.example(),
                    });
                }
                lines.push(ReadingLine {
                    text: String::new(),
                    style: Style::default(),
                });
            }
        }
    }
}

fn empty_state(app: &App) -> String {
    if app.loading {
        return "Looking up…".into();
    }
    let query = crate::normalize(&app.input);
    if query.is_empty() {
        "Start typing an English word or phrase.".into()
    } else if query.chars().count() < 3 {
        format!("No word starts with \"{query}\". Spelling suggestions need at least 3 letters.")
    } else {
        format!("No matching words for \"{query}\". Check the spelling.")
    }
}

/// Dim every reading line so a retained definition reads as stale while a new
/// query is in flight.
pub(super) fn stale_style(lines: Vec<ReadingLine>) -> Vec<ReadingLine> {
    lines
        .into_iter()
        .map(|mut line| {
            line.style = line.style.add_modifier(Modifier::DIM);
            line
        })
        .collect()
}

pub(super) fn reading_lines(app: &App) -> Vec<ReadingLine> {
    let mut lines = vec![];
    if let Some(error) = &app.error {
        lines.push(ReadingLine {
            text: error.clone(),
            style: app.theme.error(),
        });
    }
    let Some(preview) = &app.preview else {
        if lines.is_empty() {
            lines.push(ReadingLine {
                text: empty_state(app),
                style: app.theme.dim(),
            });
        }
        return lines;
    };
    if !preview.related.is_empty() {
        let relations = preview
            .related
            .iter()
            .map(|e| e.headword.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(ReadingLine {
            text: format!("{} → {} (word form)", preview.entry.headword, relations),
            style: app.theme.accent(),
        });
        for related in &preview.related {
            append_entry(
                &mut lines,
                related,
                true,
                app.theme,
                app.expand_ipa,
                app.expand_examples,
            );
        }
        lines.push(ReadingLine {
            text: format!("Original form: {}", preview.entry.headword),
            style: app.theme.dim(),
        });
    }
    append_entry(
        &mut lines,
        &preview.entry,
        false,
        app.theme,
        app.expand_ipa,
        app.expand_examples,
    );
    lines.push(ReadingLine {
        text: format!("Source: {}", preview.entry.source_url),
        style: app.theme.dim(),
    });
    lines
}

pub(super) fn hanging_indent(text: &str) -> usize {
    let digits_len = text.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits_len > 0 && text[digits_len..].starts_with(". ") {
        return text[..digits_len + 2].width();
    }
    let ws_len: usize = text
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(char::len_utf8)
        .sum();
    let ws = &text[..ws_len];
    let rest = &text[ws_len..];
    if rest.starts_with("• ") {
        return text[..ws_len + "• ".len()].width();
    }
    if rest.starts_with("— ") {
        return text[..ws_len + "— ".len()].width();
    }
    if ws_len > 0 {
        return ws.width();
    }
    0
}

// Wrap first, then label; annotations never change line lengths or scroll coordinates.

pub(super) fn wrap(lines: Vec<ReadingLine>, width: usize) -> Vec<ReadingLine> {
    let width = width.max(1);
    let mut result = vec![];
    let lines = lines.into_iter().flat_map(|line| {
        line.text
            .split('\n')
            .map(|text| ReadingLine {
                text: text
                    .chars()
                    .filter_map(|c| {
                        if c == '\t' {
                            Some(' ')
                        } else if c.is_control() {
                            None
                        } else {
                            Some(c)
                        }
                    })
                    .collect(),
                style: line.style,
            })
            .collect::<Vec<_>>()
    });
    for line in lines {
        if line.text.width() <= width {
            result.push(line);
            continue;
        }
        let raw_indent = hanging_indent(&line.text);
        let indent = raw_indent.min(width.saturating_sub(1));
        let indent_str = " ".repeat(indent);
        let mut current = String::new();
        let mut columns = 0;
        let mut is_continuation = false;
        for token in line.text.split_word_bounds() {
            let is_ws = token.chars().all(char::is_whitespace);
            if is_continuation && columns == indent && is_ws {
                continue;
            }
            let n = token.width();
            let min_cols = if is_continuation { indent } else { 0 };
            if columns + n > width && columns > min_cols {
                result.push(ReadingLine {
                    text: current,
                    style: line.style,
                });
                is_continuation = true;
                current = indent_str.clone();
                columns = indent;
                if is_ws {
                    continue;
                }
            }
            if columns + n <= width {
                current.push_str(token);
                columns += n;
            } else {
                for grapheme in token.graphemes(true) {
                    let gn = grapheme.width();
                    let min_cols = if is_continuation { indent } else { 0 };
                    if columns + gn > width && columns > min_cols {
                        result.push(ReadingLine {
                            text: current,
                            style: line.style,
                        });
                        is_continuation = true;
                        current = indent_str.clone();
                        columns = indent;
                    }
                    current.push_str(grapheme);
                    columns += gn;
                }
            }
        }
        let min_cols = if is_continuation { indent } else { 0 };
        if !is_continuation || columns > min_cols {
            result.push(ReadingLine {
                text: current,
                style: line.style,
            });
        }
    }
    result
}

pub(super) fn label_line(
    line: &ReadingLine,
    map: &HashMap<String, String>,
    theme: Theme,
) -> Line<'static> {
    let mut spans = vec![];
    for token in line.text.split_word_bounds() {
        if let Some(label) = map.get(&crate::normalize(token)) {
            // Replace the first two display columns with the hint (single-letter words use one).
            let graphemes: Vec<_> = token.graphemes(true).collect();
            let count = graphemes.len().min(2);
            let hint = label.chars().take(count).collect::<String>();
            spans.push(Span::styled(hint, theme.hint_label()));
            spans.push(Span::styled(graphemes[count..].concat(), line.style));
        } else {
            spans.push(Span::styled(token.to_string(), line.style));
        }
    }
    Line::from(spans)
}
