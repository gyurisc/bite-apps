//! Behavior of the engine's commands, exercised through the public API only.

use bite_engine::{COMMANDS, Command, DocumentId, Engine, EngineError, Outcome, text_from_utf8};

fn create(engine: &mut Engine) -> DocumentId {
    engine.execute(Command::CreateDocument).unwrap().id()
}

fn replace(engine: &mut Engine, document: DocumentId, text: &str) {
    let text = text.to_owned();
    engine
        .execute(Command::ReplaceText { document, text })
        .unwrap();
}

fn text(engine: &Engine, document: DocumentId) -> &str {
    engine.document(document).unwrap().text()
}

#[test]
fn create_returns_an_empty_document_without_history() {
    let mut engine = Engine::new();

    let document = engine
        .execute(Command::CreateDocument)
        .unwrap()
        .document()
        .unwrap();

    assert_eq!(document.text(), "");
    assert!(!document.can_undo());
    assert!(!document.can_redo());
}

#[test]
fn each_created_document_gets_its_own_id() {
    let mut engine = Engine::new();

    let first = create(&mut engine);
    let second = create(&mut engine);

    assert_ne!(first, second);
}

#[test]
fn replace_text_sets_the_document_text() {
    let mut engine = Engine::new();
    let id = create(&mut engine);

    let text = "hello".to_owned();
    let document = engine
        .execute(Command::ReplaceText { document: id, text })
        .unwrap()
        .document()
        .unwrap();

    assert_eq!(document.text(), "hello");
    assert!(document.can_undo());
}

#[test]
fn replace_text_keeps_the_text_byte_for_byte() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    let original = "line one\r\nline two\n\ttabbed \u{1F34E} e\u{301}\0end";

    replace(&mut engine, id, original);

    assert_eq!(text(&engine, id), original);
}

#[test]
fn undo_restores_the_previous_text() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    replace(&mut engine, id, "first");
    replace(&mut engine, id, "second");

    let document = engine
        .execute(Command::Undo { document: id })
        .unwrap()
        .document()
        .unwrap();

    assert_eq!(document.text(), "first");
    assert!(document.can_redo());
}

#[test]
fn redo_reapplies_the_undone_text() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    replace(&mut engine, id, "first");
    replace(&mut engine, id, "second");
    engine.execute(Command::Undo { document: id }).unwrap();

    let document = engine
        .execute(Command::Redo { document: id })
        .unwrap()
        .document()
        .unwrap();

    assert_eq!(document.text(), "second");
    assert!(!document.can_redo());
}

#[test]
fn undo_and_redo_step_through_every_change_in_order() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    for step in ["a", "ab", "abc"] {
        replace(&mut engine, id, step);
    }

    let mut seen = Vec::new();
    for _ in 0..3 {
        engine.execute(Command::Undo { document: id }).unwrap();
        seen.push(text(&engine, id).to_owned());
    }
    for _ in 0..3 {
        engine.execute(Command::Redo { document: id }).unwrap();
        seen.push(text(&engine, id).to_owned());
    }

    assert_eq!(seen, ["ab", "a", "", "a", "ab", "abc"]);
}

#[test]
fn a_new_edit_after_undo_discards_the_redo_history() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    replace(&mut engine, id, "first");
    engine.execute(Command::Undo { document: id }).unwrap();

    replace(&mut engine, id, "other");

    assert!(!engine.document(id).unwrap().can_redo());
    assert_eq!(
        engine.execute(Command::Redo { document: id }),
        Err(EngineError::NothingToRedo(id))
    );
    assert_eq!(text(&engine, id), "other");
}

#[test]
fn replacing_with_identical_text_records_no_undo_step() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    replace(&mut engine, id, "same");

    replace(&mut engine, id, "same");
    engine.execute(Command::Undo { document: id }).unwrap();

    assert_eq!(text(&engine, id), "");
    assert!(!engine.document(id).unwrap().can_undo());
}

#[test]
fn replacing_with_identical_text_keeps_the_redo_history() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    replace(&mut engine, id, "first");
    engine.execute(Command::Undo { document: id }).unwrap();

    replace(&mut engine, id, "");

    assert!(engine.document(id).unwrap().can_redo());
}

#[test]
fn documents_keep_separate_text_and_history() {
    let mut engine = Engine::new();
    let first = create(&mut engine);
    let second = create(&mut engine);
    replace(&mut engine, first, "one");
    replace(&mut engine, second, "two");

    engine.execute(Command::Undo { document: first }).unwrap();

    assert_eq!(text(&engine, first), "");
    assert_eq!(text(&engine, second), "two");
    assert!(engine.document(second).unwrap().can_undo());
}

