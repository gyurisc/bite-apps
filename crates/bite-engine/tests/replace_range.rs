//! `document.replace_range`: range edits, their validation, and how they
//! group into undo steps. See `docs/architecture/0004-edit-model.md`.

use bite_engine::{Command, DocumentId, Engine, EngineError};

fn load(engine: &mut Engine, text: &str) -> DocumentId {
    engine
        .dispatch("document.load", &[("text", text)])
        .unwrap()
        .id()
}

fn replace_range(
    engine: &mut Engine,
    document: DocumentId,
    start: usize,
    end: usize,
    text: &str,
) -> Result<String, EngineError> {
    let text = text.to_owned();
    engine
        .execute(Command::ReplaceRange {
            document,
            start,
            end,
            text,
        })
        .map(|document| document.text().to_owned())
}

fn text(engine: &Engine, document: DocumentId) -> &str {
    engine.document(document).unwrap().text()
}

/// Undoes every step and returns the text after each undo, newest first.
fn undo_all(engine: &mut Engine, document: DocumentId) -> Vec<String> {
    let mut seen = Vec::new();
    while engine.document(document).unwrap().can_undo() {
        engine.execute(Command::Undo { document }).unwrap();
        seen.push(text(engine, document).to_owned());
    }
    seen
}

#[test]
fn an_empty_range_inserts_the_text() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "held");

    assert_eq!(
        replace_range(&mut engine, id, 2, 2, "LLO WOR"),
        Ok("heLLO WORld".to_owned())
    );
}

#[test]
fn empty_text_deletes_the_range() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "hello");

    assert_eq!(
        replace_range(&mut engine, id, 1, 4, ""),
        Ok("ho".to_owned())
    );
}

#[test]
fn a_range_and_text_replace_the_bytes_between_the_offsets() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "caf\u{E9} \u{1F34E}");

    // "é" is bytes 3..5, the space 5..6, and the apple 6..10.
    let replaced = replace_range(&mut engine, id, 3, 10, "e pear").unwrap();

    assert_eq!(replaced, "cafe pear");
}

#[test]
fn offsets_may_be_at_either_end_of_the_text() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "middle");

    replace_range(&mut engine, id, 6, 6, "!").unwrap();
    replace_range(&mut engine, id, 0, 0, "the ").unwrap();

    assert_eq!(text(&engine, id), "the middle!");
}

#[test]
fn a_range_edit_can_be_undone_and_redone() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "before");
    replace_range(&mut engine, id, 0, 6, "after").unwrap();

    let undone = engine.execute(Command::Undo { document: id }).unwrap();
    assert_eq!(undone.text(), "before");
    let redone = engine.execute(Command::Redo { document: id }).unwrap();
    assert_eq!(redone.text(), "after");
}

#[test]
fn replacing_a_range_with_identical_text_is_not_a_change() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "same");
    replace_range(&mut engine, id, 0, 4, "SAME").unwrap();
    engine.execute(Command::Undo { document: id }).unwrap();

    replace_range(&mut engine, id, 1, 3, "am").unwrap();
    replace_range(&mut engine, id, 2, 2, "").unwrap();

    let document = engine.document(id).unwrap();
    assert!(!document.can_undo());
    assert!(document.can_redo());
}

#[test]
fn dispatch_reads_offsets_written_in_decimal_digits() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "abcdef").to_string();

    let params = [
        ("end", "04"),
        ("text", "X"),
        ("document", id.as_str()),
        ("start", "2"),
    ];
    let document = engine.dispatch("document.replace_range", &params).unwrap();

    assert_eq!(document.text(), "abXef");
}

#[test]
fn dispatch_rejects_an_offset_that_is_not_decimal_digits() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "unchanged");
    let document = id.to_string();

    for name in ["start", "end"] {
        for value in [
            "",
            "+1",
            "-1",
            " 1",
            "1 ",
            "1.0",
            "0x1",
            "one",
            "\u{661}",
            "99999999999999999999999999",
        ] {
            let mut params = vec![("document", document.as_str()), ("text", "x")];
            params.push((name, value));
            params.push((if name == "start" { "end" } else { "start" }, "1"));

            let error = engine
                .dispatch("document.replace_range", &params)
                .unwrap_err();

            assert_eq!(
                error,
                EngineError::InvalidOffset {
                    command: "document.replace_range",
                    name,
                    value: value.to_owned(),
                }
            );
            let message = error.to_string();
            assert!(message.contains(name), "{message}");
            assert!(message.contains(&format!("{value:?}")), "{message}");
        }
    }
    assert_eq!(text(&engine, id), "unchanged");
}

