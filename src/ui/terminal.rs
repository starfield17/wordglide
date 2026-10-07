use crate::{App, AppearanceOverrides, Dictionary, config::ConfigStore};
use anyhow::Result;
use crossterm::{
    Command,
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{fmt, io, path::PathBuf, time::Duration};

use super::pointer::{Pointer, on_mouse};
use super::render;

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

pub fn run(dictionary: Dictionary, query: &str, color: bool, mouse: bool) -> Result<()> {
    run_with_options(
        dictionary,
        query,
        RunOptions {
            color,
            mouse,
            ..RunOptions::default()
        },
    )
}

/// Terminal session options. Without a configuration path, changes are session-only.
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub color: bool,
    pub mouse: bool,
    pub appearance: AppearanceOverrides,
    pub config_path: Option<PathBuf>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            color: true,
            mouse: true,
            appearance: AppearanceOverrides::default(),
            config_path: None,
        }
    }
}

/// Load preferences before entering raw mode, then persist panel changes.
/// Session overrides never write configuration merely by starting the program.
pub fn run_with_options(dictionary: Dictionary, query: &str, options: RunOptions) -> Result<()> {
    let (mut config, appearance) =
        ConfigStore::load(options.config_path.clone(), options.appearance)?;
    let mut app = App::new(dictionary, query);
    app.set_color(options.color);
    app.mouse_enabled = options.mouse;
    app.set_appearance(appearance);
    if options.config_path.is_none() {
        app.appearance_status = Some("Session only: no configuration path".into());
    }
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
    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    if options.mouse {
        execute!(io::stdout(), MOUSE_ON)?;
    }
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut pointer = Pointer::default();
    let mut dirty = true;
    while !app.exit {
        dirty |= app.poll();
        if dirty {
            terminal.draw(|frame| render(frame, &mut app, &mut pointer))?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(if app.loading { 5 } else { 100 }))? {
            let before = app.appearance();
            let mouse_before = app.mouse_enabled;
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
            if mouse_before != app.mouse_enabled {
                execute!(
                    io::stdout(),
                    MouseTracking {
                        enabled: app.mouse_enabled
                    }
                )?;
            }
            let after = app.appearance();
            if before != after {
                app.appearance_status = Some(match config.save_change(before, after) {
                    Ok(true) => "Saved automatically".into(),
                    Ok(false) => "Session only: no configuration path".into(),
                    Err(error) => format!("Not saved: {error:#}"),
                });
                dirty = true;
            }
        }
    }
    terminal.show_cursor()?;
    Ok(())
}