#[test]
fn undo_without_history_is_an_error_and_changes_nothing() {
    let mut engine = Engine::new();
    let id = create(&mut engine);

    let result = engine.execute(Command::Undo { document: id });

    assert_eq!(result, Err(EngineError::NothingToUndo(id)));
    assert_eq!(text(&engine, id), "");
}

#[test]
fn redo_without_undone_changes_is_an_error_and_changes_nothing() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    replace(&mut engine, id, "kept");

    let result = engine.execute(Command::Redo { document: id });

    assert_eq!(result, Err(EngineError::NothingToRedo(id)));
    assert_eq!(text(&engine, id), "kept");
}

#[test]
fn commands_for_an_unknown_document_are_errors() {
    let mut engine = Engine::new();
    let missing: DocumentId = "7".parse().unwrap();
    let text = "x".to_owned();

    for command in [
        Command::ReplaceText {
            document: missing,
            text,
        },
        Command::Undo { document: missing },
        Command::Redo { document: missing },
    ] {
        assert_eq!(
            engine.execute(command),
            Err(EngineError::UnknownDocument(missing))
        );
    }
    assert!(engine.document(missing).is_none());
}

#[test]
fn dispatch_runs_each_command_by_its_stable_id() {
    let mut engine = Engine::new();

    let id = engine
        .dispatch("document.create", &[])
        .unwrap()
        .id()
        .to_string();
    let params = [("document", id.as_str()), ("text", "hello")];
    let replaced = engine
        .dispatch("document.replace_text", &params)
        .unwrap()
        .document()
        .unwrap();
    assert_eq!(replaced.text(), "hello");

    let target = [("document", id.as_str())];
    let undone = engine
        .dispatch("document.undo", &target)
        .unwrap()
        .document()
        .unwrap();
    assert_eq!(undone.text(), "");
    let redone = engine
        .dispatch("document.redo", &target)
        .unwrap()
        .document()
        .unwrap();
    assert_eq!(redone.text(), "hello");
}

#[test]
fn parse_accepts_parameters_in_any_order() {
    let parsed = Command::parse(
        "document.replace_text",
        &[("text", "hi"), ("document", "3")],
    );

    assert_eq!(
        parsed,
        Ok(Command::ReplaceText {
            document: "3".parse().unwrap(),
            text: "hi".to_owned(),
        })
    );
}

#[test]
fn dispatch_rejects_an_unknown_command_id() {
    let mut engine = Engine::new();

    for id in ["document.delete", "Document.Create", "document.create ", ""] {
        assert_eq!(
            engine.dispatch(id, &[]),
            Err(EngineError::UnknownCommand(id.to_owned()))
        );
    }
    assert!(engine.document("1".parse().unwrap()).is_none());
}

#[test]
fn dispatch_rejects_a_parameter_the_command_does_not_take() {
    let mut engine = Engine::new();

    let result = engine.dispatch("document.create", &[("text", "x")]);

    assert_eq!(
        result,
        Err(EngineError::UnknownParameter {
            command: "document.create",
            name: "text".to_owned(),
        })
    );
    assert!(engine.document("1".parse().unwrap()).is_none());
}

#[test]
fn dispatch_rejects_a_repeated_parameter() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    let document = id.to_string();

    let result = engine.dispatch(
        "document.replace_text",
        &[
            ("document", document.as_str()),
            ("text", "a"),
            ("text", "b"),
        ],
    );

    assert_eq!(
        result,
        Err(EngineError::DuplicateParameter {
            command: "document.replace_text",
            name: "text".to_owned(),
        })
    );
    assert_eq!(text(&engine, id), "");
}

#[test]
fn dispatch_rejects_a_missing_parameter() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    let document = id.to_string();

    assert_eq!(
        engine.dispatch("document.replace_text", &[("document", document.as_str())]),
        Err(EngineError::MissingParameter {
            command: "document.replace_text",
            name: "text",
        })
    );
    assert_eq!(
        engine.dispatch("document.undo", &[]),
        Err(EngineError::MissingParameter {
            command: "document.undo",
            name: "document",
        })
    );
    assert_eq!(text(&engine, id), "");
}

#[test]
fn dispatch_rejects_a_malformed_document_id() {
    let mut engine = Engine::new();
    create(&mut engine);

    for value in ["", "abc", "-1", "1.5", " 1", "1 ", "18446744073709551616"] {
        assert_eq!(
            engine.dispatch("document.undo", &[("document", value)]),
            Err(EngineError::InvalidDocumentId {
                command: "document.undo",
                value: value.to_owned(),
            })
        );
    }
}

