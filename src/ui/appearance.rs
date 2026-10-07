use super::pointer::{Pointer, Region};
use crate::App;
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

pub(super) fn render_download(frame: &mut Frame, app: &App, pointer: &mut Pointer) {
    let area = super::panels::panel_area(frame.area(), 78, 12);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Dictionary download ")
        .style(app.theme.surface())
        .border_style(app.theme.focused_border());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines: Vec<Line<'_>> = app
        .download
        .message
        .lines()
        .map(|line| Line::styled(line, app.theme.body()))
        .collect();
    if app.download.total > 0 {
        lines.push(Line::styled(
            format!(
                "{:.1} / {:.1} MiB · {}%",
                app.download.downloaded as f64 / 1048576.0,
                app.download.total as f64 / 1048576.0,
                app.download.downloaded.saturating_mul(100) / app.download.total
            ),
            app.theme.heading(),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(2),
        ),
    );
    let button = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
    pointer.download_button = Region::from(button);
    frame.render_widget(
        Paragraph::new(if app.download.running {
            "Esc cancel · Ctrl+C quit"
        } else {
            "Enter / Esc return to Settings"
        })
        .style(app.theme.heading()),
        button,
    );
}

pub(super) fn render_appearance(frame: &mut Frame, app: &App, pointer: &mut Pointer) {
    let screen = frame.area();
    let width = screen.width.min(68);
    let height = screen.height.min(16);
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(if app.theme.color {
            " Settings · Esc closes "
        } else {
            " Settings · no color "
        })
        .style(app.theme.surface())
        .border_style(app.theme.focused_border());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let appearance = app.appearance();
    let rows = [
        format!("Theme: {}", appearance.color_theme),
        format!(
            "Theme background: {}",
            if appearance.theme_background {
                "on"
            } else {
                "off"
            }
        ),
        format!(
            "Truecolor: {}",
            if appearance.truecolor {
                "on"
            } else {
                "off (256 colors)"
            }
        ),
        format!(
            "Reading layout: {}",
            if app.view.reading_layout == crate::ReadingLayout::Split {
                "split"
            } else {
                "focus"
            }
        ),
        format!(
            "Examples / references: {}",
            if app.view.expand_examples {
                "full"
            } else {
                "compact"
            }
        ),
        format!(
            "Pronunciation (IPA): {}",
            if app.view.expand_ipa { "full" } else { "short" }
        ),
        "Download / update dictionary…".to_string(),
    ];
    let list_height = inner.height.saturating_sub(3).clamp(1, 7);
    let offset = app
        .view
        .appearance_row
        .saturating_sub(list_height.saturating_sub(1) as usize);
    for (index, text) in rows
        .into_iter()
        .enumerate()
        .skip(offset)
        .take(list_height as usize)
    {
        let row = Rect::new(inner.x, inner.y + (index - offset) as u16, inner.width, 1);
        pointer.appearance_rows[index] = Region::from(row);
        let style = if index == app.view.appearance_row {
            app.theme.selected()
        } else {
            app.theme.body()
        };
        frame.render_widget(Paragraph::new(text).style(style), row);
    }
    let message = if !app.theme.color {
        "No color this session"
    } else if !appearance.theme_background {
        "Terminal background; selection stays highlighted"
    } else {
        "Changes apply and save immediately"
    };
    let status = app
        .appearance_status
        .as_deref()
        .unwrap_or("Change a setting to save it");
    let status_style = if status.starts_with("Not saved:") {
        app.theme.error()
    } else {
        app.theme.dim()
    };
    let details = Rect::new(
        inner.x,
        inner.y + list_height,
        inner.width,
        inner.height.saturating_sub(list_height + 2),
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(status, status_style),
            Line::styled(message, app.theme.dim()),
        ])
        .wrap(Wrap { trim: false }),
        details,
    );
    let help = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(2),
        inner.width,
        2,
    );
    frame.render_widget(
        Paragraph::new(if app.view.appearance_row == 6 {
            "Enter / Space download\nEsc close"
        } else {
            "↑/↓ select · ←/→ change\nSpace change · Enter close"
        })
        .style(app.theme.dim()),
        help,
    );
}
