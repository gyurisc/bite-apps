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

    func replaceText(with text: String) {
        guard let document else { return }
        run("document.replace_text", ["document": String(document.id), "text": text])
    }

    private func run(_ command: String, _ params: KeyValuePairs<String, String> = [:]) {
        do {
            document = try engine.dispatch(command, params)
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
