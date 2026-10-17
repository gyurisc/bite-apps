# ADR 0005: Composed input reaches the engine on commit; replies report the changed range

- Status: accepted for the first slice
- Date: 2026-10-17

## Context

ADR 0004 left two questions to M5.

Input methods, such as those for Japanese and Chinese and the accent menu shown when a key is held down, change the text several times before the user commits a character. The text view shows the uncommitted text as marked text. If every change reached the engine, undo would step back through half-composed text, and a cancelled composition would leave steps behind.

After `document.undo` or `document.redo` the app receives the whole new text. To select what was restored, keep the scroll position, and avoid laying out the whole document again, it needs to know which part of the text changed.

## Options considered

### Composed input

- Send every change, marked text included. The engine always matches the view. Undo steps through intermediate states, and each change of marked text replaces a range, so it starts a new undo step.
- Send on commit. While a composition is in progress, changes stay in the view. When it ends, the app sends one edit for the committed text. The engine is behind the view only while a composition is in progress.

### Changed range

- The app compares the text it showed with the text in the reply. Nothing crosses the bridge, but the comparison lives in Swift and is tested only through the UI.
- The engine reports the changed range in every reply. The comparison lives in the engine, is tested without a window, and is available to the later MCP server. The C reply gains fields.

## Decision

Send composed input on commit, and report the changed range in every reply.

### Composed input

The text view's delegate cannot tell a composition apart from typing: AppKit sends the commit while the view still reports marked text, and it sends a commit made by `unmarkText` as a deletion followed by an insertion. So the app tracks compositions in a subclass of `NSTextView` that overrides `setMarkedText(_:selectedRange:replacementRange:)`, `insertText(_:replacementRange:)`, and `unmarkText()`.

- **Start.** The first `setMarkedText` while no composition is in progress starts one. The app converts the range it replaces to byte offsets in the text of the last reply, which is the view's text at that moment, and keeps them as the composition's engine range.
- **During.** The delegate lets every change through without dispatching. When a later call's replacement range reaches outside the marked text, the engine range grows to include the committed text it covers. Positions before the marked text are the same in both texts; positions after it are shifted by the difference between the marked text's length and the engine range's length.
- **End.** `insertText` commits its string, `unmarkText` commits the marked text as it stands, and `setMarkedText` with an empty string cancels with nothing committed. Each `insertText` ends a composition; a `setMarkedText` right after it starts the next one, as input methods do when they commit one segment at a time.
- **Dispatch.** Before passing the ending call to `NSTextView`, the app dispatches one `document.replace_range` with the engine range and the committed text, possibly empty. An empty range with empty text changes nothing in the engine, so a cancel that replaced nothing needs no special case. If the engine rejects the edit, the app ends the composition and replaces the view's text with the last reply's text.
- **Other commands.** While a composition is in progress, Undo and Redo are disabled. Before the app dispatches any other command for the document, such as `document.close` or a save in M6, it ends the composition with `unmarkText`, which commits it.
- **Grouping.** A committed composition is an ordinary range edit and groups by ADR 0004's rules. Committed text inserted where typing ended joins the typing step. The accent menu is expected to mark the letter already typed and replace it, which M5 confirms by hand. Typing `caf`, holding `e`, and choosing `é` then gives two steps: undo first returns `cafe`, which is text the user did type.

### Changed range

- Every successful reply that leaves a document reports whether the text changed and, if it did, the byte range `changed_start..changed_end` of the new text that differs from the text before the command.
- The range is found from the longest common prefix of the two texts, taken first, and then the longest common suffix of what remains, so the two never overlap. The start moves back and the end moves forward to the nearest boundary between Unicode scalar values. For example, `aa` to `aaa` reports `2..3`.
- An empty range marks the place where text was only removed. A created or loaded document reports its whole text as changed, `0..0` for an empty one. A command that leaves the text unchanged reports no change; that includes an undo whose step ended where it began.
- The engine compares only text it already holds. A range edit compares the replaced bytes with the new text. `document.replace_text`, `document.undo`, and `document.redo` compare the two whole texts, one of which is the undo or redo snapshot.
- In Rust the range is part of the `Outcome` of a command that leaves a document, not state of the document. In the C reply, `changed` is a `bool` and `changed_start` and `changed_end` are `size_t`, like `text_len`. Error replies and the reply for a closed document report `false`, `0`, and `0`.
- The app replaces only the changed span of the text view's storage. The replaced bytes of the old text end at its length minus the bytes after `changed_end` in the new text. After `document.undo` and `document.redo` it selects the range, or places the cursor at an empty one, and scrolls it into view. It may widen the selection to whole characters as a reader sees them. When nothing changed, it leaves the selection as it was.

## Consequences

- Composition handling is presentation logic in the app. It is tested by UI tests that call the text view's input methods directly, and checked by hand with a real input method and the accent menu.
- The engine and the view differ while a composition is in progress, and only then.
- Comparing texts is linear in what is compared, like the whole-text snapshots ADR 0004 already accepts. Range edits, which are every keystroke, compare only the replaced bytes.
- `BiteReply` gains `changed`, `changed_start`, and `changed_end`. This amends ADR 0002 again; the bridge still has one call.
- The range reflects what changed, not what a step's commands did. An undo whose step deleted and retyped the same text reports a smaller range than the edits covered.
