use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};
use wordglide::{App, Dictionary, build_pack, draw};

fn app(query: &str) -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sample");
    let pack = dir.path().join("pack");
    build_pack(
        &sample.join("entries.jsonl"),
        &sample.join("source.json"),
        &pack,
    )
    .unwrap();
    let mut app = App::new(Dictionary::open(&pack).unwrap(), query);
    settle(&mut app);
    (dir, app)
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.loading {
        app.poll();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn command_filter_and_cancel_do_not_edit_the_lookup() {
    let (_dir, mut app) = app("fist");
    let original = (
        app.input.clone(),
        app.cursor,
        app.selected,
        app.scroll,
        app.focus,
    );
    key(&mut app, KeyCode::F(3));
    app.paste("appearance");
    assert!(
        screen(&mut app, 120, 40)
            .join("\n")
            .contains("Appearance settings")
    );
    key(&mut app, KeyCode::Esc);
    assert_eq!(
        (
            app.input.clone(),
            app.cursor,
            app.selected,
            app.scroll,
            app.focus
        ),
        original
    );
    key(&mut app, KeyCode::F(3));
    app.paste("appearance");
    key(&mut app, KeyCode::Enter);
    assert!(screen(&mut app, 120, 40).join("\n").contains("Settings"));
    assert_eq!(app.input, "fist");
}

#[test]
fn help_can_reach_its_last_section_on_a_short_terminal() {
    let (_dir, mut app) = app("fist");
    key(&mut app, KeyCode::F(1));
    key(&mut app, KeyCode::End);
    let last = screen(&mut app, 80, 12).join("\n");
    assert!(last.contains("F2"), "{last}");
    assert_eq!(app.input, "fist");
    key(&mut app, KeyCode::Home);
    let first = screen(&mut app, 80, 12).join("\n");
    assert!(first.contains("Lookup"), "{first}");
    assert_ne!(first, last);
}

#[test]
fn candidate_width_is_independent_of_result_word_lengths() {
    let (_dir, mut app) = app("ho");
    let before = screen(&mut app, 120, 40)[3].clone();
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.paste("take off");
    settle(&mut app);
    let after = screen(&mut app, 120, 40)[3].clone();
    let corners = |line: &str| {
        line.chars()
            .enumerate()
            .filter(|(_, c)| *c == '╮' || *c == '╭')
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    };
    assert_eq!(corners(&before), corners(&after));
}

#[test]
fn reading_find_is_transactional_and_does_not_search_the_dictionary() {
    let (_dir, mut app) = app("set");
    key(&mut app, KeyCode::Enter);
    screen(&mut app, 120, 24);
    key(&mut app, KeyCode::PageDown);
    let original = app.scroll;
    key(&mut app, KeyCode::Char('/'));
    app.paste("divided by ability");
    let text = screen(&mut app, 120, 24).join("\n");
    assert!(text.contains("divided by ability"), "{text}");
    assert!(text.contains("Find"), "{text}");
    assert_eq!(app.input, "set");
    assert!(!app.loading);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.scroll, original);
    key(&mut app, KeyCode::Char('/'));
    app.paste("zzzz-not-present");
    assert!(screen(&mut app, 120, 24).join("\n").contains("No matches"));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.scroll, original);
}

#[test]
fn outline_and_session_navigation_restore_real_locations() {
    let (_dir, mut app) = app("set");
    key(&mut app, KeyCode::Enter);
    screen(&mut app, 120, 24);
    key(&mut app, KeyCode::Char('o'));
    assert!(screen(&mut app, 120, 24).join("\n").contains("Outline"));
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);
    screen(&mut app, 120, 24);
    assert!(app.scroll > 0);
    let position = app.scroll;
    app.jump_to("fist");
    settle(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    app.paste("set");
    assert!(
        screen(&mut app, 120, 24)
            .join("\n")
            .contains("Session navigation")
    );
    key(&mut app, KeyCode::Enter);
    screen(&mut app, 120, 24);
    assert_eq!(app.input, "set");
    assert_eq!(app.scroll, position);
    app.forward();
    assert_eq!(app.input, "fist");
}

#[test]
fn focus_layout_and_reading_preferences_are_additive() {
    use wordglide::{Focus, ReadingLayout, ReadingPreferences};
    let (_dir, mut app) = app("take");
    assert_eq!(app.reading_preferences(), ReadingPreferences::default());
    key(&mut app, KeyCode::Enter);
    screen(&mut app, 120, 24);
    key(&mut app, KeyCode::Char(']'));
    let before = screen(&mut app, 120, 24).join("\n");
    assert!(before.contains("noun"));
    key(&mut app, KeyCode::F(4));
    let focused = screen(&mut app, 120, 24).join("\n");
    assert!(!focused.contains("Candidates"));
    assert!(focused.contains("noun"));
    assert_eq!(
        app.reading_preferences().reading_layout,
        ReadingLayout::Focus
    );
    key(&mut app, KeyCode::Char('e'));
    screen(&mut app, 80, 24);
    assert!(app.reading_preferences().expand_examples);
    assert!(screen(&mut app, 80, 24).join("\n").contains("noun"));
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.focus, Focus::Input);
    assert!(screen(&mut app, 120, 24).join("\n").contains("Candidates"));
}

