use crate::{App, Dictionary, Entry, Focus, MatchKind, theme::Theme};
use anyhow::Result;
use crossterm::{
    Command,
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::{collections::HashMap, fmt, io, time::Duration};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Lines moved per wheel notch. Terminals send one event per notch, so this
/// stays small enough to feel continuous.
const WHEEL_LINES: usize = 3;

#[derive(Clone)]
struct ReadingLine {
    text: String,
    style: Style,
}

// Screen regions and word positions from the most recent frame, filled during
// rendering because ratatui performs no hit testing. Kept out of `App` so the
// application state stays independent of the terminal backend.
#[derive(Default, Clone, Copy)]
struct Region {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl Region {
    fn contains(&self, column: u16, row: u16) -> bool {
        self.width > 0
            && self.height > 0
            && column >= self.x
            && column < self.x + self.width
            && row >= self.y
            && row < self.y + self.height
    }
}

impl From<Rect> for Region {
    fn from(area: Rect) -> Self {
        Self {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        }
    }
}

struct HitToken {
    start: u16,
    end: u16,
    word: String,
}

struct HitRow {
    y: u16,
    tokens: Vec<HitToken>,
}

#[derive(Default)]
struct Pointer {
    input: Region,
    definition: Region,
    candidates: Region,
    candidates_offset: usize,
    candidates_len: usize,
    rows: Vec<HitRow>,
}

impl Pointer {
    fn reset(&mut self) {
        self.input = Region::default();
        self.definition = Region::default();
        self.candidates = Region::default();
        self.candidates_offset = 0;
        self.candidates_len = 0;
        self.rows.clear();
    }

    fn record_rows(&mut self, inner: Rect, visible: &[&ReadingLine]) {
        self.rows.clear();
        for (i, line) in visible.iter().enumerate() {
            let y = inner.y + i as u16;
            let mut tokens = Vec::new();
            let mut column = inner.x;
            for token in line.text.split_word_bounds() {
                let width = token.width() as u16;
                // Whitespace and punctuation cannot be looked up; skip allocating
                // hit targets for them.
                if width > 0 && token.chars().any(char::is_alphanumeric) {
                    tokens.push(HitToken {
                        start: column,
                        end: column + width,
                        word: token.to_string(),
                    });
                }
                column += width;
            }
            self.rows.push(HitRow { y, tokens });
        }
    }

    fn definition_word(&self, column: u16, row: u16) -> Option<String> {
        self.rows
            .iter()
            .find(|r| r.y == row)?
            .tokens
            .iter()
            .find(|t| column >= t.start && column < t.end)
            .map(|t| t.word.clone())
    }

    #[cfg(test)]
    fn word_position(&self, word: &str) -> Option<(u16, u16)> {
        for row in &self.rows {
            if let Some(token) = row.tokens.iter().find(|t| t.word == word) {
                return Some((token.start, row.y));
            }
        }
        None
    }
}

fn append_entry(lines: &mut Vec<ReadingLine>, entry: &Entry, related: bool, theme: Theme) {
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
            lines.push(ReadingLine {
                text: format!(
                    "{}  {}  {}{}",
                    group.headword,
                    group.pos,
                    group.ipa.join(" · "),
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
                lines.push(ReadingLine {
                    text: String::new(),
                    style: Style::default(),
                });
            }
        }
    }
}

fn reading_lines(app: &App) -> Vec<ReadingLine> {
    let mut lines = vec![];
    if let Some(error) = &app.error {
        lines.push(ReadingLine {
            text: error.clone(),
            style: app.theme.error(),
        });
        return lines;
    }
    let Some(preview) = &app.preview else {
        let text = if app.loading {
            "Looking up…"
        } else if app.input.is_empty() {
            "Start typing an English word or phrase."
        } else {
            "No matching words."
        };
        return vec![ReadingLine {
            text: text.into(),
            style: app.theme.dim(),
        }];
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
            append_entry(&mut lines, related, true, app.theme);
        }
        lines.push(ReadingLine {
            text: format!("Original form: {}", preview.entry.headword),
            style: app.theme.dim(),
        });
    }
    append_entry(&mut lines, &preview.entry, false, app.theme);
    lines.push(ReadingLine {
        text: format!("Source: {}", preview.entry.source_url),
        style: app.theme.dim(),
    });
    lines
}

fn hanging_indent(text: &str) -> usize {
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
fn wrap(lines: Vec<ReadingLine>, width: usize) -> Vec<ReadingLine> {
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

fn label_line(line: &ReadingLine, map: &HashMap<String, String>, theme: Theme) -> Line<'static> {
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

pub fn draw(frame: &mut Frame, app: &mut App) {
    let mut pointer = Pointer::default();
    render(frame, app, &mut pointer);
}

fn render(frame: &mut Frame, app: &mut App, pointer: &mut Pointer) {
    pointer.reset();
    let area = frame.area();
    if area.width < 30 || area.height < 10 {
        frame.render_widget(
            Paragraph::new("Resize to at least 30 × 10. Ctrl+C exits."),
            area,
        );
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(if area.height < 12 { 1 } else { 2 }),
        ])
        .split(area);
    pointer.input = Region::from(rows[0]);
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(" Wordglide · English ")
        .border_style(app.theme.border(app.focus == Focus::Input));
    let available = rows[0].width.saturating_sub(3) as usize;
    let before = &app.input[..app.cursor];
    let mut start = 0;
    while before[start..].width() > available.saturating_sub(1) {
        start += before[start..].chars().next().map_or(0, char::len_utf8);
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(app.input[start..].to_string()),
            Span::styled(app.inline_suffix().unwrap_or_default(), app.theme.dim()),
        ]))
        .block(input_block),
        rows[0],
    );
    if app.focus == Focus::Input && !app.picking {
        frame.set_cursor_position((
            rows[0].x + 1 + before[start..].width() as u16,
            rows[0].y + 1,
        ));
    }
    let panes = if area.width >= 80 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
            .split(rows[1])
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(rows[1])
    };
    pointer.definition = Region::from(panes[1]);
    render_candidates(frame, app, panes[0], pointer);
    render_definition(frame, app, panes[1], pointer);
    let help_lines = footer_help(app, rows[2].width as usize, rows[2].height as usize);
    frame.render_widget(
        Paragraph::new(help_lines.join("\n")).style(app.theme.dim()),
        rows[2],
    );
}

