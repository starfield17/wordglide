use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Built-in reading palettes. Custom theme files are not supported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreset {
    #[default]
    Default,
    Orange,
    GruvboxLight,
    GruvboxDarkV2,
    Whiteout,
}

impl ThemePreset {
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::Orange,
        Self::GruvboxLight,
        Self::GruvboxDarkV2,
        Self::Whiteout,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Orange => "orange",
            Self::GruvboxLight => "gruvbox_light",
            Self::GruvboxDarkV2 => "gruvbox_dark_v2",
            Self::Whiteout => "whiteout",
        }
    }

    pub(crate) fn cycle(self, backwards: bool) -> Self {
        let index = Self::ALL
            .iter()
            .position(|&preset| preset == self)
            .unwrap_or(0);
        Self::ALL[(index + if backwards { Self::ALL.len() - 1 } else { 1 }) % Self::ALL.len()]
    }
}

impl fmt::Display for ThemePreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ThemePreset {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|preset| preset.as_str() == value)
            .ok_or_else(|| format!("Unknown theme {value:?}; choose default, orange, gruvbox_light, gruvbox_dark_v2, or whiteout"))
    }
}

/// Appearance preferences, independent of the color-free rendering override.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub color_theme: ThemePreset,
    pub theme_background: bool,
    pub truecolor: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            color_theme: ThemePreset::Default,
            theme_background: true,
            truecolor: true,
        }
    }
}

/// Per-session overrides applied after loading saved appearance preferences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AppearanceOverrides {
    pub color_theme: Option<ThemePreset>,
    pub theme_background: Option<bool>,
    pub truecolor: Option<bool>,
}

