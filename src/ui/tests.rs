use super::*;
use crate::{Dictionary, build_pack};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Modifier, Style},
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
    (dir, Dictionary::open(&output).unwrap())
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.loading {
        app.poll();
        assert!(Instant::now() < deadline, "worker did not finish");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(app.error.is_none(), "{:?}", app.error);
}

fn click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn paint(app: &mut App, terminal: &mut Terminal<TestBackend>, pointer: &mut Pointer) {
    terminal.draw(|frame| render(frame, app, pointer)).unwrap();
}

fn stroke(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn hard_line_breaks_and_source_controls() {
    let lines = wrap(
        vec![ReadingLine {
            text: "first\nsecond\n\u{1b}[31mthird".into(),
            style: Style::default(),
            styles: Vec::new(),
        }],
        80,
    );
    assert_eq!(
        lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
        vec!["first", "second", "[31mthird"]
    );
    assert!(
        lines
            .iter()
            .all(|line| !line.text.contains('\n') && !line.text.contains('\u{1b}'))
    );
}

#[test]
fn hanging_indent_computes_expected_prefix_widths() {
    assert_eq!(hanging_indent("1. text"), 3);
    assert_eq!(hanging_indent("12. text"), 4);
    assert_eq!(hanging_indent("   • text"), 5);
    assert_eq!(hanging_indent("     — text"), 7);
    assert_eq!(hanging_indent("house  noun  /haʊs/"), 0);
    assert_eq!(hanging_indent("Source: https://example.com"), 0);
    assert_eq!(hanging_indent("  whitespace only"), 2);
    assert_eq!(hanging_indent(""), 0);
}

#[test]
fn wrapped_definition_lines_hanging_indents_and_display_widths() {
    let long_sense = "1. Economics is a messy discipline: too fluid to be a science, too rigorous to be an art, yet full of profound insights into human behavior and complex institutions.";
    let long_example = "   • Economics is a messy discipline: too fluid to be a science, too rigorous to be an art, yet full of profound insights into human behavior and complex institutions.";
    let long_reference = "     — 1993, Francis J. Sheed, Theology and Sanity, Sheed & Ward, London and New York, page 1234, an extensive reference citation.";
    let long_heading = "house  noun  /haʊs/  [archaic / obsolete]  an established residence or dwelling place throughout history";
    let long_source = "Source: https://en.wiktionary.org/wiki/economics_is_a_messy_discipline_too_fluid_to_be_a_science";

    for width in [20, 30, 40, 80] {
        // A long "1. ..." sense wraps with every continuation prefix equal to 3 spaces
        let sense_lines = wrap(
            vec![ReadingLine {
                text: long_sense.into(),
                style: Style::default(),
                styles: Vec::new(),
            }],
            width,
        );
        assert!(sense_lines.len() > 1, "sense must wrap at width {width}");
        assert!(sense_lines[0].text.starts_with("1. "));
        for (idx, line) in sense_lines.iter().enumerate().skip(1) {
            assert!(
                line.text.starts_with("   ") && !line.text.starts_with("    "),
                "continuation line {idx} prefix mismatch at width {width}: {:?}",
                line.text
            );
        }

        // A long "   • ..." example wraps with every continuation prefix equal to 5 spaces
        let example_lines = wrap(
            vec![ReadingLine {
                text: long_example.into(),
                style: Style::default(),
                styles: Vec::new(),
            }],
            width,
        );
        assert!(
            example_lines.len() > 1,
            "example must wrap at width {width}"
        );
        assert!(example_lines[0].text.starts_with("   • "));
        for (idx, line) in example_lines.iter().enumerate().skip(1) {
            assert!(
                line.text.starts_with("     ") && !line.text.starts_with("      "),
                "continuation line {idx} prefix mismatch at width {width}: {:?}",
                line.text
            );
        }

        // A long "     — ..." reference wraps with every continuation prefix equal to 7 spaces
        let ref_lines = wrap(
            vec![ReadingLine {
                text: long_reference.into(),
                style: Style::default(),
                styles: Vec::new(),
            }],
            width,
        );
        assert!(ref_lines.len() > 1, "reference must wrap at width {width}");
        assert!(ref_lines[0].text.starts_with("     — "));
        for (idx, line) in ref_lines.iter().enumerate().skip(1) {
            assert!(
                line.text.starts_with("       ") && !line.text.starts_with("        "),
                "continuation line {idx} prefix mismatch at width {width}: {:?}",
                line.text
            );
        }

        // A wrapped heading and a wrapped "Source: ..." line have no indent
        let heading_lines = wrap(
            vec![ReadingLine {
                text: long_heading.into(),
                style: Style::default(),
                styles: Vec::new(),
            }],
            width,
        );
        assert!(
            heading_lines.len() > 1,
            "heading must wrap at width {width}"
        );
        for (idx, line) in heading_lines.iter().enumerate().skip(1) {
            assert!(
                !line.text.starts_with(' '),
                "heading continuation line {idx} must have no indent at width {width}: {:?}",
                line.text
            );
        }

        let source_lines = wrap(
            vec![ReadingLine {
                text: long_source.into(),
                style: Style::default(),
                styles: Vec::new(),
            }],
            width,
        );
        assert!(source_lines.len() > 1, "source must wrap at width {width}");
        for (idx, line) in source_lines.iter().enumerate().skip(1) {
            assert!(
                !line.text.starts_with(' '),
                "source continuation line {idx} must have no indent at width {width}: {:?}",
                line.text
            );
        }

        // Every produced line's display width <= width for width in {20, 30, 40, 80}
        for all_lines in [
            &sense_lines,
            &example_lines,
            &ref_lines,
            &heading_lines,
            &source_lines,
        ] {
            for line in all_lines.iter() {
                assert!(
                    line.text.width() <= width,
                    "line width {} exceeds max {width}: {:?}",
                    line.text.width(),
                    line.text
                );
            }
        }
    }
}

#[test]
fn click_focuses_definition_then_jumps_to_the_visible_word() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);

    let (column, row) = pointer.word_position("hand").expect("visible word");
    assert!(on_mouse(&mut app, &pointer, click(column, row)));
    assert_eq!(app.focus, Focus::Definition);
    assert_eq!(app.input, "fist");
    assert_eq!(app.history_len(), 0);

    paint(&mut app, &mut terminal, &mut pointer);
    let (column, row) = pointer.word_position("hand").unwrap();
    assert!(on_mouse(&mut app, &pointer, click(column, row)));
    settle(&mut app);
    assert_eq!(app.input, "hand");
    assert_eq!(app.preview.as_ref().unwrap().entry.key, "hand");
    assert_eq!(app.history_len(), 1);

    app.back();
    assert_eq!(app.input, "fist");
    assert_eq!(app.focus, Focus::Definition);
}