#[derive(Clone, Debug)]
struct HelpSegment {
    text: String,
    line: usize,
    order: usize,
    priority: usize,
}

fn format_line(segments: &[&HelpSegment]) -> String {
    segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut current_width = 0;
    let mut result = String::new();
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if current_width + w > max_width {
            break;
        }
        result.push(c);
        current_width += w;
    }
    result
}

fn fit_segments<'a>(
    mandatory: &[&'a HelpSegment],
    candidates: &[&'a HelpSegment],
    width: usize,
    sort_by_line: bool,
) -> Vec<&'a HelpSegment> {
    let mut included = mandatory.to_vec();
    for candidate in candidates {
        let mut test_included = included.clone();
        test_included.push(candidate);
        if sort_by_line {
            test_included.sort_by_key(|s| (s.line, s.order));
        } else {
            test_included.sort_by_key(|s| s.order);
        }
        if format_line(&test_included).width() <= width {
            included = test_included;
        }
    }
    if sort_by_line {
        included.sort_by_key(|s| (s.line, s.order));
    } else {
        included.sort_by_key(|s| s.order);
    }
    included
}

fn format_footer_help(
    segments: &[HelpSegment],
    available_width: usize,
    available_lines: usize,
) -> Vec<String> {
    if available_lines == 0 || available_width == 0 || segments.is_empty() {
        return Vec::new();
    }

    let exit_seg = segments.iter().find(|s| s.priority == 0);

    if available_lines == 1 {
        let mandatory: Vec<&HelpSegment> = exit_seg.into_iter().collect();
        let mut candidates: Vec<&HelpSegment> =
            segments.iter().filter(|s| s.priority != 0).collect();
        candidates.sort_by_key(|s| s.priority);

        let included = fit_segments(&mandatory, &candidates, available_width, true);
        let line = format_line(&included);
        if line.width() <= available_width {
            vec![line]
        } else if let Some(exit) = exit_seg {
            vec![truncate_to_width(&exit.text, available_width)]
        } else {
            vec![]
        }
    } else {
        let max_line = segments.iter().map(|s| s.line).max().unwrap_or(0);
        if max_line == 0 {
            let mandatory: Vec<&HelpSegment> = exit_seg.into_iter().collect();
            let mut candidates: Vec<&HelpSegment> =
                segments.iter().filter(|s| s.priority != 0).collect();
            candidates.sort_by_key(|s| s.priority);

            let included = fit_segments(&mandatory, &candidates, available_width, false);
            let line = format_line(&included);
            if line.width() <= available_width {
                vec![line]
            } else if let Some(exit) = exit_seg {
                vec![truncate_to_width(&exit.text, available_width)]
            } else {
                vec![]
            }
        } else {
            let mandatory_l1: Vec<&HelpSegment> =
                exit_seg.filter(|s| s.line == 1).into_iter().collect();
            let mut candidates_l1: Vec<&HelpSegment> = segments
                .iter()
                .filter(|s| s.line == 1 && s.priority != 0)
                .collect();
            candidates_l1.sort_by_key(|s| s.priority);
            let included_l1 = fit_segments(&mandatory_l1, &candidates_l1, available_width, false);

            let mut candidates_l0: Vec<&HelpSegment> =
                segments.iter().filter(|s| s.line == 0).collect();
            candidates_l0.sort_by_key(|s| s.priority);
            let included_l0 = fit_segments(&[], &candidates_l0, available_width, false);

            let line0 = format_line(&included_l0);
            let line1 = format_line(&included_l1);

            let line0 = if line0.width() <= available_width {
                line0
            } else {
                truncate_to_width(&line0, available_width)
            };
            let line1 = if line1.width() <= available_width {
                line1
            } else {
                truncate_to_width(&line1, available_width)
            };

            if line0.is_empty() {
                vec![line1]
            } else {
                vec![line0, line1]
            }
        }
    }
}

