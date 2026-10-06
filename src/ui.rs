use crate::{App, Dictionary, Entry, Focus, MatchKind};
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
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::{collections::HashMap, fmt, io, time::Duration};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const ACCENT: Color = Color::Cyan;

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
        Paragraph::new(Line::from(vec![
            Span::raw(app.input[start..].to_string()),
            Span::styled(
                app.inline_suffix().unwrap_or_default(),
                Style::default().fg(Color::DarkGray),
            ),
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
    let help = if app.picking {
        format!(
            "Label: {}_  · type both letters · PgUp/PgDn scroll · Esc cancels",
            app.label_input
        )
    } else if app.focus == Focus::Definition {
        format!(
            "Reading · PgUp/PgDn or wheel scroll · Home/End top/bottom · f follow · Esc input\nCtrl+L focus · Ctrl+O back ({}) · Ctrl+C quit{}",
            app.history_len(),
            if app.loading { " · loading…" } else { "" }
        )
    } else {
        format!(
            "Tab complete · Shift+Tab previous · Enter read · Ctrl+L focus · PgUp/PgDn or wheel scroll\nf follow · Ctrl+O back ({}) · Ctrl+U new · Ctrl+C quit{}",
            app.history_len(),
            if app.loading { " · loading…" } else { "" }
        )
    };
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::DarkGray)),
        rows[2],
    );
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
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().fg(Color::Black).bg(ACCENT))
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
        .border_style(Style::default().fg(if app.focus == Focus::Definition {
            ACCENT
        } else {
            Color::DarkGray
        }));
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
                label_line(l, &map)
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

pub fn run(dictionary: Dictionary, query: &str) -> Result<()> {
    let mut app = App::new(dictionary, query);
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
}
