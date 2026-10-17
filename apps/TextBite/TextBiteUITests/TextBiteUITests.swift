import AppKit
import XCTest

/// Drives the app the way a person does, so UI changes can be checked
/// without a manual run. Each test launches a fresh app and engine.
///
/// Launched with `--ui-testing`, the app also shows the engine's own text,
/// so each test checks that the engine and the text view agree.
final class TextBiteUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testTypedTextReachesTheEngine() throws {
        let editor = launchEditor()

        editor.typeText("hello world")

        assertEngineAndEditorShow("hello world", editor)
    }

    @MainActor
    func testBackspaceAndForwardDeleteReachTheEngine() throws {
        let editor = launchEditor()
        editor.typeText("hello")

        editor.typeKey(.delete, modifierFlags: [])
        editor.typeKey(.leftArrow, modifierFlags: [])
        editor.typeKey(.leftArrow, modifierFlags: [])
        editor.typeKey(.forwardDelete, modifierFlags: [])

        assertEngineAndEditorShow("hel", editor)
    }

    @MainActor
    func testTypingOverASelectionReplacesItInTheEngine() throws {
        let editor = launchEditor()
        editor.typeText("one two")

        editor.typeKey("a", modifierFlags: .command)
        editor.typeText("three")

        assertEngineAndEditorShow("three", editor)
    }

    @MainActor
    func testEditsAroundLettersOutsideASCIIUseTheRightOffsets() throws {
        let editor = launchEditor()
        editor.typeText("caf\u{e9} \u{fc}ber")

        for _ in 0..<5 {
            editor.typeKey(.leftArrow, modifierFlags: [])
        }
        editor.typeText("!")
        editor.typeKey(.rightArrow, modifierFlags: [])
        editor.typeKey(.rightArrow, modifierFlags: [])
        editor.typeKey(.delete, modifierFlags: [])

        assertEngineAndEditorShow("caf\u{e9}! ber", editor)
    }

    @MainActor
    func testPastedTextAndAnEditAfterAnEmojiReachTheEngine() throws {
        // The apple is two UTF-16 code units and four UTF-8 bytes, so an edit
        // after it shows whether offsets are converted correctly.
        borrowPasteboard(holding: "a\u{1F34E}b")
        let editor = launchEditor()

        editor.typeKey("v", modifierFlags: .command)
        editor.typeKey(.leftArrow, modifierFlags: [])
        editor.typeText("!")

        assertEngineAndEditorShow("a\u{1F34E}!b", editor)
    }

    @MainActor
    func testCommandZDoesNotChangeTheTextYet() throws {
        // The text view's own undo is off; engine undo arrives in a later item.
        let editor = launchEditor()
        editor.typeText("kept")

        editor.typeKey("z", modifierFlags: .command)

        assertEngineAndEditorShow("kept", editor)
    }

    // MARK: - Helpers

    /// Puts `string` on the general pasteboard for one test and restores
    /// what was there afterwards, so running the tests keeps the clipboard.
    private func borrowPasteboard(holding string: String) {
        let pasteboard = NSPasteboard.general
        let saved = (pasteboard.pasteboardItems ?? []).map { item in
            let copy = NSPasteboardItem()
            for type in item.types {
                if let data = item.data(forType: type) { copy.setData(data, forType: type) }
            }
            return copy
        }
        addTeardownBlock {
            pasteboard.clearContents()
            pasteboard.writeObjects(saved)
        }
        pasteboard.clearContents()
        pasteboard.setString(string, forType: .string)
    }

    private var app: XCUIApplication!

    /// Launches the app and returns its focused text view.
    @MainActor
    private func launchEditor() -> XCUIElement {
        app = XCUIApplication()
        app.launchArguments.append("--ui-testing")
        app.launch()
        let editor = app.textViews["editor"]
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        editor.click()
        return editor
    }

    /// Checks that the text view and the engine both hold exactly `expected`.
    /// Strings are compared as UTF-8 bytes, because Swift's `==` treats a
    /// precomposed and a decomposed letter as equal.
    @MainActor
    private func assertEngineAndEditorShow(
        _ expected: String, _ editor: XCUIElement, file: StaticString = #filePath, line: UInt = #line
    ) {
        let bytes = Array(expected.utf8)
        XCTAssertEqual(
            Array((editor.value as? String ?? "").utf8), bytes, "text view", file: file, line: line)

        let engineText = app.descendants(matching: .any)["engine-text"]
        XCTAssertTrue(engineText.waitForExistence(timeout: 5), file: file, line: line)
        var shown = ""
        let deadline = Date().addingTimeInterval(5)
        repeat {
            // SwiftUI may expose a text's string as its label or as its value.
            shown = engineText.label.isEmpty ? (engineText.value as? String ?? "") : engineText.label
            if Array(shown.utf8) == bytes { return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        } while Date() < deadline
        XCTFail(
            "the engine's text is \(shown.debugDescription), not \(expected.debugDescription)",
            file: file, line: line)
    }
}
