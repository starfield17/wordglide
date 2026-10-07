use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier},
};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};
use wordglide::{App, Appearance, Dictionary, ThemePreset, build_pack, draw};

fn app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sample");
    let pack = dir.path().join("pack");
    build_pack(
        &sample.join("entries.jsonl"),
        &sample.join("source.json"),
        &pack,
    )
    .unwrap();
    let mut app = App::new(Dictionary::open(&pack).unwrap(), "fist");
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.loading {
        app.poll();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    (dir, app)
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn presets_cover_full_frames_and_color_free_rendering() {
    let (_dir, mut app) = app();
    for preset in ThemePreset::ALL {
        for background in [false, true] {
            for truecolor in [false, true] {
                app.set_appearance(Appearance {
                    color_theme: preset,
                    theme_background: background,
                    truecolor,
                });
                app.set_color(true);
                app.focus = wordglide::Focus::Definition;
                key(&mut app, KeyCode::Char('f'));
                app.error = Some("Simulated preview error".into());
                app.loading = true;
                for (width, height) in [(120, 40), (60, 24), (30, 10), (20, 8)] {
                    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                    for overlay in [None, Some(KeyCode::F(1)), Some(KeyCode::F(2))] {
                        if let Some(code) = overlay {
                            key(&mut app, code);
                        }
                        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
                        for cell in terminal.backend().buffer().content() {
                            if !truecolor {
                                assert!(!matches!(cell.fg, Color::Rgb(..)));
                                assert!(!matches!(cell.bg, Color::Rgb(..)));
                            }
                        }
                        if overlay.is_some() {
                            key(&mut app, KeyCode::Esc);
                        }
                    }
                }
                app.set_plain();
                let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
                for code in [KeyCode::F(1), KeyCode::F(2)] {
                    key(&mut app, code);
                    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
                    for cell in terminal.backend().buffer().content() {
                        assert_eq!(cell.fg, Color::Reset);
                        assert_eq!(cell.bg, Color::Reset);
                    }
                    key(&mut app, KeyCode::Esc);
                }
                assert_eq!(app.appearance().color_theme, preset);
                app.picking = false;
                app.loading = false;
                app.error = None;
                app.focus = wordglide::Focus::Input;
            }
        }
    }
}

#[test]
fn appearance_panel_changes_settings_without_editing_lookup() {
    let (_dir, mut app) = app();
    let original = (
        app.input.clone(),
        app.cursor,
        app.selected,
        app.focus,
        app.scroll,
        app.history_len(),
    );
    key(&mut app, KeyCode::F(2));
    key(&mut app, KeyCode::Char('x'));
    key(&mut app, KeyCode::Right);
    assert_eq!(app.appearance().color_theme, ThemePreset::Orange);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char(' '));
    assert!(!app.appearance().theme_background);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Left);
    assert!(!app.appearance().truecolor);
    key(&mut app, KeyCode::Esc);
    assert_eq!(
        (
            app.input.clone(),
            app.cursor,
            app.selected,
            app.focus,
            app.scroll,
            app.history_len()
        ),
        original
    );
    assert_eq!(app.appearance().color_theme, ThemePreset::Orange);
}

#[test]
fn switching_from_light_to_transparent_and_plain_clears_old_backgrounds() {
    let (_dir, mut app) = app();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    app.set_appearance(Appearance {
        color_theme: ThemePreset::Whiteout,
        ..Appearance::default()
    });
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(119, 39)].bg,
        Color::Rgb(255, 255, 255)
    );
    app.set_appearance(Appearance {
        theme_background: false,
        ..app.appearance()
    });
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    assert_eq!(terminal.backend().buffer()[(119, 39)].bg, Color::Reset);
    assert!(
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.bg == Color::Rgb(21, 40, 61))
    );
    app.set_plain();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.modifier.contains(Modifier::REVERSED))
    );
    for cell in terminal.backend().buffer().content() {
        assert_eq!(cell.fg, Color::Reset);
        assert_eq!(cell.bg, Color::Reset);
    }
}

#[test]
fn headword_and_ipa_have_distinct_reading_styles() {
    let (_dir, mut app) = app();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let buffer = terminal.backend().buffer();
    let row = (4..35)
        .find(|&y| {
            (0..116).any(|x| {
                buffer[(x, y)].symbol() == "f"
                    && buffer[(x, y)].fg == Color::Cyan
                    && buffer[(x + 1, y)].symbol() == "i"
                    && buffer[(x + 2, y)].symbol() == "s"
            })
        })
        .unwrap();
    assert!(
        buffer
            .content()
            .iter()
            .any(|cell| cell.fg == Color::Cyan && cell.modifier.contains(Modifier::BOLD))
    );
    let slash = (0..120)
        .find(|&x| buffer[(x, row)].symbol() == "/")
        .unwrap();
    assert_eq!(buffer[(slash, row)].fg, Color::DarkGray);
    assert!(!buffer[(slash, row)].modifier.contains(Modifier::BOLD));
}
