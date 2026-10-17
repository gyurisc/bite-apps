import AppKit
import XCTest

@testable import TextBite

/// How the app turns the engine's changed range into an edit of the text
/// view after undo and redo, as ADR 0005 describes, and when the text view
/// offers Undo and Redo.
final class UndoRedoTests: XCTestCase {
    // MARK: - From the changed range to a view edit

    func testAnInsertionReplacesNothingAndSelectsTheInsertedText() {
        XCTAssertEqual(
            DocumentModel.viewChange(from: "abc", to: "abXc", changed: 2..<3),
            ViewChange(replaced: NSRange(location: 2, length: 0), text: "X", changed: NSRange(location: 2, length: 1)))
    }

    func testARemovalReplacesTheRemovedTextAndPlacesTheCursor() {
        XCTAssertEqual(
            DocumentModel.viewChange(from: "abc def", to: "abc ", changed: 4..<4),
            ViewChange(replaced: NSRange(location: 4, length: 3), text: "", changed: NSRange(location: 4, length: 0)))
    }

    func testByteOffsetsAfterAnEmojiBecomeUTF16Offsets() {
        // The apple is four UTF-8 bytes and two UTF-16 code units.
        XCTAssertEqual(
            DocumentModel.viewChange(from: "a\u{1F34E}b", to: "a\u{1F34E}!b", changed: 5..<6),
            ViewChange(replaced: NSRange(location: 3, length: 0), text: "!", changed: NSRange(location: 3, length: 1)))
    }

    func testAReplacementOfALongerLetterCountsTheOldTextsBytes() {
        // "é" is two bytes and "e" one, so the old text's replaced span ends
        // one byte later than the new text's.
        XCTAssertEqual(
            DocumentModel.viewChange(from: "caf\u{e9}!", to: "cafe!", changed: 3..<4),
            ViewChange(replaced: NSRange(location: 3, length: 1), text: "e", changed: NSRange(location: 3, length: 1)))
    }

    func testTheWholeTextCanChange() {
        XCTAssertEqual(
            DocumentModel.viewChange(from: "", to: "hello", changed: 0..<5),
            ViewChange(replaced: NSRange(location: 0, length: 0), text: "hello", changed: NSRange(location: 0, length: 5)))
    }

    func testARangeThatDoesNotFitTheTextsIsRejected() {
        XCTAssertNil(DocumentModel.viewChange(from: "abc", to: "abcd", changed: 2..<9))
        // The new text has two bytes after offset 2, more than the old has.
        XCTAssertNil(DocumentModel.viewChange(from: "a", to: "abcd", changed: 2..<2))
        // Offset 2 falls inside "é".
        XCTAssertNil(DocumentModel.viewChange(from: "x", to: "\u{e9}x", changed: 0..<1))
    }

    // MARK: - Undo and redo through the engine

    @MainActor
    private func makeEditor(_ text: String = "") -> (EditorTextView, DocumentModel, DocumentEditor.Coordinator) {
        let model = DocumentModel(engine: EngineBridge())
        model.open()
        let coordinator = DocumentEditor.Coordinator(model: model)
        let view = EditorTextView(frame: NSRect(x: 0, y: 0, width: 300, height: 100))
        view.isRichText = false
        DocumentEditor.connect(view, to: coordinator)
        if !text.isEmpty {
            view.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
        }
        return (view, model, coordinator)
    }

    @MainActor
    func testUndoAndRedoChangeTheViewAndSelectWhatChanged() {
        let (view, model, coordinator) = makeEditor("abc def")
        view.deleteBackward(nil)
        view.deleteBackward(nil)
        view.deleteBackward(nil)
        XCTAssertEqual(view.string, "abc ")

        view.undo(nil)

        XCTAssertEqual(view.string, "abc def")
        XCTAssertEqual(model.document?.text, "abc def")
        XCTAssertEqual(view.selectedRange(), NSRange(location: 4, length: 3))

        view.redo(nil)

        XCTAssertEqual(view.string, "abc ")
        XCTAssertEqual(model.document?.text, "abc ")
        XCTAssertEqual(view.selectedRange(), NSRange(location: 4, length: 0))
        withExtendedLifetime(coordinator) {}
    }

    @MainActor
    func testUndoKeepsTheViewsFont() {
        let (view, _, coordinator) = makeEditor("abc")
        let font = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
        view.font = font

        view.undo(nil)
        view.redo(nil)

        XCTAssertEqual(view.textStorage?.attribute(.font, at: 0, effectiveRange: nil) as? NSFont, font)
        withExtendedLifetime(coordinator) {}
    }

    @MainActor
    func testUndoAndRedoAreOfferedOnlyWhenTheEngineCanDoThem() {
        let (view, _, coordinator) = makeEditor()
        XCTAssertFalse(offers(view, #selector(EditorTextView.undo(_:))))
        XCTAssertFalse(offers(view, #selector(EditorTextView.redo(_:))))

        view.insertText("a", replacementRange: NSRange(location: NSNotFound, length: 0))
        XCTAssertTrue(offers(view, #selector(EditorTextView.undo(_:))))
        XCTAssertFalse(offers(view, #selector(EditorTextView.redo(_:))))

        view.undo(nil)
        XCTAssertFalse(offers(view, #selector(EditorTextView.undo(_:))))
        XCTAssertTrue(offers(view, #selector(EditorTextView.redo(_:))))
        withExtendedLifetime(coordinator) {}
    }

    @MainActor
    func testUndoShowsTheEnginesTextIfTheViewWasOutOfStep() {
        let (view, model, coordinator) = makeEditor("abc def")
        // Set directly, so neither the delegate nor the engine sees it.
        view.string = "x"

        view.undo(nil)

        XCTAssertEqual(model.document?.text, "")
        XCTAssertEqual(view.string, "")
        withExtendedLifetime(coordinator) {}
    }

    @MainActor
    func testTheMenuItemsAreNamedPlainlyWhileTheEditorHasThem() {
        let (view, _, coordinator) = makeEditor("a")
        let item = NSMenuItem(title: "Undo Typing", action: #selector(EditorTextView.undo(_:)), keyEquivalent: "")

        _ = view.validateMenuItem(item)

        XCTAssertEqual(item.title, "Undo")
        withExtendedLifetime(coordinator) {}
    }

    @MainActor
    func testUndoAndRedoAreNotOfferedDuringAComposition() {
        let (view, model, coordinator) = makeEditor("ab")
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: NSRange(location: NSNotFound, length: 0))

        XCTAssertFalse(offers(view, #selector(EditorTextView.undo(_:))))
        view.undo(nil)

        XCTAssertTrue(view.isComposing)
        XCTAssertEqual(model.document?.text, "ab")
        withExtendedLifetime(coordinator) {}
    }

    /// Whether `view` enables a menu item and other controls for `action`.
    /// Menus ask `validateMenuItem`, other controls the generic validation;
    /// both must agree.
    @MainActor
    private func offers(_ view: EditorTextView, _ action: Selector, line: UInt = #line) -> Bool {
        let item = NSMenuItem(title: "", action: action, keyEquivalent: "")
        let offered = view.validateMenuItem(item)
        XCTAssertEqual(view.validateUserInterfaceItem(item), offered, line: line)
        return offered
    }
}
