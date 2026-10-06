use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use local_english_dict::{App, Dictionary, Focus, build_pack, draw};
use ratatui::{Terminal, backend::TestBackend};
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

fn control(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::CONTROL));
}

fn alt(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::ALT));
}

#[test]
fn real_source_pack_supports_forms_phrases_and_no_rewrites() {
    let (_dir, mut dict) = dictionary();
    assert_eq!(dict.candidate_count(), 89);
    let went = dict.search("went").unwrap();
    assert_eq!(went[0].key, "went");
    let preview = dict.preview(&went[0]).unwrap();
    assert_eq!(preview.related[0].key, "go");
    let better = dict.search("better").unwrap();
    assert!(better.iter().any(|c| c.key == "good"));
    assert!(better.iter().any(|c| c.key == "well"));
    assert!(!better.iter().any(|c| c.key == "good and well"));
    let preview = dict.preview(&better[0]).unwrap();
    assert_eq!(
        preview
            .related
            .iter()
            .map(|e| e.key.as_str())
            .collect::<Vec<_>>(),
        vec!["good", "well"]
    );
    assert_eq!(dict.search("take off").unwrap()[0].key, "take off");
    let fist = dict.search("fist").unwrap();
    let preview = dict.preview(&fist[0]).unwrap();
    assert!(preview.entry.lemmas.is_empty());
    assert_eq!(
        preview.entry.groups[0].senses[0].glosses,
        ["A hand with the fingers clenched or curled inward."]
    );
}

#[test]
fn follow_label_and_back_restore_reading_location() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    key(&mut app, KeyCode::Enter);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    key(&mut app, KeyCode::Char('f'));
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let label = app
        .labels
        .iter()
        .find(|(_, word)| word == "hand")
        .unwrap()
        .0
        .clone();
    for letter in label.chars() {
        key(&mut app, KeyCode::Char(letter));
    }
    settle(&mut app);
    assert_eq!(app.input, "hand");
    assert_eq!(app.preview.as_ref().unwrap().entry.key, "hand");
    assert_eq!(app.history_len(), 1);
    app.back();
    assert_eq!(app.input, "fist");
    assert_eq!(app.preview.as_ref().unwrap().entry.key, "fist");
    assert_eq!(app.focus, Focus::Definition);
    assert_eq!(app.scroll, 0);
    assert_eq!(app.history_len(), 0);

    app.scroll = 5;
    app.jump_to("palm");
    settle(&mut app);
    app.jump_to("house");
    settle(&mut app);
    app.back();
    assert_eq!(app.input, "palm");
    app.back();
    assert_eq!(app.input, "fist");
    assert_eq!(app.scroll, 5);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    assert_eq!(app.scroll, 5);
}

#[test]
fn pending_query_and_candidate_selection_survive_back() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "ho");
    app.jump_to("palm");
    app.back();
    settle(&mut app);
    assert_eq!(app.input, "ho");
    assert!(!app.results.is_empty());
    key(&mut app, KeyCode::Down);
    settle(&mut app);
    let selected = app.selected;
    let preview = app.preview.as_ref().unwrap().entry.key.clone();
    app.focus = Focus::Definition;
    app.scroll = 3;
    app.jump_to("fist");
    settle(&mut app);
    app.back();
    assert_eq!(app.selected, selected);
    assert_eq!(app.preview.as_ref().unwrap().entry.key, preview);
    assert_eq!(app.scroll, 3);
    assert_eq!(app.focus, Focus::Definition);
}

#[test]
fn rapid_input_never_publishes_old_preview() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "");
    for c in "house".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    settle(&mut app);
    assert_eq!(app.results[0].key, "house");
    assert_eq!(app.preview.as_ref().unwrap().entry.key, "house");
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.paste("hosue");
    settle(&mut app);
    assert_eq!(app.preview.as_ref().unwrap().entry.key, "house");
}

