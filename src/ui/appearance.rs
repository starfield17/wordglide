use super::pointer::{Pointer, Region};
use crate::App;
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

pub(super) fn render_appearance(frame: &mut Frame, app: &App, pointer: &mut Pointer) {
    let screen = frame.area();
    let width = screen.width.min(68);
    let height = screen.height.min(12);
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
            " Appearance · Esc closes "
        } else {
            " Appearance · no color "
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
    ];
    for (index, text) in rows.into_iter().enumerate() {
        let row = Rect::new(inner.x, inner.y + index as u16, inner.width, 1);
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
        inner.y + 4,
        inner.width,
        inner.height.saturating_sub(6),
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
        Paragraph::new("↑/↓ select · ←/→ change\nSpace change · Enter close")
            .style(app.theme.dim()),
        help,
    );
}
