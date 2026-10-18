import AppKit
import SwiftUI

/// A plain-text document, as ADR 0006 describes. Its text and history live
/// in the app's engine; this class moves the text's bytes to and from files
/// and keeps AppKit's unsaved-changes state in step with the engine's.
@objc(TextDocument)
final class TextDocument: NSDocument {
    let model: DocumentModel
    private let engine: EngineBridge

    init(engine: EngineBridge) {
        self.engine = engine
        model = DocumentModel(engine: engine)
        super.init()
        // The engine owns undo, as ADR 0004 describes, and says whether the
        // document has unsaved changes.
        hasUndoManager = false
        model.stateDidChange = { [weak self] state in
            self?.updateChangeCount(state.modified ? .changeDone : .changeCleared)
        }
    }

    /// AppKit makes documents with this initializer; they use the app's
    /// engine.
    convenience override init() {
        self.init(engine: .shared)
    }

    /// Files are written only on Save and Save As.
    nonisolated override class var autosavesInPlace: Bool { false }

    override func makeWindowControllers() {
        addWindowController(DocumentWindowController(document: self, engine: engine))
    }

    /// Ends any composition, so it is saved, then saves. Only a Save or Save
    /// As that wrote the document's own file marks the engine's state as
    /// saved; Save To writes a copy.
    override func save(
        to url: URL, ofType typeName: String, for saveOperation: NSDocument.SaveOperationType,
        completionHandler: @escaping (Error?) -> Void
    ) {
        model.finishEditing()
        super.save(to: url, ofType: typeName, for: saveOperation) { [weak self] error in
            if error == nil, saveOperation == .saveOperation || saveOperation == .saveAsOperation {
                self?.model.markSaved()
            }
            completionHandler(error)
        }
    }

    /// The bytes of the engine's text, as ADR 0003 describes.
    override func data(ofType typeName: String) throws -> Data {
        guard let document = model.document else { throw CocoaError(.fileWriteUnknown) }
        return Data(document.text.utf8)
    }

    /// Opens the file, or reads it again for Revert, passing its bytes to the
    /// engine unchanged. Bytes that are not UTF-8 are refused with the
    /// offset of the first invalid one, and nothing changes.
    nonisolated override func read(from url: URL, ofType typeName: String) throws {
        let bytes = try Data(contentsOf: url)
        try MainActor.assumeIsolated {
            do {
                try model.load(bytes)
            } catch let error as EngineError {
                throw CocoaError(
                    .fileReadCorruptFile,
                    userInfo: [
                        NSLocalizedDescriptionKey: "TextBite can’t read “\(url.lastPathComponent)” as plain text.",
                        NSLocalizedRecoverySuggestionErrorKey: error.message,
                        NSURLErrorKey: url,
                    ])
            }
        }
    }

    /// Closes the document in the engine too, whether its last window
    /// closed or the app closed it.
    override func close() {
        model.close()
        super.close()
    }
}

/// The window for one document. Its content is the SwiftUI `ContentView`.
/// The document has no undo manager, so AppKit gives the window its own,
/// which the window's other fields use; it does not mark the document
/// edited.
final class DocumentWindowController: NSWindowController {
    /// Where the next window's top left goes, so new windows step down and
    /// right from one another.
    private static var cascadePoint = NSPoint.zero

    init(document: TextDocument, engine: EngineBridge) {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 600, height: 420),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered, defer: false)
        window.contentViewController = NSHostingController(
            rootView: ContentView(model: document.model, engine: engine))
        window.setContentSize(NSSize(width: 600, height: 420))
        window.center()
        Self.cascadePoint = window.cascadeTopLeft(from: Self.cascadePoint)
        // A UI test must not find windows that an earlier run left behind.
        window.isRestorable = !ProcessInfo.processInfo.arguments.contains("--ui-testing")
        super.init(window: window)
    }

    required init?(coder: NSCoder) {
        nil
    }
}
