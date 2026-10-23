import Foundation
import Observation

/// What the engine reports about a document after a command.
struct DocumentState: Equatable {
    let id: UInt64
    let text: String
    let canUndo: Bool
    let canRedo: Bool
    /// The UTF-8 byte range of `text` that differs from the text before the
    /// command, or `nil` if the text did not change, as ADR 0005 describes.
    let changed: Range<Int>?
    /// Whether the document has changes since it was last marked as saved,
    /// as ADR 0006 describes.
    let modified: Bool
}

/// A command the engine rejected, with the engine's own explanation.
struct EngineError: LocalizedError {
    let message: String

    var errorDescription: String? { message }
}

/// Owns one Rust engine and sends it commands through the C interface
/// declared in `crates/bite-ffi/include/bite_ffi.h`. The app has one, and
/// each window's document lives in it.
@Observable
final class EngineBridge {
    /// The app's engine.
    static let shared = EngineBridge()

    @ObservationIgnored private let engine = bite_engine_new()

    /// The documents the engine's replies have shown open and not yet
    /// closed. The app shows how many to UI tests.
    private(set) var openDocuments: Set<UInt64> = []

    deinit {
        bite_engine_free(engine)
    }

    /// Runs the engine command `command` and returns the document it affected.
    func dispatch(
        _ command: String,
        _ params: KeyValuePairs<String, String> = [:]
    ) throws -> DocumentState {
        try dispatch(command, params: params.map { ($0.key, Array($0.value.utf8)) })
    }

    /// Runs `command` with parameter values given as bytes, such as a
    /// file's, which reach the engine unchanged. The engine refuses bytes
    /// that are not UTF-8, giving the offset of the first invalid one.
    func dispatch(_ command: String, _ params: KeyValuePairs<String, Data>) throws -> DocumentState {
        try dispatch(command, params: params.map { ($0.key, Array($0.value)) })
    }

    private func dispatch(_ command: String, params: [(String, [UInt8])]) throws -> DocumentState {
        // Rust borrows every string for the length of the call, so lay them
        // out in one buffer and point into it.
        var bytes: [UInt8] = []
        var ranges: [Range<Int>] = []
        for string in [Array(command.utf8)] + params.flatMap({ [Array($0.0.utf8), $0.1] }) {
            let start = bytes.count
            bytes.append(contentsOf: string)
            ranges.append(start..<bytes.count)
        }

        let reply = bytes.withUnsafeBufferPointer { buffer in
            func borrowed(_ index: Int) -> BiteStr {
                let range = ranges[index]
                return BiteStr(ptr: buffer.baseAddress.map { $0 + range.lowerBound }, len: range.count)
            }
            let borrowedParams = params.indices.map {
                BiteParam(name: borrowed(2 * $0 + 1), value: borrowed(2 * $0 + 2))
            }
            return bite_engine_dispatch(engine, borrowed(0), borrowedParams, borrowedParams.count)
        }
        defer { bite_reply_free(reply) }

        let text = String(
            decoding: UnsafeBufferPointer(start: reply.text, count: reply.text_len),
            as: UTF8.self
        )
        guard reply.ok else {
            throw EngineError(message: text)
        }
        if command == "document.close" {
            openDocuments.remove(reply.document)
        } else if !openDocuments.contains(reply.document) {
            openDocuments.insert(reply.document)
        }
        return DocumentState(
            id: reply.document,
            text: text,
            canUndo: reply.can_undo,
            canRedo: reply.can_redo,
            changed: reply.changed && reply.changed_start <= reply.changed_end
                ? reply.changed_start..<reply.changed_end : nil,
            modified: reply.modified
        )
    }
}
