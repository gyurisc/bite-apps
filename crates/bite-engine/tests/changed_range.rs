//! The changed range each reply reports, as ADR 0005 defines it: the byte
//! range of the new text that differs from the text before the command.

use bite_engine::{Command, DocumentId, Engine};
use std::ops::Range;

fn changed(engine: &mut Engine, command: Command) -> Option<Range<usize>> {
    engine.execute(command).unwrap().changed()
}

fn load(engine: &mut Engine, text: &str) -> DocumentId {
    let text = text.to_owned();
    engine.execute(Command::LoadDocument { text }).unwrap().id()
}

fn replace_text(document: DocumentId, text: &str) -> Command {
    let text = text.to_owned();
    Command::ReplaceText { document, text }
}

fn replace_range(document: DocumentId, start: usize, end: usize, text: &str) -> Command {
    let text = text.to_owned();
    Command::ReplaceRange {
        document,
        start,
        end,
        text,
    }
}

/// The range `replace_text` reports for changing `old` to `new`.
fn whole_text_change(old: &str, new: &str) -> Option<Range<usize>> {
    let mut engine = Engine::new();
    let id = load(&mut engine, old);
    changed(&mut engine, replace_text(id, new))
}

#[test]
fn a_created_document_reports_its_empty_text_as_changed() {
    let mut engine = Engine::new();

    assert_eq!(changed(&mut engine, Command::CreateDocument), Some(0..0));
}

#[test]
fn a_loaded_document_reports_its_whole_text_as_changed() {
    let mut engine = Engine::new();
    let text = "caf\u{e9}\n".to_owned();

    assert_eq!(
        changed(&mut engine, Command::LoadDocument { text }),
        Some(0..6)
    );
}

#[test]
fn the_range_leaves_out_what_the_texts_share_at_each_end() {
    assert_eq!(whole_text_change("hello", "help!"), Some(3..5));
    assert_eq!(
        whole_text_change("one two three", "one 2 three"),
        Some(4..5)
    );
    assert_eq!(whole_text_change("", "new"), Some(0..3));
    assert_eq!(whole_text_change("old", ""), Some(0..0));
}

#[test]
fn removed_text_is_reported_as_an_empty_range_where_it_was() {
    assert_eq!(whole_text_change("abc", "ac"), Some(1..1));
    assert_eq!(whole_text_change("abc", "ab"), Some(2..2));
}

#[test]
fn the_common_prefix_is_taken_before_the_suffix() {
    assert_eq!(whole_text_change("aa", "aaa"), Some(2..3));
    assert_eq!(whole_text_change("aaa", "aa"), Some(2..2));
    assert_eq!(whole_text_change("abab", "ab"), Some(2..2));
}

#[test]
fn the_start_moves_to_a_scalar_boundary_before_the_suffix_is_taken() {
    // "è" is C3 A8 and "é" is C3 A9: the shared C3 is given back to the
    // range first, so the suffix may then match the whole "è".
    assert_eq!(whole_text_change("\u{e8}", "\u{e9}a\u{e8}"), Some(0..3));
    assert_eq!(
        whole_text_change("\u{1F350}\u{1F350}\u{1F34E}", "\u{1F34E}"),
        Some(0..0)
    );
}

#[test]
fn the_range_widens_to_whole_scalar_values() {
    // "é" is C3 A9 and "è" is C3 A8: they share their first byte.
    assert_eq!(whole_text_change("\u{e9}", "\u{e8}"), Some(0..2));
    // "é" is C3 A9 and "ũ" is C5 A9: they share their last byte.
    assert_eq!(whole_text_change("\u{e9}", "\u{169}"), Some(0..2));
    assert_eq!(whole_text_change("a\u{e9}b", "a\u{169}b"), Some(1..3));
    // The apple is F0 9F 8D 8E and the pear F0 9F 8D 90.
    assert_eq!(whole_text_change("\u{1F34E}", "\u{1F350}"), Some(0..4));
}

#[test]
fn identical_text_reports_no_change() {
    assert_eq!(whole_text_change("same", "same"), None);
}

