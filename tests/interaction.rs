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
    assert!(screen(&mut app, 120, 40).join("\n").contains("Appearance"));
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