#[test]
fn dispatch_reports_a_document_that_does_not_exist() {
    let mut engine = Engine::new();
    create(&mut engine);

    let result = engine.dispatch("document.redo", &[("document", "99")]);

    assert_eq!(
        result,
        Err(EngineError::UnknownDocument("99".parse().unwrap()))
    );
}

#[test]
fn error_messages_name_the_command_and_the_offending_input() {
    let cases = [
        (Command::parse("document.nope", &[]), vec!["document.nope"]),
        (
            Command::parse("document.create", &[("colour", "red")]),
            vec!["document.create", "colour"],
        ),
        (
            Command::parse("document.undo", &[("document", "1"), ("document", "2")]),
            vec!["document.undo", "document", "more than once"],
        ),
        (
            Command::parse("document.replace_text", &[("document", "1")]),
            vec!["document.replace_text", "text"],
        ),
        (
            Command::parse("document.redo", &[("document", "seven")]),
            vec!["document.redo", "seven"],
        ),
    ];

    for (result, expected_parts) in cases {
        let message = result.unwrap_err().to_string();
        for part in expected_parts {
            assert!(
                message.contains(part),
                "{message:?} should mention {part:?}"
            );
        }
    }
}

#[test]
fn state_errors_name_the_document() {
    let mut engine = Engine::new();
    let id = create(&mut engine);

    let nothing_to_undo = engine.execute(Command::Undo { document: id }).unwrap_err();
    let unknown = engine
        .dispatch("document.undo", &[("document", "42")])
        .unwrap_err();

    let nothing_to_undo = nothing_to_undo.to_string();
    assert!(nothing_to_undo.contains(&id.to_string()) && nothing_to_undo.contains("undo"));
    assert!(unknown.to_string().contains("42"));
}

#[test]
fn registry_lists_exactly_the_parameters_each_command_requires() {
    let ids: Vec<&str> = COMMANDS.iter().map(|spec| spec.id).collect();
    assert_eq!(
        ids,
        [
            "document.create",
            "document.load",
            "document.replace_text",
            "document.replace_range",
            "document.undo",
            "document.redo",
            "document.close",
            "document.mark_saved",
        ]
    );

    for spec in COMMANDS {
        let all: Vec<(&str, &str)> = spec.params.iter().map(|name| (*name, "1")).collect();
        assert!(
            Command::parse(spec.id, &all).is_ok(),
            "{} should accept its listed parameters",
            spec.id
        );

        for omitted in spec.params {
            let rest: Vec<(&str, &str)> = all
                .iter()
                .copied()
                .filter(|(name, _)| name != omitted)
                .collect();
            assert_eq!(
                Command::parse(spec.id, &rest),
                Err(EngineError::MissingParameter {
                    command: spec.id,
                    name: omitted,
                }),
            );
        }
    }
}

#[test]
fn a_document_id_converts_to_the_number_it_is_written_as() {
    let id: DocumentId = "7".parse().unwrap();

    assert_eq!(u64::from(id), 7);
    assert_eq!(id.to_string(), "7");
}

fn load(engine: &mut Engine, text: &str) -> DocumentId {
    engine
        .dispatch("document.load", &[("text", text)])
        .unwrap()
        .id()
}

#[test]
fn load_creates_a_document_holding_the_text_without_history() {
    let mut engine = Engine::new();

    let document = engine
        .dispatch("document.load", &[("text", "from a file\r\n")])
        .unwrap()
        .document()
        .unwrap();

    assert_eq!(document.text(), "from a file\r\n");
    assert!(!document.can_undo());
    assert!(!document.can_redo());
}

#[test]
fn loading_is_not_an_undoable_change() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "loaded");

    let result = engine.execute(Command::Undo { document: id });

    assert_eq!(result, Err(EngineError::NothingToUndo(id)));
    assert_eq!(text(&engine, id), "loaded");
}

#[test]
fn undoing_every_edit_returns_to_the_loaded_text() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "loaded");
    replace(&mut engine, id, "edited once");
    replace(&mut engine, id, "edited twice");

    engine.execute(Command::Undo { document: id }).unwrap();
    engine.execute(Command::Undo { document: id }).unwrap();

    assert_eq!(text(&engine, id), "loaded");
    assert!(!engine.document(id).unwrap().can_undo());
}