fn footer_help(app: &App, width: usize, line_count: usize) -> Vec<String> {
    let segments = if app.picking {
        vec![
            HelpSegment {
                text: format!("Label: {}_ ", app.label_input),
                line: 0,
                order: 0,
                priority: 1,
            },
            HelpSegment {
                text: "type both letters".into(),
                line: 0,
                order: 1,
                priority: 3,
            },
            HelpSegment {
                text: "PgUp/PgDn scroll".into(),
                line: 0,
                order: 2,
                priority: 4,
            },
            HelpSegment {
                text: "Esc cancels".into(),
                line: 0,
                order: 3,
                priority: 2,
            },
            HelpSegment {
                text: "Ctrl+C quit".into(),
                line: 0,
                order: 4,
                priority: 0,
            },
        ]
    } else if app.focus == Focus::Definition {
        let loading_suffix = if app.loading { " · loading…" } else { "" };
        vec![
            HelpSegment {
                text: "Reading".into(),
                line: 0,
                order: 0,
                priority: 3,
            },
            HelpSegment {
                text: "PgUp/PgDn or wheel scroll".into(),
                line: 0,
                order: 1,
                priority: 6,
            },
            HelpSegment {
                text: "Home/End top/bottom".into(),
                line: 0,
                order: 2,
                priority: 8,
            },
            HelpSegment {
                text: "f follow".into(),
                line: 0,
                order: 3,
                priority: 2,
            },
            HelpSegment {
                text: "Esc input".into(),
                line: 0,
                order: 4,
                priority: 1,
            },
            HelpSegment {
                text: "Ctrl+L focus".into(),
                line: 1,
                order: 0,
                priority: 4,
            },
            HelpSegment {
                text: format!("Ctrl+Z back ({})", app.history_len()),
                line: 1,
                order: 1,
                priority: 5,
            },
            HelpSegment {
                text: "Ctrl+Y forward".into(),
                line: 1,
                order: 2,
                priority: 7,
            },
            HelpSegment {
                text: format!("Ctrl+C quit{loading_suffix}"),
                line: 1,
                order: 3,
                priority: 0,
            },
        ]
    } else {
        let loading_suffix = if app.loading { " · loading…" } else { "" };
        vec![
            HelpSegment {
                text: "Tab complete".into(),
                line: 0,
                order: 0,
                priority: 2,
            },
            HelpSegment {
                text: "Shift+Tab previous".into(),
                line: 0,
                order: 1,
                priority: 7,
            },
            HelpSegment {
                text: "Enter read".into(),
                line: 0,
                order: 2,
                priority: 1,
            },
            HelpSegment {
                text: "Ctrl+L focus".into(),
                line: 0,
                order: 3,
                priority: 4,
            },
            HelpSegment {
                text: "PgUp/PgDn or wheel scroll".into(),
                line: 0,
                order: 4,
                priority: 9,
            },
            HelpSegment {
                text: "f follow".into(),
                line: 1,
                order: 0,
                priority: 5,
            },
            HelpSegment {
                text: format!("Ctrl+Z back ({})", app.history_len()),
                line: 1,
                order: 1,
                priority: 6,
            },
            HelpSegment {
                text: "Ctrl+Y forward".into(),
                line: 1,
                order: 2,
                priority: 8,
            },
            HelpSegment {
                text: "Ctrl+U new".into(),
                line: 1,
                order: 3,
                priority: 3,
            },
            HelpSegment {
                text: format!("Ctrl+C quit{loading_suffix}"),
                line: 1,
                order: 4,
                priority: 0,
            },
        ]
    };

    format_footer_help(&segments, width, line_count)
}

fn render_candidates(frame: &mut Frame, app: &App, area: Rect, pointer: &mut Pointer) {
    let items: Vec<_> = app
        .results
        .iter()
        .map(|c| {
            let hint = match c.kind {
                MatchKind::Fuzzy => " ≈",
                MatchKind::Inflection => " →",
                _ => "",
            };
            ListItem::new(format!("{}{}", c.headword, hint))
        })
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Candidates · {} ", app.results.len()))
        .border_style(app.theme.idle_border());
    let inner = block.inner(area);
    let list = List::new(items)
        .block(block)
        .highlight_style(app.theme.selected())
        .highlight_symbol("› ");
    let mut state = ListState::default();
    if !app.results.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
    // The list writes back the first visible index, and single-line items map
    // one row per candidate, so a click row resolves to `offset + row`.
    pointer.candidates = Region::from(inner);
    pointer.candidates_offset = state.offset();
    pointer.candidates_len = app.results.len();
}