#[test]
fn click_input_returns_focus_without_navigation() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    app.focus = Focus::Definition;
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);

    let (column, row) = (pointer.input.x + 1, pointer.input.y + 1);
    assert!(on_mouse(&mut app, &pointer, click(column, row)));
    assert_eq!(app.focus, Focus::Input);
    assert_eq!(app.history_len(), 0);
}

#[test]
fn click_without_a_dictionary_word_and_mouse_motion_do_nothing() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    app.focus = Focus::Definition;
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);

    let (column, row) = (pointer.definition.x, pointer.definition.y);
    assert!(!on_mouse(&mut app, &pointer, click(column, row)));
    let moved = MouseEvent {
        kind: MouseEventKind::Moved,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    assert!(!on_mouse(&mut app, &pointer, moved));
    assert_eq!(app.history_len(), 0);
    assert_eq!(app.input, "fist");
}

#[test]
fn stacked_narrow_layout_click_targets_the_definition_pane() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);

    assert!(pointer.definition.y > pointer.input.y);
    let (column, row) = pointer.word_position("hand").expect("visible word");
    assert!(on_mouse(&mut app, &pointer, click(column, row)));
    assert_eq!(app.focus, Focus::Definition);
    assert_eq!(app.input, "fist");
}

#[test]
fn wheel_scrolls_the_reading_pane_and_clamps() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);
    assert!(app.max_scroll > 4 * WHEEL_LINES);

    let (column, row) = (pointer.definition.x, pointer.definition.y);
    let notch = |kind| MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    assert!(on_mouse(
        &mut app,
        &pointer,
        notch(MouseEventKind::ScrollDown)
    ));
    assert_eq!(app.scroll, WHEEL_LINES);
    assert_eq!(app.focus, Focus::Input);
    assert!(on_mouse(
        &mut app,
        &pointer,
        notch(MouseEventKind::ScrollUp)
    ));
    assert_eq!(app.scroll, 0);
    assert!(!on_mouse(
        &mut app,
        &pointer,
        notch(MouseEventKind::ScrollUp)
    ));

    app.scroll = app.max_scroll;
    assert!(!on_mouse(
        &mut app,
        &pointer,
        notch(MouseEventKind::ScrollDown)
    ));
}