#[test]
fn definition_focus_ignores_plain_letters_and_input_shortcuts() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    app.focus = Focus::Definition;
    app.cursor = 2;

    for code in [
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::Char('x'),
        KeyCode::Backspace,
    ] {
        key(&mut app, code);
    }
    for code in [
        KeyCode::Char('u'),
        KeyCode::Char('a'),
        KeyCode::Char('e'),
        KeyCode::Char('f'),
        KeyCode::Char('w'),
        KeyCode::Char('k'),
        KeyCode::Left,
        KeyCode::Right,
    ] {
        control(&mut app, code);
    }
    alt(&mut app, KeyCode::Backspace);
    alt(&mut app, KeyCode::Right);

    assert_eq!(app.input, "fist");
    assert_eq!(app.cursor, 2);
    assert_eq!(app.focus, Focus::Definition);
    assert_eq!(app.scroll, 0);
    assert!(!app.picking);
}

#[test]
fn word_level_editing_and_alt_right_history() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "take off");
    settle(&mut app);
    assert_eq!(app.cursor, 8);

    control(&mut app, KeyCode::Char('w'));
    settle(&mut app);
    assert_eq!(app.input, "take ");
    assert_eq!(app.cursor, 5);
    assert_eq!(app.preview.as_ref().unwrap().entry.key, "take");

    app.paste("off");
    settle(&mut app);
    assert_eq!(app.input, "take off");

    control(&mut app, KeyCode::Left);
    assert_eq!(
        app.cursor, 5,
        "Ctrl+Left crosses the space to the word start"
    );
    control(&mut app, KeyCode::Right);
    assert_eq!(
        app.cursor, 8,
        "Ctrl+Right crosses the space to the next word"
    );

    control(&mut app, KeyCode::Left);
    control(&mut app, KeyCode::Char('k'));
    settle(&mut app);
    assert_eq!(app.input, "take ");

    app.paste("off");
    settle(&mut app);
    assert_eq!(app.input, "take off");
    alt(&mut app, KeyCode::Backspace);
    settle(&mut app);
    assert_eq!(app.input, "take ");

    app.jump_to("hand");
    settle(&mut app);
    assert_eq!(app.input, "hand");
    alt(&mut app, KeyCode::Left);
    settle(&mut app);
    assert_eq!(app.input, "take ");
    alt(&mut app, KeyCode::Right);
    settle(&mut app);
    assert_eq!(app.input, "hand");
}

#[test]
fn smallest_supported_terminal_has_definition_and_resize_clamps_scroll() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    let mut small = Terminal::new(TestBackend::new(30, 10)).unwrap();
    small.draw(|frame| draw(frame, &mut app)).unwrap();
    let text = small
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("fist"));
    assert!(text.contains("noun"), "definition body is missing: {text}");
    app.scroll = usize::MAX;
    small.draw(|frame| draw(frame, &mut app)).unwrap();
    assert!(app.scroll < usize::MAX);
    let mut wide = Terminal::new(TestBackend::new(120, 40)).unwrap();
    wide.draw(|frame| draw(frame, &mut app)).unwrap();
}

#[test]
fn ctrl_z_and_ctrl_y_walk_follow_history_both_ways() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);

    app.jump_to("hand");
    settle(&mut app);
    app.jump_to("palm");
    settle(&mut app);
    assert_eq!(app.input, "palm");
    assert_eq!(app.history_len(), 2);

    control(&mut app, KeyCode::Char('z'));
    assert_eq!(app.input, "hand", "Ctrl+Z steps back one followed word");
    control(&mut app, KeyCode::Char('z'));
    assert_eq!(app.input, "fist");
    assert_eq!(app.history_len(), 0);

    control(&mut app, KeyCode::Char('y'));
    settle(&mut app);
    assert_eq!(app.input, "hand", "Ctrl+Y undoes the last back");
    control(&mut app, KeyCode::Char('y'));
    settle(&mut app);
    assert_eq!(app.input, "palm");
    assert_eq!(app.history_len(), 2);

    control(&mut app, KeyCode::Char('y'));
    assert_eq!(app.input, "palm", "nothing ahead to restore");
}

#[test]
fn new_navigation_and_typing_clear_the_forward_stack() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    app.jump_to("hand");
    settle(&mut app);
    app.jump_to("palm");
    settle(&mut app);

    app.back();
    app.back();
    assert_eq!(app.input, "fist");
    app.jump_to("house");
    settle(&mut app);
    app.forward();
    assert_eq!(app.input, "house", "a follow drops the replaced states");

    app.back();
    assert_eq!(app.input, "fist");
    key(&mut app, KeyCode::Char('x'));
    app.forward();
    assert_eq!(app.input, "fistx", "editing drops the replaced states");
}
