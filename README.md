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

The Rust engine owns document state, command execution, validation, and undo/history. The SwiftUI app observes engine state and dispatches commands; it must not become a second implementation of document rules. The CLI is a headless test and automation surface. MCP can be added after the command contract is useful and stable. The engine does not read or write files: each host moves the bytes and hands the engine text, as recorded in `docs/architecture/0003-hosts-move-bytes.md`.

The Rust-to-Swift bridge is a small hand-written C interface in `crates/bite-ffi`; `docs/architecture/0002-swift-rust-bridge.md` records the choice and its alternatives. Keep the boundary small.

## Repository layout

```text
bite-apps/
├── apps/
│   └── TextBite/             # native macOS SwiftUI app
├── crates/
│   ├── bite-engine/          # headless document model and commands
│   ├── bite-ffi/             # C interface used by the SwiftUI app
│   └── bite-cli/             # command-line interface
├── docs/
│   └── architecture/        # decisions and design notes
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
```

The CLI opens a plain-text file, or creates it, runs engine commands on it by their IDs, and saves the result:

```sh
cargo run -p bite-cli -- notes.txt document.replace_text 'text=Hello'
cargo run -p bite-cli -- --help
```

Open the TextBite Xcode project under `apps/` and run it with **⌘R**. The Xcode build runs `cargo` to build the engine, so a Rust toolchain must be installed. App Store distribution, signing for public release, cloud sync, accounts, subscriptions, and monetization are out of scope for the first slice.

GitHub Actions runs the formatting, Clippy, and test checks from `AGENTS.md` and an unsigned build of the app on macOS for every pull request and every push to `main`; see `.github/workflows/ci.yml`. The Rust version is pinned in `rust-toolchain.toml`; raise it in its own change.

## First-slice acceptance criteria

1. The Rust engine has a document model and a small command registry with stable IDs.
2. Commands cover document creation, loading, text editing, and undo/redo as they are implemented.
3. The CLI can exercise the same command path without launching the UI.
4. The SwiftUI app displays engine state and dispatches commands through the agreed bridge.
5. Tests cover command behavior and plain-text round trips.
6. The app runs locally from Xcode without requiring App Store submission or a cloud service.

## License

The code in this repository is licensed under the MIT License; see [LICENSE](LICENSE).
