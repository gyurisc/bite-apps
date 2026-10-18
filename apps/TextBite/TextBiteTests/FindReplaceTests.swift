import AppKit
import XCTest

@testable import TextBite

/// The find bar's Replace and Replace All reach the engine as one edit each,
/// through the editor's delegate like typing, as ADR 0004 describes, and
/// typing in the find bar's own fields leaves the document alone.
final class FindReplaceTests: XCTestCase {
    /// Records each change the text view asks its delegate about, then lets
    /// the editor's coordinator decide it.
    private final class DelegateSpy: NSObject, NSTextViewDelegate {
        let coordinator: DocumentEditor.Coordinator
        private(set) var changes: [[NSRange]] = []

        init(_ coordinator: DocumentEditor.Coordinator) {
            self.coordinator = coordinator
        }

        func textView(
            _ textView: NSTextView, shouldChangeTextInRanges ranges: [NSValue], replacementStrings: [String]?
        ) -> Bool {
            if replacementStrings != nil {
                changes.append(ranges.map(\.rangeValue))
            }
            return coordinator.textView(
                textView, shouldChangeTextInRanges: ranges, replacementStrings: replacementStrings)
        }

        func textDidChange(_ notification: Notification) {
            coordinator.textDidChange(notification)
        }
    }

    /// A document window showing `text`, with its editor's changes recorded.
    /// The find pasteboard is shared with other apps, so it is put back
    /// afterwards.
    @MainActor
    private func makeEditor(_ text: String) throws -> (TextDocument, NSWindow, EditorTextView, DelegateSpy) {
        let findPasteboard = NSPasteboard(name: .find)
        let savedSearch = findPasteboard.string(forType: .string)
        let document = TextDocument(engine: EngineBridge())
        try document.model.load(Data(text.utf8))
        document.makeWindowControllers()
        let window = try XCTUnwrap(document.windowControllers.first?.window)
        window.layoutIfNeeded()
        let view = try XCTUnwrap(Self.editor(in: window.contentView))
        let spy = DelegateSpy(try XCTUnwrap(view.delegate as? DocumentEditor.Coordinator))
        view.delegate = spy
        window.makeFirstResponder(view)
        addTeardownBlock { @MainActor in
            withExtendedLifetime(spy) {}
            document.close()
            findPasteboard.clearContents()
            if let savedSearch { findPasteboard.setString(savedSearch, forType: .string) }
        }
        return (document, window, view, spy)
    }

