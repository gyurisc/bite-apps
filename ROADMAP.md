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
| M1 | Engine vertical slice | Headless Rust document model; stable command IDs; tests for create/edit/undo | Done |
| M2 | Plain-text persistence | Open and save UTF-8 text; round-trip tests; clear error handling | Planned |
| M3 | CLI surface | CLI opens/creates a document and applies the same engine commands | Planned |
| M4 | SwiftUI integration | UI displays engine state and dispatches commands through one small Rust bridge | Planned |
| M5 | First useful release | New/open/save, editing, undo/redo, basic menus and keyboard shortcuts, local use without a service | Planned |
| M6 | Agent interface | MCP server exposes a small, documented subset of the stable command surface | Later |
| M7 | Second app | Reassess shared engine boundaries using a concrete second app | Later |

## Decisions to make when evidence is available

- Rust-to-Swift binding mechanism: decide after the engine API and one UI command are concrete.
- Text document representation: begin with UTF-8 plain text; preserve line endings and encoding behavior deliberately.
- Native Bite document format: introduce only when plain text and the next app reveal what metadata needs preserving.
- App Store and outside distribution: revisit after local use validates the app.
- Cross-platform UI: revisit after the macOS command surface and document model stabilize.

## Working rule

Complete a small end-to-end behavior before generalizing it. Do not add features solely to match a reference app. Record architecture decisions in `docs/architecture/` when they affect multiple crates or future apps.
