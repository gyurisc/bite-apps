import Foundation
import Observation

/// An engine command that moves through the document's undo history.
enum HistoryCommand {
    case undo, redo
}

/// An edit that makes a text view show the engine's text after a command:
/// replace `replaced`, a UTF-16 range of the old text, with `text`, after
/// which `changed` is the UTF-16 range of the new text that differs.
struct ViewChange: Equatable {
    let replaced: NSRange
    let text: String
    let changed: NSRange
}

/// The app's view of one engine document, owned by one `TextDocument`. Every change
/// goes through an engine command; this type only holds what the engine
/// last reported.
@Observable
final class DocumentModel {
    @ObservationIgnored private let engine: EngineBridge
    private(set) var document: DocumentState?
    private(set) var errorMessage: String?
    /// True once the document is closed; a closed model stays closed.
    private var closed = false
    /// Ends editing that the engine has not seen yet, such as a composition,
    /// before a command other than an edit. The editor sets it.
    @ObservationIgnored var finishEditing: () -> Void = {}
    /// Called with every state the engine reports. The document sets it to
    /// follow whether there are unsaved changes.
    @ObservationIgnored var stateDidChange: (DocumentState) -> Void = { _ in }

    /// A model for a document in `engine`. It creates the document when
    /// opened, so making a model has no effect on the engine.
    init(engine: EngineBridge) {
        self.engine = engine
    }

    // In this target a plain Swift class without an explicit deinit gets a
    // main-actor one, and releasing such an object under XCTest on macOS 15
    // crashes in swift_task_deinitOnExecutor. An explicit deinit, like
    // EngineBridge's, is not isolated. Nothing here needs the main actor.
    nonisolated deinit {}

    /// Creates the document, once.
    func open() {
        guard document == nil, !closed else { return }
        run("document.create")
    }

    /// Makes the document hold a file's bytes with `document.load`, which
    /// gets them unchanged. An open document is replaced, as Revert does:
    /// editing in progress ends, the new engine document takes its place,
    /// and the old one is closed, as ADR 0006 describes. If the engine
    /// refuses the bytes, the document stays as it was.
    func load(_ bytes: Data) throws {
        guard !closed else { return }
        finishEditing()
        let old = document
        let state = try engine.dispatch("document.load", ["text": bytes])
        document = state
        errorMessage = nil
        stateDidChange(state)
        if let old {
            // The engine refuses only an unknown document, so this is not
            // expected, and the new document is in place either way.
            _ = try? engine.dispatch("document.close", ["document": String(old.id)])
        }
    }

    /// Ends editing in progress, then closes the document with
    /// `document.close`, as when its `TextDocument` closes.
    func close() {
        guard document != nil else { return }
        finishEditing()
        guard let document else { return }
        do {
            _ = try engine.dispatch("document.close", ["document": String(document.id)])
            self.document = nil
            closed = true
            errorMessage = nil
        } catch {
            // The engine refuses only an unknown document, so this is not
            // expected; the window is closing, so nobody sees the message.
            errorMessage = error.localizedDescription
        }
    }

    /// Marks the engine's current state as saved, after the document's file
    /// was written.
    @discardableResult
    func markSaved() -> Bool {
        guard let document else { return false }
        return run("document.mark_saved", ["document": String(document.id)])
    }

    /// Asks the engine to replace `range`, counted in UTF-16 code units of
    /// the engine's text as a text view counts, with `replacement`. Returns
    /// whether the engine applied it, so the view applies the same edit.
    func replace(_ range: NSRange, with replacement: String) -> Bool {
        guard let document else { return false }
        guard let (start, end) = Self.byteOffsets(of: range, in: document.text) else {
            errorMessage = "The edit at \(range) does not fall between characters."
            return false
        }
        return run(
            "document.replace_range",
            [
                "document": String(document.id),
                "start": String(start),
                "end": String(end),
                "text": replacement,
            ]
        )
    }

