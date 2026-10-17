import AppKit
import XCTest

@testable import TextBite

/// How `EditorTextView` reports input-method compositions, as ADR 0005
/// describes. These tests call the text input methods an input method calls,
/// which UI tests cannot reach from their separate process. A fake engine
/// applies each commit, so every test can check that the view and the engine
/// end up with the same text.
final class CompositionTests: XCTestCase {
    /// One edit the view asked the engine to apply: a UTF-16 range of the
    /// engine's text and the committed text.
    private struct Commit: Equatable {
        let range: NSRange
        let text: String
    }

    /// Holds the engine's text and applies or rejects commits.
    private final class FakeEngine {
        var text: NSMutableString
        var accepts = true
        var commits: [Commit] = []

        init(_ text: String) { self.text = NSMutableString(string: text) }
    }

    private let notFound = NSRange(location: NSNotFound, length: 0)

    /// A view and an engine both holding `text`, with the selection at
    /// `selection`, connected the way `DocumentEditor` connects them.
    @MainActor
    private func makeView(_ text: String = "abc", selection: NSRange? = nil) -> (EditorTextView, FakeEngine) {
        let view = EditorTextView(frame: NSRect(x: 0, y: 0, width: 300, height: 100))
        let engine = FakeEngine(text)
        view.string = text
        view.setSelectedRange(selection ?? NSRange(location: (text as NSString).length, length: 0))
        view.commit = { range, committed in
            engine.commits.append(Commit(range: range, text: committed))
            guard engine.accepts else { return false }
            engine.text.replaceCharacters(in: range, with: committed)
            return true
        }
        view.restore = { [unowned view] in
            if view.string != engine.text as String { view.string = engine.text as String }
        }
        return (view, engine)
    }

    @MainActor
    private func assertInStep(
        _ view: EditorTextView, _ engine: FakeEngine, _ expected: String,
        file: StaticString = #filePath, line: UInt = #line
    ) {
        XCTAssertEqual(view.string, expected, "view", file: file, line: line)
        XCTAssertEqual(engine.text as String, expected, "engine", file: file, line: line)
        XCTAssertFalse(view.isComposing, file: file, line: line)
        XCTAssertFalse(view.hasMarkedText(), file: file, line: line)
    }

    @MainActor
    func testACompositionReachesTheEngineOnceWhenCommitted() {
        let (view, engine) = makeView()

        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        view.setMarkedText("\u{304B}", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        XCTAssertTrue(view.isComposing)
        XCTAssertTrue(view.letsChangesThrough)
        XCTAssertEqual(engine.commits, [])

        view.insertText("\u{304B}", replacementRange: notFound)

        XCTAssertEqual(engine.commits, [Commit(range: NSRange(location: 3, length: 0), text: "\u{304B}")])
        assertInStep(view, engine, "abc\u{304B}")
    }

    @MainActor
    func testUnmarkTextCommitsTheMarkedTextAsItStands() {
        let (view, engine) = makeView()
        view.setMarkedText("ka", selectedRange: NSRange(location: 2, length: 0), replacementRange: notFound)

        view.unmarkText()

        XCTAssertEqual(engine.commits, [Commit(range: NSRange(location: 3, length: 0), text: "ka")])
        assertInStep(view, engine, "abcka")
    }

    @MainActor
    func testEmptyMarkedTextCancelsWithNothingCommitted() {
        let (view, engine) = makeView()
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)

        view.setMarkedText("", selectedRange: NSRange(location: 0, length: 0), replacementRange: notFound)

        XCTAssertEqual(engine.commits, [Commit(range: NSRange(location: 3, length: 0), text: "")])
        assertInStep(view, engine, "abc")
    }

    @MainActor
    func testACompositionOverASelectionReplacesIt() {
        let (view, engine) = makeView(selection: NSRange(location: 1, length: 1))

        view.setMarkedText("x", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        view.insertText("y", replacementRange: notFound)

        XCTAssertEqual(engine.commits, [Commit(range: NSRange(location: 1, length: 1), text: "y")])
        assertInStep(view, engine, "ayc")
    }

    @MainActor
    func testTheAccentMenuReplacesTheLetterAlreadyTyped() {
        let (view, engine) = makeView("cafe")

        view.setMarkedText("\u{e9}", selectedRange: NSRange(location: 1, length: 0), replacementRange: NSRange(location: 3, length: 1))
        view.insertText("\u{e9}", replacementRange: notFound)

        XCTAssertEqual(engine.commits, [Commit(range: NSRange(location: 3, length: 1), text: "\u{e9}")])
        assertInStep(view, engine, "caf\u{e9}")
    }

    @MainActor
    func testACompositionThatReachesBackOverTypedTextReplacesItToo() {
        let (view, engine) = makeView()
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)

        // The input method reconverts "c" together with the marked "k".
        view.setMarkedText("Z", selectedRange: NSRange(location: 1, length: 0), replacementRange: NSRange(location: 2, length: 2))
        view.insertText("Q", replacementRange: notFound)

        XCTAssertEqual(engine.commits, [Commit(range: NSRange(location: 2, length: 1), text: "Q")])
        assertInStep(view, engine, "abQ")
    }