#[test]
fn page_keys_scroll_by_the_visible_height() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(100, 14)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);
    app.focus = Focus::Definition;
    let page = app.page;
    assert!(
        page >= 2 && page < app.max_scroll,
        "page {page} against max {}",
        app.max_scroll
    );

    stroke(&mut app, KeyCode::PageDown);
    assert_eq!(app.scroll, page);
    stroke(&mut app, KeyCode::PageDown);
    assert_eq!(app.scroll, (2 * page).min(app.max_scroll));
    stroke(&mut app, KeyCode::End);
    assert_eq!(app.scroll, app.max_scroll);
    stroke(&mut app, KeyCode::Home);
    assert_eq!(app.scroll, 0);
    stroke(&mut app, KeyCode::PageUp);
    assert_eq!(app.scroll, 0);
}

#[test]
fn hints_stay_usable_after_scrolling_a_page() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(100, 14)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);
    app.focus = Focus::Definition;
    stroke(&mut app, KeyCode::Char('f'));
    assert!(app.picking);
    assert!(app.page < app.max_scroll);

    stroke(&mut app, KeyCode::PageDown);
    assert_eq!(app.scroll, app.page, "hint mode must scroll");
    assert!(app.picking, "scrolling must not cancel hint mode");
    assert!(app.label_input.is_empty());

    // Labels are rebuilt for the newly visible lines on the next frame.
    paint(&mut app, &mut terminal, &mut pointer);
    let (label, word) = app.labels.first().cloned().expect("hints on the new page");
    for letter in label.chars() {
        stroke(&mut app, KeyCode::Char(letter));
    }
    settle(&mut app);
    assert_eq!(app.input, word);
    assert!(!app.picking);
}

fn candidate_row(pointer: &Pointer, index: usize) -> u16 {
    pointer.candidates.y + (index - pointer.candidates_offset) as u16
}

fn click_candidate(app: &mut App, pointer: &Pointer, index: usize) -> bool {
    let column = pointer.candidates.x + 1;
    on_mouse(app, pointer, click(column, candidate_row(pointer, index)))
}

#[test]
fn click_candidate_selects_it_without_moving_focus() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "ho");
    settle(&mut app);
    assert!(app.results.len() > 1, "need several candidates");
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);
    assert_eq!(pointer.candidates_offset, 0);

    assert!(click_candidate(&mut app, &pointer, 1));
    assert_eq!(app.selected, 1);
    settle(&mut app);
    assert_eq!(app.input, "ho", "selection must not accept the word");
    assert_eq!(app.focus, Focus::Input);
    assert_eq!(app.preview.as_ref().unwrap().entry.key, app.results[1].key);
}

#[test]
fn clicking_the_highlighted_candidate_accepts_and_focuses_definition() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "ho");
    settle(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);
    let expected = app.results[app.selected].headword.clone();

    assert_eq!(app.selected, 0);
    let selected = app.selected;
    assert!(click_candidate(&mut app, &pointer, selected));
    settle(&mut app);
    assert_eq!(app.input, expected);
    assert_eq!(app.focus, Focus::Definition);
    assert_eq!(app.preview.as_ref().unwrap().entry.key, expected);
}

#[test]
fn candidate_clicks_follow_a_scrolled_list() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "ho");
    settle(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(100, 14)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);
    let visible = pointer.candidates.height as usize;
    assert!(
        app.results.len() > visible + 1,
        "need a list longer than the pane"
    );

    // Move the selection past the pane so the list has to scroll.
    for _ in 0..visible + 1 {
        stroke(&mut app, KeyCode::Down);
    }
    settle(&mut app);
    paint(&mut app, &mut terminal, &mut pointer);
    assert!(pointer.candidates_offset > 0, "list should have scrolled");

    let target = pointer.candidates_offset + 1;
    assert!(click_candidate(&mut app, &pointer, target));
    assert_eq!(app.selected, target);
}

