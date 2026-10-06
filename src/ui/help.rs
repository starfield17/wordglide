use crate::{App, Focus, theme::Theme};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn help_lines(theme: Theme) -> Vec<Line<'static>> {
    let heading = |text: &str| Line::styled(text.to_string(), theme.heading());
    let body = |text: &str| Line::styled(text.to_string(), Style::default());
    vec![
        heading("Lookup"),
        body("  Type to search · Enter accept and read · Ctrl+L switch focus"),
        body("  ↑/↓ or Ctrl+P/N select · Tab/Shift+Tab complete · → or Ctrl+F accept prediction"),
        heading("Reading"),
        body("  PgUp/PgDn or wheel scroll · Home/End top/bottom"),
        body("  f follow a visible word (type the two hint letters)"),
        body("  e examples and references: compact / full"),
        body("  p pronunciation (IPA): short / full"),
        heading("Input editing"),
        body("  Ctrl+W or Alt+Backspace delete word · Ctrl+K kill to end"),
        body("  Ctrl+←/→ word motion · Alt+←/→ history back / forward"),
        body("  Ctrl+A start · Ctrl+E end · Ctrl+U clear · Esc cancel"),
        heading("Mouse"),
        body("  Click the definition to focus; a second click on a word follows it"),
        body("  Click a candidate to preview; click it again to accept"),
        heading("Other"),
        body("  Ctrl+Z back · Ctrl+Y forward · Ctrl+C quit · ? or F1 this help"),
    ]
}

pub(super) fn render_help(frame: &mut Frame, app: &App) {
    let area = centered_rect(88, 84, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Keys · Esc closes ")
        .border_style(app.theme.focused_border());
    frame.render_widget(
        Paragraph::new(help_lines(app.theme))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
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

pub(super) fn footer_help(app: &App, width: usize, line_count: usize) -> Vec<String> {
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
                text: "e examples".into(),
                line: 0,
                order: 5,
                priority: 9,
            },
            HelpSegment {
                text: "p IPA".into(),
                line: 0,
                order: 6,
                priority: 10,
            },
            HelpSegment {
                text: "? help".into(),
                line: 0,
                order: 7,
                priority: 11,
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
                text: "F1 help".into(),
                line: 0,
                order: 5,
                priority: 10,
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
