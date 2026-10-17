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

The text view's delegate cannot tell a composition apart from typing: AppKit sends the commit while the view still reports marked text, and it sends a commit made by `unmarkText` as a deletion followed by an insertion. AppKit also does not always apply input-method calls to the whole marked text: a replacement range can fall outside it or inside it, and Backspace can remove the marking without `unmarkText`. So the app does not predict how a call changes the text. A subclass of `NSTextView` watches every change instead.

- **Start.** A `setMarkedText` with non-empty text while no composition is in progress starts one. The view's text is the engine's text at that moment.
- **During.** Every change to the text passes through `shouldChangeText(inRanges:replacementStrings:)`, which the subclass overrides. Each change grows a region so that it covers the change: the region starts at the earliest position any change touched and ends where the text no change touched begins. Text before and after the region is the engine's text, unchanged, so the region's place in the engine's text follows from its place in the view's. The delegate lets every change through without dispatching.
- **End.** A composition ends when no marked text remains: after `insertText`, `unmarkText`, or `setMarkedText` with empty text, after any other change such as Backspace through the marked text, and when the view loses focus, which commits the marked text. Each `insertText` ends a composition; a `setMarkedText` right after it starts the next one, as input methods do when they commit one segment at a time.
- **Dispatch.** When a composition ends, the app dispatches one `document.replace_range` that replaces the region in the engine's text with the region's text in the view. Unlike other edits, the view applies the change first: ADR 0004's rule, that the engine applies an edit before the view, gives way here because the view is ahead of the engine for the whole composition. The app then compares the view with the engine's text, and the engine's text wins if they differ, which is what happens when the engine rejects the edit.
- **Other commands.** While a composition is in progress, Undo and Redo are disabled. Before the app dispatches any other command for the document, such as `document.close` or a save in M6, it ends the composition, which commits it.
- **Grouping.** A committed composition is an ordinary range edit and groups by ADR 0004's rules. Committed text inserted where typing ended joins the typing step. The accent menu is expected to mark the letter already typed and replace it, which M5 confirms by hand. Typing `caf`, holding `e`, and choosing `é` then gives two steps: undo first returns `cafe`, which is text the user did type.

### Changed range

- Every successful reply that leaves a document reports whether the text changed and, if it did, the byte range `changed_start..changed_end` of the new text that differs from the text before the command.
- The range is found in two steps, so the parts left out never overlap. First the longest common prefix of the two texts is left out, and the start moves back to the nearest boundary between Unicode scalar values. Then the longest common suffix of what remains is left out, and the end moves forward to the nearest boundary. Because the shared bytes are the same in both texts, those boundaries are boundaries in both. For example, `aa` to `aaa` reports `2..3`, and `è` to `éaè` reports `0..3`.
- An empty range marks the place where text was only removed. A created or loaded document reports its whole text as changed, `0..0` for an empty one. A command that leaves the text unchanged reports no change; that includes an undo whose step ended where it began.
- The engine compares only text it already holds. A range edit compares the replaced bytes with the new text. `document.replace_text`, `document.undo`, and `document.redo` compare the two whole texts, one of which is the undo or redo snapshot.
- In Rust the range is part of the `Outcome` of a command that leaves a document, not state of the document. In the C reply, `changed` is a `bool` and `changed_start` and `changed_end` are `size_t`, like `text_len`. Error replies and the reply for a closed document report `false`, `0`, and `0`.
- The app replaces only the changed span of the text view's storage. The replaced bytes of the old text end at its length minus the bytes after `changed_end` in the new text. After `document.undo` and `document.redo` it selects the range, or places the cursor at an empty one, and scrolls it into view. It may widen the selection to whole characters as a reader sees them. When nothing changed, it leaves the selection as it was.

## Consequences

- Composition handling is presentation logic in the app. UI tests drive the app from another process and cannot call the text view's input methods, so the composition tracking is tested by unit tests in the app's test bundle that call them directly, and checked by hand with a real input method and the accent menu.
- The engine and the view differ while a composition is in progress, and only then. The region can cover more than the marked text, for example when an input method replaces committed text; the edit then replaces all of it, which is still correct.
- Comparing texts is linear in what is compared, like the whole-text snapshots ADR 0004 already accepts. Range edits, which are every keystroke, compare only the replaced bytes.
- `BiteReply` gains `changed`, `changed_start`, and `changed_end`. This amends ADR 0002 again; the bridge still has one call.
- The range reflects what changed, not what a step's commands did. An undo whose step deleted and retyped the same text reports a smaller range than the edits covered.
