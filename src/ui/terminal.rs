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
use crate::download::{Progress, Task};
use std::io::{IsTerminal, Write};

/// Download, verify, and install the latest published dictionary in the user data directory.
/// Progress is printed to stderr. This never generates or rebuilds dictionary data.
pub fn download_data() -> Result<PathBuf> {
    download_data_with_cancel(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
        false,
    )))
}

/// Install with cooperative cancellation. Set `cancel` to true to cancel the operation.
/// The caller owns process signal handling; this function does not install signal handlers.
pub fn download_data_with_cancel(
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<PathBuf> {
    struct RawGuard(bool);
    impl Drop for RawGuard {
        fn drop(&mut self) {
            if self.0 {
                let _ = disable_raw_mode();
            }
        }
    }
    let interactive = io::stdin().is_terminal() && io::stderr().is_terminal();
    let task = Task::start_with_cancel(cancel)?;
    if interactive {
        enable_raw_mode()?;
    }
    let guard = RawGuard(interactive);
    let mut message = String::new();
    let mut last_draw = std::time::Instant::now();
    loop {
        for update in task.poll() {
            match update {
                Progress::Info(text) => {
                    message = text;
                    if !interactive {
                        eprintln!("{message}");
                    }
                }
                Progress::Bytes { downloaded, total } => {
                    message = format!(
                        "Downloading: {:.1} / {:.1} MiB · {}%",
                        downloaded as f64 / 1048576.0,
                        total as f64 / 1048576.0,
                        downloaded.saturating_mul(100) / total.max(1)
                    );
                }
                Progress::Finished(result) => {
                    drop(guard);
                    if interactive {
                        eprint!("\r\x1b[2K");
                    }
                    let installed = result.map_err(anyhow::Error::msg)?;
                    eprintln!(
                        "{} · snapshot {}\nInstalled at {}\nRun wordglide to open the dictionary.",
                        if installed.already_current {
                            "Dictionary is already up to date"
                        } else {
                            "Dictionary installed"
                        },
                        installed.snapshot,
                        installed.path.display()
                    );
                    return Ok(installed.path);
                }
            }
        }
        if interactive {
            if last_draw.elapsed() >= Duration::from_millis(100) {
                eprint!("\r\x1b[2K{message} · Esc / Ctrl+C cancel");
                io::stderr().flush()?;
                last_draw = std::time::Instant::now();
            }
            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
                && (key.code == crossterm::event::KeyCode::Esc
                    || (key.code == crossterm::event::KeyCode::Char('c')
                        && key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)))
            {
                task.cancel();
                message = "Cancelling download…".into();
            }
        } else {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
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
    run_session(App::new(dictionary, query), options)
}

/// Open Settings and lookup input even when dictionary discovery or validation failed.
pub fn run_without_dictionary(query: &str, notice: String, options: RunOptions) -> Result<()> {
    run_session(App::without_dictionary(query, notice), options)
}

fn run_session(mut app: App, options: RunOptions) -> Result<()> {
    let (mut config, appearance) =
        ConfigStore::load(options.config_path.clone(), options.appearance)?;
    app.download_enabled = crate::download::root().is_ok();
    app.set_color(options.color);
    app.mouse_enabled = options.mouse;
    app.set_appearance(appearance);
    app.set_reading_preferences(config.reading_preferences());
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
    let mut download: Option<Task> = None;
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
        if app.download_requested {
            app.download_requested = false;
            match Task::start() {
                Ok(task) => download = Some(task),
                Err(error) => {
                    app.download.message = format!("Download failed: {error:#}");
                    app.download.running = false;
                }
            }
            dirty = true;
        }
        if let Some(task) = &download {
            if app.download_cancelled {
                task.cancel();
            }
            for update in task.poll() {
                match update {
                    Progress::Info(message) => {
                        if !app.download_cancelled {
                            app.download.message = message;
                        }
                        app.download.total = 0;
                    }
                    Progress::Bytes { downloaded, total } => {
                        app.download.downloaded = downloaded;
                        app.download.total = total;
                    }
                    Progress::Finished(result) => {
                        finish_download(&mut app, result);
                    }
                }
                dirty = true;
            }
            if !app.download.running {
                download = None;
            }
        }
        dirty |= app.poll();
        if dirty {
            terminal.draw(|frame| render(frame, &mut app, &mut pointer))?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(if app.loading { 5 } else { 100 }))? {
            let before = app.appearance();
            let reading_before = app.reading_preferences();
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
            let reading_after = app.reading_preferences();
            if before != after || reading_before != reading_after {
                app.appearance_status = Some(
                    match config.save_preferences(before, after, reading_before, reading_after) {
                        Ok(true) => "Saved automatically".into(),
                        Ok(false) => "Session only: no configuration path".into(),
                        Err(error) => format!("Not saved: {error:#}"),
                    },
                );
                dirty = true;
            }
        }
    }
    terminal.show_cursor()?;
    Ok(())
}

pub(super) fn finish_download(app: &mut App, result: Result<crate::download::Installed, String>) {
    app.download.running = false;
    app.download.total = 0;
    app.download.message = match result {
        Ok(installed) => {
            let status = if app.has_dictionary() {
                "Restart Wordglide to use this dictionary."
            } else {
                match Dictionary::open(&installed.path) {
                    Ok(dictionary) => {
                        app.activate_dictionary(dictionary);
                        "Dictionary ready. Close this panel to start looking up words."
                    }
                    Err(error) => {
                        app.download.message = format!(
                            "Cannot open installed dictionary: {error:#}\nReturn to Settings to retry."
                        );
                        return;
                    }
                }
            };
            format!(
                "{} · snapshot {}\n{}\n{status} --data and WORDGLIDE_DATA overrides still take precedence on restart.",
                if installed.already_current {
                    "Already up to date"
                } else {
                    "Download complete"
                },
                installed.snapshot,
                installed.path.display()
            )
        }
        Err(error) => {
            format!("{error}\nReturn to Settings to retry. Any active dictionary was kept.")
        }
    };
}
