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
    func testCommandZAndShiftCommandZUndoAndRedoInTheEngine() throws {
        let editor = launchEditor()
        editor.typeText("hello")

        editor.typeKey("z", modifierFlags: .command)
        assertEngineAndEditorShow("", editor)

        editor.typeKey("z", modifierFlags: [.command, .shift])
        assertEngineAndEditorShow("hello", editor)
    }

    @MainActor
    func testUndoSelectsTheTextItRestores() throws {
        let editor = launchEditor()
        editor.typeText("abc def")
        for _ in 0..<3 {
            editor.typeKey(.delete, modifierFlags: [])
        }

        editor.typeKey("z", modifierFlags: .command)
        assertEngineAndEditorShow("abc def", editor)
        // Typing replaces the selection, so this shows "def" was selected.
        editor.typeText("Q")

        assertEngineAndEditorShow("abc Q", editor)
    }

    @MainActor
    func testUndoAndRedoAreEnabledFromTheEngine() throws {
        let editor = launchEditor()
        assertEditMenu(undo: false, redo: false)

        editor.typeText("a")
        assertEditMenu(undo: true, redo: false)

        editor.typeKey("z", modifierFlags: .command)
        assertEngineAndEditorShow("", editor)
        assertEditMenu(undo: false, redo: true)
    }

    @MainActor
    func testAnotherFieldKeepsItsOwnUndo() throws {
        let editor = launchEditor()
        editor.typeText("doc")
        let field = app.textFields["scratch-field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.click()
        field.typeText("note")

        field.typeKey("z", modifierFlags: .command)

        XCTAssertEqual(field.value as? String ?? "", "")
        assertEngineAndEditorShow("doc", editor)

        // The text view records no undo of its own for this ⌘Z to reach.
        field.typeKey("z", modifierFlags: .command)
        assertEngineAndEditorShow("doc", editor)
    }

    @MainActor
    func testEachWindowEditsItsOwnDocumentAndClosingOneClosesIt() throws {
        let editor = launchEditor()
        editor.typeText("first")
        let firstWindow = app.windows.element(boundBy: 0)
        assertOpenDocuments(1)

        app.typeKey("n", modifierFlags: .command)
        XCTAssertTrue(waitUntil { self.app.windows.count == 2 })
        assertOpenDocuments(2)
        // The new window is in front.
        let secondWindow = app.windows.element(boundBy: 0)
        let secondEditor = secondWindow.textViews["editor"]
        secondEditor.click()
        secondEditor.typeText("second")
        assertEngineAndEditorShow("second", secondEditor, in: secondWindow)

        app.typeKey("w", modifierFlags: .command)
        answer(.dontSave)
        XCTAssertTrue(waitUntil { self.app.windows.count == 1 })
        assertOpenDocuments(1)

        let firstEditor = firstWindow.textViews["editor"]
        firstEditor.click()
        firstEditor.typeKey(.downArrow, modifierFlags: .command)
        firstEditor.typeText("!")
        assertEngineAndEditorShow("first!", firstEditor, in: firstWindow)
    }

    @MainActor
    func testTypingInAnotherFieldLeavesTheDocumentUnchanged() throws {
        _ = launchEditor()
        let field = app.textFields["scratch-field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.click()
        field.typeText("note")

        // An unchanged document closes without asking to save.
        app.typeKey("w", modifierFlags: .command)

        XCTAssertTrue(waitUntil { self.app.windows.count == 0 }, "the window asked to save")
    }

    @MainActor
    func testUndoingEveryEditLeavesTheDocumentUnchanged() throws {
        let editor = launchEditor()
        editor.typeText("a")
        editor.typeKey("z", modifierFlags: .command)
        assertEngineAndEditorShow("", editor)

        app.typeKey("w", modifierFlags: .command)

        XCTAssertTrue(waitUntil { self.app.windows.count == 0 }, "the window asked to save")
    }

    @MainActor
    func testSavingANewDocumentWritesTheEnginesBytesAndMarksItSaved() throws {
        // Typed text, so no emoji: the tests paste those.
        let text = "caf\u{e9} \u{fc}ber"
        let folder = try scratchFolder()
        let file = folder.appendingPathComponent("saved.txt")
        let editor = launchEditor()
        editor.typeText(text)
        assertEngineAndEditorShow(text, editor)
        let window = app.windows.element(boundBy: 0)

        app.typeKey("s", modifierFlags: .command)
        let panel = window.sheets.firstMatch
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        // The name field has focus with the name selected, and the panel
        // adds the extension whether or not it shows it.
        app.typeText("saved")
        choose(folder.path)

        XCTAssertTrue(waitUntil { FileManager.default.fileExists(atPath: file.path) }, "nothing was saved")
        XCTAssertTrue(waitUntil { !panel.exists })
        XCTAssertEqual(try Data(contentsOf: file), Data(text.utf8))
        // Undoing an edit after the save returns to the saved state.
        editor.typeText("!")
        editor.typeKey("z", modifierFlags: .command)
        assertEngineAndEditorShow(text, editor)
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(waitUntil { self.app.windows.count == 0 }, "the saved window asked to save")
    }

    @MainActor
    func testOpeningAFileShowsItsTextAndSaveWritesAnEdit() throws {
        let file = try scratchFolder().appendingPathComponent("sample.txt")
        try Data("first line\nsecond line".utf8).write(to: file)
        _ = launchEditor()

        app.typeKey("o", modifierFlags: .command)
        choose(file.path)

        let window = app.windows.matching(NSPredicate(format: "title BEGINSWITH 'sample'")).firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 10), "no window for the file")
        let editor = window.textViews["editor"]
        assertEngineAndEditorShow("first line\nsecond line", editor, in: window)
        editor.click()
        editor.typeKey(.downArrow, modifierFlags: .command)
        editor.typeText("!")
        app.typeKey("s", modifierFlags: .command)

        XCTAssertTrue(
            waitUntil { (try? Data(contentsOf: file)) == Data("first line\nsecond line!".utf8) },
            "the edit was not saved")
    }

    @MainActor
    func testAFileThatIsNotUTF8IsRefusedWithTheOffsetOfItsFirstInvalidByte() throws {
        let file = try scratchFolder().appendingPathComponent("latin1.txt")
        // "café" in Latin-1: the fourth byte is not UTF-8.
        try Data([0x63, 0x61, 0x66, 0xE9, 0x0A]).write(to: file)
        _ = launchEditor()

        app.typeKey("o", modifierFlags: .command)
        choose(file.path)

        let message = app.descendants(matching: .staticText)
            .matching(NSPredicate(format: "value CONTAINS 'offset 3' OR label CONTAINS 'offset 3'")).firstMatch
        XCTAssertTrue(message.waitForExistence(timeout: 10), "no error gave the offset")
        app.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(waitUntil { !message.exists })
        XCTAssertEqual(
            app.windows.matching(NSPredicate(format: "title BEGINSWITH 'latin1'")).count, 0,
            "a window opened for the file")
    }

    @MainActor
    func testRevertToSavedShowsTheFileAgain() throws {
        let file = try scratchFolder().appendingPathComponent("revert.txt")
        try Data("saved".utf8).write(to: file)
        _ = launchEditor()
        app.typeKey("o", modifierFlags: .command)
        choose(file.path)
        let window = app.windows.matching(NSPredicate(format: "title BEGINSWITH 'revert'")).firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 10), "no window for the file")
        let editor = window.textViews["editor"]
        editor.click()
        editor.typeKey(.downArrow, modifierFlags: .command)
        editor.typeText(" and changed")
        assertEngineAndEditorShow("saved and changed", editor, in: window)

        app.menuBars.menuBarItems["File"].click()
        app.menuBars.menuItems["Revert to Saved"].click()
        let revert = app.sheets.buttons["Revert"]
        XCTAssertTrue(revert.waitForExistence(timeout: 5), "no sheet asked to revert")
        revert.click()

        assertEngineAndEditorShow("saved", editor, in: window)
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(waitUntil { !window.exists }, "the reverted window asked to save")
    }

    @MainActor
    func testCancellingCloseKeepsTheWindowAndItsText() throws {
        let editor = launchEditor()
        editor.typeText("draft")

        app.typeKey("w", modifierFlags: .command)
        answer(.cancel)

        XCTAssertTrue(waitUntil { self.app.windows.count == 1 })
        assertEngineAndEditorShow("draft", editor)
    }

    @MainActor
    func testClosingAnEditedFileWithSaveWritesIt() throws {
        let file = try scratchFolder().appendingPathComponent("close.txt")
        try Data("kept".utf8).write(to: file)
        _ = launchEditor()
        app.typeKey("o", modifierFlags: .command)
        choose(file.path)
        let window = app.windows.matching(NSPredicate(format: "title BEGINSWITH 'close'")).firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 10), "no window for the file")
        let editor = window.textViews["editor"]
        editor.click()
        editor.typeKey(.downArrow, modifierFlags: .command)
        editor.typeText(" and saved")
        assertEngineAndEditorShow("kept and saved", editor, in: window)

        app.typeKey("w", modifierFlags: .command)
        answer(.save)

        XCTAssertTrue(waitUntil { !window.exists }, "the window stayed open")
        XCTAssertEqual(try Data(contentsOf: file), Data("kept and saved".utf8))
    }

    @MainActor
    func testQuittingWithUnsavedChangesAsksFirst() throws {
        let editor = launchEditor()
        editor.typeText("draft")

        app.typeKey("q", modifierFlags: .command)
        answer(.cancel)
        XCTAssertTrue(waitUntil { self.app.windows.count == 1 })
        XCTAssertEqual(app.state, .runningForeground)
        assertEngineAndEditorShow("draft", editor)

        app.typeKey("q", modifierFlags: .command)
        answer(.dontSave)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10), "the app did not quit")
    }

    // MARK: - Helpers

    /// A new folder in the test runner's temporary folder, removed after the
    /// test, for files the app opens or saves.
    private func scratchFolder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("TextBiteUITests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: folder) }
        return folder
    }

    /// The Open panel, or the Save panel's sheet.
    private var panel: XCUIElement {
        app.descendants(matching: .any).matching(
            NSPredicate(
                format: "elementType == %d OR elementType == %d OR (elementType == %d AND title == 'Open')",
                XCUIElement.ElementType.sheet.rawValue, XCUIElement.ElementType.dialog.rawValue,
                XCUIElement.ElementType.window.rawValue)
        ).firstMatch
    }

    /// Once the Open or Save panel is up, goes to `path` and confirms: a
    /// file to open, or the folder to save in. Return confirms the panel
    /// only if going to the path left it open.
    @MainActor
    private func choose(_ path: String, file: StaticString = #filePath, line: UInt = #line) {
        let panel = self.panel
        XCTAssertTrue(panel.waitForExistence(timeout: 10), "no panel opened", file: file, line: line)
        RunLoop.current.run(until: Date().addingTimeInterval(0.5))
        app.typeKey("g", modifierFlags: [.command, .shift])
        RunLoop.current.run(until: Date().addingTimeInterval(1))
        app.typeText(path + "\r")
        RunLoop.current.run(until: Date().addingTimeInterval(1))
        if panel.exists {
            app.typeKey(.return, modifierFlags: [])
        }
    }

    /// The answers AppKit offers when a document with unsaved changes
    /// closes. For a new document the sheet calls Don't Save Delete; for a
    /// saved one its title has a typographic apostrophe. The sheet's
    /// identifiers are the same either way.
    private enum Answer {
        case save, dontSave, cancel

        var identifier: String {
            switch self {
            case .save: "OKButton"
            case .dontSave: "DontSaveButton"
            case .cancel: "CancelButton"
            }
        }

        var titleStart: String {
            switch self {
            case .save: "Save"
            case .dontSave: "Don"
            case .cancel: "Cancel"
            }
        }
    }

    /// Gives `answer` when AppKit asks whether to keep a document's changes.
    /// If nothing asks, the failure lists what the app shows.
    @MainActor
    private func answer(_ answer: Answer, file: StaticString = #filePath, line: UInt = #line) {
        let button = app.buttons.matching(
            NSPredicate(
                format: "identifier == %@ OR label BEGINSWITH %@ OR title BEGINSWITH %@",
                answer.identifier, answer.titleStart, answer.titleStart)
        ).firstMatch
        guard button.waitForExistence(timeout: 5) else {
            XCTFail("nothing asked to save:\n\(app.debugDescription)", file: file, line: line)
            return
        }
        button.click()
    }

    /// Checks how many documents the app's engine holds, as every window
    /// shows it to UI tests.
    @MainActor
    private func assertOpenDocuments(_ count: Int, file: StaticString = #filePath, line: UInt = #line) {
        let expected = "\(count) open"
        let shown = app.windows.element(boundBy: 0).staticTexts["open-documents"]
        XCTAssertTrue(
            waitUntil { (shown.label.isEmpty ? shown.value as? String : shown.label) == expected },
            "open documents: \(shown.label) \(shown.value ?? ""), not \(expected)", file: file, line: line)
    }

    /// Polls `condition` for up to five seconds.
    @MainActor
    private func waitUntil(_ condition: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(5)
        repeat {
            if condition() { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        } while Date() < deadline
        return false
    }

    /// Opens the Edit menu, which makes AppKit validate its items, checks
    /// whether Undo and Redo are enabled, and closes it again. Escape would
    /// reach the text view and open its completion list.
    @MainActor
    private func assertEditMenu(undo: Bool, redo: Bool, file: StaticString = #filePath, line: UInt = #line) {
        let edit = app.menuBars.menuBarItems["Edit"]
        edit.click()
        let undoItem = app.menuBars.menuItems["Undo"]
        XCTAssertTrue(undoItem.waitForExistence(timeout: 5), file: file, line: line)
        XCTAssertEqual(undoItem.isEnabled, undo, "Undo", file: file, line: line)
        XCTAssertEqual(app.menuBars.menuItems["Redo"].isEnabled, redo, "Redo", file: file, line: line)
        edit.click()
    }

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
        _ expected: String, _ editor: XCUIElement, in window: XCUIElement? = nil,
        file: StaticString = #filePath, line: UInt = #line
    ) {
        let bytes = Array(expected.utf8)
        XCTAssertEqual(
            Array((editor.value as? String ?? "").utf8), bytes, "text view", file: file, line: line)

        // SwiftUI leaves an empty text out of the accessibility tree, so a
        // missing element means the engine's text is empty.
        let engineText = (window ?? app).descendants(matching: .any)["engine-text"]
        var shown = ""
        let deadline = Date().addingTimeInterval(5)
        repeat {
            if !engineText.exists {
                shown = ""
            } else {
                // SwiftUI may expose a text's string as its label or as its value.
                shown = engineText.label.isEmpty ? (engineText.value as? String ?? "") : engineText.label
            }
            if Array(shown.utf8) == bytes { return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        } while Date() < deadline
        XCTFail(
            "the engine's text is \(shown.debugDescription), not \(expected.debugDescription)",
            file: file, line: line)
    }
}
