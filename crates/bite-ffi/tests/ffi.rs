//! The C interface, called the way a foreign caller would call it.

use bite_ffi::{
    BiteEngine, BiteParam, BiteReply, BiteStr, bite_engine_dispatch, bite_engine_free,
    bite_engine_new, bite_reply_free,
};
use std::ptr;

fn bytes(value: &[u8]) -> BiteStr {
    BiteStr {
        ptr: value.as_ptr(),
        len: value.len(),
    }
}

fn s(value: &str) -> BiteStr {
    bytes(value.as_bytes())
}

/// What a reply said, copied out so the reply itself can be freed.
#[derive(Debug, PartialEq)]
struct Outcome {
    ok: bool,
    document: u64,
    text: String,
    can_undo: bool,
    can_redo: bool,
}

fn take(reply: BiteReply) -> Outcome {
    let text = unsafe { std::slice::from_raw_parts(reply.text, reply.text_len) };
    let outcome = Outcome {
        ok: reply.ok,
        document: reply.document,
        text: String::from_utf8(text.to_vec()).unwrap(),
        can_undo: reply.can_undo,
        can_redo: reply.can_redo,
    };
    unsafe { bite_reply_free(reply) };
    outcome
}

fn dispatch(engine: *mut BiteEngine, command: &str, params: &[(&str, &str)]) -> Outcome {
    let params: Vec<BiteParam> = params
        .iter()
        .map(|(name, value)| BiteParam {
            name: s(name),
            value: s(value),
        })
        .collect();
    take(unsafe { bite_engine_dispatch(engine, s(command), params.as_ptr(), params.len()) })
}

/// Runs `body` with a fresh engine holding one empty document.
fn with_document(body: impl FnOnce(*mut BiteEngine, &str)) {
    let engine = bite_engine_new();
    let created = dispatch(engine, "document.create", &[]);
    assert!(created.ok, "{}", created.text);
    body(engine, &created.document.to_string());
    unsafe { bite_engine_free(engine) };
}

#[test]
fn create_replies_with_an_empty_document() {
    let engine = bite_engine_new();

    let reply = take(unsafe { bite_engine_dispatch(engine, s("document.create"), ptr::null(), 0) });

    assert_eq!(
        reply,
        Outcome {
            ok: true,
            document: 1,
            text: String::new(),
            can_undo: false,
            can_redo: false,
        }
    );
    unsafe { bite_engine_free(engine) };
}

#[test]
fn replace_text_replies_with_the_new_document_state() {
    with_document(|engine, id| {
        let reply = dispatch(
            engine,
            "document.replace_text",
            &[("document", id), ("text", "hello")],
        );

        assert!(reply.ok);
        assert_eq!(reply.document.to_string(), id);
        assert_eq!(reply.text, "hello");
        assert!(reply.can_undo);
        assert!(!reply.can_redo);
    });
}

#[test]
fn the_engine_keeps_state_between_calls() {
    with_document(|engine, id| {
        dispatch(
            engine,
            "document.replace_text",
            &[("document", id), ("text", "first")],
        );
        dispatch(
            engine,
            "document.replace_text",
            &[("document", id), ("text", "second")],
        );

        let undone = dispatch(engine, "document.undo", &[("document", id)]);

        assert_eq!(undone.text, "first");
        assert!(undone.can_redo);
    });
}

#[test]
fn text_with_nul_bytes_and_non_ascii_survives_the_round_trip() {
    with_document(|engine, id| {
        let original = "a\0b \u{1F34E} e\u{301}\r\n";

        let reply = dispatch(
            engine,
            "document.replace_text",
            &[("document", id), ("text", original)],
        );

        assert_eq!(reply.text, original);
    });
}

#[test]
fn range_edits_reply_with_the_new_text_and_reject_offsets_inside_a_character() {
    with_document(|engine, id| {
        let edit = |start, end, text| {
            let params = [
                ("document", id),
                ("start", start),
                ("end", end),
                ("text", text),
            ];
            dispatch(engine, "document.replace_range", &params)
        };

        let typed = edit("0", "0", "caf\u{e9}");
        let inside = edit("4", "4", "x");

        assert_eq!(typed.text, "caf\u{e9}");
        assert!(typed.can_undo);
        assert!(!inside.ok);
        assert!(inside.text.contains("`start` 4"), "{}", inside.text);
        let undone = dispatch(engine, "document.undo", &[("document", id)]);
        assert_eq!(undone.text, "");
    });
}

