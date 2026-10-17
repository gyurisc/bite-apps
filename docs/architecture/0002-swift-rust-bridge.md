# ADR 0002: Hand-written C interface between Swift and the Rust engine

- Status: accepted for the first slice; amended by ADR 0004, which adds `document.close`. `Engine::dispatch` now returns an `Outcome` that may name a closed document, and the C reply reports a closed document as its ID with empty text and no undo or redo.
- Date: 2026-10-13

## Context

The SwiftUI app must dispatch engine commands without becoming a second implementation of document rules (ADR 0001). Since M1 the engine has one entry point for untrusted callers: `Engine::dispatch` takes a command ID and named string parameters and returns the affected document or an error. The bridge therefore has to carry one call, not one call per command.

## Options considered

- Hand-written C interface: `extern "C"` functions in a static-library crate, a C header, and a Swift bridging header. No dependencies and no generated code. Memory ownership and the header are maintained by hand.
- UniFFI: generates the Swift bindings and handles strings, errors, and memory. Adds dependencies, a binding-generation build step, and generated Swift to keep in step. Its value grows with the number of typed functions; this boundary has one.
- swift-bridge: a similar trade-off to UniFFI with a smaller ecosystem.

## Decision

Use a hand-written C interface in `crates/bite-ffi` with four functions: `bite_engine_new`, `bite_engine_free`, `bite_engine_dispatch`, and `bite_reply_free`, declared in `crates/bite-ffi/include/bite_ffi.h`. Strings cross as UTF-8 bytes with an explicit length.

The TextBite target runs `apps/TextBite/build-rust.sh` as its first build phase to build the static library, links it, and imports the header as its bridging header. `EngineBridge.swift` is the only Swift file that calls the C interface.

## Evidence

The M2 spike builds and links from Xcode with one script build phase. Through the bridge, the Swift wrapper creates a document, replaces its text, undoes the change, and receives engine error messages; text containing NUL bytes and non-ASCII characters returns unchanged.

## Consequences

- Adding an engine command does not change the bridge.
- The C header and the Rust declarations are kept in step by hand; nothing checks one against the other yet.
- A reply carries the document text, undo and redo availability, or an error message. Structured errors or typed values would need new fields or a different mechanism.
- An engine instance is used from one thread at a time. The Swift wrapper is main-actor isolated.
- Building the app requires a Rust toolchain. Release builds for both Mac architectures also need the `x86_64-apple-darwin` Rust target.
- Revisit this decision if the boundary grows beyond dispatch, for example callbacks from Rust, asynchronous work, or many typed calls.
