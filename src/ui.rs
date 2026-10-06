use crate::{App, Dictionary, Entry, Focus, MatchKind};
use anyhow::Result;
use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::{collections::HashMap, io, time::Duration};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const ACCENT: Color = Color::Cyan;

#[derive(Clone)]
struct ReadingLine {
    text: String,
    style: Style,
}

fn append_entry(lines: &mut Vec<ReadingLine>, entry: &Entry, related: bool) {
    if related {
        lines.push(ReadingLine {
            text: format!("→ {}", entry.headword),
            style: Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
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
                style: Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
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
                        style: Style::default()
                            .fg(Color::Gray)
                            .add_modifier(Modifier::ITALIC),
                    });
                    if !example.reference.is_empty() {
                        lines.push(ReadingLine {
                            text: format!("     — {}", example.reference),
                            style: Style::default().fg(Color::DarkGray),
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
            style: Style::default().fg(Color::Red),
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
            style: Style::default().fg(Color::DarkGray),
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
            style: Style::default().fg(ACCENT),
        });
        for related in &preview.related {
            append_entry(&mut lines, related, true);
        }
        lines.push(ReadingLine {
            text: format!("Original form: {}", preview.entry.headword),
            style: Style::default().fg(Color::DarkGray),
        });
    }
    append_entry(&mut lines, &preview.entry, false);
    lines.push(ReadingLine {
        text: format!("Source: {}", preview.entry.source_url),
        style: Style::default().fg(Color::DarkGray),
    });
    lines
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
        let mut current = String::new();
        let mut columns = 0;
        for token in line.text.split_word_bounds() {
            let n = token.width();
            if columns + n > width && columns > 0 {
                result.push(ReadingLine {
                    text: current,
                    style: line.style,
                });
                current = String::new();
                columns = 0;
            }
            for grapheme in token.graphemes(true) {
                let n = grapheme.width();
                if columns + n > width && columns > 0 {
                    result.push(ReadingLine {
                        text: current,
                        style: line.style,
                    });
                    current = String::new();
                    columns = 0;
                }
                current.push_str(grapheme);
                columns += n;
            }
        }
        result.push(ReadingLine {
            text: current,
            style: line.style,
        });
    }
    result
}

fn label_line(line: &ReadingLine, map: &HashMap<String, String>) -> Line<'static> {
    let mut spans = vec![];
    for token in line.text.split_word_bounds() {
        if let Some(label) = map.get(&crate::normalize(token)) {
            // Replace the first two display columns with the hint (single-letter words use one).
            let graphemes: Vec<_> = token.graphemes(true).collect();
            let count = graphemes.len().min(2);
            let hint = label.chars().take(count).collect::<String>();
            spans.push(Span::styled(
                hint,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(graphemes[count..].concat(), line.style));
        } else {
            spans.push(Span::styled(token.to_string(), line.style));
        }
    }
    Line::from(spans)
}

pub fn draw(frame: &mut Frame, app: &mut App) {
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
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(" English · local ")
        .border_style(Style::default().fg(if app.focus == Focus::Input {
            ACCENT
        } else {
            Color::DarkGray
        }));
    let available = rows[0].width.saturating_sub(3) as usize;
    let before = &app.input[..app.cursor];
    let mut start = 0;
    while before[start..].width() > available.saturating_sub(1) {
        start += before[start..].chars().next().map_or(0, char::len_utf8);
    }
    frame.render_widget(
        Paragraph::new(app.input[start..].to_string()).block(input_block),
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
    render_candidates(frame, app, panes[0]);
    render_definition(frame, app, panes[1]);
    let help = if app.picking {
        format!(
            "Label: {}_  · type both letters · Esc cancels",
            app.label_input
        )
    } else {
        format!(
            "↑↓ candidates · PgUp/Dn scroll · Tab focus · f follow · Ctrl+O back ({})\nCtrl+U new lookup · Ctrl+C quit{}",
            app.history_len(),
            if app.loading { " · loading…" } else { "" }
        )
    };
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::DarkGray)),
        rows[2],
    );
}

fn render_candidates(frame: &mut Frame, app: &App, area: Rect) {
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
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Candidates · {} ", app.results.len()))
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().fg(Color::Black).bg(ACCENT))
        .highlight_symbol("› ");
    let mut state = ListState::default();
    if !app.results.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_definition(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Definition ")
        .border_style(Style::default().fg(if app.focus == Focus::Definition {
            ACCENT
        } else {
            Color::DarkGray
        }));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = wrap(reading_lines(app), inner.width as usize);
    app.max_scroll = lines.len().saturating_sub(inner.height as usize);
    app.scroll = app.scroll.min(app.max_scroll);
    let visible: Vec<_> = lines
        .iter()
        .skip(app.scroll)
        .take(inner.height as usize)
        .collect();
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
                label_line(l, &map)
            } else {
                Line::styled(l.text.clone(), l.style)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(rendered), inner);
}

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen);
    }
}

pub fn run(dictionary: Dictionary, query: &str) -> Result<()> {
    let mut app = App::new(dictionary, query);
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen);
        old_hook(info);
    }));
    enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut dirty = true;
    while !app.exit {
        dirty |= app.poll();
        if dirty {
            terminal.draw(|frame| draw(frame, &mut app))?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(5))? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    app.handle_key(key);
                    dirty = true;
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
}
