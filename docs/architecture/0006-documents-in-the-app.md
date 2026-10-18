# ADR 0006: The app's documents use NSDocument; the engine tracks unsaved changes and line endings

- Status: accepted for the first slice
- Date: 2026-10-18

## Context

M6 adds New, Open, Save, and closing and quitting with unsaved changes. ADR 0003 settles that the app moves file bytes with macOS document support and that the engine holds text. Four questions remain, which the maintainer answered on 2026-10-18 when M6's task list was written. With those answers, this ADR also settles how file bytes reach the engine, and what Revert and changes made to the file by other apps do.

- **Document support.** SwiftUI's `DocumentGroup` and AppKit's `NSDocument` both provide New, Open, Save, recent files, and the review of unsaved documents on quit.
- **Unsaved changes.** A window marks a document with unsaved changes, and undoing back to the saved text clears the mark. Document support usually learns about changes from an undo manager, but ADR 0004 gives undo to the engine, and the text view's own undo is off.
- **When files are written.** macOS apps either save in place automatically, with versions to go back to, or write a file only when the user chooses Save.
- **Line endings.** ADR 0003 keeps text exactly as loaded and leaves open what happens when the text view inserts line breaks unlike the file's. Pressing Return in an `NSTextView` inserts a line feed (LF), so typing in a file with CRLF line endings would mix the two.

## Options considered

### Document support

- `DocumentGroup` with a `ReferenceFileDocument`. Everything stays in SwiftUI. SwiftUI learns that a document changed when an action is registered with the window's undo manager, so the app would mirror every engine undo step there and keep two histories in step, which ADR 0004 rejected.
- `NSDocument`. The app tells the document directly whether it has unsaved changes, with `updateChangeCount(_:)`, and needs no undo manager. Each window's content stays a SwiftUI view, hosted in an `NSWindowController`.

### Unsaved changes

- The engine tracks them. Every reply says whether the text differs from the last save, and a command marks the current state as saved. The rule is tested without a window, and the CLI and the later MCP server can use it.
- The app compares the engine's text with the saved text after each change. The engine is unchanged, but a document rule lives in Swift, contrary to `AGENTS.md`, and every edit costs a comparison of the whole text.

### When files are written

- Save in place automatically. Familiar from current macOS apps, with versions to browse. Files change on disk without the user saving, and background saves are harder to test.
- Write only on Save and Save As. Closing a window or quitting with unsaved changes asks whether to save. The user decides when a file changes.

### Line endings

- Leave inserted line breaks as they are. A CRLF file gains LF lines where the user types.
- Give inserted line breaks the document's line ending. The file stays consistent, and LF files, the usual case, are unaffected.

## Decision

Use `NSDocument`, track unsaved changes in the engine, write files only on Save and Save As, and give inserted line breaks the document's line ending.

### NSDocument

- One `NSDocument` subclass handles plain text. The app declares that it edits `public.plain-text`. Its user-selected file access becomes read/write, as ADR 0003 says.
- The document owns its `DocumentModel`. Reading a file passes the file's bytes to `document.load`; writing a file returns the bytes of the engine's text. The document has no undo manager: `hasUndoManager` is false. Other fields in the window, such as the find field, keep their own undo, as ADR 0004 requires, and typing in them does not mark the document as having unsaved changes. A document with no undo manager leaves AppKit to give each window its own, and undo registered there does not mark the document edited; `TextDocumentTests` and the UI tests check both.
- The file's bytes cross the bridge unchanged, as the `text` parameter. The bridge checks every parameter with `text_from_utf8`, so a file that is not UTF-8 is rejected with the offset of its first invalid byte, and the app shows that error and opens nothing. Swift does not check or repair the bytes: decoding them in Swift would either copy the engine's rule or replace invalid bytes and lose content.
- Revert to Saved reads the file again into a new engine document with `document.load`, and closes the old one. Its undo history is lost, as it would be if the file were closed and opened again; AppKit asks before reverting.
- Changes made to the file by other apps are not loaded automatically. If the file changed on disk since it was opened or saved, AppKit warns when the user saves, and the user chooses whether to overwrite it.
- The document makes one window controller, whose window hosts the SwiftUI `ContentView` for its model. The window's title, its close button's unsaved-changes dot, and the Save, Save As, Open Recent, and Close items come from AppKit.
- `document.close` is dispatched when the document closes, not when a window closes.
- A composition is not in the engine yet, so a document can look unchanged while one is in progress. Before saving, and before AppKit decides whether closing a window must ask about unsaved changes (`canClose(withDelegate:shouldCloseSelector:contextInfo:)`), the document ends its window's composition, which commits it, as ADR 0005 requires. Before quitting reviews unsaved documents, the app ends the composition in every window.
- Quitting does not close documents: AppKit reviews the documents with unsaved changes, then the process exits, and the engine goes with it. Nothing is lost, because every edit is in the engine by then and the user has saved or discarded it. The app does not opt into sudden termination, which would skip the review.
- The app's entry point is AppKit's, not a SwiftUI `App`: `main.swift` starts `NSApplicationMain` with an application delegate that builds the menu bar in code. Each menu item sends its standard action, such as `saveDocument:` or `undo:`, along the responder chain, so AppKit enables it only where something handles it, and `NSDocumentController` makes the untitled document at launch. The pull request that made each window an `NSDocument` chose this; a SwiftUI `App` hosting `NSDocument` windows was not tried.

