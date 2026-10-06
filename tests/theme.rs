use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use local_english_dict::{App, Dictionary, Focus, build_pack, draw};
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

fn dictionary() -> (tempfile::TempDir, Dictionary) {
    let dir = tempfile::tempdir().unwrap();
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sample");
    let output = dir.path().join("pack");
    build_pack(
        &sample.join("entries.jsonl"),
        &sample.join("source.json"),
        &output,
    )
    .unwrap();
    let dict = Dictionary::open(&output).unwrap();
    (dir, dict)
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        app.poll();
        if !app.loading {
            break;
        }
        assert!(Instant::now() < deadline, "worker did not finish");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(app.error.is_none(), "{:?}", app.error);
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn assert_no_disallowed_colors(terminal: &Terminal<TestBackend>) {
    let disallowed = [
        Color::Cyan,
        Color::Yellow,
        Color::Red,
        Color::Gray,
        Color::DarkGray,
        Color::Black,
    ];
    let buffer = terminal.backend().buffer();
    for (idx, cell) in buffer.content().iter().enumerate() {
        let x = idx as u16 % buffer.area.width;
        let y = idx as u16 / buffer.area.width;
        assert!(
            !disallowed.contains(&cell.fg),
            "cell at ({x}, {y}) with symbol {:?} has disallowed fg: {:?}",
            cell.symbol(),
            cell.fg
        );
        assert!(
            !disallowed.contains(&cell.bg),
            "cell at ({x}, {y}) with symbol {:?} has disallowed bg: {:?}",
            cell.symbol(),
            cell.bg
        );
    }
}

fn definition_border_x(terminal: &Terminal<TestBackend>) -> u16 {
    let buffer = terminal.backend().buffer();
    let mut count = 0;
    for x in 0..buffer.area.width {
        if buffer[(x, 3)].symbol() == "┌" {
            count += 1;
            if count == 2 {
                return x;
            }
        }
    }
    panic!("second '┌' on border row 3 not found");
}

#[test]
fn colored_app_uses_expected_accent_and_dim_colors() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);

    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buffer = terminal.backend().buffer();

    // Input border has Cyan accent when focused.
    assert_eq!(buffer[(0, 0)].fg, Color::Cyan);

    // Idle border of definition pane has DarkGray.
    let def_border_x = definition_border_x(&terminal);
    assert_eq!(buffer[(def_border_x, 3)].fg, Color::DarkGray);

    // Candidate highlight row has Black on Cyan.
    let mut found_selected = false;
    for y in 4..25 {
        for x in 1..29 {
            let cell = &buffer[(x, y)];
            if cell.fg == Color::Black && cell.bg == Color::Cyan {
                found_selected = true;
                break;
            }
        }
    }
    assert!(
        found_selected,
        "selected candidate cell with Black on Cyan not found"
    );

    // Definition heading has Cyan.
    let heading_cell = buffer
        .content()
        .iter()
        .find(|c| c.fg == Color::Cyan && c.symbol() == "f");
    assert!(heading_cell.is_some(), "Cyan heading cell not found");

    // Definition example has Gray.
    let example_cell = buffer.content().iter().find(|c| c.fg == Color::Gray);
    assert!(example_cell.is_some(), "Gray example cell not found");

    // Footer help has DarkGray.
    let footer_cell = &buffer[(0, 39)];
    assert_eq!(footer_cell.fg, Color::DarkGray);

    // Enter definition mode: focused border changes to Cyan.
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.focus, Focus::Definition);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buffer = terminal.backend().buffer();
    let def_border_x = definition_border_x(&terminal);
    assert_eq!(buffer[(def_border_x, 3)].fg, Color::Cyan);

    // Hint picking mode: labels use Black on Yellow with BOLD.
    key(&mut app, KeyCode::Char('f'));
    assert!(app.picking);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buffer = terminal.backend().buffer();
    let found_hint = buffer.content().iter().any(|c| {
        c.fg == Color::Black && c.bg == Color::Yellow && c.modifier.contains(Modifier::BOLD)
    });
    assert!(
        found_hint,
        "hint label with Black on Yellow + BOLD not found"
    );
}

#[test]
fn colored_app_ghost_suffix_has_dim_color() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fis");
    settle(&mut app);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buffer = terminal.backend().buffer();

    assert_eq!(buffer[(4, 1)].symbol(), "t");
    assert_eq!(buffer[(4, 1)].fg, Color::DarkGray);
}

#[test]
fn plain_app_rendered_frames_contain_no_colors() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    app.set_plain();

    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();

    // Frame 1: Input focus with candidate list and definition.
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert_no_disallowed_colors(&terminal);

    // Check that selected candidate uses REVERSED modifier without color.
    let buffer = terminal.backend().buffer();
    let reversed_cells: Vec<_> = buffer
        .content()
        .iter()
        .filter(|c| c.modifier.contains(Modifier::REVERSED))
        .collect();
    assert!(
        !reversed_cells.is_empty(),
        "expected REVERSED candidate cells in plain mode"
    );

    // Frame 2: Definition focus.
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.focus, Focus::Definition);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert_no_disallowed_colors(&terminal);

    // Frame 3: Hint mode ('f') with labels.
    key(&mut app, KeyCode::Char('f'));
    assert!(app.picking);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert_no_disallowed_colors(&terminal);
    let buffer = terminal.backend().buffer();
    let reversed_hint: Vec<_> = buffer
        .content()
        .iter()
        .filter(|c| c.modifier.contains(Modifier::REVERSED))
        .collect();
    assert!(
        !reversed_hint.is_empty(),
        "expected REVERSED hint cells in plain mode"
    );

    // Frame 4: Ghost suffix in plain mode.
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.focus, Focus::Input);
    key(&mut app, KeyCode::Backspace);
    settle(&mut app);
    assert_eq!(app.input, "fis");
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert_no_disallowed_colors(&terminal);
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(4, 1)].symbol(), "t");
    assert_eq!(buffer[(4, 1)].fg, Color::Reset);

    // Frame 5: Error state.
    app.error = Some("Simulated error line".into());
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert_no_disallowed_colors(&terminal);
    let buffer = terminal.backend().buffer();
    let bold_cells: Vec<_> = buffer
        .content()
        .iter()
        .filter(|c| c.modifier.contains(Modifier::BOLD) && c.symbol() == "S")
        .collect();
    assert!(
        !bold_cells.is_empty(),
        "expected BOLD modifier for error in plain mode"
    );
}
