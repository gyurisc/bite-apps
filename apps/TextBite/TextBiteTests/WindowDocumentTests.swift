import AppKit
import XCTest

@testable import TextBite

/// Each window's document lives in the app's one engine, and closing the
/// window commits any composition and then closes the document with
/// `document.close`.
final class WindowDocumentTests: XCTestCase {
    private let notFound = NSRange(location: NSNotFound, length: 0)

    @MainActor
    func testWindowsShareTheEngineButNotTheDocument() throws {
        let engine = EngineBridge()
        let first = DocumentModel(engine: engine)
        let second = DocumentModel(engine: engine)
        first.open()
        second.open()

        XCTAssertTrue(first.replaceText(with: "one"))
        XCTAssertTrue(second.replaceText(with: "two"))

        XCTAssertEqual(first.document?.text, "one")
        XCTAssertEqual(second.document?.text, "two")
        let ids = try [XCTUnwrap(first.document?.id), XCTUnwrap(second.document?.id)]
        XCTAssertEqual(engine.openDocuments, Set(ids))
        XCTAssertEqual(engine.openDocuments.count, 2)
    }

    @MainActor
    func testAModelCreatesItsDocumentOnlyWhenOpened() {
        let engine = EngineBridge()
        let model = DocumentModel(engine: engine)
        XCTAssertNil(model.document)
        XCTAssertEqual(engine.openDocuments, [])

        model.open()
        model.open()

        XCTAssertNotNil(model.document)
        XCTAssertEqual(engine.openDocuments.count, 1)
    }

    @MainActor
    func testClosingRemovesTheDocumentFromTheEngine() throws {
        let engine = EngineBridge()
        let model = DocumentModel(engine: engine)
        model.open()
        let id = try XCTUnwrap(model.document?.id)

        model.close()

        XCTAssertNil(model.document)
        XCTAssertNil(model.errorMessage)
        XCTAssertEqual(engine.openDocuments, [])
        XCTAssertThrowsError(try engine.dispatch("document.replace_text", ["document": String(id), "text": "x"]))
        XCTAssertFalse(model.replaceText(with: "x"))
        // A closed model neither reopens nor closes again.
        model.open()
        XCTAssertNil(model.document)
        XCTAssertEqual(engine.openDocuments, [])
        model.close()
        XCTAssertNil(model.errorMessage)
    }

    @MainActor
    func testClosingTheWindowCommitsTheCompositionThenClosesTheDocument() {
        let engine = EngineBridge()
        let model = DocumentModel(engine: engine)
        model.open()
        let coordinator = DocumentEditor.Coordinator(model: model)
        let scrollView = EditorTextView.scrollableTextView()
        let view = scrollView.documentView as! EditorTextView
        DocumentEditor.connect(view, to: coordinator)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 300, height: 200), styleMask: [.titled, .closable],
            backing: .buffered, defer: true)
        window.isReleasedWhenClosed = false
        window.contentView = scrollView

        view.insertText("ab", replacementRange: notFound)
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        let commit = view.commit
        var committedText: String?
        view.commit = { range, text in
            let applied = commit(range, text)
            committedText = model.document?.text
            return applied
        }

        window.close()

        XCTAssertEqual(committedText, "abk", "the composition reaches the document before it closes")
        XCTAssertFalse(view.isComposing)
        XCTAssertNil(model.document)
        XCTAssertNil(model.errorMessage)
        XCTAssertEqual(engine.openDocuments, [])
        withExtendedLifetime(coordinator) {}
    }

    @MainActor
    func testMovingTheViewToAnotherWindowFollowsThatWindow() {
        let engine = EngineBridge()
        let model = DocumentModel(engine: engine)
        model.open()
        let coordinator = DocumentEditor.Coordinator(model: model)
        let view = EditorTextView(frame: NSRect(x: 0, y: 0, width: 300, height: 100))
        DocumentEditor.connect(view, to: coordinator)
        let first = makeWindow()
        let second = makeWindow()
        first.contentView?.addSubview(view)
        view.removeFromSuperview()
        second.contentView?.addSubview(view)

        first.close()
        XCTAssertNotNil(model.document, "the window the view left does not close the document")

        second.close()
        XCTAssertNil(model.document)
        withExtendedLifetime(coordinator) {}
    }

    @MainActor
    private func makeWindow() -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 300, height: 200), styleMask: [.titled, .closable],
            backing: .buffered, defer: true)
        window.isReleasedWhenClosed = false
        return window
    }
}
