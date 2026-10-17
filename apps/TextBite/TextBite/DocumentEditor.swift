import AppKit
import SwiftUI

/// A plain-text `NSTextView` that shows the engine's text and lets an edit
/// through only after the engine has applied the same edit, as ADR 0004
/// describes. The text view's own undo is off: ⌘Z must not change the view
/// without the engine.
struct DocumentEditor: NSViewRepresentable {
    let model: DocumentModel

    func makeCoordinator() -> Coordinator {
        Coordinator(model: model)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSTextView.scrollableTextView()
        let textView = scrollView.documentView as! NSTextView
        textView.isRichText = false
        textView.importsGraphics = false
        textView.allowsUndo = false
        textView.font = .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        textView.textContainerInset = NSSize(width: 4, height: 6)
        textView.string = model.document?.text ?? ""
        textView.delegate = context.coordinator
        textView.setAccessibilityIdentifier("editor")
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.model = model
        guard let textView = scrollView.documentView as? NSTextView else { return }
        context.coordinator.show(model.document?.text ?? "", in: textView)
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var model: DocumentModel

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
            // A change to attributes only does not touch the text.
            guard let replacementStrings else { return true }
            let accepted = send(ranges.map(\.rangeValue), replacementStrings, from: textView.string)
            if accepted {
                applyingEdit = true
                // AppKit applies an accepted edit before control returns to
                // the run loop; this covers an edit that changes nothing.
                DispatchQueue.main.async { [weak self] in self?.applyingEdit = false }
            }
            return accepted
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
            guard !applyingEdit, !(textView.string as NSString).isEqual(to: text) else { return }
            let selection = textView.selectedRange()
            textView.string = text
            let length = (text as NSString).length
            let location = min(selection.location, length)
            textView.setSelectedRange(NSRange(location: location, length: min(selection.length, length - location)))
        }
    }
}
