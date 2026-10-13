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
| M3 | Plain-text persistence | Open and save UTF-8 text; round-trip tests; clear error handling | Planned |
| M4 | CLI surface | CLI opens/creates a document and applies the same engine commands | Planned |
| M5 | SwiftUI integration | UI displays engine state and dispatches the remaining commands through the M2 bridge | Planned |
| M6 | First useful release | New/open/save, editing, undo/redo, basic menus and keyboard shortcuts, local use without a service | Planned |
| M7 | Agent interface | MCP server exposes a small, documented subset of the stable command surface | Later |
| M8 | Second app | Reassess shared engine boundaries using a concrete second app | Later |

M0–M5 together are the first slice described in `README.md`.

## Decisions to make when evidence is available

- Text document representation: begin with UTF-8 plain text; preserve line endings and encoding behavior deliberately.
- Native Bite document format: introduce only when plain text and the next app reveal what metadata needs preserving.
- App Store and outside distribution: revisit after local use validates the app.
- Cross-platform UI: revisit after the macOS command surface and document model stabilize.

## Working rule

Complete a small end-to-end behavior before generalizing it. Do not add features solely to match a reference app. Record architecture decisions in `docs/architecture/` when they affect multiple crates or future apps.