#[test]
fn input_moves_and_deletes_whole_unicode_graphemes() {
    let (_dir, mut app) = app("");
    app.paste("a e\u{301} 👩‍💻");
    let end = app.cursor;
    key(&mut app, KeyCode::Left);
    assert_eq!(&app.input[app.cursor..end], "👩‍💻");
    key(&mut app, KeyCode::Delete);
    assert_eq!(app.input, "a e\u{301} ");
    key(&mut app, KeyCode::Backspace);
    key(&mut app, KeyCode::Backspace);
    assert_eq!(app.input, "a ");
    assert_eq!(app.cursor, 2);
}

#[test]
fn confirmed_search_wraps_and_new_word_clears_it() {
    let (_dir, mut app) = app("set");
    key(&mut app, KeyCode::Enter);
    screen(&mut app, 80, 24);
    key(&mut app, KeyCode::Char('/'));
    app.paste("a drum kit");
    screen(&mut app, 80, 24);
    key(&mut app, KeyCode::Enter);
    let position = app.scroll;
    key(&mut app, KeyCode::Char('n'));
    assert_eq!(app.scroll, position);
    key(&mut app, KeyCode::Char('N'));
    assert_eq!(app.scroll, position);
    app.jump_to("fist");
    settle(&mut app);
    screen(&mut app, 80, 24);
    key(&mut app, KeyCode::Char('n'));
    assert_eq!(app.scroll, 0);
    assert_eq!(app.input, "fist");
}

#[test]
fn new_panels_preserve_theme_and_no_color_contracts() {
    use ratatui::style::Color;
    use wordglide::{Appearance, ThemePreset};
    let (_dir, mut app) = app("set");
    key(&mut app, KeyCode::Enter);
    for theme in ThemePreset::ALL {
        for color in [false, true] {
            app.set_appearance(Appearance {
                color_theme: theme,
                truecolor: false,
                ..Default::default()
            });
            app.set_color(color);
            for (width, height) in [(80, 24), (120, 40), (180, 48), (30, 10)] {
                for panel in [KeyCode::F(3), KeyCode::Char('o'), KeyCode::Char('/')] {
                    key(&mut app, panel);
                    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                    terminal.draw(|f| draw(f, &mut app)).unwrap();
                    for cell in terminal.backend().buffer().content() {
                        assert!(!matches!(cell.fg, Color::Rgb(..)));
                        assert!(!matches!(cell.bg, Color::Rgb(..)));
                        if !color {
                            assert_eq!(cell.fg, Color::Reset);
                            assert_eq!(cell.bg, Color::Reset);
                        }
                    }
                    key(&mut app, KeyCode::Esc);
                }
            }
        }
    }
}

#[test]
fn settings_rows_edit_reading_preferences_without_changing_lookup_or_colors() {
    use wordglide::ReadingLayout;
    let (_dir, mut app) = app("fist");
    let original = (
        app.input.clone(),
        app.selected,
        app.scroll,
        app.focus,
        app.appearance(),
    );
    key(&mut app, KeyCode::F(2));
    for _ in 0..3 {
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Right);
    assert_eq!(
        app.reading_preferences().reading_layout,
        ReadingLayout::Focus
    );
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Right);
    assert!(app.reading_preferences().expand_examples);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Right);
    assert!(app.reading_preferences().expand_ipa);
    key(&mut app, KeyCode::Esc);
    assert_eq!(
        (
            app.input.clone(),
            app.selected,
            app.scroll,
            app.focus,
            app.appearance()
        ),
        original
    );
}

#[test]
fn deleting_a_separator_keeps_the_caret_outside_joined_graphemes() {
    let (_dir, mut app) = app("");
    for deletion in [KeyCode::Backspace, KeyCode::Delete] {
        app.paste("🇦x🇧");
        key(&mut app, KeyCode::Left);
        if deletion == KeyCode::Delete {
            key(&mut app, KeyCode::Left);
        }
        key(&mut app, deletion);
        assert_eq!(app.input, "🇦🇧");
        assert_eq!(app.cursor, app.input.len());
        key(&mut app, KeyCode::Backspace);
        assert!(app.input.is_empty());
        assert_eq!(app.cursor, 0);
    }
}