fn render_definition(frame: &mut Frame, app: &mut App, area: Rect, pointer: &mut Pointer) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Definition ")
        .border_style(app.theme.border(app.focus == Focus::Definition));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // A page keeps one line of overlap so no definition line is skipped.
    app.page = inner.height.saturating_sub(1).max(1) as usize;
    let lines = wrap(reading_lines(app), inner.width as usize);
    app.max_scroll = lines.len().saturating_sub(inner.height as usize);
    app.scroll = app.scroll.min(app.max_scroll);
    let visible: Vec<_> = lines
        .iter()
        .skip(app.scroll)
        .take(inner.height as usize)
        .collect();
    pointer.record_rows(inner, &visible);
    let mut map = HashMap::new();
    app.labels.clear();
    if app.picking {
        for line in &visible {
            for token in line.text.split_word_bounds() {
                let key = crate::normalize(token);
                if token.graphemes(true).take(2).count() == 2
                    && token.graphemes(true).take(2).all(|g| g.width() == 1)
                    && app.contains(&key)
                    && !map.contains_key(&key)
                    && map.len() < 676
                {
                    let n = map.len();
                    let label = format!(
                        "{}{}",
                        (b'a' + (n / 26) as u8) as char,
                        (b'a' + (n % 26) as u8) as char
                    );
                    app.labels.push((label.clone(), key.clone()));
                    map.insert(key, label);
                }
            }
        }
    }
    let rendered: Vec<Line> = visible
        .iter()
        .map(|l| {
            if app.picking {
                label_line(l, &map, app.theme)
            } else {
                Line::styled(l.text.clone(), l.style)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(rendered), inner);
}

/// Handle a mouse event against the last rendered layout. Returns whether the
/// visible state changed. Motion and drag are ignored so `?1003h` traffic never
/// forces a redraw.
fn on_mouse(app: &mut App, pointer: &Pointer, mouse: MouseEvent) -> bool {
    match mouse.kind {
        MouseEventKind::ScrollUp => return app.scroll_by(-(WHEEL_LINES as isize)),
        MouseEventKind::ScrollDown => return app.scroll_by(WHEEL_LINES as isize),
        MouseEventKind::Down(MouseButton::Left) => {}
        // Motion, drag, and other buttons are never read, so they must not
        // force a redraw either.
        _ => return false,
    }
    if pointer.definition.contains(mouse.column, mouse.row) {
        if app.picking {
            app.picking = false;
            app.label_input.clear();
            return true;
        }
        if app.focus != Focus::Definition {
            app.focus = Focus::Definition;
            return true;
        }
        if let Some(word) = pointer.definition_word(mouse.column, mouse.row)
            && app.contains(&word)
            && crate::normalize(&word) != crate::normalize(&app.input)
        {
            app.jump_to(&word);
            return true;
        }
        return false;
    }
    if pointer.candidates.contains(mouse.column, mouse.row) {
        let row = (mouse.row - pointer.candidates.y) as usize;
        let index = pointer.candidates_offset + row;
        return index < pointer.candidates_len && app.click_candidate(index);
    }
    if pointer.input.contains(mouse.column, mouse.row) && app.focus != Focus::Input {
        app.focus = Focus::Input;
        app.picking = false;
        app.label_input.clear();
        return true;
    }
    false
}

/// Mouse reporting narrowed to button presses and wheel scrolling.
///
/// `crossterm::event::EnableMouseCapture` additionally turns on `?1003h`, which
/// reports every pointer motion. Nothing here reads motion events, so that
/// traffic only slows the event loop down. Enabling `?1000h` with SGR
/// coordinates keeps clicks and the wheel and leaves native drag-selection
/// working in most terminals.
///
/// Deliberately ANSI-only: Wordglide ships for macOS and Linux, so there is no
/// WinAPI fallback to keep in step with the sequences below.
#[derive(Clone, Copy)]
struct MouseTracking {
    enabled: bool,
}

const MOUSE_ON: MouseTracking = MouseTracking { enabled: true };
const MOUSE_OFF: MouseTracking = MouseTracking { enabled: false };

impl Command for MouseTracking {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(if self.enabled {
            "\x1b[?1000h\x1b[?1006h"
        } else {
            "\x1b[?1006l\x1b[?1000l"
        })
    }
}

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            MOUSE_OFF,
            LeaveAlternateScreen
        );
    }
}