### Unsaved changes in the engine

- Every reply that has a document reports `modified`: whether the document's current state differs from the state last marked as saved. The C reply gains a `modified` field, which amends ADR 0002 as ADR 0005 did for the changed range.
- A new command, `document.mark_saved`, takes a document and marks its current state as saved. It changes no text, adds no undo step, and ends the current undo step, so typing after a save starts a new step and undo can return to the saved text.
- A document made by `document.create` or `document.load` is not modified. A reply with no document, an error or the reply to `document.close`, reports `modified` as false, as it reports `changed`.
- `document.mark_saved` is a command that takes a document, so ADR 0004's grouping rules gain it: it ends the current step although it changes no text.
- `modified` compares states in the history, not texts. Typing a letter and deleting it is two edits, and the document is modified until it is saved or both are undone. When an edit discards the redo history that held the saved state, the document stays modified until the next save.
- The app dispatches `document.mark_saved` after a Save or Save As has written the file, and not after a failed write or a Save To, which writes a copy and leaves the document's file as it was. It sets the document's edited state from each reply's `modified` with `updateChangeCount`.
- `document.mark_saved` marks the state the engine holds when it is dispatched, not the state that was written. That is the same state as long as writing and the dispatch happen on the main thread with no edit between them, so the document does not write asynchronously (`canAsynchronouslyWrite`) unless the engine first learns to mark a given state.
- The CLI saves after every run, so it refuses `document.mark_saved` as a usage error, as it refuses `document.close`.

### Saving

- `autosavesInPlace` is false. Save and Save As write the file; until then, the file on disk is unchanged.
- Closing a window whose document has unsaved changes asks Save, Don't Save, or Cancel, and quitting asks the same for each such document, through `NSDocument` and `NSDocumentController`. Cancel keeps the document open and its composition committed in the engine.
- M6 writes no background copies for crash recovery. Window restoration after relaunch follows AppKit's defaults; UI tests must not depend on it.

### Line endings

- When a document is created by `document.load`, the engine notes its line ending: the first line break in the text, which is LF, CRLF, or a lone carriage return (CR). A text with no line break, and every document made by `document.create`, uses LF.
- `document.replace_range` converts every line break in the inserted text, whether LF, CRLF, or CR, to the document's line ending. Typing and pasting reach the engine this way. Text that is already in the document is never changed, so an unedited file still saves back byte for byte, and a file with mixed line endings keeps them.
- `document.replace_text` keeps its text exactly, for every host. A change to several ranges, such as Replace All, reaches the engine as one `document.replace_text`, so a replacement that contains a line break keeps the line break the user typed. Converting only the changed part of the text is not safe: the changed range can begin or end between the CR and LF of a pair.
- For undo grouping, the typing position after a converted insertion is the end of the converted text in the engine's bytes, and the app places the insertion point there.
- The view then holds different text from the engine. The text view's delegate refuses an edit whose inserted text the engine changed, and applies the engine's text over the reply's changed range instead, as it does after undo, placing the insertion point after the inserted text. That changes the view while AppKit is still handling the edit, so the pull request that adds this shows that it is safe or applies the change on the next turn of the run loop, and its UI test checks where the insertion point ends up.
- Only LF, CRLF, and CR count as line breaks. Other separators, such as U+2028, are inserted as typed.

## Consequences

- AppKit drives the document's life: New, Open, Save, Close, and Quit. SwiftUI keeps the window's content. The editor's code from M5 is unchanged apart from where its model comes from and who closes it.
- The engine gains one command and one reply field, with tests in the engine, the bridge, and the CLI. `document.replace_range` changes the line breaks it inserts for every host, including the CLI. `modified` is part of the command contract that MCP will expose.
- The UI tests can check unsaved changes through the window's document-edited state, and saving by reading the file's bytes back.
- An edit in a CRLF or CR file that inserts a line break replaces part of the view's text from the engine. Edits without line breaks, and every edit in an LF file, apply as before.
- Replace All with a line break in the replacement can still mix line endings in a CRLF or CR file.
- Autosave, versions, and crash recovery can be added later without changing the engine.