#[test]
fn clicks_below_the_last_candidate_are_ignored() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "ho");
    settle(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);
    let selected = app.selected;

    let column = pointer.candidates.x + 1;
    let row = pointer.candidates.y + app.results.len() as u16 + 1;
    assert!(!on_mouse(&mut app, &pointer, click(column, row)));
    assert_eq!(app.selected, selected);
    assert_eq!(app.input, "ho");
}

#[test]
fn width_adaptive_help_footer_never_exceeds_width_and_keeps_exit_hint() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);

    let widths = [30, 45, 60, 80, 100, 120];
    let heights = [10, 12, 24];

    for &width in &widths {
        for &height in &heights {
            let line_count = if height < 12 { 1 } else { 2 };

            // 1. Input focus
            app.focus = Focus::Input;
            app.picking = false;
            for loading in [false, true] {
                app.loading = loading;
                let lines = footer_help(&app, width, line_count);

                assert!(
                    lines.len() <= line_count,
                    "Input lines count {} > available {line_count} at {width}x{height}",
                    lines.len()
                );
                for line in &lines {
                    assert!(
                        line.width() <= width,
                        "Input line '{line}' display width {} > {width} at {width}x{height}",
                        line.width()
                    );
                }
                assert!(
                    lines.iter().any(|l| l.contains("Ctrl+C quit")),
                    "Input exit hint missing at {width}x{height}: {lines:?}"
                );
                if width >= 120 && height >= 12 {
                    let expected = vec![
                        format!("Lookup · exact · 1/1 · Back 0 / Forward 0{}", if loading { " · loading…" } else { "" }),
                        "Enter read · Tab complete · Ctrl+G actions · F1 help · F2 settings · Ctrl+C quit".into(),
                    ];
                    assert_eq!(
                        lines, expected,
                        "Input full long-form mismatch at {width}x{height}"
                    );
                }
            }

            // 2. Definition focus
            app.focus = Focus::Definition;
            app.picking = false;
            for loading in [false, true] {
                app.loading = loading;
                let lines = footer_help(&app, width, line_count);

                assert!(
                    lines.len() <= line_count,
                    "Definition lines count {} > available {line_count} at {width}x{height}",
                    lines.len()
                );
                for line in &lines {
                    assert!(
                        line.width() <= width,
                        "Definition line '{line}' display width {} > {width} at {width}x{height}",
                        line.width()
                    );
                }
                assert!(
                    lines.iter().any(|l| l.contains("Ctrl+C quit")),
                    "Definition exit hint missing at {width}x{height}: {lines:?}"
                );
                if width >= 120 && height >= 12 {
                    let expected = vec![
                        format!("Reading · exact · 1/1 · Back 0 / Forward 0{}", if loading { " · loading…" } else { "" }),
                        "f follow · Ctrl+G actions · Esc input · F1 help · F2 settings · Ctrl+C quit".into(),
                    ];
                    assert_eq!(
                        lines, expected,
                        "Definition full long-form mismatch at {width}x{height}"
                    );
                }
            }

            // 3. Label-picking
            app.picking = true;
            for label_input in ["", "a"] {
                app.label_input = label_input.to_string();
                let lines = footer_help(&app, width, line_count);

                assert!(
                    lines.len() <= line_count,
                    "Picking lines count {} > available {line_count} at {width}x{height}",
                    lines.len()
                );
                for line in &lines {
                    assert!(
                        line.width() <= width,
                        "Picking line '{line}' display width {} > {width} at {width}x{height}",
                        line.width()
                    );
                }
                assert!(
                    lines.iter().any(|l| l.contains("Ctrl+C quit")),
                    "Picking exit hint missing at {width}x{height}: {lines:?}"
                );
                if width >= 120 && height >= 12 {
                    let expected = vec![format!(
                        "Label: {}_  · type both letters · PgUp/PgDn scroll · Esc cancels · Ctrl+C quit",
                        app.label_input
                    )];
                    assert_eq!(
                        lines, expected,
                        "Picking full long-form mismatch at {width}x{height}"
                    );
                }
            }
        }
    }
}

#[test]
fn footer_rendering_in_terminal_fits_all_test_dimensions() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);

    let widths = [30, 45, 60, 80, 100, 120];
    let heights = [10, 12, 24];

    for &width in &widths {
        for &height in &heights {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut pointer = Pointer::default();
            paint(&mut app, &mut terminal, &mut pointer);

            let buffer = terminal.backend().buffer();
            let footer_height = if height < 12 { 1 } else { 2 };
            let start_y = height - footer_height;
            let mut footer_text = String::new();
            for y in start_y..height {
                for x in 0..width {
                    footer_text.push_str(buffer[(x, y)].symbol());
                }
                footer_text.push('\n');
            }
            assert!(
                footer_text.contains("Ctrl+C quit"),
                "Terminal buffer missing exit hint at {width}x{height}:\n{footer_text}"
            );
        }
    }
}