pub fn run(dictionary: Dictionary, query: &str, color: bool) -> Result<()> {
    let mut app = App::new(dictionary, query);
    app.set_color(color);
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            MOUSE_OFF,
            LeaveAlternateScreen
        );
        old_hook(info);
    }));
    enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        MOUSE_ON
    )?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut pointer = Pointer::default();
    let mut dirty = true;
    while !app.exit {
        dirty |= app.poll();
        if dirty {
            terminal.draw(|frame| render(frame, &mut app, &mut pointer))?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(5))? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    app.handle_key(key);
                    dirty = true;
                }
                Event::Mouse(mouse) => {
                    dirty |= on_mouse(&mut app, &pointer, mouse);
                }
                Event::Paste(text) => {
                    app.paste(&text);
                    dirty = true;
                }
                Event::Resize(_, _) => {
                    dirty = true;
                }
                _ => {}
            }
        }
    }
    terminal.show_cursor()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dictionary, build_pack};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use std::{
        path::Path,
        thread,
        time::{Duration, Instant},
    };

    fn dictionary() -> (tempfile::TempDir, Dictionary) {
        let dir = tempfile::tempdir().unwrap();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sample");
        let output = dir.path().join("pack");
        build_pack(
            &sample.join("entries.jsonl"),
            &sample.join("source.json"),
            &output,
        )
        .unwrap();
        (dir, Dictionary::open(&output).unwrap())
    }

    fn settle(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.loading {
            app.poll();
            assert!(Instant::now() < deadline, "worker did not finish");
            thread::sleep(Duration::from_millis(1));
        }
        assert!(app.error.is_none(), "{:?}", app.error);
    }

    fn click(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn paint(app: &mut App, terminal: &mut Terminal<TestBackend>, pointer: &mut Pointer) {
        terminal.draw(|frame| render(frame, app, pointer)).unwrap();
    }

    fn stroke(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn hard_line_breaks_and_source_controls() {
        let lines = wrap(
            vec![ReadingLine {
                text: "first\nsecond\n\u{1b}[31mthird".into(),
                style: Style::default(),
            }],
            80,
        );
        assert_eq!(
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            vec!["first", "second", "[31mthird"]
        );
        assert!(
            lines
                .iter()
                .all(|line| !line.text.contains('\n') && !line.text.contains('\u{1b}'))
        );
    }

    #[test]
    fn hanging_indent_computes_expected_prefix_widths() {
        assert_eq!(hanging_indent("1. text"), 3);
        assert_eq!(hanging_indent("12. text"), 4);
        assert_eq!(hanging_indent("   • text"), 5);
        assert_eq!(hanging_indent("     — text"), 7);
        assert_eq!(hanging_indent("house  noun  /haʊs/"), 0);
        assert_eq!(hanging_indent("Source: https://example.com"), 0);
        assert_eq!(hanging_indent("  whitespace only"), 2);
        assert_eq!(hanging_indent(""), 0);
    }

    #[test]
    fn wrapped_definition_lines_hanging_indents_and_display_widths() {
        let long_sense = "1. Economics is a messy discipline: too fluid to be a science, too rigorous to be an art, yet full of profound insights into human behavior and complex institutions.";
        let long_example = "   • Economics is a messy discipline: too fluid to be a science, too rigorous to be an art, yet full of profound insights into human behavior and complex institutions.";
        let long_reference = "     — 1993, Francis J. Sheed, Theology and Sanity, Sheed & Ward, London and New York, page 1234, an extensive reference citation.";
        let long_heading = "house  noun  /haʊs/  [archaic / obsolete]  an established residence or dwelling place throughout history";
        let long_source = "Source: https://en.wiktionary.org/wiki/economics_is_a_messy_discipline_too_fluid_to_be_a_science";

        for width in [20, 30, 40, 80] {
            // A long "1. ..." sense wraps with every continuation prefix equal to 3 spaces
            let sense_lines = wrap(
                vec![ReadingLine {
                    text: long_sense.into(),
                    style: Style::default(),
                }],
                width,
            );
            assert!(sense_lines.len() > 1, "sense must wrap at width {width}");
            assert!(sense_lines[0].text.starts_with("1. "));
            for (idx, line) in sense_lines.iter().enumerate().skip(1) {
                assert!(
                    line.text.starts_with("   ") && !line.text.starts_with("    "),
                    "continuation line {idx} prefix mismatch at width {width}: {:?}",
                    line.text
                );
            }

            // A long "   • ..." example wraps with every continuation prefix equal to 5 spaces
            let example_lines = wrap(
                vec![ReadingLine {
                    text: long_example.into(),
                    style: Style::default(),
                }],
                width,
            );
            assert!(
                example_lines.len() > 1,
                "example must wrap at width {width}"
            );
            assert!(example_lines[0].text.starts_with("   • "));
            for (idx, line) in example_lines.iter().enumerate().skip(1) {
                assert!(
                    line.text.starts_with("     ") && !line.text.starts_with("      "),
                    "continuation line {idx} prefix mismatch at width {width}: {:?}",
                    line.text
                );
            }

            // A long "     — ..." reference wraps with every continuation prefix equal to 7 spaces
            let ref_lines = wrap(
                vec![ReadingLine {
                    text: long_reference.into(),
                    style: Style::default(),
                }],
                width,
            );
            assert!(ref_lines.len() > 1, "reference must wrap at width {width}");
            assert!(ref_lines[0].text.starts_with("     — "));
            for (idx, line) in ref_lines.iter().enumerate().skip(1) {
                assert!(
                    line.text.starts_with("       ") && !line.text.starts_with("        "),
                    "continuation line {idx} prefix mismatch at width {width}: {:?}",
                    line.text
                );
            }

            // A wrapped heading and a wrapped "Source: ..." line have no indent
            let heading_lines = wrap(
                vec![ReadingLine {
                    text: long_heading.into(),
                    style: Style::default(),
                }],
                width,
            );
            assert!(
                heading_lines.len() > 1,
                "heading must wrap at width {width}"
            );
            for (idx, line) in heading_lines.iter().enumerate().skip(1) {
                assert!(
                    !line.text.starts_with(' '),
                    "heading continuation line {idx} must have no indent at width {width}: {:?}",
                    line.text
                );
            }

            let source_lines = wrap(
                vec![ReadingLine {
                    text: long_source.into(),
                    style: Style::default(),
                }],
                width,
            );
            assert!(source_lines.len() > 1, "source must wrap at width {width}");
            for (idx, line) in source_lines.iter().enumerate().skip(1) {
                assert!(
                    !line.text.starts_with(' '),
                    "source continuation line {idx} must have no indent at width {width}: {:?}",
                    line.text
                );
            }

            // Every produced line's display width <= width for width in {20, 30, 40, 80}
            for all_lines in [
                &sense_lines,
                &example_lines,
                &ref_lines,
                &heading_lines,
                &source_lines,
            ] {
                for line in all_lines.iter() {
                    assert!(
                        line.text.width() <= width,
                        "line width {} exceeds max {width}: {:?}",
                        line.text.width(),
                        line.text
                    );
                }
            }
        }
    }

    #[test]
    fn click_focuses_definition_then_jumps_to_the_visible_word() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "fist");
        settle(&mut app);
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);

        let (column, row) = pointer.word_position("hand").expect("visible word");
        assert!(on_mouse(&mut app, &pointer, click(column, row)));
        assert_eq!(app.focus, Focus::Definition);
        assert_eq!(app.input, "fist");
        assert_eq!(app.history_len(), 0);

        paint(&mut app, &mut terminal, &mut pointer);
        let (column, row) = pointer.word_position("hand").unwrap();
        assert!(on_mouse(&mut app, &pointer, click(column, row)));
        settle(&mut app);
        assert_eq!(app.input, "hand");
        assert_eq!(app.preview.as_ref().unwrap().entry.key, "hand");
        assert_eq!(app.history_len(), 1);

        app.back();
        assert_eq!(app.input, "fist");
        assert_eq!(app.focus, Focus::Definition);
    }

    #[test]
    fn click_input_returns_focus_without_navigation() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "fist");
        settle(&mut app);
        app.focus = Focus::Definition;
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);

        let (column, row) = (pointer.input.x + 1, pointer.input.y + 1);
        assert!(on_mouse(&mut app, &pointer, click(column, row)));
        assert_eq!(app.focus, Focus::Input);
        assert_eq!(app.history_len(), 0);
    }

    #[test]
    fn click_without_a_dictionary_word_and_mouse_motion_do_nothing() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "fist");
        settle(&mut app);
        app.focus = Focus::Definition;
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);

        let (column, row) = (pointer.definition.x, pointer.definition.y);
        assert!(!on_mouse(&mut app, &pointer, click(column, row)));
        let moved = MouseEvent {
            kind: MouseEventKind::Moved,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        assert!(!on_mouse(&mut app, &pointer, moved));
        assert_eq!(app.history_len(), 0);
        assert_eq!(app.input, "fist");
    }

    #[test]
    fn stacked_narrow_layout_click_targets_the_definition_pane() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "fist");
        settle(&mut app);
        let mut terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);

        assert!(pointer.definition.y > pointer.input.y);
        let (column, row) = pointer.word_position("hand").expect("visible word");
        assert!(on_mouse(&mut app, &pointer, click(column, row)));
        assert_eq!(app.focus, Focus::Definition);
        assert_eq!(app.input, "fist");
    }

    #[test]
    fn wheel_scrolls_the_reading_pane_and_clamps() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "fist");
        settle(&mut app);
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);
        assert!(app.max_scroll > 4 * WHEEL_LINES);

        let (column, row) = (pointer.definition.x, pointer.definition.y);
        let notch = |kind| MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        assert!(on_mouse(
            &mut app,
            &pointer,
            notch(MouseEventKind::ScrollDown)
        ));
        assert_eq!(app.scroll, WHEEL_LINES);
        assert_eq!(app.focus, Focus::Input);
        assert!(on_mouse(
            &mut app,
            &pointer,
            notch(MouseEventKind::ScrollUp)
        ));
        assert_eq!(app.scroll, 0);
        assert!(!on_mouse(
            &mut app,
            &pointer,
            notch(MouseEventKind::ScrollUp)
        ));

        app.scroll = app.max_scroll;
        assert!(!on_mouse(
            &mut app,
            &pointer,
            notch(MouseEventKind::ScrollDown)
        ));
    }

    #[test]
    fn page_keys_scroll_by_the_visible_height() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "fist");
        settle(&mut app);
        let mut terminal = Terminal::new(TestBackend::new(100, 14)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);
        app.focus = Focus::Definition;
        let page = app.page;
        assert!(
            page >= 2 && page < app.max_scroll,
            "page {page} against max {}",
            app.max_scroll
        );

        stroke(&mut app, KeyCode::PageDown);
        assert_eq!(app.scroll, page);
        stroke(&mut app, KeyCode::PageDown);
        assert_eq!(app.scroll, (2 * page).min(app.max_scroll));
        stroke(&mut app, KeyCode::End);
        assert_eq!(app.scroll, app.max_scroll);
        stroke(&mut app, KeyCode::Home);
        assert_eq!(app.scroll, 0);
        stroke(&mut app, KeyCode::PageUp);
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn hints_stay_usable_after_scrolling_a_page() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "fist");
        settle(&mut app);
        let mut terminal = Terminal::new(TestBackend::new(100, 14)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);
        app.focus = Focus::Definition;
        stroke(&mut app, KeyCode::Char('f'));
        assert!(app.picking);
        assert!(app.page < app.max_scroll);

        stroke(&mut app, KeyCode::PageDown);
        assert_eq!(app.scroll, app.page, "hint mode must scroll");
        assert!(app.picking, "scrolling must not cancel hint mode");
        assert!(app.label_input.is_empty());

        // Labels are rebuilt for the newly visible lines on the next frame.
        paint(&mut app, &mut terminal, &mut pointer);
        let (label, word) = app.labels.first().cloned().expect("hints on the new page");
        for letter in label.chars() {
            stroke(&mut app, KeyCode::Char(letter));
        }
        settle(&mut app);
        assert_eq!(app.input, word);
        assert!(!app.picking);
    }

    fn candidate_row(pointer: &Pointer, index: usize) -> u16 {
        pointer.candidates.y + (index - pointer.candidates_offset) as u16
    }

    fn click_candidate(app: &mut App, pointer: &Pointer, index: usize) -> bool {
        let column = pointer.candidates.x + 1;
        on_mouse(app, pointer, click(column, candidate_row(pointer, index)))
    }

    #[test]
    fn click_candidate_selects_it_without_moving_focus() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "ho");
        settle(&mut app);
        assert!(app.results.len() > 1, "need several candidates");
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);
        assert_eq!(pointer.candidates_offset, 0);

        assert!(click_candidate(&mut app, &pointer, 1));
        assert_eq!(app.selected, 1);
        settle(&mut app);
        assert_eq!(app.input, "ho", "selection must not accept the word");
        assert_eq!(app.focus, Focus::Input);
        assert_eq!(app.preview.as_ref().unwrap().entry.key, app.results[1].key);
    }

    #[test]
    fn clicking_the_highlighted_candidate_accepts_and_focuses_definition() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "ho");
        settle(&mut app);
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);
        let expected = app.results[app.selected].headword.clone();

        assert_eq!(app.selected, 0);
        let selected = app.selected;
        assert!(click_candidate(&mut app, &pointer, selected));
        settle(&mut app);
        assert_eq!(app.input, expected);
        assert_eq!(app.focus, Focus::Definition);
        assert_eq!(app.preview.as_ref().unwrap().entry.key, expected);
    }

    #[test]
    fn candidate_clicks_follow_a_scrolled_list() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "ho");
        settle(&mut app);
        let mut terminal = Terminal::new(TestBackend::new(100, 14)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);
        let visible = pointer.candidates.height as usize;
        assert!(
            app.results.len() > visible + 1,
            "need a list longer than the pane"
        );

        // Move the selection past the pane so the list has to scroll.
        for _ in 0..visible + 1 {
            stroke(&mut app, KeyCode::Down);
        }
        settle(&mut app);
        paint(&mut app, &mut terminal, &mut pointer);
        assert!(pointer.candidates_offset > 0, "list should have scrolled");

        let target = pointer.candidates_offset + 1;
        assert!(click_candidate(&mut app, &pointer, target));
        assert_eq!(app.selected, target);
    }

    #[test]
    fn clicks_below_the_last_candidate_are_ignored() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "ho");
        settle(&mut app);
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let mut pointer = Pointer::default();
        paint(&mut app, &mut terminal, &mut pointer);
        let selected = app.selected;

        let column = pointer.candidates.x + 1;
        let row = pointer.candidates.y + app.results.len() as u16 + 1;
        assert!(!on_mouse(&mut app, &pointer, click(column, row)));
        assert_eq!(app.selected, selected);
        assert_eq!(app.input, "ho");
    }

    #[test]
    fn width_adaptive_help_footer_never_exceeds_width_and_keeps_exit_hint() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "fist");
        settle(&mut app);

        let widths = [30, 45, 60, 80, 100, 120];
        let heights = [10, 12, 24];

        for &width in &widths {
            for &height in &heights {
                let line_count = if height < 12 { 1 } else { 2 };

                // 1. Input focus
                app.focus = Focus::Input;
                app.picking = false;
                for loading in [false, true] {
                    app.loading = loading;
                    let lines = footer_help(&app, width, line_count);

                    assert!(
                        lines.len() <= line_count,
                        "Input lines count {} > available {line_count} at {width}x{height}",
                        lines.len()
                    );
                    for line in &lines {
                        assert!(
                            line.width() <= width,
                            "Input line '{line}' display width {} > {width} at {width}x{height}",
                            line.width()
                        );
                    }
                    assert!(
                        lines.iter().any(|l| l.contains("Ctrl+C quit")),
                        "Input exit hint missing at {width}x{height}: {lines:?}"
                    );
                    if width >= 120 && height >= 12 {
                        let expected = vec![
                            "Tab complete · Shift+Tab previous · Enter read · Ctrl+L focus · PgUp/PgDn or wheel scroll".to_string(),
                            format!(
                                "f follow · Ctrl+Z back ({}) · Ctrl+Y forward · Ctrl+U new · Ctrl+C quit{}",
                                app.history_len(),
                                if loading { " · loading…" } else { "" }
                            ),
                        ];
                        assert_eq!(
                            lines, expected,
                            "Input full long-form mismatch at {width}x{height}"
                        );
                    }
                }

                // 2. Definition focus
                app.focus = Focus::Definition;
                app.picking = false;
                for loading in [false, true] {
                    app.loading = loading;
                    let lines = footer_help(&app, width, line_count);

                    assert!(
                        lines.len() <= line_count,
                        "Definition lines count {} > available {line_count} at {width}x{height}",
                        lines.len()
                    );
                    for line in &lines {
                        assert!(
                            line.width() <= width,
                            "Definition line '{line}' display width {} > {width} at {width}x{height}",
                            line.width()
                        );
                    }
                    assert!(
                        lines.iter().any(|l| l.contains("Ctrl+C quit")),
                        "Definition exit hint missing at {width}x{height}: {lines:?}"
                    );
                    if width >= 120 && height >= 12 {
                        let expected = vec![
                            "Reading · PgUp/PgDn or wheel scroll · Home/End top/bottom · f follow · Esc input".to_string(),
                            format!(
                                "Ctrl+L focus · Ctrl+Z back ({}) · Ctrl+Y forward · Ctrl+C quit{}",
                                app.history_len(),
                                if loading { " · loading…" } else { "" }
                            ),
                        ];
                        assert_eq!(
                            lines, expected,
                            "Definition full long-form mismatch at {width}x{height}"
                        );
                    }
                }

                // 3. Label-picking
                app.picking = true;
                for label_input in ["", "a"] {
                    app.label_input = label_input.to_string();
                    let lines = footer_help(&app, width, line_count);

                    assert!(
                        lines.len() <= line_count,
                        "Picking lines count {} > available {line_count} at {width}x{height}",
                        lines.len()
                    );
                    for line in &lines {
                        assert!(
                            line.width() <= width,
                            "Picking line '{line}' display width {} > {width} at {width}x{height}",
                            line.width()
                        );
                    }
                    assert!(
                        lines.iter().any(|l| l.contains("Ctrl+C quit")),
                        "Picking exit hint missing at {width}x{height}: {lines:?}"
                    );
                    if width >= 120 && height >= 12 {
                        let expected = vec![format!(
                            "Label: {}_  · type both letters · PgUp/PgDn scroll · Esc cancels · Ctrl+C quit",
                            app.label_input
                        )];
                        assert_eq!(
                            lines, expected,
                            "Picking full long-form mismatch at {width}x{height}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn footer_rendering_in_terminal_fits_all_test_dimensions() {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, "fist");
        settle(&mut app);

        let widths = [30, 45, 60, 80, 100, 120];
        let heights = [10, 12, 24];

        for &width in &widths {
            for &height in &heights {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                let mut pointer = Pointer::default();
                paint(&mut app, &mut terminal, &mut pointer);

                let buffer = terminal.backend().buffer();
                let footer_height = if height < 12 { 1 } else { 2 };
                let start_y = height - footer_height;
                let mut footer_text = String::new();
                for y in start_y..height {
                    for x in 0..width {
                        footer_text.push_str(buffer[(x, y)].symbol());
                    }
                    footer_text.push('\n');
                }
                assert!(
                    footer_text.contains("Ctrl+C quit"),
                    "Terminal buffer missing exit hint at {width}x{height}:\n{footer_text}"
                );
            }
        }
    }
}