#[test]
fn close_replies_with_the_closed_id_and_later_commands_fail() {
    with_document(|engine, id| {
        dispatch(
            engine,
            "document.replace_text",
            &[("document", id), ("text", "unsaved")],
        );

        let closed = dispatch(engine, "document.close", &[("document", id)]);
        let after = dispatch(engine, "document.undo", &[("document", id)]);

        assert_eq!(
            closed,
            Outcome {
                ok: true,
                document: id.parse().unwrap(),
                text: String::new(),
                can_undo: false,
                can_redo: false,
            }
        );
        assert!(!after.ok);
        assert!(after.text.contains(id), "{}", after.text);
    });
}

#[test]
fn an_engine_error_comes_back_as_a_failed_reply_with_its_message() {
    with_document(|engine, _| {
        let reply = dispatch(engine, "document.nope", &[]);

        assert!(!reply.ok);
        assert!(reply.text.contains("document.nope"), "{}", reply.text);
    });
}

#[test]
fn a_failed_command_leaves_the_document_unchanged() {
    with_document(|engine, id| {
        dispatch(
            engine,
            "document.replace_text",
            &[("document", id), ("text", "kept")],
        );

        let failed = dispatch(engine, "document.replace_text", &[("document", id)]);
        let after = dispatch(engine, "document.undo", &[("document", id)]);

        assert!(!failed.ok);
        assert_eq!(after.text, "");
        assert!(!after.can_undo);
    });
}

#[test]
fn invalid_utf8_is_reported_instead_of_trusted() {
    with_document(|engine, id| {
        let invalid = [0x66, 0xFF, 0x6F];

        let bad_command =
            take(unsafe { bite_engine_dispatch(engine, bytes(&invalid), ptr::null(), 0) });
        let params = [
            BiteParam {
                name: s("document"),
                value: s(id),
            },
            BiteParam {
                name: s("text"),
                value: bytes(&invalid),
            },
        ];
        let bad_value = take(unsafe {
            bite_engine_dispatch(
                engine,
                s("document.replace_text"),
                params.as_ptr(),
                params.len(),
            )
        });

        assert!(!bad_command.ok);
        assert!(bad_command.text.contains("UTF-8"), "{}", bad_command.text);
        assert!(!bad_value.ok);
        assert!(bad_value.text.contains("UTF-8"), "{}", bad_value.text);
    });
}

#[test]
fn null_pointers_are_reported_instead_of_dereferenced() {
    with_document(|engine, _| {
        let no_engine = take(unsafe {
            bite_engine_dispatch(ptr::null_mut(), s("document.create"), ptr::null(), 0)
        });
        let no_params =
            take(unsafe { bite_engine_dispatch(engine, s("document.undo"), ptr::null(), 1) });
        let null_text = BiteStr {
            ptr: ptr::null(),
            len: 3,
        };
        let no_command = take(unsafe { bite_engine_dispatch(engine, null_text, ptr::null(), 0) });

        assert!(!no_engine.ok);
        assert!(!no_params.ok);
        assert!(!no_command.ok);
    });
}

#[test]
fn an_empty_string_may_have_a_null_pointer() {
    with_document(|engine, id| {
        let empty = BiteStr {
            ptr: ptr::null(),
            len: 0,
        };
        let params = [
            BiteParam {
                name: s("document"),
                value: s(id),
            },
            BiteParam {
                name: s("text"),
                value: empty,
            },
        ];
        dispatch(
            engine,
            "document.replace_text",
            &[("document", id), ("text", "something")],
        );

        let reply = take(unsafe {
            bite_engine_dispatch(
                engine,
                s("document.replace_text"),
                params.as_ptr(),
                params.len(),
            )
        });

        assert!(reply.ok, "{}", reply.text);
        assert_eq!(reply.text, "");
    });
}

#[test]
fn freeing_a_null_engine_does_nothing() {
    unsafe { bite_engine_free(ptr::null_mut()) };
}
