# ADR 0003: Hosts move bytes; the engine holds text

- Status: accepted for the first slice
- Date: 2026-10-13

## Context

Plain-text persistence needs a home for reading and writing files. Two hosts need it: the CLI, which runs unsandboxed, and the TextBite app, which runs in the App Sandbox.

A sandboxed app is granted access to the file the user picks, not to the folder around it. The usual safe save, writing a temporary file beside the target and renaming it into place, is therefore not available to code that manages files itself. macOS document support performs safe saves inside the sandbox and also supplies New, Open, Save, recent files, and autosave.

## Options considered

- The engine reads and writes files and hosts pass paths. One implementation serves both hosts, but inside the sandbox it cannot save safely, and the app would hand-build the document features macOS already provides.
- The engine reads and writes files and the sandbox is turned off. This is the least work for this milestone. Saving would be reworked if the sandbox returns, for example for App Store distribution, and the document features are still hand-built.
- Hosts move bytes and the engine holds text.

## Decision

Hosts move bytes and the engine holds text.

- The engine does not access files. `text_from_utf8` turns file bytes into text and rejects anything that is not UTF-8, reporting the offset of the first invalid byte. The `document.load` command creates a document from that text with no undo history. To save, a host writes the bytes of the document's text.
- Text is kept exactly as loaded. Line endings and a leading byte-order mark are not normalized, so an unedited document saves back byte for byte.
- The CLI reads and writes files with the Rust standard library and saves through a temporary file and a rename.
- The app keeps the App Sandbox and uses macOS document support to read and write files. Its user-selected file access changes from read-only to read/write in the milestone where the app gains open and save.

## Consequences

- The engine stays testable without a file system; round trips are tested on bytes.
- File-moving code exists once per host. Rules about text stay in the engine.
- No engine command takes a file path. Hosts report file-system errors themselves.
- Only UTF-8 is supported. Other encodings are rejected, not guessed.
- Editing through a native text view can introduce line endings that differ from the loaded ones. Deciding what to do about that belongs to the milestone that integrates the editor.
- The engine does not track whether a document has unsaved changes. Add that when a host needs it.