    /// Sends `action` to the editor as the Find menu's items do.
    @MainActor
    private func perform(_ action: NSTextFinder.Action, in view: EditorTextView) {
        let item = NSMenuItem(title: "", action: #selector(NSTextView.performTextFinderAction(_:)), keyEquivalent: "")
        item.tag = action.rawValue
        view.performTextFinderAction(item)
        RunLoop.current.run(until: Date().addingTimeInterval(0.2))
    }

    /// Opens the find bar with its replace field and types `find` and
    /// `replacement` into its fields, as a user would.
    @MainActor
    private func showReplace(find: String, with replacement: String, in view: EditorTextView, _ window: NSWindow)
        throws
    {
        perform(.showReplaceInterface, in: view)
        try type(find, into: searchField(of: view), window)
        try type(replacement, into: replaceField(of: view), window)
        XCTAssertEqual(try searchField(of: view).stringValue, find)
        XCTAssertEqual(try replaceField(of: view).stringValue, replacement)
    }

    /// Replaces the text of `field` with `text` through its field editor.
    @MainActor
    private func type(_ text: String, into field: NSTextField, _ window: NSWindow) throws {
        XCTAssertTrue(window.makeFirstResponder(field))
        let fieldEditor = try XCTUnwrap(window.firstResponder as? NSTextView)
        fieldEditor.selectAll(nil)
        fieldEditor.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    @MainActor
    private func searchField(of view: EditorTextView) throws -> NSSearchField {
        try XCTUnwrap(findBarFields(of: view).compactMap { $0 as? NSSearchField }.first, "no search field")
    }

    @MainActor
    private func replaceField(of view: EditorTextView) throws -> NSTextField {
        try XCTUnwrap(findBarFields(of: view).first { !($0 is NSSearchField) }, "no replace field")
    }

    @MainActor
    private func findBarFields(of view: EditorTextView) throws -> [NSTextField] {
        let bar = try XCTUnwrap(view.enclosingScrollView?.findBarView, "no find bar")
        return Self.editableFields(in: bar)
    }

    private static func editableFields(in view: NSView) -> [NSTextField] {
        let own = (view as? NSTextField).map { $0.isEditable ? [$0] : [] } ?? []
        return own + view.subviews.flatMap(editableFields)
    }

    private static func editor(in view: NSView?) -> EditorTextView? {
        guard let view else { return nil }
        if let editor = view as? EditorTextView { return editor }
        return view.subviews.lazy.compactMap { editor(in: $0) }.first
    }

    @MainActor
    func testReplaceAllChangesEveryMatchInOneCallAndOneUndoStep() throws {
        let (document, window, view, spy) = try makeEditor("cat and cat and cat")
        try showReplace(find: "cat", with: "dog", in: view, window)

        perform(.replaceAll, in: view)

        XCTAssertEqual(
            spy.changes,
            [[NSRange(location: 0, length: 3), NSRange(location: 8, length: 3), NSRange(location: 16, length: 3)]])
        XCTAssertEqual(view.string, "dog and dog and dog")
        XCTAssertEqual(document.model.document?.text, "dog and dog and dog")
        XCTAssertTrue(document.isDocumentEdited)

        XCTAssertNotNil(document.model.runHistory(.undo))
        XCTAssertEqual(document.model.document?.text, "cat and cat and cat")
        XCTAssertEqual(document.model.document?.canUndo, false)
    }

    @MainActor
    func testReplaceChangesTheCurrentMatchAsOneRange() throws {
        let (document, window, view, spy) = try makeEditor("a cat and a cat")
        try showReplace(find: "cat", with: "dog", in: view, window)
        view.setSelectedRange(NSRange(location: 0, length: 0))
        perform(.nextMatch, in: view)
        XCTAssertEqual(view.selectedRange(), NSRange(location: 2, length: 3))

        perform(.replace, in: view)

        XCTAssertEqual(spy.changes, [[NSRange(location: 2, length: 3)]])
        XCTAssertEqual(view.string, "a dog and a cat")
        XCTAssertEqual(document.model.document?.text, "a dog and a cat")
    }

    @MainActor
    func testReplaceAllWithOneMatchSendsOneRange() throws {
        let (document, window, view, spy) = try makeEditor("a cat sat")
        try showReplace(find: "cat", with: "dog", in: view, window)

        perform(.replaceAll, in: view)

        XCTAssertEqual(spy.changes, [[NSRange(location: 2, length: 3)]])
        XCTAssertEqual(view.string, "a dog sat")
        XCTAssertEqual(document.model.document?.text, "a dog sat")
    }

    @MainActor
    func testTypingInTheFindFieldsLeavesTheDocumentUnedited() throws {
        let (document, window, view, spy) = try makeEditor("a cat sat")
        perform(.showReplaceInterface, in: view)

        try type("cat", into: searchField(of: view), window)
        try type("dog", into: replaceField(of: view), window)

        XCTAssertEqual(try searchField(of: view).stringValue, "cat")
        XCTAssertEqual(try replaceField(of: view).stringValue, "dog")
        XCTAssertEqual(spy.changes, [])
        XCTAssertEqual(view.string, "a cat sat")
        XCTAssertEqual(document.model.document?.text, "a cat sat")
        XCTAssertEqual(document.model.document?.modified, false)
        XCTAssertFalse(document.isDocumentEdited)
    }
}
