import AppKit
import XCTest

@testable import TextBite

/// When the engine gives inserted line breaks the document's line ending,
/// the view shows the engine's text and puts the insertion point after what
/// was inserted, as ADR 0006 describes.
final class LineEndingTests: XCTestCase {
    private let notFound = NSRange(location: NSNotFound, length: 0)

    /// An editor showing a document loaded with `text`, its insertion point
    /// at `caret`.
    @MainActor
    private func editor(loading text: String, caret: Int) throws -> (DocumentModel, EditorTextView) {
        let model = DocumentModel(engine: EngineBridge())
        try model.load(Data(text.utf8))
        let coordinator = DocumentEditor.Coordinator(model: model)
        let view = EditorTextView(frame: NSRect(x: 0, y: 0, width: 300, height: 100))
        view.isRichText = false
        view.allowsUndo = false
        DocumentEditor.connect(view, to: coordinator)
        view.setSelectedRange(NSRange(location: caret, length: 0))
        addTeardownBlock { @MainActor in withExtendedLifetime(coordinator) {} }
        return (model, view)
    }

    @MainActor
    private func assertShows(
        _ expected: String, _ model: DocumentModel, _ view: EditorTextView, caret: Int,
        file: StaticString = #filePath, line: UInt = #line
    ) {
        XCTAssertEqual(Array(view.string.utf8), Array(expected.utf8), "view", file: file, line: line)
        XCTAssertEqual(
            model.document.map { Array($0.text.utf8) }, Array(expected.utf8), "engine", file: file, line: line)
        XCTAssertEqual(view.selectedRange(), NSRange(location: caret, length: 0), file: file, line: line)
    }

    @MainActor
    func testReturnInACRLFDocumentInsertsCRLFAndTypingContinuesAfterIt() throws {
        let (model, view) = try editor(loading: "one\r\ntwo", caret: 8)

        view.insertText("\n", replacementRange: notFound)
        assertShows("one\r\ntwo\r\n", model, view, caret: 10)

        view.insertText("x", replacementRange: notFound)
        assertShows("one\r\ntwo\r\nx", model, view, caret: 11)

        // Return and the letter after it are one typing step.
        XCTAssertNotNil(model.runHistory(.undo))
        XCTAssertEqual(model.document?.text, "one\r\ntwo")
    }

    @MainActor
    func testReturnBeforeAnExistingBreakPutsTheInsertionPointAfterTheNewOne() throws {
        let (model, view) = try editor(loading: "a\r\nb", caret: 1)

        view.insertText("\n", replacementRange: notFound)
        assertShows("a\r\n\r\nb", model, view, caret: 3)

        view.insertText("x", replacementRange: notFound)
        assertShows("a\r\nx\r\nb", model, view, caret: 4)
    }

    @MainActor
    func testPastedLineBreaksTakeTheDocumentsLineEnding() throws {
        let (model, view) = try editor(loading: "a\rb", caret: 1)

        view.insertText("x\ny\r\nz", replacementRange: notFound)

        assertShows("ax\ry\rz\rb", model, view, caret: 6)
    }

    @MainActor
    func testReplacingASelectionWithALineBreakConvertsIt() throws {
        let (model, view) = try editor(loading: "one two\r\n", caret: 0)
        view.setSelectedRange(NSRange(location: 3, length: 1))

        view.insertText("\n", replacementRange: notFound)

        assertShows("one\r\ntwo\r\n", model, view, caret: 5)
    }

    @MainActor
    func testEditsThatNeedNoConversionApplyAsTyped() throws {
        let (model, view) = try editor(loading: "a\nb", caret: 1)

        view.insertText("\n", replacementRange: notFound)
        assertShows("a\n\nb", model, view, caret: 2)

        let (crlfModel, crlfView) = try editor(loading: "a\r\nb", caret: 4)
        crlfView.insertText("c\u{e9}", replacementRange: notFound)
        assertShows("a\r\nbc\u{e9}", crlfModel, crlfView, caret: 6)
    }

    @MainActor
    func testTheViewKeepsItsFontWhenItShowsTheEnginesText() throws {
        let (_, view) = try editor(loading: "a\r\nb", caret: 4)
        let font = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
        view.font = font
        view.typingAttributes[.font] = font

        view.insertText("\n", replacementRange: notFound)

        XCTAssertEqual(view.textStorage?.attribute(.font, at: 4, effectiveRange: nil) as? NSFont, font)
    }
}
