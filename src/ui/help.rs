use super::pointer::{Pointer, Region};
use crate::{App, Focus, app::Action, theme::Theme};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
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

fn help_lines(theme: Theme, focus: Focus) -> Vec<Line<'static>> {
    let heading = |text: &str| Line::styled(text.to_string(), theme.heading());
    let body = |text: &str| {
        let (keys, description) = text.split_once(" · ").unwrap_or((text, ""));
        Line::from(vec![
            ratatui::text::Span::styled(keys.to_string(), theme.body()),
            ratatui::text::Span::styled(
                if description.is_empty() {
                    String::new()
                } else {
                    format!(" · {description}")
                },
                theme.dim(),
            ),
        ])
    };
    let mut lines = vec![
        heading("Lookup"),
        body("  Type to search · Enter accept and read · Ctrl+L switch focus"),
        body("  ↑/↓ or Ctrl+P/N select · Alt+P quick peek card"),
        body("  Tab/Shift+Tab complete · → or Ctrl+F accept prediction"),
        heading("Reading"),
        body("  PgUp/PgDn or wheel scroll · Home/End top/bottom"),
        body("  Space or Alt+P quick peek candidate definition card"),
        body("  f follow a visible word (type the two hint letters)"),
        body("  e examples and references: compact / full"),
        body("  p pronunciation (IPA): short / full"),
        body("  Ctrl+J/K scroll one line · [/] previous/next group"),
        body("  / find text · n/N next/previous match · o outline · F4 reading focus"),
        heading("Input editing"),
        body("  Ctrl+W or Alt+Backspace delete word · Ctrl+K kill to end"),
        body("  Ctrl+←/→ word motion · Alt+←/→ history back / forward"),
        body("  Ctrl+A start · Ctrl+E end · Ctrl+U clear · Esc cancel"),
        heading("Mouse"),
        body("  Click the definition to focus; a second click on a word follows it"),
        body("  Click a candidate to preview; click it again to accept"),
        heading("Other"),
        body("  Ctrl+Z back · Ctrl+Y forward · Ctrl+C quit · ? or F1 this help"),
        body("  Ctrl+G / F3 actions · Ctrl+R session navigation"),
        body("  F2 Settings: appearance, reading, dictionary download"),
    ];
    if focus == Focus::Definition {
        let start = lines
            .iter()
            .position(|line| line.spans.iter().any(|span| span.content == "Reading"))
            .unwrap_or(0);
        let end = lines
            .iter()
            .position(|line| {
                line.spans
                    .iter()
                    .any(|span| span.content == "Input editing")
            })
            .unwrap_or(start);
        let reading = lines.drain(start..end).collect::<Vec<_>>();
        lines.splice(0..0, reading);
    }
    lines
}

