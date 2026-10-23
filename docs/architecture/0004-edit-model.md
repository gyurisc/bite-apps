# ADR 0004: Range edits, byte offsets, and engine-owned undo

- Status: accepted for the first slice; ADR 0005 settles the two questions it leaves to M5, composed input and the selection after undo and redo.
- Date: 2026-10-17

## Context

M5 connects the SwiftUI app to the engine. The engine changes text only through `document.replace_text`, which replaces the whole text, and it records one undo step per change.

A native text view changes text in small pieces. SwiftUI's `TextEditor` reports only the whole new text through a binding. An `NSTextView` wrapped for SwiftUI asks its delegate before each change, giving the range to replace, counted in UTF-16 code units, and the replacement string. Sending the whole text for every keystroke would make each undo step a single keystroke and would hide what changed.

Four questions follow: what command an editor uses for a small change, what its positions count, how keystrokes become undo steps, and whether the engine or the text view owns undo. Separately, an app with windows needs to release a document when its window closes.

## Options considered

### Positions

- UTF-16 code units: what `NSTextView` reports, so the app passes them through unchanged. The engine would scan the text to convert each position, and scripts and agents would count in a unit they rarely see.
- Unicode scalar values, Rust's `char`: closer to what a script means by a character. The engine still scans to convert, and Swift converts through its `unicodeScalars` view.
- Grapheme clusters: what a person sees as one character. Swift and Rust can disagree on cluster boundaries when they follow different Unicode versions, so the two sides could address different text.
- UTF-8 bytes: how the engine stores text, so a position is checked and applied without scanning. Swift converts through its `utf8` view. Scripts count bytes, which differs from characters outside ASCII.

### Undo

- The text view owns undo and the engine keeps no history for editor changes. This contradicts AGENTS.md, and the CLI and agents would see a different history from the app.
- The text view's undo manager records engine commands. Two histories have to be kept in step, and grouping follows the text view's rules, which the engine cannot test.
- The engine owns undo and groups keystrokes itself. The app turns off the text view's undo and routes Undo and Redo to engine commands.

## Decision

Add `document.replace_range`, use UTF-8 byte offsets, group adjacent typing into one undo step in the engine, let the engine own undo, and add `document.close`.

### `document.replace_range`

Parameters `document`, `start`, `end`, and `text`. It replaces the bytes from `start` up to but not including `end` with `text`.

- `start` and `end` are byte offsets written as ASCII decimal digits, with no sign.
- `start <= end <=` the text's length in bytes.
- Each offset falls on a boundary between Unicode scalar values, as Rust's `str::is_char_boundary` checks. Such a boundary can fall inside what a reader sees as one character, for example between a letter and a combining accent, or between CR and LF.

Any other value is rejected with an error that names the parameter and its value, or both parameters when `start > end`, and the document is unchanged. Replacing a range with identical text is not a change, as for `document.replace_text`.

`document.replace_text` stays for hosts that have only the whole text, such as the CLI.

### Undo steps

The engine sorts each `document.replace_range` that changes the text by what it does:

- an insertion has an empty range and non-empty text;
- a deletion has a non-empty range and empty text;
- a replacement has both.

An insertion joins the current undo step when the previous change was an insertion or replacement and the new text goes where the previous text ended, as typing does. A deletion joins it when the previous change was a deletion and the new range ends where the previous one started, as Backspace does, or starts where it started, as Forward Delete does. Every other change starts a new step.

Only commands that change the document count as the previous change. A rejected command, or one that changes nothing, leaves grouping as it was. `document.replace_text`, `document.undo`, and `document.redo` each end the current step.

The engine sees only the edits, not the keys or the selection behind them. Grouping depends only on the order of edits, not on timing, so the CLI, tests, and the app produce the same steps from the same edits. These cases become tests:

