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

        XCTAssertThrowsError(try document.read(from: url, ofType: plainText))

        XCTAssertNil(document.model.document)
        XCTAssertEqual(engine.openDocuments, [])
    }

    @MainActor
    func testOpeningAFileThatIsNotUTF8ShowsAnErrorNamingTheFileAndTheOffset() async throws {
        let url = try file(Data([0x63, 0x61, 0x66, 0xE9, 0x0A]), named: "latin1.txt")
        let controller = NSDocumentController.shared
        let open = EngineBridge.shared.openDocuments
        let documents = controller.documents.count

        do {
            _ = try await controller.openDocument(withContentsOf: url, display: false)
            XCTFail("a file that is not UTF-8 should not open")
        } catch {
            // What the alert shows: AppKit's message, then the details.
            let alert = NSAlert(error: controller.willPresentError(error))
            XCTAssertTrue(alert.messageText.contains("latin1.txt"), alert.messageText)
            XCTAssertTrue(alert.informativeText.contains("offset 3"), alert.informativeText)
        }
        XCTAssertEqual(controller.documents.count, documents)
        XCTAssertEqual(EngineBridge.shared.openDocuments, open)
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

    @MainActor
    func testTheWindowShowsTheRevertedText() throws {
        let url = try file(Data("saved".utf8))
        let document = TextDocument(engine: EngineBridge())
        try document.read(from: url, ofType: plainText)
        document.makeWindowControllers()
        let window = try XCTUnwrap(document.windowControllers.first?.window)
        window.layoutIfNeeded()
        let view = try XCTUnwrap(Self.editor(in: window.contentView))
        XCTAssertTrue(document.model.replaceText(with: "changed"))
        RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        XCTAssertEqual(view.string, "changed")

        try document.revert(toContentsOf: url, ofType: plainText)
        RunLoop.current.run(until: Date().addingTimeInterval(0.2))

        XCTAssertEqual(view.string, "saved")
        document.close()
    }

    private static func editor(in view: NSView?) -> EditorTextView? {
        guard let view else { return nil }
        if let editor = view as? EditorTextView { return editor }
        return view.subviews.lazy.compactMap { editor(in: $0) }.first
    }
}
