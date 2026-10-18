import AppKit
import XCTest

@testable import TextBite

/// Opening a file passes its bytes to `document.load` unchanged, and Revert
/// reads it again into a new engine document, as ADR 0006 describes.
final class OpenDocumentTests: XCTestCase {
    private let plainText = "public.plain-text"

    /// A file holding `bytes`, removed after the test.
    private func file(_ bytes: Data, named name: String = "sample.txt") throws -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("OpenDocumentTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent(name)
        try bytes.write(to: url)
        return url
    }

    @MainActor
    func testOpeningAFileLoadsItsTextExactlyAndWritesItBackByteForByte() throws {
        // A byte-order mark, both line endings, a NUL, and letters outside
        // ASCII stay exactly as they are.
        let bytes = Data("\u{FEFF}one\r\ntwo\n\0caf\u{e9} e\u{301} \u{1F34E}\r".utf8)
        let url = try file(bytes)
        let engine = EngineBridge()
        let document = TextDocument(engine: engine)

        try document.read(from: url, ofType: plainText)

        XCTAssertEqual(document.model.document.map { Data($0.text.utf8) }, bytes)
        XCTAssertEqual(document.model.document?.canUndo, false)
        XCTAssertFalse(document.isDocumentEdited)
        XCTAssertEqual(engine.openDocuments.count, 1)
        XCTAssertEqual(try document.data(ofType: plainText), bytes)
    }

    @MainActor
    func testAFileThatIsNotUTF8IsRefusedWithTheOffsetOfItsFirstInvalidByte() throws {
        // "café" in Latin-1: the fourth byte is not UTF-8.
        let url = try file(Data([0x63, 0x61, 0x66, 0xE9, 0x0A]), named: "latin1.txt")
        let engine = EngineBridge()
        let document = TextDocument(engine: engine)

        XCTAssertThrowsError(try document.read(from: url, ofType: plainText)) { error in
            let error = error as NSError
            XCTAssertTrue(error.localizedDescription.contains("latin1.txt"), error.localizedDescription)
            let reason = error.localizedRecoverySuggestion ?? ""
            XCTAssertTrue(reason.contains("offset 3"), reason)
        }
        XCTAssertNil(document.model.document)
        XCTAssertEqual(engine.openDocuments, [])
    }

    @MainActor
    func testRevertReadsTheFileAgainIntoANewEngineDocument() throws {
        let url = try file(Data("saved\n".utf8))
        let engine = EngineBridge()
        let document = TextDocument(engine: engine)
        try document.read(from: url, ofType: plainText)
        let before = try XCTUnwrap(document.model.document?.id)
        XCTAssertTrue(document.model.replaceText(with: "changed"))
        XCTAssertTrue(document.isDocumentEdited)

        try document.revert(toContentsOf: url, ofType: plainText)

        let after = try XCTUnwrap(document.model.document)
        XCTAssertEqual(after.text, "saved\n")
        XCTAssertNotEqual(after.id, before)
        XCTAssertFalse(after.canUndo, "a revert cannot be undone")
        XCTAssertFalse(document.isDocumentEdited)
        XCTAssertEqual(engine.openDocuments, [after.id], "the old engine document is closed")
    }

    @MainActor
    func testARevertThatFailsKeepsTheDocumentAsItWas() throws {
        let url = try file(Data("saved\n".utf8))
        let engine = EngineBridge()
        let document = TextDocument(engine: engine)
        try document.read(from: url, ofType: plainText)
        XCTAssertTrue(document.model.replaceText(with: "changed"))
        let before = try XCTUnwrap(document.model.document)
        try Data([0xFF]).write(to: url)

        XCTAssertThrowsError(try document.revert(toContentsOf: url, ofType: plainText))

        XCTAssertEqual(document.model.document, before)
        XCTAssertTrue(document.isDocumentEdited)
        XCTAssertEqual(engine.openDocuments, [before.id])
    }

    @MainActor
    func testRevertCommitsTheCompositionBeforeReplacingTheText() throws {
        let url = try file(Data("saved".utf8))
        let document = TextDocument(engine: EngineBridge())
        try document.read(from: url, ofType: plainText)
        let coordinator = DocumentEditor.Coordinator(model: document.model)
        let view = EditorTextView(frame: NSRect(x: 0, y: 0, width: 300, height: 100))
        DocumentEditor.connect(view, to: coordinator)
        let notFound = NSRange(location: NSNotFound, length: 0)
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)

        try document.revert(toContentsOf: url, ofType: plainText)

        XCTAssertFalse(view.isComposing)
        XCTAssertEqual(document.model.document?.text, "saved")
        withExtendedLifetime(coordinator) {}
    }
}