    @MainActor
    func testPositionsAfterTheMarkedTextMapBackToTheEngine() {
        let (view, engine) = makeView(selection: NSRange(location: 1, length: 0))
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)

        // In the view "akbc", this covers the marked "k" and the typed "bc".
        view.setMarkedText("Z", selectedRange: NSRange(location: 1, length: 0), replacementRange: NSRange(location: 1, length: 3))
        view.insertText("Q", replacementRange: notFound)

        XCTAssertEqual(engine.commits, [Commit(range: NSRange(location: 1, length: 2), text: "Q")])
        assertInStep(view, engine, "aQ")
    }

    @MainActor
    func testACommitAwayFromTheMarkedTextKeepsTheTextBetween() {
        let (view, engine) = makeView("cafe")
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)

        view.insertText("Q", replacementRange: NSRange(location: 0, length: 1))

        assertInStep(view, engine, view.string)
        XCTAssertTrue(view.string.hasPrefix("Qafe"), view.string)
    }

    @MainActor
    func testCommittingPartOfTheMarkedTextKeepsTheRest() {
        let (view, engine) = makeView("cafe")
        view.setMarkedText("abcd", selectedRange: NSRange(location: 4, length: 0), replacementRange: notFound)

        view.insertText("AB", replacementRange: NSRange(location: 4, length: 2))

        assertInStep(view, engine, view.string)
        XCTAssertTrue(view.string.hasPrefix("cafeAB"), view.string)
    }

    @MainActor
    func testRemarkingPartOfTheMarkedTextKeepsTheRest() {
        let (view, engine) = makeView("cafe")
        view.setMarkedText("abcd", selectedRange: NSRange(location: 4, length: 0), replacementRange: notFound)

        view.setMarkedText("X", selectedRange: NSRange(location: 1, length: 0), replacementRange: NSRange(location: 5, length: 1))
        view.insertText("Y", replacementRange: notFound)

        assertInStep(view, engine, view.string)
        XCTAssertTrue(view.string.hasPrefix("cafe"), view.string)
    }

    @MainActor
    func testBackspaceThroughTheMarkedTextEndsTheComposition() {
        let (view, engine) = makeView("cafe!", selection: NSRange(location: 4, length: 0))
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)

        view.deleteBackward(nil)

        assertInStep(view, engine, "cafe!")
    }

    @MainActor
    func testEachCommittedSegmentIsItsOwnComposition() {
        let (view, engine) = makeView()

        view.setMarkedText("ab", selectedRange: NSRange(location: 2, length: 0), replacementRange: notFound)
        view.insertText("A", replacementRange: notFound)
        view.setMarkedText("c", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        view.insertText("C", replacementRange: notFound)

        XCTAssertEqual(
            engine.commits,
            [
                Commit(range: NSRange(location: 3, length: 0), text: "A"),
                Commit(range: NSRange(location: 4, length: 0), text: "C"),
            ])
        assertInStep(view, engine, "abcAC")
    }

    @MainActor
    func testEndCompositionCommitsTheMarkedText() {
        let (view, engine) = makeView()
        view.setMarkedText("ka", selectedRange: NSRange(location: 2, length: 0), replacementRange: notFound)

        view.endComposition()

        assertInStep(view, engine, "abcka")
    }

    @MainActor
    func testARejectedCommitPutsTheEnginesTextBack() {
        let (view, engine) = makeView()
        engine.accepts = false
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)

        view.insertText("\u{304B}", replacementRange: notFound)

        XCTAssertEqual(engine.commits.count, 1)
        assertInStep(view, engine, "abc")
    }

    @MainActor
    func testTypingWithoutAnInputMethodIsNotAComposition() {
        let (view, engine) = makeView()

        view.insertText("d", replacementRange: notFound)

        XCTAssertEqual(engine.commits, [])
        XCTAssertFalse(view.isComposing)
        XCTAssertEqual(view.string, "abcd")
    }
}