impl AppearanceOverrides {
    pub(crate) fn apply(self, mut appearance: Appearance) -> Appearance {
        if let Some(value) = self.color_theme {
            appearance.color_theme = value;
        }
        if let Some(value) = self.theme_background {
            appearance.theme_background = value;
        }
        if let Some(value) = self.truecolor {
            appearance.truecolor = value;
        }
        appearance
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Theme {
    pub(crate) appearance: Appearance,
    pub(crate) color: bool,
}

// Original palettes inspired by btop's named themes; reading-specific secondary
// colors deliberately have stronger contrast than btop's inactive text.
#[derive(Clone, Copy)]
struct Palette {
    background: Color,
    foreground: Color,
    accent: Color,
    secondary: Color,
    example: Color,
    selected_background: Color,
    selected_foreground: Color,
    error: Color,
}

impl Theme {
    pub(crate) fn colored() -> Self {
        Self {
            appearance: Appearance::default(),
            color: true,
        }
    }
    #[cfg(test)]
    pub(crate) fn plain() -> Self {
        Self {
            color: false,
            ..Self::colored()
        }
    }

    fn palette(self) -> Palette {
        let rgb = |value: u32| Color::Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8);
        let (bg, fg, accent, secondary, selected_bg, selected_fg, error) =
            match self.appearance.color_theme {
                ThemePreset::Default => {
                    return Palette {
                        background: Color::Reset,
                        foreground: Color::Reset,
                        accent: Color::Cyan,
                        secondary: Color::DarkGray,
                        example: Color::Gray,
                        selected_background: Color::Cyan,
                        selected_foreground: Color::Black,
                        error: Color::Red,
                    };
                }
                ThemePreset::Orange => (
                    0x000000, 0xe8d8b8, 0xffa500, 0xb69b72, 0xffa500, 0x000000, 0xff766b,
                ),
                ThemePreset::GruvboxLight => (
                    0xfbf1c7, 0x3c3836, 0x8f3f71, 0x665c54, 0xf2e5bc, 0x8f3f71, 0xb00020,
                ),
                ThemePreset::GruvboxDarkV2 => (
                    0x282828, 0xebdbb2, 0xfabd2f, 0xa89984, 0x32302f, 0xd3869b, 0xff766b,
                ),
                ThemePreset::Whiteout => (
                    0xffffff, 0x303030, 0x284d75, 0x666666, 0x15283d, 0xffffff, 0xb00020,
                ),
            };
        Palette {
            background: rgb(bg),
            foreground: rgb(fg),
            accent: rgb(accent),
            secondary: rgb(secondary),
            example: rgb(secondary),
            selected_background: rgb(selected_bg),
            selected_foreground: rgb(selected_fg),
            error: rgb(error),
        }
    }

    fn convert(self, color: Color) -> Color {
        if self.appearance.truecolor {
            color
        } else {
            indexed(color)
        }
    }

    /// Explicit Reset clears a previous theme, including unused cells and overlays.
    pub(crate) fn surface(self) -> Style {
        if !self.color {
            return Style::default().fg(Color::Reset).bg(Color::Reset);
        }
        let palette = self.palette();
        Style::default().fg(self.convert(palette.foreground)).bg(
            if self.appearance.theme_background {
                self.convert(palette.background)
            } else {
                Color::Reset
            },
        )
    }

    pub(crate) fn body(self) -> Style {
        if self.color {
            Style::default().fg(self.convert(self.palette().foreground))
        } else {
            Style::default()
        }
    }

    pub(crate) fn heading(self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }
    pub(crate) fn accent(self) -> Style {
        self.foreground(self.palette().accent)
    }
    pub(crate) fn dim(self) -> Style {
        self.foreground(self.palette().secondary)
    }
    pub(crate) fn example(self) -> Style {
        self.foreground(self.palette().example)
            .add_modifier(Modifier::ITALIC)
    }
    pub(crate) fn error(self) -> Style {
        if self.color {
            self.foreground(self.palette().error)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        }
    }
    fn foreground(self, color: Color) -> Style {
        if self.color {
            Style::default().fg(self.convert(color))
        } else {
            Style::default()
        }
    }
    pub(crate) fn hint_label(self) -> Style {
        if !self.color {
            return Style::default().add_modifier(Modifier::REVERSED);
        }
        let (fg, bg) = if self.appearance.color_theme == ThemePreset::Default {
            (Color::Black, Color::Yellow)
        } else {
            (Color::Rgb(40, 40, 40), Color::Rgb(250, 189, 47))
        };
        Style::default()
            .fg(self.convert(fg))
            .bg(self.convert(bg))
            .add_modifier(Modifier::BOLD)
    }
    pub(crate) fn selected(self) -> Style {
        if !self.color {
            return Style::default().add_modifier(Modifier::REVERSED);
        }
        let palette = self.palette();
        Style::default()
            .fg(self.convert(palette.selected_foreground))
            .bg(self.convert(palette.selected_background))
    }
    pub(crate) fn focused_border(self) -> Style {
        self.accent()
    }
    pub(crate) fn idle_border(self) -> Style {
        self.dim()
    }
    pub(crate) fn border(self, focused: bool) -> Style {
        if focused {
            self.focused_border()
        } else {
            self.idle_border()
        }
    }
}

/// Nearest color in xterm's fixed color cube and grayscale ramp. The first 16
/// entries are terminal-configurable, so RGB conversion never guesses those.
fn indexed(color: Color) -> Color {
    let Color::Rgb(r, g, b) = color else {
        return color;
    };
    let mut best = (u32::MAX, 16);
    let levels = [0, 95, 135, 175, 215, 255];
    let distance = |red: u8, green: u8, blue: u8| {
        (i32::from(r) - i32::from(red)).pow(2) as u32
            + (i32::from(g) - i32::from(green)).pow(2) as u32
            + (i32::from(b) - i32::from(blue)).pow(2) as u32
    };
    for (ri, &red) in levels.iter().enumerate() {
        for (gi, &green) in levels.iter().enumerate() {
            for (bi, &blue) in levels.iter().enumerate() {
                let d = distance(red, green, blue);
                if d < best.0 {
                    best = (d, (16 + 36 * ri + 6 * gi + bi) as u8);
                }
            }
        }
    }
    for index in 0..24u8 {
        let value = 8 + index * 10;
        let d = distance(value, value, value);
        if d < best.0 {
            best = (d, 232 + index);
        }
    }
    Color::Indexed(best.1)
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
