# Bite Apps

Bite Apps is a personal project to reimplement familiar stock macOS apps for fun. The goal is to build clean-room, open-source, native-feeling tools while learning along the way and keeping documents portable where practical. Future paid apps, support, or related services remain possible; there is no monetization requirement for the first slice.

The name is intentionally short and playful. Each app uses the `Bite` suffix: TextBite, GridBite, NoteBite, and so on.

## Current focus

Build for macOS first. The first vertical slice is **TextBite**, a small plain-text editor. It should prove that a SwiftUI app can dispatch commands to a headless Rust engine and that the same engine can be exercised without the UI.

The first slice covers creating, editing, opening, and saving plain-text documents. Add RTF only after this flow works reliably.

## Product principles

- Local-first. Documents live on the user's Mac and remain usable without an account or network connection.
- Portable documents. Prefer documented, versioned formats and standard import/export. Do not build sync abstractions before there is a sync requirement.
- Native Mac experience. Use SwiftUI, adding AppKit only where native behavior requires it.
- Agent-controllable. User-visible behavior belongs in stable, documented engine commands that can be called by the UI, CLI, and later MCP server.
- Clean-room. Use public specifications and independently observed behavior. Do not copy Apple source code, assets, icons, wording beyond product feature names, or proprietary implementation details.
- Small shared engine. Extract shared logic when a second app demonstrates the need; avoid speculative framework design.

## Architecture

```text
SwiftUI app ─┐
             ├── command interface ── Rust engine
CLI ─────────┘                            │
                                         └── portable document formats
MCP server: later client of the same engine command interface
```

The Rust engine owns document state, command execution, validation, and undo/history. The SwiftUI app observes engine state and dispatches commands; it must not become a second implementation of document rules. The CLI is a headless test and automation surface. MCP can be added after the command contract is useful and stable. The engine does not read or write files: each host moves the bytes and hands the engine text, as recorded in `docs/architecture/0003-hosts-move-bytes.md`. How editors change text, what positions count, and how typing becomes undo steps are recorded in `docs/architecture/0004-edit-model.md`. How composed input reaches the engine and how replies report what changed are recorded in `docs/architecture/0005-composition-and-changed-range.md`. How the app opens and saves documents, tracks unsaved changes, and handles line endings is recorded in `docs/architecture/0006-documents-in-the-app.md`.

The Rust-to-Swift bridge is a small hand-written C interface in `crates/bite-ffi`; `docs/architecture/0002-swift-rust-bridge.md` records the choice and its alternatives. Keep the boundary small.

## Repository layout

```text
bite-apps/
├── apps/
│   └── TextBite/             # native macOS SwiftUI app, its unit tests, and its UI tests
├── crates/
│   ├── bite-engine/          # headless document model and commands
│   ├── bite-ffi/             # C interface used by the SwiftUI app
│   └── bite-cli/             # command-line interface
├── docs/
│   └── architecture/        # decisions and design notes
├── scripts/                 # developer checks
├── AGENTS.md
├── CLAUDE.md
├── Cargo.toml
└── ROADMAP.md
```

Paths may be adjusted to match the Xcode project generated on the developer's machine. Keep one Git repository for the suite and one Cargo workspace for Rust packages.

## Build and run

From the repository root:

```sh
cargo check --workspace
cargo test --workspace
scripts/check.sh          # formatting, Clippy, and tests, as CI runs them
scripts/check.sh --app    # also builds the app and its unit and UI tests
scripts/check.sh --ui     # also runs the app's unit and UI tests
```

UI tests drive the app through macOS Automation Mode, which asks for a password the first time it is needed. To let them run unattended, enable it once with `sudo automationmodetool enable-automationmode-without-authentication`, and turn it back off with `sudo automationmodetool disable-automationmode-without-authentication`.

The CLI opens a plain-text file, or creates it, runs engine commands on it by their IDs, and saves the result:

```sh
cargo run -p bite-cli -- notes.txt document.replace_text 'text=Hello'
cargo run -p bite-cli -- --help
```

Open the TextBite Xcode project under `apps/` and run it with **⌘R**. The Xcode build runs `cargo` to build the engine, so a Rust toolchain must be installed. App Store distribution, signing for public release, cloud sync, accounts, subscriptions, and monetization are out of scope for the first slice.

GitHub Actions runs the formatting, Clippy, and test checks from `AGENTS.md`, builds the app signed ad hoc, and runs its UI tests on macOS for every pull request and every push to `main`; see `.github/workflows/ci.yml`. The Rust version is pinned in `rust-toolchain.toml`; raise it in its own change.

## Using TextBite

### Build a copy for everyday use

From the repository root:

