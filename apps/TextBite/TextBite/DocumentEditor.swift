import AppKit
import SwiftUI

/// A plain-text `EditorTextView` that shows the engine's text and lets an
/// edit through only after the engine has applied the same edit, as ADR 0004
/// describes. Input-method compositions reach the engine when they end, as
/// ADR 0005 describes. The text view's own undo is off: Undo and Redo run
/// the engine's, so the view never changes without the engine.
struct DocumentEditor: NSViewRepresentable {
    let model: DocumentModel

    func makeCoordinator() -> Coordinator {
        Coordinator(model: model)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = EditorTextView.scrollableTextView()
        let textView = scrollView.documentView as! EditorTextView
        textView.isRichText = false
        textView.importsGraphics = false
        textView.allowsUndo = false
        textView.font = .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        textView.textContainerInset = NSSize(width: 4, height: 6)
        textView.setAccessibilityIdentifier("editor")
        Self.connect(textView, to: context.coordinator)
        return scrollView
    }

    /// Shows the model's text in `textView` and routes its edits and
    /// compositions through `coordinator` to the engine.
    static func connect(_ textView: EditorTextView, to coordinator: Coordinator) {
        textView.string = coordinator.model.document?.text ?? ""
        textView.delegate = coordinator
        textView.commit = { [weak coordinator] range, text in
            coordinator?.model.replace(range, with: text) ?? false
        }
        textView.restore = { [weak coordinator, weak textView] in
            guard let coordinator, let textView else { return }
            coordinator.show(coordinator.model.document?.text ?? "", in: textView)
        }
        textView.canRunHistory = { [weak coordinator] command in
            coordinator?.model.canRunHistory(command) ?? false
        }
        textView.runHistory = { [weak coordinator, weak textView] command in
            guard let coordinator, let textView else { return }
            coordinator.runHistory(command, in: textView)
        }
        coordinator.model.finishEditing = { [weak textView] in textView?.endComposition() }
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? NSTextView else { return }
        context.coordinator.show(context.coordinator.model.document?.text ?? "", in: textView)
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        /// The document's model. A window keeps its model, so an editor
        /// stays connected to the model it was made with, which closes with
        /// the document.
        let model: DocumentModel

        /// True from the engine accepting an edit until the view has applied
        /// it. The engine's text is ahead of the view's then, so it must not
        /// be shown.
        private var applyingEdit = false

        init(model: DocumentModel) {
            self.model = model
        }

        /// Sends the edit to the engine first, and lets the view apply it
        /// only if the engine did. One range is a range edit. A change to
        /// several ranges in one call, such as Replace All, is sent as the
        /// whole resulting text, so it is one undo step and all or nothing.
        /// Typing or deleting with several selections arrives as one call
        /// per range, so each range is its own edit.
        func textView(
            _ textView: NSTextView,
            shouldChangeTextInRanges ranges: [NSValue],
            replacementStrings: [String]?
        ) -> Bool {
            // A change to attributes only does not touch the text, and the
            // view sends compositions to the engine itself.
            guard let replacementStrings else { return true }
            if (textView as? EditorTextView)?.letsChangesThrough == true { return true }
            let ranges = ranges.map(\.rangeValue)
            let before = textView.string
            guard send(ranges, replacementStrings, from: before) else { return false }
            if ranges.count == 1, let after = model.document?.text,
                !Self.engineInserted(replacementStrings[0], at: ranges[0], before: before, after: after)
            {
                showConverted(ranges[0], before: before, after: after, in: textView)
                return false
            }
            applyingEdit = true
            // AppKit applies an accepted edit before control returns to the
            // run loop; this covers an edit that changes nothing.
            DispatchQueue.main.async { [weak self] in self?.applyingEdit = false }
            return true
        }

        /// Whether the engine inserted `replacement` as given, so the view can
        /// apply the same edit. It converts line breaks to the document's
        /// line ending, as ADR 0006 describes; text without one is unchanged.
        static func engineInserted(_ replacement: String, at range: NSRange, before: String, after: String) -> Bool {
            guard replacement.unicodeScalars.contains(where: { $0 == "\r" || $0 == "\n" }) else { return true }
            let length = (replacement as NSString).length
            let newText = after as NSString
            guard newText.length - (before as NSString).length == length - range.length else { return false }
            return newText.substring(with: NSRange(location: range.location, length: length)) == replacement
        }

        /// Shows the engine's text after it converted the line breaks of an
        /// edit of `range`, instead of the edit the view would have applied,
        /// and puts the insertion point after the inserted text. The change
        /// is made while AppKit asks whether it may apply its own, before it
        /// changes anything, and AppKit then applies nothing.
        private func showConverted(_ range: NSRange, before: String, after: String, in textView: NSTextView) {
            let inserted = (after as NSString).length - (before as NSString).length + range.length
            if let changed = model.document?.changed,
                let change = DocumentModel.viewChange(from: before, to: after, changed: changed)
            {
                apply(change, to: textView)
            } else {
                show(after, in: textView)
            }
            let caret = NSRange(location: range.location + inserted, length: 0)
            textView.setSelectedRange(caret)
            textView.scrollRangeToVisible(caret)
        }

        /// Replaces the changed span of the view's text with the engine's,
        /// in the view's typing attributes.
        private func apply(_ change: ViewChange, to textView: NSTextView) {
            textView.textStorage?.replaceCharacters(
                in: change.replaced,
                with: NSAttributedString(string: change.text, attributes: textView.typingAttributes))
        }

        private func send(_ ranges: [NSRange], _ replacementStrings: [String], from current: String) -> Bool {
            if ranges.count == 1 {
                return model.replace(ranges[0], with: replacementStrings[0])
            }
            let text = NSMutableString(string: current)
            for (range, replacement) in zip(ranges, replacementStrings).sorted(by: {
                $0.0.location > $1.0.location
            }) {
                text.replaceCharacters(in: range, with: replacement)
            }
            return model.replaceText(with: text as String)
        }

        /// Undoes or redoes a step in the engine, then replaces only the
        /// changed span of the view's text, selects it and scrolls to it, as
        /// ADR 0005 describes. If nothing changed, the selection stays.
        func runHistory(_ command: HistoryCommand, in textView: NSTextView) {
            // The change is worked out from the engine's text, so it fits the
            // view only if the view showed that text.
            let inStep = (textView.string as NSString).isEqual(to: model.document?.text ?? "")
            if let change = model.runHistory(command), inStep {
                apply(change, to: textView)
                textView.setSelectedRange(change.changed)
                textView.scrollRangeToVisible(change.changed)
            }
            // The engine's text wins if the edit did not make the two agree.
            show(model.document?.text ?? "", in: textView)
        }

        /// After the view applies an edit, its text must be the engine's.
        /// If they differ, the engine's text wins.
        func textDidChange(_ notification: Notification) {
            applyingEdit = false
            guard let textView = notification.object as? NSTextView else { return }
            show(model.document?.text ?? "", in: textView)
        }

        /// Replaces the view's text with `text` if they differ, keeping the
        /// selection where it still fits. The comparison is literal, code
        /// unit by code unit: text that only looks the same has different
        /// offsets.
        func show(_ text: String, in textView: NSTextView) {
            // During a composition the view is ahead of the engine on purpose.
            guard !applyingEdit, (textView as? EditorTextView)?.letsChangesThrough != true,
                !(textView.string as NSString).isEqual(to: text)
            else { return }
            let selection = textView.selectedRange()
            textView.string = text
            let length = (text as NSString).length
            let location = min(selection.location, length)
            textView.setSelectedRange(NSRange(location: location, length: min(selection.length, length - location)))
        }
    }
}
