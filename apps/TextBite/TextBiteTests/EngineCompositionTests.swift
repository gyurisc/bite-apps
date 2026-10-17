import AppKit
import XCTest

@testable import TextBite

/// An `EditorTextView` connected to a real engine through `DocumentEditor`,
/// so typing and compositions are checked against the engine's own text.
final class EngineCompositionTests: XCTestCase {
    private let notFound = NSRange(location: NSNotFound, length: 0)

    @MainActor
    private func makeEditor() -> (EditorTextView, DocumentModel, DocumentEditor.Coordinator) {
        let model = DocumentModel()
        let coordinator = DocumentEditor.Coordinator(model: model)
        let view = EditorTextView(frame: NSRect(x: 0, y: 0, width: 300, height: 100))
        DocumentEditor.connect(view, to: coordinator)
        return (view, model, coordinator)
    }

    @MainActor
    func testTypingAndAComposedCharacterReachTheEngineOnce() throws {
        let (view, model, coordinator) = makeEditor()

        view.insertText("ab", replacementRange: notFound)
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        view.setMarkedText("\u{304B}", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)

        // The engine does not see the marked text.
        XCTAssertEqual(model.document?.text, "ab")
        XCTAssertEqual(view.string, "ab\u{304B}")

        view.insertText("\u{304B}", replacementRange: notFound)

        XCTAssertEqual(model.document?.text, "ab\u{304B}")
        XCTAssertEqual(view.string, "ab\u{304B}")
        withExtendedLifetime(coordinator) {}
    }

    @MainActor
    func testACancelledCompositionLeavesTheEngineUnchanged() throws {
        let (view, model, coordinator) = makeEditor()
        view.insertText("ab", replacementRange: notFound)

        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        view.setMarkedText("", selectedRange: NSRange(location: 0, length: 0), replacementRange: notFound)

        XCTAssertEqual(model.document?.text, "ab")
        XCTAssertEqual(view.string, "ab")
        withExtendedLifetime(coordinator) {}
    }

    @MainActor
    func testTypingAfterACompositionStillGoesThroughTheEngine() throws {
        let (view, model, coordinator) = makeEditor()
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        view.insertText("\u{304B}", replacementRange: notFound)

        view.insertText("!", replacementRange: notFound)
        view.setSelectedRange(NSRange(location: 0, length: 0))
        view.insertText("<", replacementRange: notFound)

        XCTAssertFalse(view.isComposing)
        XCTAssertEqual(model.document?.text, "<\u{304B}!")
        XCTAssertEqual(view.string, "<\u{304B}!")
        XCTAssertEqual(model.document?.canUndo, true)
        withExtendedLifetime(coordinator) {}
    }
}