```sh
xcodebuild build -project apps/TextBite/TextBite.xcodeproj -scheme TextBite \
  -configuration Release -destination "platform=macOS,arch=$(uname -m)" \
  -derivedDataPath build/TextBite ONLY_ACTIVE_ARCH=YES \
  CODE_SIGN_IDENTITY=- DEVELOPMENT_TEAM=
rm -rf /Applications/TextBite.app
cp -R build/TextBite/Build/Products/Release/TextBite.app /Applications/
```

Quit TextBite before replacing it. TextBite needs macOS 15.8 or later. The build is for this Mac's processor only and is signed ad hoc, so no developer account is needed; it is not notarized, so to use TextBite on another Mac, build it there. Build again after pulling changes; `build/` is ignored by Git.

### What TextBite does

- Edits plain-text files in UTF-8, one document per window; windows can be merged into tabs.
- New, Open, Open Recent, Save, Save As, and Revert to Saved. A file is written only when you save it.
- Marks a window with unsaved changes, and asks before closing it: Save, Don't Save, or Cancel. A new document is asked about in the Save panel, with Delete instead of Don't Save. Quitting asks the same about an unsaved document; with several, macOS first offers to review them one by one or discard them all.
- Undo and Redo for every edit, with typing grouped into steps; undoing back to the saved text clears the unsaved-changes mark.
- Find, Find and Replace, Find Next, and Find Previous.
- Keeps a file exactly as it was: a byte-order mark and every line ending stay, so an unedited file saves back byte for byte. Line breaks you type or paste take the file's line ending.
- Keeps what you type in the editor: quotes stay straight, `--` stays two hyphens, and macOS text replacements do not apply. Each can be turned back on for one window under Substitutions in the editor's Control-click menu; the choice is not remembered. The find bar's Replace field still applies text replacements.
- Input methods, such as Japanese input and the accent menu, send text to the document when it is committed. Until then, Undo and Redo are greyed out and ⌘Z does nothing. Text can look finished before it is committed: with Japanese live conversion, converted text such as 日本語 keeps a thin underline until you press Return, click outside the editor, or save. Unit tests cover committing and the greyed-out Undo and Redo; the maintainer's check by hand, which ADR 0005 asks for, is still open.

### What it does not do

- No autosave, versions, or crash recovery: unsaved changes are lost if the app or the Mac stops.
- Opens only UTF-8; a file in another encoding is refused with the position of its first byte that is not UTF-8.
- Opens only files macOS counts as plain text, such as `.txt`, `.md`, `.csv`, and source code. The Open panel greys out JSON, YAML, XML, HTML, property lists, and files with no extension that macOS does not recognize, such as `.gitignore`.
- Plain text only: no rich text, fonts, or styles, and no printing.
- Does not notice when another app changes an open file, until you save; then macOS asks whether to overwrite it.
- Every edit passes the whole text between the engine and the window, and every undo step keeps a copy of the whole text, so very large files are slow to edit and use a lot of memory.

## TextBite keyboard shortcuts

TextBite's menus hold only commands that work. Each shortcut below is TextBite's own and is used by a UI test in `apps/TextBite/TextBiteUITests`.

| Shortcut | Command |
|---|---|
| ⌘N | New |
| ⌘O | Open… |
| ⌘W | Close |
| ⌘S | Save… |
| ⇧⌘S | Save As… |
| ⌘Q | Quit TextBite |
| ⌘Z | Undo |
| ⇧⌘Z | Redo |
| ⌘X | Cut |
| ⌘C | Copy |
| ⌘V | Paste |
| ⌘A | Select All |
| ⌘F | Find… |
| ⌥⌘F | Find and Replace… |
| ⌘G | Find Next |
| ⇧⌘G | Find Previous |
| ⌘E | Use Selection for Find |
| ⌘M | Minimize |
| ⌘H | Hide TextBite |

AppKit adds items of its own to these menus, with their standard shortcuts, and carries them out without TextBite's code: Hide Others (⌥⌘H), Open Recent, AutoFill, Start Dictation, Emoji & Symbols, full screen, and the window tab and tiling items. No test uses those shortcuts; Hide Others would hide the test runner.

## First-slice acceptance criteria

1. The Rust engine has a document model and a small command registry with stable IDs.
2. Commands cover document creation, loading, text editing, and undo/redo as they are implemented.
3. The CLI can exercise the same command path without launching the UI.
4. The SwiftUI app displays engine state and dispatches commands through the agreed bridge.
5. Tests cover command behavior and plain-text round trips.
6. The app runs locally from Xcode without requiring App Store submission or a cloud service.

## License

The code in this repository is licensed under the MIT License; see [LICENSE](LICENSE).