pub(super) fn render_help(frame: &mut Frame, app: &mut App) {
    let area = centered_rect(88, 84, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(app.theme.surface())
        .title(" Keys · Esc closes ")
        .border_style(app.theme.focused_border());
    let inner = block.inner(area);
    app.view.help_page = inner.height.saturating_sub(1).max(1) as usize;
    let lines = help_lines(app.theme, app.focus);
    let lines = super::reading::wrap(
        lines
            .into_iter()
            .map(|source| {
                let mut line = super::reading::ReadingLine::new(String::new(), source.style);
                for span in source.spans {
                    let start = line.text.len();
                    line.text.push_str(&span.content);
                    line.styles
                        .push((start..line.text.len(), source.style.patch(span.style)));
                }
                line
            })
            .collect(),
        inner.width as usize,
    );
    let count = lines.len();
    app.view.help_scroll = app
        .view
        .help_scroll
        .min(count.saturating_sub(inner.height as usize));
    frame.render_widget(
        Paragraph::new(lines.iter().map(|l| l.rendered()).collect::<Vec<_>>())
            .scroll((app.view.help_scroll.min(u16::MAX as usize) as u16, 0))
            .block(block),
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

pub(super) fn footer_actions(app: &App, width: usize) -> Vec<(&'static str, Action)> {
    let actions = if app.focus == Focus::Definition {
        vec![
            ("f follow", Action::Follow),
            ("Ctrl+G actions", Action::Commands),
            ("Esc input", Action::Focus),
            ("F1 help", Action::Help),
            ("F2 settings", Action::Settings),
        ]
    } else {
        vec![
            ("Enter read", Action::Accept),
            ("Tab complete", Action::Complete),
            ("Ctrl+G actions", Action::Commands),
            ("F1 help", Action::Help),
            ("F2 settings", Action::Settings),
        ]
    };
    let mut used = "Ctrl+C quit".width();
    let mut result = Vec::new();
    for item in actions {
        if used + 3 + item.0.width() <= width {
            used += 3 + item.0.width();
            result.push(item);
        }
    }
    result.push(("Ctrl+C quit", Action::Quit));
    result
}

pub(super) fn footer_help(app: &App, width: usize, line_count: usize) -> Vec<String> {
    if width == 0 || line_count == 0 {
        return Vec::new();
    }
    if app.picking {
        let segments = [
            (format!("Label: {}_ ", app.label_input), 1),
            ("type both letters".into(), 3),
            ("PgUp/PgDn scroll".into(), 4),
            ("Esc cancels".into(), 2),
            ("Ctrl+C quit".into(), 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(order, (text, priority))| HelpSegment {
            text,
            line: 0,
            order,
            priority,
        })
        .collect::<Vec<_>>();
        return format_footer_help(&segments, width, line_count);
    }
    let shortcuts = truncate_to_width(
        &footer_actions(app, width)
            .iter()
            .map(|(text, _)| *text)
            .collect::<Vec<_>>()
            .join(" · "),
        width,
    );
    if line_count == 1 {
        return vec![shortcuts];
    }
    let kind = app
        .results
        .get(app.selected)
        .map_or("no match", |c| match c.kind {
            crate::MatchKind::Exact => "exact",
            crate::MatchKind::Prefix => "prefix",
            crate::MatchKind::Inflection => "word form",
            crate::MatchKind::Fuzzy => "spelling suggestion",
        });
    let state = if app.focus == Focus::Input {
        "Lookup"
    } else {
        "Reading"
    };
    let status = format!(
        "{state} · {kind} · {}/{} · Back {} / Forward {}{}",
        if app.results.is_empty() {
            0
        } else {
            app.selected + 1
        },
        app.results.len(),
        app.history_len(),
        app.forward_len(),
        if app.loading { " · loading…" } else { "" }
    );
    let status = app
        .error
        .as_deref()
        .or_else(|| {
            app.appearance_status
                .as_deref()
                .filter(|s| s.starts_with("Not saved:"))
        })
        .unwrap_or(&status);
    vec![truncate_to_width(status, width), shortcuts]
}

pub(super) fn render_footer(frame: &mut Frame, app: &App, area: Rect, pointer: &mut Pointer) {
    let lines = footer_help(app, area.width as usize, area.height as usize);
    frame.render_widget(
        Paragraph::new(lines.clone().into_iter().map(Line::raw).collect::<Vec<_>>()).style(
            if app.error.is_some()
                || app
                    .appearance_status
                    .as_deref()
                    .is_some_and(|s| s.starts_with("Not saved:"))
            {
                app.theme.error()
            } else {
                app.theme.dim()
            },
        ),
        area,
    );
    if !app.picking {
        let mut x = area.x;
        let y = area.y + lines.len().saturating_sub(1) as u16;
        for (label, action) in footer_actions(app, area.width as usize) {
            let width = label.width().min(area.width as usize) as u16;
            pointer
                .footer
                .push((Region::from(Rect::new(x, y, width, 1)), action));
            x += width + 3;
        }
    }
}