#[test]
fn collapsible_ipa_water() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "water");
    settle(&mut app);

    let preview = app.preview.as_ref().expect("preview for water");
    let group_with_34 = preview
        .entry
        .groups
        .iter()
        .find(|g| g.ipa.len() == 34)
        .expect("group with 34 IPA variants");
    assert_eq!(group_with_34.ipa.len(), 34);

    // Compact reading_lines (app.view.expand_ipa is false by default)
    assert!(!app.view.expand_ipa);
    let compact_lines = reading_lines(&app);
    let heading = compact_lines
        .iter()
        .find(|l| {
            l.text.starts_with(&format!(
                "{}  {}  ",
                group_with_34.headword, group_with_34.pos
            ))
        })
        .expect("group heading line");
    assert!(heading.text.ends_with('…'));
    assert!(
        !group_with_34
            .ipa
            .iter()
            .all(|variant| heading.text.contains(variant))
    );
    assert!(
        !group_with_34
            .ipa
            .iter()
            .all(|variant| compact_lines.iter().any(|l| l.text.contains(variant)))
    );

    // After setting app.view.expand_ipa = true the heading contains every variant
    app.view.expand_ipa = true;
    let expanded_lines = reading_lines(&app);
    let expanded_heading = expanded_lines
        .iter()
        .find(|l| {
            l.text.starts_with(&format!(
                "{}  {}  ",
                group_with_34.headword, group_with_34.pos
            ))
        })
        .expect("expanded group heading line");
    for variant in &group_with_34.ipa {
        assert!(
            expanded_heading.text.contains(variant),
            "expanded heading must contain variant {variant}"
        );
    }
    assert_eq!(heading.text.matches(" · ").count(), 1);
    assert_eq!(expanded_heading.text.matches(" · ").count(), 33);
    assert!(!expanded_heading.text.ends_with('…'));

    // Toggle via key 'p' in definition focus
    app.focus = Focus::Definition;
    stroke(&mut app, KeyCode::Char('p'));
    assert!(!app.view.expand_ipa);
    stroke(&mut app, KeyCode::Char('p'));
    assert!(app.view.expand_ipa);
}

#[test]
fn collapsible_examples_and_references_a() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "a");
    settle(&mut app);

    assert!(!app.view.expand_examples);
    let compact_lines = reading_lines(&app);
    assert!(
        compact_lines.iter().all(|l| !l.text.starts_with("     — ")),
        "compact reading_lines must contain no line starting with '     — '"
    );

    let mut current_sense_examples = 0;
    for line in &compact_lines {
        if line.text.is_empty() {
            assert!(
                current_sense_examples <= 1,
                "each sense must have at most one example line, got {current_sense_examples}"
            );
            current_sense_examples = 0;
        } else if line.text.starts_with("   • ") {
            current_sense_examples += 1;
        }
    }
    assert!(current_sense_examples <= 1);

    // After setting app.view.expand_examples = true, at least one "     — " line appears
    app.view.expand_examples = true;
    let expanded_lines = reading_lines(&app);
    assert!(
        expanded_lines.iter().any(|l| l.text.starts_with("     — ")),
        "expanded reading_lines must contain at least one reference line"
    );

    // In expanded mode, at least one sense has more than one example
    let mut max_sense_examples = 0;
    let mut count = 0;
    for line in &expanded_lines {
        if line.text.is_empty() {
            max_sense_examples = max_sense_examples.max(count);
            count = 0;
        } else if line.text.starts_with("   • ") {
            count += 1;
        }
    }
    assert!(max_sense_examples > 1);

    // Toggle via key 'e' in definition focus
    app.focus = Focus::Definition;
    stroke(&mut app, KeyCode::Char('e'));
    assert!(!app.view.expand_examples);
    stroke(&mut app, KeyCode::Char('e'));
    assert!(app.view.expand_examples);
}