    /// Asks the engine to replace the whole text, for a change that touches
    /// several ranges at once. Returns whether the engine applied it.
    func replaceText(with text: String) -> Bool {
        guard let document else { return false }
        return run("document.replace_text", ["document": String(document.id), "text": text])
    }

    /// Converts a UTF-16 range in `text` to UTF-8 byte offsets, as ADR 0004
    /// describes. Returns `nil` for a range outside the text or with an end
    /// inside a surrogate pair.
    static func byteOffsets(of range: NSRange, in text: String) -> (Int, Int)? {
        let utf16 = text.utf16
        guard range.location != NSNotFound, range.location >= 0, range.length >= 0,
            range.location <= utf16.count, range.length <= utf16.count - range.location
        else { return nil }
        let start16 = utf16.index(utf16.startIndex, offsetBy: range.location)
        let end16 = utf16.index(start16, offsetBy: range.length)
        guard let start = start16.samePosition(in: text.utf8),
            let end = end16.samePosition(in: text.utf8)
        else { return nil }
        let utf8 = text.utf8
        return (utf8.distance(from: utf8.startIndex, to: start), utf8.distance(from: utf8.startIndex, to: end))
    }

    /// Asks the engine to undo or redo a step. Returns the edit that makes a
    /// view showing the engine's text before the command show its text
    /// after, or `nil` if the engine refused or the text did not change.
    func runHistory(_ command: HistoryCommand) -> ViewChange? {
        guard let before = document else { return nil }
        let name = command == .undo ? "document.undo" : "document.redo"
        guard run(name, ["document": String(before.id)]), let after = document, let changed = after.changed
        else { return nil }
        return Self.viewChange(from: before.text, to: after.text, changed: changed)
    }

    /// Whether the engine reported that it can undo or redo a step.
    func canRunHistory(_ command: HistoryCommand) -> Bool {
        command == .undo ? document?.canUndo == true : document?.canRedo == true
    }

    /// Turns `changed`, the UTF-8 byte range of `new` that differs from
    /// `old`, into an edit of a view showing `old`, as ADR 0005 describes.
    /// The bytes after the range are the same in both texts, so the replaced
    /// span of `old` ends that many bytes before its end. Returns `nil` if
    /// the range does not fit both texts or splits a character.
    static func viewChange(from old: String, to new: String, changed: Range<Int>) -> ViewChange? {
        let oldEnd = old.utf8.count - (new.utf8.count - changed.upperBound)
        guard changed.upperBound <= new.utf8.count, oldEnd >= changed.lowerBound,
            let replacedStart = utf16Offset(ofByte: changed.lowerBound, in: old),
            let replacedEnd = utf16Offset(ofByte: oldEnd, in: old),
            let changedStart = utf16Offset(ofByte: changed.lowerBound, in: new),
            let changedEnd = utf16Offset(ofByte: changed.upperBound, in: new)
        else { return nil }
        let changedRange = NSRange(location: changedStart, length: changedEnd - changedStart)
        return ViewChange(
            replaced: NSRange(location: replacedStart, length: replacedEnd - replacedStart),
            text: (new as NSString).substring(with: changedRange),
            changed: changedRange)
    }

    /// The UTF-16 offset of the UTF-8 byte offset `offset` in `text`, or `nil`
    /// if it is outside the text or not between two Unicode scalar values.
    private static func utf16Offset(ofByte offset: Int, in text: String) -> Int? {
        let utf8 = text.utf8
        guard offset >= 0, offset <= utf8.count,
            let index = utf8.index(utf8.startIndex, offsetBy: offset).samePosition(in: text.utf16)
        else { return nil }
        return text.utf16.distance(from: text.utf16.startIndex, to: index)
    }

    @discardableResult
    private func run(_ command: String, _ params: KeyValuePairs<String, String> = [:]) -> Bool {
        do {
            let state = try engine.dispatch(command, params)
            document = state
            errorMessage = nil
            stateDidChange(state)
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }
}
