import Foundation
import Observation

/// The app's view of one engine document. Every change goes through an engine
/// command; this type only holds what the engine last reported.
@Observable
final class DocumentModel {
    private let engine = EngineBridge()
    private(set) var document: DocumentState?
    private(set) var errorMessage: String?

    init() {
        run("document.create")
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

    @discardableResult
    private func run(_ command: String, _ params: KeyValuePairs<String, String> = [:]) -> Bool {
        do {
            document = try engine.dispatch(command, params)
            errorMessage = nil
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }
}
