//! Whether a document has changes since it was last marked as saved, as
//! `docs/architecture/0006-documents-in-the-app.md` describes.

use bite_engine::{Engine, EngineError, Outcome, command_id};

fn create(engine: &mut Engine) -> String {
    engine
        .dispatch(command_id::CREATE, &[])
        .unwrap()
        .id()
        .to_string()
}

fn modified(outcome: Result<Outcome<'_>, EngineError>) -> bool {
    outcome.unwrap().document().unwrap().is_modified()
}

fn insert(engine: &mut Engine, id: &str, at: usize, text: &str) -> bool {
    let at = at.to_string();
    let params = [
        ("document", id),
        ("start", at.as_str()),
        ("end", at.as_str()),
        ("text", text),
    ];
    modified(engine.dispatch(command_id::REPLACE_RANGE, &params))
}

fn delete(engine: &mut Engine, id: &str, start: usize, end: usize) -> bool {
    let (start, end) = (start.to_string(), end.to_string());
    let params = [
        ("document", id),
        ("start", start.as_str()),
        ("end", end.as_str()),
        ("text", ""),
    ];
    modified(engine.dispatch(command_id::REPLACE_RANGE, &params))
}

fn run(engine: &mut Engine, command: &str, id: &str) -> bool {
    modified(engine.dispatch(command, &[("document", id)]))
}

fn text(engine: &Engine, id: &str) -> String {
    engine
        .document(id.parse().unwrap())
        .unwrap()
        .text()
        .to_owned()
}

#[test]
fn created_and_loaded_documents_are_not_modified() {
    let mut engine = Engine::new();

    assert!(!modified(engine.dispatch(command_id::CREATE, &[])));
    assert!(!modified(
        engine.dispatch(command_id::LOAD, &[("text", "from a file")])
    ));
}

#[test]
fn an_edit_modifies_the_document_and_undoing_it_does_not() {
    let mut engine = Engine::new();
    let id = create(&mut engine);

    assert!(insert(&mut engine, &id, 0, "a"));
    assert!(!run(&mut engine, command_id::UNDO, &id));
    assert!(run(&mut engine, command_id::REDO, &id));
    let replaced = [("document", id.as_str()), ("text", "whole")];
    assert!(modified(
        engine.dispatch(command_id::REPLACE_TEXT, &replaced)
    ));
}

#[test]
fn mark_saved_clears_modified_and_changes_nothing_else() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    insert(&mut engine, &id, 0, "ab");
    insert(&mut engine, &id, 2, "c");
    run(&mut engine, command_id::UNDO, &id);

    let outcome = engine
        .dispatch(command_id::MARK_SAVED, &[("document", id.as_str())])
        .unwrap();

    assert_eq!(outcome.changed(), None);
    let document = outcome.document().unwrap();
    assert!(!document.is_modified());
    assert_eq!(document.text(), "");
    assert!(!document.can_undo());
    assert!(document.can_redo());
}

#[test]
fn typing_after_a_save_starts_a_new_undo_step_so_undo_returns_to_the_saved_text() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    insert(&mut engine, &id, 0, "a");
    insert(&mut engine, &id, 1, "b");
    run(&mut engine, command_id::MARK_SAVED, &id);

    assert!(insert(&mut engine, &id, 2, "c"));

    assert!(!run(&mut engine, command_id::UNDO, &id));
    assert_eq!(text(&engine, &id), "ab");
    assert!(run(&mut engine, command_id::UNDO, &id));
    assert_eq!(text(&engine, &id), "");
}

#[test]
fn a_save_also_ends_a_deletion_step() {
    let mut engine = Engine::new();
    let load = engine
        .dispatch(command_id::LOAD, &[("text", "abc")])
        .unwrap();
    let id = load.id().to_string();
    delete(&mut engine, &id, 2, 3);
    run(&mut engine, command_id::MARK_SAVED, &id);

    assert!(delete(&mut engine, &id, 1, 2));

    assert!(!run(&mut engine, command_id::UNDO, &id));
    assert_eq!(text(&engine, &id), "ab");
}

#[test]
fn undoing_past_the_save_and_redoing_back_to_it_clears_modified() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    insert(&mut engine, &id, 0, "a");
    run(&mut engine, command_id::MARK_SAVED, &id);

    assert!(run(&mut engine, command_id::UNDO, &id));
    assert!(!run(&mut engine, command_id::REDO, &id));
}