#[test]
fn a_range_edit_reports_only_the_bytes_that_differ() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "abcd");

    assert_eq!(
        changed(&mut engine, replace_range(id, 2, 2, "xy")),
        Some(2..4)
    );
    assert_eq!(
        changed(&mut engine, replace_range(id, 1, 5, "bX")),
        Some(2..3)
    );
    assert_eq!(
        changed(&mut engine, replace_range(id, 0, 1, "")),
        Some(0..0)
    );
    assert_eq!(changed(&mut engine, replace_range(id, 0, 0, "")), None);
    assert_eq!(changed(&mut engine, replace_range(id, 0, 1, "b")), None);
}

#[test]
fn a_range_edit_reports_a_range_inside_the_edit() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "aa");

    // Comparing the whole texts would report 2..3.
    assert_eq!(
        changed(&mut engine, replace_range(id, 1, 1, "a")),
        Some(1..2)
    );
}

/// A fixed-seed generator, so the test runs the same every time.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) % n as u64) as usize
    }
}

/// The scalar boundaries of `text`.
fn boundaries(text: &str) -> Vec<usize> {
    let mut all: Vec<usize> = text.char_indices().map(|(at, _)| at).collect();
    all.push(text.len());
    all
}

#[test]
fn every_reported_range_rebuilds_the_new_text_from_the_old() {
    let pieces = [
        "a",
        "b",
        "\u{e9}",
        "\u{e8}",
        "\u{301}",
        "\u{1F34E}",
        "\u{1F350}",
        "\n",
        "\r\n",
        "\r",
    ];
    let mut random = Lcg(7);
    let mut engine = Engine::new();
    // A CRLF document, so range edits convert the line breaks they insert.
    let id = load(&mut engine, "\r\n");

    for _ in 0..20_000 {
        let old = engine.document(id).unwrap().text().to_owned();
        let piece_count = random.below(3);
        let text: String = (0..piece_count)
            .map(|_| pieces[random.below(pieces.len())])
            .collect();
        let command = match random.below(5) {
            0 => Command::Undo { document: id },
            1 => Command::Redo { document: id },
            2 => replace_text(id, &(text.clone() + &old)),
            _ => {
                let at = boundaries(&old);
                let start = at[random.below(at.len())];
                let end = at[random.below(at.len())].max(start);
                replace_range(id, start, end, &text)
            }
        };
        let Ok(outcome) = engine.execute(command) else {
            continue;
        };
        let changed = outcome.changed();
        let new = engine.document(id).unwrap().text().to_owned();

        let Some(range) = changed else {
            assert_eq!(old, new);
            continue;
        };
        let old_end = old.len() - (new.len() - range.end);
        assert!(range.start <= old_end, "{old:?} -> {new:?}: {range:?}");
        for (text, at) in [(&new, range.start), (&new, range.end), (&old, old_end)] {
            assert!(text.is_char_boundary(at), "{old:?} -> {new:?}: {range:?}");
        }
        let rebuilt = format!(
            "{}{}{}",
            &old[..range.start],
            &new[range.clone()],
            &old[old_end..]
        );
        assert_eq!(rebuilt, new, "{old:?} -> {new:?}: {range:?}");
    }
}

#[test]
fn a_range_edit_widens_to_whole_scalar_values() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "x\u{e9}y");

    assert_eq!(
        changed(&mut engine, replace_range(id, 1, 3, "\u{e8}")),
        Some(1..3)
    );
}

#[test]
fn undo_and_redo_report_what_they_changed() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "say ");
    for (at, key) in [(4, "h"), (5, "i")] {
        engine.execute(replace_range(id, at, at, key)).unwrap();
    }

    let undone = changed(&mut engine, Command::Undo { document: id });
    let redone = changed(&mut engine, Command::Redo { document: id });

    assert_eq!(undone, Some(4..4));
    assert_eq!(redone, Some(4..6));
}

#[test]
fn an_undo_that_ends_where_its_step_began_reports_no_change() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "xabcx");
    engine.execute(replace_range(id, 1, 4, "a")).unwrap();
    // Typing where the replacement ended joins its undo step.
    engine.execute(replace_range(id, 2, 2, "bc")).unwrap();

    let undone = changed(&mut engine, Command::Undo { document: id });

    assert_eq!(undone, None);
    assert_eq!(engine.document(id).unwrap().text(), "xabcx");
}

#[test]
fn closing_a_document_reports_no_change() {
    let mut engine = Engine::new();
    let id = load(&mut engine, "text");

    assert_eq!(changed(&mut engine, Command::Close { document: id }), None);
}