#[test]
fn loaded_and_created_documents_share_one_id_sequence() {
    let mut engine = Engine::new();

    let created = create(&mut engine);
    let loaded = load(&mut engine, "x");

    assert_ne!(created, loaded);
    assert_eq!(text(&engine, created), "");
    assert_eq!(text(&engine, loaded), "x");
}

#[test]
fn load_requires_its_text() {
    let mut engine = Engine::new();

    let result = engine.dispatch("document.load", &[]);

    assert_eq!(
        result,
        Err(EngineError::MissingParameter {
            command: "document.load",
            name: "text",
        })
    );
}

#[test]
fn file_bytes_survive_decoding_loading_and_saving_unchanged() {
    let files: [&[u8]; 11] = [
        b"",
        b"plain",
        b"unix\nlines\n",
        b"windows\r\nlines\r\n",
        b"old mac\rlines\r",
        b"mixed\r\nline\nendings\r",
        b"no trailing newline",
        b"\n\n\n",
        b"\xEF\xBB\xBFstarts with a byte-order mark\n",
        b"nul\0inside",
        "accents \u{E9}, emoji \u{1F34E}, combining e\u{301}\n".as_bytes(),
    ];

    for file in files {
        let mut engine = Engine::new();
        let decoded = text_from_utf8(file).unwrap();
        let id = load(&mut engine, decoded);

        assert_eq!(text(&engine, id).as_bytes(), file);
    }
}

#[test]
fn bytes_that_are_not_utf8_are_rejected_at_the_first_bad_byte() {
    let cases: [(&[u8], usize); 6] = [
        (b"\xFF", 0),
        // The offset counts bytes: "é" is two bytes, one character.
        (b"caf\xC3\xA9 \xFF", 6),
        (b"ab\xFFcd", 2),
        (b"caf\xE9 in Latin-1", 3),
        (b"cut off mid-character \xE2\x82", 22),
        (b"\xFF\xFEU\0T\0F\0-\x001\x006\0", 0),
    ];

    for (bytes, offset) in cases {
        assert_eq!(
            text_from_utf8(bytes),
            Err(EngineError::InvalidUtf8 { offset })
        );
    }
}

#[test]
fn the_invalid_utf8_message_says_where_the_bad_byte_is() {
    let message = text_from_utf8(b"0123456\xFF").unwrap_err().to_string();

    assert!(message.contains("UTF-8"), "{message}");
    assert!(message.contains('7'), "{message}");
}

#[test]
fn close_removes_the_document_and_reports_its_id() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    replace(&mut engine, id, "unsaved");

    let closed = engine.execute(Command::Close { document: id }).unwrap();

    assert_eq!(closed, Outcome::Closed(id));
    assert_eq!(closed.id(), id);
    assert_eq!(closed.document(), None);
    assert!(engine.document(id).is_none());
}

#[test]
fn commands_naming_a_closed_document_fail_as_for_an_unknown_document() {
    let mut engine = Engine::new();
    let id = create(&mut engine);
    engine.execute(Command::Close { document: id }).unwrap();
    let text = "x".to_owned();

    for command in [
        Command::ReplaceText {
            document: id,
            text: text.clone(),
        },
        Command::ReplaceRange {
            document: id,
            start: 0,
            end: 0,
            text,
        },
        Command::Undo { document: id },
        Command::Redo { document: id },
        Command::Close { document: id },
    ] {
        assert_eq!(
            engine.execute(command),
            Err(EngineError::UnknownDocument(id))
        );
    }
}

#[test]
fn a_closed_documents_id_is_not_reused() {
    let mut engine = Engine::new();
    let first = create(&mut engine);
    let last = create(&mut engine);
    engine.execute(Command::Close { document: first }).unwrap();
    engine.execute(Command::Close { document: last }).unwrap();

    let next = create(&mut engine);

    assert_ne!(next, first);
    assert_ne!(next, last);
}

#[test]
fn closing_one_document_leaves_the_others_and_their_history() {
    let mut engine = Engine::new();
    let closed = create(&mut engine);
    let kept = create(&mut engine);
    replace(&mut engine, kept, "kept");

    engine
        .dispatch("document.close", &[("document", &closed.to_string())])
        .unwrap();

    assert_eq!(text(&engine, kept), "kept");
    let undone = engine
        .execute(Command::Undo { document: kept })
        .unwrap()
        .document()
        .unwrap();
    assert_eq!(undone.text(), "");
}

#[test]
fn an_outcome_with_a_document_reports_that_document() {
    let mut engine = Engine::new();

    let outcome = engine.execute(Command::CreateDocument).unwrap();

    assert_eq!(outcome.id(), outcome.document().unwrap().id());
}
