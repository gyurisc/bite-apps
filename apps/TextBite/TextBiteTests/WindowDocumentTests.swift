import AppKit
import XCTest

@testable import TextBite

/// Each document lives in the app's one engine, and closing it removes it
/// from the engine with `document.close`. `TextDocumentTests` covers closing
/// through AppKit.
final class WindowDocumentTests: XCTestCase {
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
}