#[test]
fn plain_e_and_p_in_input_focus_edits_query_instead_of_toggling() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "wat");
    settle(&mut app);

    assert_eq!(app.focus, Focus::Input);
    assert!(!app.view.expand_ipa);
    assert!(!app.view.expand_examples);

    stroke(&mut app, KeyCode::Char('e'));
    assert_eq!(app.input, "wate");
    assert!(!app.view.expand_ipa);
    assert!(!app.view.expand_examples);

    stroke(&mut app, KeyCode::Char('p'));
    assert_eq!(app.input, "watep");
    assert!(!app.view.expand_ipa);
    assert!(!app.view.expand_examples);
}

#[test]
fn pressing_e_and_p_in_hint_picking_mode_does_not_toggle() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "water");
    settle(&mut app);

    app.focus = Focus::Definition;
    stroke(&mut app, KeyCode::Char('f'));
    assert!(app.picking);
    assert!(!app.view.expand_ipa);
    assert!(!app.view.expand_examples);

    stroke(&mut app, KeyCode::Char('p'));
    assert!(!app.view.expand_ipa);
    assert_eq!(app.label_input, "p");

    stroke(&mut app, KeyCode::Char('e'));
    assert!(!app.view.expand_examples);
}

#[test]
fn definition_focus_without_preview_ignores_e_and_p() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "");
    settle(&mut app);
    app.focus = Focus::Definition;
    assert!(app.preview.is_none());

    stroke(&mut app, KeyCode::Char('p'));
    assert!(!app.view.expand_ipa);

    stroke(&mut app, KeyCode::Char('e'));
    assert!(!app.view.expand_examples);
}

#[test]
fn scroll_percent_acceptance() {
    assert_eq!(scroll_percent(0, 0), None);
    assert_eq!(scroll_percent(0, 10), Some(0));
    assert_eq!(scroll_percent(5, 10), Some(50));
    assert_eq!(scroll_percent(10, 10), Some(100));
}

#[test]
fn definition_title_shows_headword_and_scroll_progress() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "water");
    settle(&mut app);

    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);

    // Frame 1: water at scroll 0 shows the word "water" in the definition-pane title area
    let def_x = pointer.definition.x;
    let def_w = pointer.definition.width;
    let def_top_y = pointer.definition.y;
    let buffer = terminal.backend().buffer();
    let mut title_row = String::new();
    for x in def_x..def_x + def_w {
        title_row.push_str(buffer[(x, def_top_y)].symbol());
    }
    assert!(
        title_row.contains("water"),
        "definition title must contain 'water': {title_row}"
    );
    assert!(
        title_row.contains("0%"),
        "definition title at scroll 0 must contain '0%': {title_row}"
    );

    // Frame 2: repeats after scrolling to the end with "100%"
    assert!(app.max_scroll > 0);
    app.scroll = app.max_scroll;
    paint(&mut app, &mut terminal, &mut pointer);

    let buffer = terminal.backend().buffer();
    let mut scrolled_title_row = String::new();
    for x in def_x..def_x + def_w {
        scrolled_title_row.push_str(buffer[(x, def_top_y)].symbol());
    }
    assert!(
        scrolled_title_row.contains("water"),
        "scrolled definition title must contain 'water': {scrolled_title_row}"
    );
    assert!(
        scrolled_title_row.contains("100%"),
        "scrolled definition title must contain '100%': {scrolled_title_row}"
    );
}

#[test]
fn adaptive_candidate_pane_size_narrow_60x24() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "ho");
    settle(&mut app);

    let mut terminal = Terminal::new(TestBackend::new(60, 24)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);

    // A rendered 60x24 frame for ho shows at least 3 candidate rows
    // (count distinct candidate labels in the candidates pane inner area), up from 1.
    let buffer = terminal.backend().buffer();
    let mut distinct_candidates = std::collections::HashSet::new();
    for y in pointer.candidates.y..pointer.candidates.y + pointer.candidates.height {
        let mut row = String::new();
        for x in pointer.candidates.x..pointer.candidates.x + pointer.candidates.width {
            row.push_str(buffer[(x, y)].symbol());
        }
        let trimmed = row.trim();
        if !trimmed.is_empty() {
            distinct_candidates.insert(trimmed.to_string());
        }
    }
    assert!(
        distinct_candidates.len() >= 3,
        "expected at least 3 distinct candidate labels in candidate pane inner area, got {}: {:?}",
        distinct_candidates.len(),
        distinct_candidates
    );
}

