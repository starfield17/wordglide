use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use local_english_dict::{App, Dictionary, Focus, build_pack, draw};
use ratatui::{Terminal, backend::TestBackend, style::Color};
use std::{
    fs, thread,
    time::{Duration, Instant},
};

fn app(words: &[(&str, i32)], query: &str) -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let mut template: serde_json::Value = serde_json::from_str(
        include_str!("../examples/sample/entries.jsonl")
            .lines()
            .find(|line| line.contains("\"key\":\"fist\""))
            .unwrap(),
    )
    .unwrap();
    let mut lines = String::new();
    for (word, score) in words {
        template["key"] = (*word).into();
        template["headword"] = (*word).into();
        template["score"] = (*score).into();
        lines.push_str(&serde_json::to_string(&template).unwrap());
        lines.push('\n');
    }
    fs::write(dir.path().join("entries"), lines).unwrap();
    fs::write(dir.path().join("source"), "{}").unwrap();
    let pack = dir.path().join("pack");
    build_pack(
        &dir.path().join("entries"),
        &dir.path().join("source"),
        &pack,
    )
    .unwrap();
    let dict = Dictionary::open(&pack).unwrap();
    (dir, App::new(dict, query))
}

fn settle(app: &mut App) {
    let until = Instant::now() + Duration::from_secs(3);
    while app.loading {
        app.poll();
        assert!(Instant::now() < until, "worker timeout");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(app.error.is_none(), "{:?}", app.error);
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn pending_tab_expands_common_prefix_then_cycles_and_esc_restores() {
    let (_dir, mut app) = app(&[("abcde", 100), ("abcdf", 90)], "ab");
    key(&mut app, KeyCode::Tab);
    settle(&mut app);
    assert_eq!(app.input, "abcd");
    let original = app.results.clone();
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.input, "abcde");
    key(&mut app, KeyCode::Tab);
    settle(&mut app);
    assert_eq!(app.input, "abcdf");
    assert_eq!(app.preview.as_ref().unwrap().entry.key, "abcdf");
    assert_eq!(app.results, original);
    key(&mut app, KeyCode::BackTab);
    settle(&mut app);
    assert_eq!(app.input, "abcde");
    key(&mut app, KeyCode::Esc);
    settle(&mut app);
    assert_eq!(app.input, "ab");
    assert_eq!(app.focus, Focus::Input);
}

#[test]
fn common_prefix_uses_unshown_vocabulary_and_reverse_cycle_wraps() {
    let names: Vec<_> = (0..25).map(|i| format!("prea{i:02}")).collect();
    let mut words: Vec<_> = names.iter().map(|s| (s.as_str(), 100)).collect();
    words.push(("prez", -100));
    let (_dir, mut app) = app(&words, "pre");
    settle(&mut app);
    assert_eq!(app.results.len(), 20);
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.input, "pre");
    key(&mut app, KeyCode::BackTab);
    assert_eq!(app.input, app.results.last().unwrap().headword);
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.input, app.results[0].headword);
}

#[test]
fn inline_suggestion_accepts_only_at_end_and_enter_enters_reading() {
    let (_dir, mut app) = app(&[("home", 100), ("house", 90)], "ho");
    settle(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(3, 1)].symbol(), "m");
    assert_eq!(buffer[(3, 1)].fg, Color::DarkGray);
    assert_eq!(app.input, "ho");
    key(&mut app, KeyCode::Left);
    key(&mut app, KeyCode::Right);
    assert_eq!(app.input, "ho");
    key(&mut app, KeyCode::Right);
    settle(&mut app);
    assert_eq!(app.input, "home");
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.focus, Focus::Definition);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.focus, Focus::Input);
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.paste("ho");
    settle(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
    settle(&mut app);
    assert_eq!(app.input, "home");
}

#[test]
fn phrase_fuzzy_unique_and_typing_after_completion() {
    let (_dir, mut phrase) = app(&[("take off", 100)], "take o");
    settle(&mut phrase);
    key(&mut phrase, KeyCode::Tab);
    assert_eq!(phrase.input, "take off");
    key(&mut phrase, KeyCode::Esc);
    assert_eq!(phrase.input, "take o");
    let (_dir, mut fuzzy) = app(&[("house", 100)], "hosue");
    settle(&mut fuzzy);
    key(&mut fuzzy, KeyCode::Tab);
    assert_eq!(fuzzy.input, "house");
    key(&mut fuzzy, KeyCode::Char('s'));
    settle(&mut fuzzy);
    assert_eq!(fuzzy.input, "houses");
    key(&mut fuzzy, KeyCode::Esc);
    assert_eq!(fuzzy.input, "houses");
}

#[test]
fn pending_completion_is_cancelled_by_new_input_and_empty_query_is_safe() {
    let (_dir, mut app) = app(&[("house", 100), ("home", 90)], "ho");
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Char('u'));
    settle(&mut app);
    assert_eq!(app.input, "hou");
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Enter);
    settle(&mut app);
    assert_eq!(app.input, "");
    assert_eq!(app.focus, Focus::Input);
}
