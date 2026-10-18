import AppKit
import XCTest

@testable import TextBite

/// `TextDocument` keeps AppKit's unsaved-changes state in step with the
/// engine, writes the engine's bytes, and closes its engine document, as
/// ADR 0006 describes.
final class TextDocumentTests: XCTestCase {
    private let notFound = NSRange(location: NSNotFound, length: 0)
    private let plainText = "public.plain-text"

    @MainActor
    private func makeDocument(_ engine: EngineBridge) -> TextDocument {
        let document = TextDocument(engine: engine)
        document.model.open()
        return document
    }

    private func scratchURL(_ name: String) throws -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("TextDocumentTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }
        return directory.appendingPathComponent(name)
    }

    @MainActor
    func testTheDocumentHasNoUndoManagerAndWritesOnlyOnSave() {
        let document = makeDocument(EngineBridge())

        XCTAssertFalse(document.hasUndoManager)
        XCTAssertNil(document.undoManager)
        XCTAssertFalse(TextDocument.autosavesInPlace)
    }

    @MainActor
    func testTheEnginesModifiedStateMarksTheDocumentEdited() {
        let document = makeDocument(EngineBridge())
        XCTAssertFalse(document.isDocumentEdited)

        XCTAssertTrue(document.model.replace(NSRange(location: 0, length: 0), with: "a"))
        XCTAssertTrue(document.isDocumentEdited)

        XCTAssertNotNil(document.model.runHistory(.undo))
        XCTAssertFalse(document.isDocumentEdited)

        XCTAssertNotNil(document.model.runHistory(.redo))
        XCTAssertTrue(document.isDocumentEdited)
    }

    @MainActor
    func testSavingWritesTheEnginesBytesAndMarksTheDocumentSaved() async throws {
        let document = makeDocument(EngineBridge())
        let text = "line\r\nn\u{e9}e\u{301} \u{1F34E}\0end"
        XCTAssertTrue(document.model.replaceText(with: text))
        let url = try scratchURL("saved.txt")

        try await document.save(to: url, ofType: plainText, for: .saveAsOperation)

        XCTAssertEqual(try Data(contentsOf: url), Data(text.utf8))
        XCTAssertEqual(document.model.document?.modified, false)
        XCTAssertFalse(document.isDocumentEdited)

        // Undo returns to text that differs from the file.
        XCTAssertNotNil(document.model.runHistory(.undo))
        XCTAssertTrue(document.isDocumentEdited)
    }

    @MainActor
    func testSavingAgainMarksTheNewStateSaved() async throws {
        let document = makeDocument(EngineBridge())
        XCTAssertTrue(document.model.replaceText(with: "first"))
        let url = try scratchURL("saved.txt")
        try await document.save(to: url, ofType: plainText, for: .saveAsOperation)
        XCTAssertTrue(document.model.replaceText(with: "second"))

        try await document.save(to: url, ofType: plainText, for: .saveOperation)

        XCTAssertEqual(try Data(contentsOf: url), Data("second".utf8))
        XCTAssertEqual(document.model.document?.modified, false)
        // Undo returns to "first", which is no longer what the file holds.
        XCTAssertNotNil(document.model.runHistory(.undo))
        XCTAssertTrue(document.isDocumentEdited)
        XCTAssertNotNil(document.model.runHistory(.redo))
        XCTAssertFalse(document.isDocumentEdited)
    }

    @MainActor
    func testSaveToWritesACopyAndLeavesTheDocumentEdited() async throws {
        let document = makeDocument(EngineBridge())
        XCTAssertTrue(document.model.replaceText(with: "draft"))
        let url = try scratchURL("copy.txt")

        try await document.save(to: url, ofType: plainText, for: .saveToOperation)

        XCTAssertEqual(try Data(contentsOf: url), Data("draft".utf8))
        XCTAssertEqual(document.model.document?.modified, true)
        XCTAssertTrue(document.isDocumentEdited)
    }

    @MainActor
    func testAFailedSaveLeavesTheDocumentEdited() async throws {
        let document = makeDocument(EngineBridge())
        XCTAssertTrue(document.model.replaceText(with: "draft"))
        let url = try scratchURL("missing").appendingPathComponent("folder/saved.txt")

        do {
            try await document.save(to: url, ofType: plainText, for: .saveAsOperation)
            XCTFail("saving into a folder that does not exist should fail")
        } catch {}

        XCTAssertEqual(document.model.document?.modified, true)
        XCTAssertTrue(document.isDocumentEdited)
    }

    @MainActor
    func testSavingCommitsTheCompositionFirst() async throws {
        let document = makeDocument(EngineBridge())
        let view = connectedEditor(document)
        view.insertText("ab", replacementRange: notFound)
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        let url = try scratchURL("composed.txt")

        try await document.save(to: url, ofType: plainText, for: .saveAsOperation)

        XCTAssertFalse(view.isComposing)
        XCTAssertEqual(try Data(contentsOf: url), Data("abk".utf8))
        XCTAssertFalse(document.isDocumentEdited)
    }

    @MainActor
    func testClosingCommitsTheCompositionThenClosesTheEngineDocument() {
        let engine = EngineBridge()
        let document = makeDocument(engine)
        let view = connectedEditor(document)
        view.insertText("ab", replacementRange: notFound)
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        let commit = view.commit
        var committedText: String?
        view.commit = { range, text in
            let applied = commit(range, text)
            committedText = document.model.document?.text
            return applied
        }

        document.close()

        XCTAssertEqual(committedText, "abk", "the composition reaches the engine before the document closes")
        XCTAssertFalse(view.isComposing)
        XCTAssertNil(document.model.document)
        XCTAssertNil(document.model.errorMessage)
        XCTAssertEqual(engine.openDocuments, [])
    }

    @MainActor
    func testTheWindowShowsTheDocumentAndKeepsItsOwnUndoForOtherFields() throws {
        let engine = EngineBridge()
        let document = makeDocument(engine)
        document.makeWindowControllers()
        let window = try XCTUnwrap(document.windowControllers.first?.window)

        let undoManager = try XCTUnwrap(window.undoManager)
        undoManager.registerUndo(withTarget: self) { _ in }

        XCTAssertNil(document.undoManager)
        XCTAssertFalse(document.isDocumentEdited)
        XCTAssertTrue(window.windowController?.document === document)
        document.close()
        XCTAssertEqual(engine.openDocuments, [])
    }

    /// An editor connected to `document`'s model, as its window connects one.
    @MainActor
    private func connectedEditor(_ document: TextDocument) -> EditorTextView {
        let coordinator = DocumentEditor.Coordinator(model: document.model)
        let view = EditorTextView(frame: NSRect(x: 0, y: 0, width: 300, height: 100))
        DocumentEditor.connect(view, to: coordinator)
        addTeardownBlock { @MainActor in withExtendedLifetime(coordinator) {} }
        return view
    }
}
