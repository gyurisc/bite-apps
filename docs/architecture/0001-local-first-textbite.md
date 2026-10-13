# ADR 0001: Local-first TextBite vertical slice

- Status: accepted for the first slice
- Date: 2026-10-13

## Context

Bite Apps is a family of local-first macOS applications with a shared Rust engine, native SwiftUI interfaces, and an agent-accessible command model. The first app and first milestone should prove the architecture without requiring a backend or broad document-format support.

## Decision

Start with TextBite as a plain-text editor. Keep document state and meaningful edit operations in a headless Rust engine. The SwiftUI app is a thin native client of that engine. The CLI exercises the same engine path. Store ordinary documents as portable plain text; defer a Bite-specific container until a concrete feature needs metadata that plain text cannot represent.

Build and run locally from Xcode. App Store distribution and public release signing are not part of this milestone.

## Consequences

- The first slice validates the native UI, Rust engine boundary, commands, history, CLI, and plain-text round trips.
- RTF and rich-text representation remain out of scope until plain-text editing works end to end.
- Cloud sync, accounts, and monetization do not influence the initial storage design.
- Rust-to-Swift interop remains open until the first small engine API is ready to bridge.
- GridBite remains a candidate for a later app and can test more structured document state.