#[test]
fn a_start_after_the_end_is_rejected() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "unchanged");

    let error = replace_range(&mut engine, id, 5, 3, "x").unwrap_err();

    assert_eq!(
        error,
        EngineError::StartAfterEnd {
            document: id,
            start: 5,
            end: 3,
        }
    );
    let message = error.to_string();
    assert!(
        message.contains("`start` 5") && message.contains("`end` 3"),
        "{message}"
    );
    assert_eq!(text(&engine, id), "unchanged");
}

#[test]
fn an_offset_past_the_end_of_the_text_is_rejected() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "short");

    for (start, end, name, offset) in [(0, 6, "end", 6), (6, 6, "start", 6), (9, 12, "start", 9)] {
        let error = replace_range(&mut engine, id, start, end, "x").unwrap_err();

        assert_eq!(
            error,
            EngineError::OffsetPastEnd {
                document: id,
                name,
                offset,
                len: 5,
            }
        );
        let message = error.to_string();
        assert!(message.contains(&format!("`{name}` {offset}")), "{message}");
        assert!(message.contains('5'), "{message}");
    }
    assert_eq!(text(&engine, id), "short");
}

#[test]
fn an_offset_inside_a_character_is_rejected() {
    let mut engine = Engine::new();
    // "é" is bytes 1..3 and the apple 3..7.
    let original = "\u{E9}\u{1F34E}";
    let id = load(&mut engine, &format!("a{original}"));

    for (start, end, name, offset) in [
        (2, 3, "start", 2),
        (1, 2, "end", 2),
        (4, 7, "start", 4),
        (3, 6, "end", 6),
        (5, 5, "start", 5),
    ] {
        let error = replace_range(&mut engine, id, start, end, "x").unwrap_err();

        assert_eq!(
            error,
            EngineError::OffsetInsideCharacter {
                document: id,
                name,
                offset,
            }
        );
        let message = error.to_string();
        assert!(message.contains(&format!("`{name}` {offset}")), "{message}");
    }
    assert_eq!(text(&engine, id), format!("a{original}"));
}

#[test]
fn an_offset_between_a_letter_and_its_combining_accent_is_accepted() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "e\u{301}");

    let replaced = replace_range(&mut engine, id, 1, 1, "x").unwrap();

    assert_eq!(replaced, "ex\u{301}");
}

#[test]
fn a_range_edit_to_an_unknown_document_is_an_error() {
    let mut engine = Engine::new();
    let missing: DocumentId = "3".parse().unwrap();

    assert_eq!(
        replace_range(&mut engine, missing, 0, 0, "x"),
        Err(EngineError::UnknownDocument(missing))
    );
}

