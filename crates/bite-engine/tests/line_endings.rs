//! Line breaks inserted by `document.replace_range` take the document's line
//! ending, as `docs/architecture/0006-documents-in-the-app.md` describes.

use bite_engine::{Command, DocumentId, Engine, Outcome};
use std::ops::Range;

fn load(engine: &mut Engine, text: &str) -> DocumentId {
    let text = text.to_owned();
    engine.execute(Command::LoadDocument { text }).unwrap().id()
}

fn create(engine: &mut Engine) -> DocumentId {
    engine.execute(Command::CreateDocument).unwrap().id()
}

/// Inserts `text` at `at` and returns the new text and the changed range.
fn insert(
    engine: &mut Engine,
    document: DocumentId,
    at: usize,
    text: &str,
) -> (String, Option<Range<usize>>) {
    let text = text.to_owned();
    let outcome: Outcome = engine
        .execute(Command::ReplaceRange {
            document,
            start: at,
            end: at,
            text,
        })
        .unwrap();
    (
        outcome.document().unwrap().text().to_owned(),
        outcome.changed(),
    )
}

#[test]
fn each_line_break_inserted_into_a_crlf_document_becomes_crlf() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "one\r\ntwo");

    let (text, changed) = insert(&mut engine, id, 8, "\nthree\rfour\r\nfive");

    assert_eq!(text, "one\r\ntwo\r\nthree\r\nfour\r\nfive");
    assert_eq!(changed, Some(8..text.len()));
}

#[test]
fn line_breaks_inserted_into_a_cr_document_become_cr() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "one\rtwo");

    let (text, _) = insert(&mut engine, id, 3, "\n1\r\n2\r");

    assert_eq!(text, "one\r1\r2\r\rtwo");
}

#[test]
fn line_breaks_inserted_into_an_lf_document_become_lf() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "one\ntwo");

    let (text, _) = insert(&mut engine, id, 7, "\r\nthree\rfour");

    assert_eq!(text, "one\ntwo\nthree\nfour");
}

#[test]
fn created_documents_and_text_without_a_line_break_use_lf() {
    let mut engine = Engine::new();
    let created = create(&mut engine);
    let unbroken = load(&mut engine, "one line");

    assert_eq!(insert(&mut engine, created, 0, "a\r\nb").0, "a\nb");
    assert_eq!(insert(&mut engine, unbroken, 8, "\r\n").0, "one line\n");
}

#[test]
fn the_first_line_break_decides_and_existing_ones_are_kept() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "a\r\nb\nc\rd");

    let (text, _) = insert(&mut engine, id, 8, "\n");

    assert_eq!(text, "a\r\nb\nc\rd\r\n");
}

#[test]
fn a_line_break_inserted_next_to_an_existing_one_stays_its_own() {
    // Inserting LF right after the CR of a CR document must not pair up with
    // it into one CRLF break.
    let mut engine = Engine::new();
    let id = load(&mut engine, "a\rb");

    let (text, _) = insert(&mut engine, id, 2, "\n");

    assert_eq!(text, "a\r\rb");
}

#[test]
fn a_line_break_inserted_just_before_a_stray_one_can_pair_with_it() {
    // ADR 0006: existing text is never converted, so a CR typed into a CR
    // document just before a stray LF makes one CRLF break with it.
    let mut engine = Engine::new();
    let id = load(&mut engine, "a\rb\nc");

    let (text, _) = insert(&mut engine, id, 3, "\n");

    assert_eq!(text, "a\rb\r\nc");
}

#[test]
fn other_separators_are_inserted_as_given() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "one\r\ntwo");

    let (text, _) = insert(&mut engine, id, 3, "\u{2028}\u{85}\u{c}");

    assert_eq!(text, "one\u{2028}\u{85}\u{c}\r\ntwo");
}

#[test]
fn replace_text_keeps_its_line_breaks_exactly() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "one\r\ntwo");

    let outcome = engine
        .execute(Command::ReplaceText {
            document: id,
            text: "x\ny\rz".to_owned(),
        })
        .unwrap();

    assert_eq!(outcome.document().unwrap().text(), "x\ny\rz");
}

#[test]
fn typing_after_a_converted_line_break_joins_the_same_undo_step() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "one\r\n");

    insert(&mut engine, id, 5, "a");
    insert(&mut engine, id, 6, "\n");
    // The converted break is two bytes long, so typing continues at 8.
    insert(&mut engine, id, 8, "b");

    let undone = engine
        .execute(Command::Undo { document: id })
        .unwrap()
        .document()
        .unwrap()
        .text()
        .to_owned();
    assert_eq!(undone, "one\r\n");
}

#[test]
fn an_insertion_that_converts_to_the_text_already_there_changes_nothing() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "a\r\nb");

    let outcome = engine
        .execute(Command::ReplaceRange {
            document: id,
            start: 1,
            end: 3,
            text: "\n".to_owned(),
        })
        .unwrap();

    assert_eq!(outcome.changed(), None);
    assert!(!outcome.document().unwrap().can_undo());
}