#[test]
fn adaptive_candidate_pane_size_wide_120x40() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "ho");
    settle(&mut app);

    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);

    // A rendered 120x40 frame for a query whose candidates are all short (e.g. ho)
    // gives the definition pane more columns than the old 75% (90 cols).
    assert!(
        pointer.definition.width > 90,
        "definition pane width {} must exceed old 75% (90 columns)",
        pointer.definition.width
    );

    // A query with a long candidate (e.g. fist-fighting if present) does not exceed 40% for the candidate pane.
    let (_dir2, dict_fist) = dictionary();
    let mut app_fist = App::new(dict_fist, "fist");
    settle(&mut app_fist);
    if !app_fist.results.iter().any(|c| c.headword.width() >= 30) {
        app_fist.results.push(crate::Candidate {
            key: "fist-fighting".into(),
            headword: "fist-fighting-champion-of-the-world".into(),
            score: 100,
            kind: crate::MatchKind::Exact,
        });
    }
    let mut pointer_fist = Pointer::default();
    paint(&mut app_fist, &mut terminal, &mut pointer_fist);

    let candidate_width = pointer_fist.definition.x;
    assert!(
        candidate_width <= 48,
        "candidate pane width {candidate_width} must not exceed 40% of 120 (48 columns)"
    );
}

#[test]
fn typing_keeps_the_previous_definition_until_the_response_arrives() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    let previous = app.preview.as_ref().unwrap().entry.key.clone();

    app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    assert!(app.loading, "typing starts a new lookup");
    assert_eq!(
        app.preview.as_ref().map(|p| p.entry.key.as_str()),
        Some(previous.as_str()),
        "the old definition stays visible while loading"
    );
    settle(&mut app);
    assert_eq!(app.input, "fistx");
}

#[test]
fn error_banner_keeps_the_last_definition() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    let headword = app.preview.as_ref().unwrap().entry.headword.clone();
    app.loading = false;
    app.error = Some("worker hiccup".into());

    let text: Vec<_> = reading_lines(&app).into_iter().map(|l| l.text).collect();
    assert!(text[0].contains("worker hiccup"), "{text:?}");
    assert!(
        text.iter().any(|l| l.contains(&headword)),
        "definition must survive an error: {text:?}"
    );
}

#[test]
fn error_without_a_preview_does_not_append_the_empty_state() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    app.preview = None;
    app.error = Some("worker hiccup".into());

    let text: Vec<_> = reading_lines(&app).into_iter().map(|l| l.text).collect();
    assert_eq!(text, vec!["worker hiccup".to_string()]);
}

#[test]
fn empty_state_explains_short_queries_and_spelling() {
    for (query, expected) in [
        ("xq", "at least 3 letters"),
        ("xqzwpy", "Check the spelling"),
    ] {
        let (_dir, dict) = dictionary();
        let mut app = App::new(dict, query);
        settle(&mut app);
        let text: String = reading_lines(&app).into_iter().map(|l| l.text).collect();
        assert!(text.contains(expected), "{query}: {text:?}");
    }
}

#[test]
fn stale_style_adds_dim_without_dropping_colour() {
    let styled = ReadingLine {
        text: "house".into(),
        style: Style::default().fg(ratatui::style::Color::Cyan),
        styles: Vec::new(),
    };
    let stale = stale_style(vec![styled]);
    assert!(stale[0].style.add_modifier.contains(Modifier::DIM));
    assert_eq!(stale[0].style.fg, Some(ratatui::style::Color::Cyan));
}

#[test]
fn help_overlay_toggles_and_renders() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);

    // F1 opens from input focus; Esc closes.
    stroke(&mut app, KeyCode::F(1));
    assert!(app.view.show_help());
    stroke(&mut app, KeyCode::Esc);
    assert!(!app.view.show_help());

    // '?' opens from definition focus and any key other than Esc is swallowed.
    app.focus = Focus::Definition;
    stroke(&mut app, KeyCode::Char('?'));
    assert!(app.view.show_help());
    stroke(&mut app, KeyCode::Char('x'));
    assert!(
        app.view.show_help(),
        "plain keys must not type while help is open"
    );

    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Keys"), "help overlay title missing");
    assert!(
        text.contains("Ctrl+W"),
        "help overlay must document word editing"
    );

    stroke(&mut app, KeyCode::Esc);
    assert!(!app.view.show_help());
}