/// One step in a sequence of commands sent to a document.
#[derive(Debug, Clone, Copy)]
enum Step {
    Insert(usize, &'static str),
    Delete(usize, usize),
    Replace(usize, usize, &'static str),
    /// A command the engine rejects.
    Rejected,
    /// `document.replace_text` with the document's current text.
    UnchangedReplaceText,
    ReplaceText(&'static str),
    Undo,
    Redo,
}

/// Runs `steps` on a document holding `0123456789` and returns how many
/// undo steps they leave.
fn undo_steps(steps: &[Step]) -> usize {
    let mut engine = Engine::new();
    let id = load(&mut engine, "0123456789");
    for step in steps {
        let document = id;
        let result = match *step {
            Step::Insert(at, text) => replace_range(&mut engine, id, at, at, text),
            Step::Delete(start, end) => replace_range(&mut engine, id, start, end, ""),
            Step::Replace(start, end, text) => replace_range(&mut engine, id, start, end, text),
            Step::Rejected => {
                let rejected = replace_range(&mut engine, id, 0, 99, "x");
                assert!(rejected.is_err(), "{step:?} should be rejected");
                continue;
            }
            Step::UnchangedReplaceText => {
                let text = text(&engine, id).to_owned();
                engine
                    .execute(Command::ReplaceText { document, text })
                    .map(|document| document.text().to_owned())
            }
            Step::ReplaceText(text) => {
                let text = text.to_owned();
                engine
                    .execute(Command::ReplaceText { document, text })
                    .map(|document| document.text().to_owned())
            }
            Step::Undo => engine
                .execute(Command::Undo { document })
                .map(|document| document.text().to_owned()),
            Step::Redo => engine
                .execute(Command::Redo { document })
                .map(|document| document.text().to_owned()),
        };
        result.unwrap_or_else(|error| panic!("{step:?} failed: {error}"));
    }
    undo_all(&mut engine, id).len()
}

#[test]
fn consecutive_edits_group_into_undo_steps_as_adr_0004_lists() {
    use Step::*;
    let cases: &[(&[Step], usize)] = &[
        // Typing continues where the previous text ended.
        (&[Insert(3, "ab"), Insert(5, "c")], 1),
        (&[Insert(3, "ab"), Insert(4, "c")], 2),
        (&[Replace(2, 5, "x"), Insert(3, "c")], 1),
        // Backspace and Forward Delete continue from where deleting started.
        (&[Delete(5, 6), Delete(4, 5)], 1),
        (&[Delete(5, 6), Delete(5, 6)], 1),
        (&[Delete(5, 6), Delete(2, 5)], 1),
        // Switching between typing and deleting starts a new step.
        (&[Delete(5, 6), Insert(5, "c")], 2),
        (&[Insert(3, "a"), Delete(3, 4)], 2),
        // A rejected command leaves grouping as it was.
        (&[Insert(3, "a"), Rejected, Insert(4, "b")], 1),
        // Undo, redo, and replacing the whole text each end the step, even
        // when the next edit goes where typing ended.
        (&[Insert(3, "a"), Undo, Insert(4, "b")], 1),
        (&[Insert(3, "a"), Undo, Redo, Insert(4, "b")], 2),
        (
            &[Insert(3, "a"), ReplaceText("new text"), Insert(4, "b")],
            3,
        ),
    ];

    for (steps, expected) in cases {
        assert_eq!(undo_steps(steps), *expected, "{steps:?}");
    }
}

#[test]
fn a_replacement_always_starts_a_new_step() {
    use Step::*;

    assert_eq!(undo_steps(&[Insert(3, "a"), Replace(4, 5, "b")]), 2);
    assert_eq!(undo_steps(&[Replace(2, 5, "x"), Replace(3, 4, "y")]), 2);
    assert_eq!(undo_steps(&[Delete(5, 6), Replace(4, 5, "y")]), 2);
}

#[test]
fn a_rejected_redo_leaves_grouping_as_it_was() {
    // Typing clears the redo history, so this redo is rejected.
    let mut engine = Engine::new();
    let id = load(&mut engine, "");
    replace_range(&mut engine, id, 0, 0, "a").unwrap();

    let redo = engine.execute(Command::Redo { document: id }).map(|_| ());
    replace_range(&mut engine, id, 1, 1, "b").unwrap();

    assert_eq!(redo, Err(EngineError::NothingToRedo(id)));
    assert_eq!(undo_all(&mut engine, id), [""]);
}

#[test]
fn an_edit_that_changes_nothing_leaves_grouping_as_it_was() {
    use Step::*;

    assert_eq!(
        undo_steps(&[Insert(3, "a"), Replace(0, 1, "0"), Insert(4, "b")]),
        1
    );
    assert_eq!(
        undo_steps(&[Insert(3, "a"), Insert(7, ""), Insert(4, "b")]),
        1
    );
    assert_eq!(
        undo_steps(&[Delete(5, 6), UnchangedReplaceText, Delete(4, 5)]),
        1
    );
}

#[test]
fn typing_a_word_one_key_at_a_time_undoes_as_one_step() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "say ");
    for (at, key) in [(4, "h"), (5, "e"), (6, "l"), (7, "l"), (8, "o")] {
        replace_range(&mut engine, id, at, at, key).unwrap();
    }

    assert_eq!(undo_all(&mut engine, id), ["say "]);
    let redone = engine.execute(Command::Redo { document: id }).unwrap();
    assert_eq!(redone.text(), "say hello");
}

#[test]
fn backspacing_and_forward_deleting_undo_as_one_step_each() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "abcdef");
    // Backspace twice from the end of "abcd", then Forward Delete twice.
    replace_range(&mut engine, id, 3, 4, "").unwrap();
    replace_range(&mut engine, id, 2, 3, "").unwrap();
    replace_range(&mut engine, id, 2, 3, "").unwrap();
    replace_range(&mut engine, id, 2, 3, "").unwrap();

    assert_eq!(text(&engine, id), "ab");
    assert_eq!(undo_all(&mut engine, id), ["abcdef"]);
}

#[test]
fn grouping_is_kept_per_document() {
    let mut engine = Engine::new();
    let first = load(&mut engine, "");
    let second = load(&mut engine, "");

    replace_range(&mut engine, first, 0, 0, "a").unwrap();
    replace_range(&mut engine, second, 0, 0, "x").unwrap();
    replace_range(&mut engine, first, 1, 1, "b").unwrap();
    replace_range(&mut engine, second, 0, 0, "y").unwrap();

    assert_eq!(undo_all(&mut engine, first), [""]);
    assert_eq!(undo_all(&mut engine, second), ["x", ""]);
}
