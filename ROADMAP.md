# Bite Apps roadmap

This roadmap tracks the first useful macOS app and the shared patterns it proves. It is intentionally narrower than the long-term suite vision.

## Product scope

- First app: TextBite, beginning with plain text.
- Platform: macOS native app using SwiftUI, with AppKit bridging only when needed.
- Engine: headless Rust library, callable by tests and CLI; same command surface is used by the UI.
- Documents: local and portable; support plain-text import/export first.
- Excluded for now: cloud sync, accounts, subscriptions, monetization, App Store distribution, Windows UI, RTF fidelity, and broad app parity.

## Milestones

| ID | Milestone | Exit criteria | Status |
|---|---|---|---|
| M0 | Workspace and native shell | Cargo workspace checks; TextBite SwiftUI template builds and launches locally | Done |
| M1 | Engine core | Headless Rust document model; stable command IDs; tests for create/edit/undo | Done |
| M2 | Bridge spike | SwiftUI app dispatches one engine command through a Rust bridge and displays the result; binding choice recorded as an ADR | Done |
| M3 | Plain-text persistence | Engine loads UTF-8 text and returns it byte for byte; round-trip tests; clear error for invalid UTF-8 | Done |
| M4 | CLI surface | CLI opens or creates a file, applies the same engine commands, and saves through a safe write | Done |
| M5 | SwiftUI integration | UI displays engine state and dispatches the remaining commands through the M2 bridge | Done; maintainer hand check open |
| M6 | First useful release | New/open/save, editing, undo/redo, basic menus and keyboard shortcuts, local use without a service | Planned |
| M7 | Agent interface | MCP server exposes a small, documented subset of the stable command surface | Later |
| M8 | Second app | Reassess shared engine boundaries using a concrete second app | Later |

M0–M5 together are the first slice described in `README.md`.

## Current milestone: M6 tasks

One pull request per item, in order. Tick an item in the pull request that completes it. M5's items are done (PRs #11, #13–#17, #19, #21) except the maintainer's hand check, carried over below. ADR 0003 settles that the app moves file bytes with macOS document support and the engine holds text; `docs/architecture/0006-documents-in-the-app.md` settles the rest.

- [x] Record in ADR 0006 the maintainer's answers of 2026-10-18: the app uses AppKit's `NSDocument`; the engine tracks unsaved changes; files are written only on Save and Save As, and closing or quitting with unsaved changes asks; inserted line breaks take the document's line ending, which ADR 0003 leaves open. The ADR also settles how file bytes reach the engine through the bridge, and what Revert and changes made to the file by other apps do
- [x] Engine and bridge: every reply reports `modified`, and `document.mark_saved` marks the current state as saved, as ADR 0006 describes; the C reply gains the field, amending ADR 0002; ADR 0004's grouping rules gain `document.mark_saved`; the CLI refuses it; engine, FFI, and CLI tests
- [x] Bridge: a parameter that is not UTF-8 is rejected with the offset of its first invalid byte, as ADR 0006 describes; FFI tests
- [x] App: each window is an `NSDocument`, as ADR 0006 describes; New makes an untitled document; the window shows unsaved changes from `modified`; Save and Save As write the bytes of the engine's text, then dispatch `document.mark_saved`; the document dispatches `document.close`; UI tests save and compare the file's bytes, and check that typing in another field keeps that field's undo and leaves the document unchanged
- [x] App: Open reads a plain-text file and dispatches `document.load`, and Revert to Saved reads it again, as ADR 0006 describes; a file that is not UTF-8 shows an error with the offset of its first invalid byte and opens nothing; an unedited file saves back byte for byte; UI tests open sample files
- [ ] App: closing a window or quitting with unsaved changes asks Save, Don't Save, or Cancel, after ending every composition, so a composition in a saved document is not lost; UI tests cover each choice, and a unit test covers the composition
- [ ] Engine and app: line breaks inserted by `document.replace_range` take the document's line ending, as ADR 0006 describes, and the view shows the engine's text where they differ; engine tests, and a UI test that types Return in a CRLF file and checks the text and the insertion point
- [ ] App: Find and Replace through the text view's find bar (⌘F, ⌥⌘F); Replace and Replace All reach the engine as one edit each; UI tests
- [ ] App: the menus show only commands that work, each with its standard shortcut; the README lists the shortcuts, and a UI test uses each one
- [ ] Docs: the README says how to build a local copy of TextBite for everyday use and lists what it does and does not do
- [ ] Maintainer: check composed input by hand with a Japanese input source and the accent menu, as ADR 0005 asks, and record the result in the ADR (from M5)
- [ ] Maintainer: use a local build for New, Open, editing, Undo and Redo, Find, Save and Save As, closing with unsaved changes, and Quit, and record anything that blocks everyday use in this file

## Decisions to make when evidence is available

- Native Bite document format: introduce only when plain text and the next app reveal what metadata needs preserving.
- App Store and outside distribution: revisit after local use validates the app.
- Cross-platform UI: revisit after the macOS command surface and document model stabilize.

## Working rule

Complete a small end-to-end behavior before generalizing it. Do not add features solely to match a reference app. Record architecture decisions in `docs/architecture/` when they affect multiple crates or future apps.
