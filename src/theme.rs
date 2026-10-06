use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Theme {
    color: bool,
}

impl Theme {
    pub(crate) fn colored() -> Self {
        Self { color: true }
    }

    pub(crate) fn plain() -> Self {
        Self { color: false }
    }

    /// Headword or group heading: Cyan + BOLD when colored, BOLD when plain.
    pub(crate) fn heading(&self) -> Style {
        if self.color {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        }
    }

    /// Accent: Cyan when colored, no fg/bg when plain.
    pub(crate) fn accent(&self) -> Style {
        if self.color {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        }
    }

    /// Dim: DarkGray when colored, no fg/bg when plain.
    pub(crate) fn dim(&self) -> Style {
        if self.color {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        }
    }

    /// Example: Gray + ITALIC when colored, ITALIC when plain.
    pub(crate) fn example(&self) -> Style {
        if self.color {
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC)
        } else {
            Style::default().add_modifier(Modifier::ITALIC)
        }
    }

    /// Error: Red when colored, BOLD when plain.
    pub(crate) fn error(&self) -> Style {
        if self.color {
            Style::default().fg(Color::Red)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        }
    }

    /// Hint label: Black on Yellow + BOLD when colored, REVERSED when plain.
    pub(crate) fn hint_label(&self) -> Style {
        if self.color {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::REVERSED)
        }
    }

    /// Selected candidate highlight: Black on Cyan when colored, REVERSED when plain.
    pub(crate) fn selected(&self) -> Style {
        if self.color {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().add_modifier(Modifier::REVERSED)
        }
    }

    /// Focused border: Cyan when colored, no fg/bg when plain.
    pub(crate) fn focused_border(&self) -> Style {
        if self.color {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        }
    }

    /// Idle border: DarkGray when colored, no fg/bg when plain.
    pub(crate) fn idle_border(&self) -> Style {
        if self.color {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        }
    }

    /// Border helper selecting focused or idle border style.
    pub(crate) fn border(&self, focused: bool) -> Style {
        if focused {
            self.focused_border()
        } else {
            self.idle_border()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colored_theme_styles() {
        let theme = Theme::colored();
        assert_eq!(
            theme.heading(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        );
        assert_eq!(theme.accent(), Style::default().fg(Color::Cyan));
        assert_eq!(theme.dim(), Style::default().fg(Color::DarkGray));
        assert_eq!(
            theme.example(),
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC)
        );
        assert_eq!(theme.error(), Style::default().fg(Color::Red));
        assert_eq!(
            theme.hint_label(),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        );
        assert_eq!(
            theme.selected(),
            Style::default().fg(Color::Black).bg(Color::Cyan)
        );
        assert_eq!(theme.focused_border(), Style::default().fg(Color::Cyan));
        assert_eq!(theme.idle_border(), Style::default().fg(Color::DarkGray));
        assert_eq!(theme.border(true), theme.focused_border());
        assert_eq!(theme.border(false), theme.idle_border());
    }

    #[test]
    fn plain_theme_styles_have_no_colors() {
        let theme = Theme::plain();
        assert_eq!(
            theme.heading(),
            Style::default().add_modifier(Modifier::BOLD)
        );
        assert_eq!(theme.accent(), Style::default());
        assert_eq!(theme.dim(), Style::default());
        assert_eq!(
            theme.example(),
            Style::default().add_modifier(Modifier::ITALIC)
        );
        assert_eq!(theme.error(), Style::default().add_modifier(Modifier::BOLD));
        assert_eq!(
            theme.hint_label(),
            Style::default().add_modifier(Modifier::REVERSED)
        );
        assert_eq!(
            theme.selected(),
            Style::default().add_modifier(Modifier::REVERSED)
        );
        assert_eq!(theme.focused_border(), Style::default());
        assert_eq!(theme.idle_border(), Style::default());
        assert_eq!(theme.border(true), Style::default());
        assert_eq!(theme.border(false), Style::default());

        let styles = [
            theme.heading(),
            theme.accent(),
            theme.dim(),
            theme.example(),
            theme.error(),
            theme.hint_label(),
            theme.selected(),
            theme.focused_border(),
            theme.idle_border(),
        ];
        for style in styles {
            assert_eq!(style.fg, None);
            assert_eq!(style.bg, None);
        }
    }
}