#[test]
fn appearance_mouse_is_modal_and_preserves_follow_hints() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    app.focus = Focus::Definition;
    stroke(&mut app, KeyCode::Char('f'));
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut pointer = Pointer::default();
    paint(&mut app, &mut terminal, &mut pointer);
    let labels = app.labels.clone();
    let original = (
        app.input.clone(),
        app.selected,
        app.focus,
        app.scroll,
        app.history_len(),
    );
    stroke(&mut app, KeyCode::F(2));
    paint(&mut app, &mut terminal, &mut pointer);
    let row = pointer.appearance_rows[1];
    assert!(on_mouse(&mut app, &pointer, click(row.x, row.y)));
    assert!(!app.appearance().theme_background);
    assert!(!on_mouse(&mut app, &pointer, click(0, 0)));
    assert!(on_mouse(
        &mut app,
        &pointer,
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            ..click(0, 0)
        }
    ));
    assert_eq!(app.view.appearance_row, 2);
    app.paste("house");
    assert_eq!(
        (
            app.input.clone(),
            app.selected,
            app.focus,
            app.scroll,
            app.history_len()
        ),
        original
    );
    stroke(&mut app, KeyCode::F(1));
    assert!(app.view.show_help());
    assert!(!app.view.show_appearance());
    stroke(&mut app, KeyCode::Esc);
    paint(&mut app, &mut terminal, &mut pointer);
    assert!(app.picking);
    assert_eq!(app.labels, labels);
}

#[test]
fn queued_completions_wait_until_appearance_closes() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fis");
    stroke(&mut app, KeyCode::Right);
    assert!(app.results.is_empty());
    stroke(&mut app, KeyCode::F(2));
    settle(&mut app);
    assert!(app.view.show_appearance());
    assert!(!app.loading);
    assert_eq!(app.appearance().color_theme, crate::ThemePreset::Default);
    assert_eq!(app.input, "fis");
    stroke(&mut app, KeyCode::Esc);
    app.poll();
    settle(&mut app);
    assert!(!app.view.show_appearance());
    assert_eq!(app.input, "fist");
}

#[test]
fn appearance_save_failure_is_visible_at_minimum_size() {
    let (_dir, dict) = dictionary();
    let mut app = App::new(dict, "fist");
    settle(&mut app);
    stroke(&mut app, KeyCode::F(2));
    app.appearance_status =
        Some("Not saved: Cannot save appearance configuration at a long path".into());
    for color in [false, true] {
        app.set_color(color);
        for (width, height) in [(30, 10), (30, 11), (120, 40)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut pointer = Pointer::default();
            paint(&mut app, &mut terminal, &mut pointer);
            let text: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(
                text.contains("Not saved"),
                "failure clipped at {width}×{height}: {text}"
            );
            if !color {
                assert!(text.contains("no color"));
            }
        }
    }
}

#[test]
fn unicode_styles_survive_controls_wrapping_and_hint_replacement() {
    let theme = crate::theme::Theme::colored();
    let text = "élève  noun\t/ɛ.lɛv/\n界 e\u{301}".to_string();
    let mut line = ReadingLine::new(text.clone(), theme.heading());
    line.styles.push(("élève".len()..text.len(), theme.dim()));
    let lines = wrap(vec![line], 10);
    assert!(lines.iter().all(|line| line.text.width() <= 10));
    let all: String = lines.iter().map(|line| line.text.as_str()).collect();
    assert!(all.contains("élève"));
    assert!(all.contains("界 e\u{301}"));
    let heading = &lines[0];
    let rendered = heading.rendered();
    assert_eq!(rendered.spans[0].content, "élève");
    assert_eq!(rendered.spans[0].style, theme.heading());
    let ipa = lines.iter().find(|line| line.text.contains('/')).unwrap();
    assert!(
        ipa.rendered()
            .spans
            .iter()
            .all(|span| span.style == theme.dim())
    );
    let map = std::collections::HashMap::from([("élève".to_string(), "ab".to_string())]);
    let hints = label_line(heading, &map, theme);
    let text: String = hints
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(text.starts_with("abève"));
    assert_eq!(text.width(), heading.text.width());
    assert_eq!(hints.spans[0].style, theme.hint_label());
    let stale = stale_style(lines);
    assert!(
        stale
            .iter()
            .flat_map(|line| line.rendered().spans)
            .all(|span| span.style.add_modifier.contains(Modifier::DIM))
    );
}