| Previous change | Next `document.replace_range` | Result |
|---|---|---|
| Insert `ab` at 3 | Insert at 5 | Joins |
| Insert `ab` at 3 | Insert at 4 | New step |
| Replace 2..5 with `x` | Insert at 3 | Joins |
| Delete 5..6 | Delete 4..5 | Joins |
| Delete 5..6 | Delete 5..6 | Joins |
| Delete 5..6 | Delete 2..5 | Joins |
| Delete 5..6 | Insert at 5 | New step |
| Insert `a` at 3 | Delete 3..4 | New step |
| Insert `a` at 3, then a rejected command | Insert at 4 | Joins |
| `document.undo`, `document.redo`, or `document.replace_text` | Any edit | New step |

### Ownership of undo

The engine owns undo. The app turns off the text view's own undo and replaces the Undo and Redo menu commands with ones that dispatch `document.undo` and `document.redo` when the document's text view has focus, enabled from the reply's undo and redo availability. Other fields, such as a find field, keep their own undo. After an undo or redo, the app shows the reply's text. M5 keeps the standard Undo and Redo menu items: the document's text view answers their actions while it has focus, and other fields answer them as usual.

### `document.close`

Parameter `document`. It removes the document and its history. Later commands naming it fail as for an unknown document, and its ID is not reused.

In M5 the app holds one engine. Each window creates its own document in it when the window appears, and when the window closes, the window ends any composition, which commits it, then dispatches `document.close`. Quitting the app closes no documents: AppKit does not report a window closing then, and nothing is lost because M5 does not save. M6's save and quit handling needs its own path through every window's document. ADR 0006 gives each window an `NSDocument`, which dispatches `document.close` when it closes, and ends every window's composition before quitting reviews unsaved documents.

Commands that only read a document are left for M7, when the agent interface needs them; hosts already receive the text in every reply.

## Consequences

- The app converts each UTF-16 range to byte offsets before dispatching, through `String.Index` and `samePosition(in:)` on the `utf8` view, and refuses a change whose position falls inside a surrogate pair. The engine never sees UTF-16.
- The intended M5 flow dispatches from the text view's delegate before the view applies a change and lets the change through only if the engine accepts it, so the view and the engine apply the same edit. M5 confirms this works with the text view.
- One action can change several ranges, for example Delete with several selections or Replace All. The intended M5 flow handles a change to several ranges as one `document.replace_text` of the resulting text, which is one step and all or nothing. M5 found that this covers only changes that reach the text view's delegate as one call with several ranges, such as Replace All. Typing, deleting, cutting, and changing case with several selections reach it as one call per range, so each range is its own edit and its own undo step, and a rejection partway leaves the earlier ranges applied. The view and the engine still agree, because each edit is checked. Making such an action one step needs a way for the host to group edits, like the way to end a step proposed below.
- Input methods that compose characters, such as those for Japanese, change the text several times before committing it. M5 decides whether those intermediate changes reach the engine and how they group.
- Because the engine does not see the selection, some actions join a step a user might expect to be separate: pasting where typing ended, deleting a selection that ends where a Backspace started, and typing after moving the cursor away and back. If that proves confusing in M5, add a way for a host to end a step.
- After an undo or redo the app has only the new whole text, so it cannot select the restored text or keep the scroll position without comparing the texts. M5 decides between comparing in Swift and adding the changed range to the reply.
- `document.close` returns no document, so the return type of `Engine::dispatch` changes to allow a command that leaves none. The C reply for it reports success with the closed document's ID, empty text, and no undo or redo. This amends ADR 0002, which says adding a command does not change the bridge: `bite-ffi` changes once to build that reply.
- Every reply still carries the whole text, and every undo step still stores a whole-text snapshot. That is linear in the document's size per keystroke. Revisit, for example with replies that carry the changed range or with history that stores edits, if large documents are slow.
- The CLI lists `document.replace_range` in its help without changes, because it lists every command that takes a document. `document.close` takes a document too, so the CLI needs an explicit exclusion to refuse it as a usage error, as it refuses `document.create` and `document.load`, because it saves the document after the last command. Its test that every such command is listed changes to match.
