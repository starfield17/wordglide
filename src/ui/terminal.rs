use crate::{App, Dictionary};
use anyhow::Result;
use crossterm::{
    Command,
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{fmt, io, time::Duration};

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
    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    if mouse {
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