#[test]
fn a_save_after_undo_is_reached_again_by_undo() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    insert(&mut engine, &id, 0, "a");
    run(&mut engine, command_id::UNDO, &id);
    run(&mut engine, command_id::MARK_SAVED, &id);

    assert!(run(&mut engine, command_id::REDO, &id));
    assert!(!run(&mut engine, command_id::UNDO, &id));
}

#[test]
fn modified_compares_history_not_text() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    insert(&mut engine, &id, 0, "a");
    run(&mut engine, command_id::MARK_SAVED, &id);

    insert(&mut engine, &id, 1, "b");
    assert!(delete(&mut engine, &id, 1, 2));
    assert_eq!(text(&engine, &id), "a");
}

#[test]
fn an_edit_that_discards_the_saved_state_leaves_the_document_modified_until_the_next_save() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    insert(&mut engine, &id, 0, "a");
    insert(&mut engine, &id, 1, " b");
    insert(&mut engine, &id, 0, "x");
    run(&mut engine, command_id::MARK_SAVED, &id);
    run(&mut engine, command_id::UNDO, &id);

    // The saved text "xa b" was in the redo history this edit discards.
    assert!(insert(&mut engine, &id, 0, "y"));
    assert_eq!(text(&engine, &id), "ya b");

    assert!(run(&mut engine, command_id::UNDO, &id));
    assert!(run(&mut engine, command_id::UNDO, &id));
    assert!(run(&mut engine, command_id::REDO, &id));
    assert!(run(&mut engine, command_id::REDO, &id));
    assert!(!run(&mut engine, command_id::MARK_SAVED, &id));
}

#[test]
fn replacing_the_whole_text_also_discards_a_saved_state_in_the_redo_history() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    insert(&mut engine, &id, 0, "a");
    run(&mut engine, command_id::MARK_SAVED, &id);
    run(&mut engine, command_id::UNDO, &id);

    // The same text as the saved state, but a different state in the history.
    let replaced = [("document", id.as_str()), ("text", "a")];
    assert!(modified(
        engine.dispatch(command_id::REPLACE_TEXT, &replaced)
    ));
}

#[test]
fn a_saved_state_deep_in_the_redo_history_is_discarded_too() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    insert(&mut engine, &id, 0, "a");
    insert(&mut engine, &id, 0, "b");
    run(&mut engine, command_id::MARK_SAVED, &id);
    run(&mut engine, command_id::UNDO, &id);
    run(&mut engine, command_id::UNDO, &id);

    insert(&mut engine, &id, 0, "c");
    insert(&mut engine, &id, 0, "d");

    assert_eq!(text(&engine, &id), "dc");
    assert!(run(&mut engine, command_id::UNDO, &id));
    assert!(run(&mut engine, command_id::REDO, &id));
}

#[test]
fn a_change_to_nothing_and_a_rejected_command_leave_modified_as_it_was() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    let same = [("document", id.as_str()), ("text", "")];
    assert!(!modified(engine.dispatch(command_id::REPLACE_TEXT, &same)));

    insert(&mut engine, &id, 0, "a");
    run(&mut engine, command_id::MARK_SAVED, &id);
    let past_end = [
        ("document", id.as_str()),
        ("start", "5"),
        ("end", "5"),
        ("text", "x"),
    ];
    assert!(
        engine
            .dispatch(command_id::REPLACE_RANGE, &past_end)
            .is_err()
    );

    let document = engine.document(id.parse().unwrap()).unwrap();
    assert!(!document.is_modified());
}

#[test]
fn mark_saved_needs_an_open_document() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    engine
        .dispatch(command_id::CLOSE, &[("document", id.as_str())])
        .unwrap();

    assert_eq!(
        engine
            .dispatch(command_id::MARK_SAVED, &[("document", id.as_str())])
            .unwrap_err(),
        EngineError::UnknownDocument(id.parse().unwrap())
    );
}

#[test]
fn documents_are_saved_separately() {
    let mut engine = Engine::new();
    let first = create(&mut engine);
    let second = create(&mut engine);
    insert(&mut engine, &first, 0, "a");
    insert(&mut engine, &second, 0, "b");

    run(&mut engine, command_id::MARK_SAVED, &first);

    let second = engine.document(second.parse().unwrap()).unwrap();
    assert!(second.is_modified());
}
