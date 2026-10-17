import XCTest

/// Drives the app the way a person does, so UI changes can be checked
/// without a manual run. Each test launches a fresh app and engine.
final class TextBiteUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testReplacingTheTextShowsWhatTheEngineReturns() throws {
        let app = XCUIApplication()
        app.launch()

        let draft = app.textFields["draft"]
        XCTAssertTrue(draft.waitForExistence(timeout: 10))
        draft.click()
        draft.typeText("hello from a UI test")
        app.buttons["Replace Text"].click()

        XCTAssertTrue(text(in: app, "document-text", is: "hello from a UI test").waitForExistence(timeout: 5))
        XCTAssertTrue(text(in: app, "undo-status", is: "The engine can undo this.").waitForExistence(timeout: 5))
    }

    /// The element with `identifier` showing `string`. SwiftUI may expose a
    /// text's string as its label or as its value, so either matches.
    @MainActor
    private func text(in app: XCUIApplication, _ identifier: String, is string: String) -> XCUIElement {
        let shown = NSPredicate(
            format: "identifier == %@ AND (label == %@ OR value == %@)",
            identifier, string, string)
        return app.descendants(matching: .any).matching(shown).firstMatch
    }
}
