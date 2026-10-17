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
| M5 | SwiftUI integration | UI displays engine state and dispatches the remaining commands through the M2 bridge | Planned |
| M6 | First useful release | New/open/save, editing, undo/redo, basic menus and keyboard shortcuts, local use without a service | Planned |
| M7 | Agent interface | MCP server exposes a small, documented subset of the stable command surface | Later |
| M8 | Second app | Reassess shared engine boundaries using a concrete second app | Later |

M0–M5 together are the first slice described in `README.md`.

## Current milestone: M5 tasks

One pull request per item, in order. Tick an item in the pull request that completes it. `docs/architecture/0004-edit-model.md` and `0005-composition-and-changed-range.md` settle their design. `document.load` is dispatched by Open, in M6.

- [x] Engine: `document.replace_range` with undo grouping (PR #11)
- [x] Engine: `document.close`, with the bridge reply and CLI refusal ADR 0004 describes (PR #13)
- [x] Decide, with the maintainer, the two questions ADR 0004 leaves to M5: how input-method composition reaches the engine and groups into undo steps, and how the selection and scroll position are restored after undo and redo; record the answers in an ADR (ADR 0005)
- [x] Engine and bridge: every reply reports the changed byte range, and the C reply carries it, as ADR 0005 describes; ADR 0002 notes the amendment
- [x] App: an `NSTextView` wrapped for SwiftUI shows the document and sends each edit to the engine before applying it, as `document.replace_range` with byte offsets, or as one `document.replace_text` for a change to several ranges; its own undo is off until the Undo item; UI tests type and delete
- [x] App: composed input is sent on commit, as ADR 0005 describes; the composition tracking has unit tests in a new unit test target
- [ ] App: while the text view has focus, Undo and Redo dispatch `document.undo` and `document.redo` and are enabled from the reply; the app selects the reply's changed range and scrolls to it; the text view's own undo is off and other fields keep theirs; UI tests cover ⌘Z and ⇧⌘Z. Edits with several selections are separate undo steps, as ADR 0004 records; grouping them is not part of M5
- [ ] Maintainer: check composed input by hand with a Japanese input source and the accent menu, as ADR 0005 asks, and record the result in the ADR
- [ ] App: each window owns one document and closes it with `document.close`

## Decisions to make when evidence is available

- Native Bite document format: introduce only when plain text and the next app reveal what metadata needs preserving.
- App Store and outside distribution: revisit after local use validates the app.
- Cross-platform UI: revisit after the macOS command surface and document model stabilize.

## Working rule

Complete a small end-to-end behavior before generalizing it. Do not add features solely to match a reference app. Record architecture decisions in `docs/architecture/` when they affect multiple crates or future apps.
