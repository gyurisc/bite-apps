import AppKit
import XCTest

@testable import TextBite

/// A composition is not in the engine until it ends, so closing a window and
/// quitting end every composition before AppKit decides whether to ask
/// about unsaved changes, as ADR 0006 describes.
final class CloseAndQuitTests: XCTestCase {
    private let notFound = NSRange(location: NSNotFound, length: 0)

    /// Records AppKit's answer to `canClose`.
    private final class CloseDelegate: NSObject {
        var shouldClose: Bool?

        @objc func document(_ document: NSDocument, shouldClose: Bool, contextInfo: UnsafeMutableRawPointer?) {
            self.shouldClose = shouldClose
        }
    }

    /// A document with a window, whose editor holds a composition the engine
    /// has not seen, so AppKit sees no unsaved changes.
    @MainActor
    private func composingDocument() throws -> (TextDocument, NSWindow, EditorTextView) {
        let document = TextDocument(engine: EngineBridge())
        document.model.open()
        document.makeWindowControllers()
        let window = try XCTUnwrap(document.windowControllers.first?.window)
        window.layoutIfNeeded()
        let view = try XCTUnwrap(Self.editor(in: window.contentView))
        view.setMarkedText("k", selectedRange: NSRange(location: 1, length: 0), replacementRange: notFound)
        XCTAssertTrue(view.isComposing)
        XCTAssertFalse(document.isDocumentEdited)
        addTeardownBlock { @MainActor in
            if let sheet = window.attachedSheet { window.endSheet(sheet) }
            document.close()
        }
        return (document, window, view)
    }

    @MainActor
    func testClosingAWindowWithOnlyACompositionAsksToSave() throws {
        let (document, window, view) = try composingDocument()
        let delegate = CloseDelegate()

        document.canClose(
            withDelegate: delegate, shouldClose: #selector(CloseDelegate.document(_:shouldClose:contextInfo:)),
            contextInfo: nil)

        XCTAssertFalse(view.isComposing)
        XCTAssertEqual(document.model.document?.text, "k")
        XCTAssertTrue(document.isDocumentEdited)
        XCTAssertNil(delegate.shouldClose, "AppKit closed the document without asking")
        XCTAssertNotNil(window.attachedSheet, "no sheet asked to save")
    }

    @MainActor
    func testTheAppsDocumentControllerEndsEveryCompositionBeforeReviewing() throws {
        let controller = try XCTUnwrap(NSDocumentController.shared as? DocumentController)
        let (document, _, view) = try composingDocument()
        controller.addDocument(document)
        addTeardownBlock { @MainActor in controller.removeDocument(document) }

        controller.finishEditing()

        XCTAssertFalse(view.isComposing)
        XCTAssertTrue(document.isDocumentEdited)
    }

    private static func editor(in view: NSView?) -> EditorTextView? {
        guard let view else { return nil }
        if let editor = view as? EditorTextView { return editor }
        return view.subviews.lazy.compactMap { editor(in: $0) }.first
    }
}
