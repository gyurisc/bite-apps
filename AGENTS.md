# Bite Apps — contributor and coding-agent guidance

## Project goal

Build clean-room, local-first macOS reimplementations of familiar stock Apple apps. TextBite is the first vertical slice. Keep the user in control of documents and make app behavior callable by both the native UI and headless tools.

## Start each task

1. Read `README.md` and `ROADMAP.md`.
2. Check the current repository state and the smallest relevant code path before editing.
3. Keep changes within the current milestone. If a decision affects multiple apps or crates, document the options and evidence in `docs/architecture/`.

## Clean-room rules

- Implement independently using public documentation, public file-format specifications, and independently observed behavior.
- Do not copy Apple source, private frameworks, assets, icons, screenshots, wording beyond ordinary feature names, or implementation details.
- Do not copy code, assets, or project-specific documentation from reference repositories. Use them only to compare high-level structure and publicly documented behavior.
- Record provenance and licenses for any third-party asset or dependency that is redistributed.
- Use original Bite branding and original sample documents.

## Architecture rules

- Rust owns document state, command execution, validation, and undo/history.
- The SwiftUI UI observes state and dispatches engine commands. Avoid duplicating document business logic in Swift.
- Keep the engine headless and independent of SwiftUI/AppKit so tests and CLI do not need a window.
- Every meaningful user action should map to a documented command with a stable ID and structured parameters. Keep UI-only presentation behavior in the UI.
- CLI and later MCP tools call the same engine command path as the UI.
- Add shared abstractions in response to a demonstrated need; do not design a generic suite framework in advance.
- Documents are local and portable. Do not add cloud, account, telemetry, subscription, or sync code to the first slice.

## Correctness and safety

- Treat document contents and command parameters as untrusted input.
- Return useful errors instead of panicking or silently losing content.
- Use atomic or recoverable writes where practical; do not overwrite a document before validation succeeds.
- Add focused tests for command behavior, undo/redo, malformed input, and import/export round trips.
- Preserve the source document on failed open/save operations.

## Scope and workflow

- Keep pull requests and commits focused on one user-visible behavior or one foundation task.
- Run `scripts/check.sh` for Rust changes. It runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- For UI changes, add or update a UI test in `apps/TextBite/TextBiteUITests` for the behavior, or a unit test in `apps/TextBite/TextBiteTests` for logic a UI test cannot reach, and run `scripts/check.sh --ui`. Where macOS Automation Mode is not enabled, run `scripts/check.sh --app` and rely on CI to run the UI tests. Report what was verified by hand, by UI test, or not at all; the maintainer checks the app by hand at each milestone.
- In the app target, classes are main-actor by default. Give a plain Swift class (not an `NSObject` subclass) an explicit `deinit` or `nonisolated deinit {}`; an implicit deinit is isolated, and releasing such an object in a unit test crashes on macOS 15.
- Do not claim parity with an Apple app unless a specific, tested behavior supports the claim.
- Do not add App Store packaging, release signing, Windows, sync, or monetization work unless the roadmap is explicitly changed.
