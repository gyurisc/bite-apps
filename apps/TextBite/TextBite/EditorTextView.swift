import AppKit

/// A plain-text view that keeps input-method compositions out of the engine
/// until they end, as ADR 0005 describes.
///
/// While a composition is in progress, every change to the text grows a
/// region that covers all the text it has touched. Text before the region and
/// after it is the engine's text, unchanged. When no marked text remains, the
/// view asks `commit` to replace the region's text in the engine with the
/// region's text in the view, then `restore` makes sure the two agree.
/// Ranges given to `commit` count UTF-16 code units of the engine's text.
final class EditorTextView: NSTextView {
    /// Applies an edit to the engine; returns whether the engine applied it.
    var commit: (NSRange, String) -> Bool = { _, _ in true }
    /// Shows the engine's text if the view's text differs from it.
    var restore: () -> Void = {}
    /// Whether the engine can undo or redo now.
    var canRunHistory: (HistoryCommand) -> Bool = { _ in false }
    /// Undoes or redoes a step in the engine and shows the result.
    var runHistory: (HistoryCommand) -> Void = { _ in }

    /// The composition in progress: where its region starts, how many code
    /// units follow it, and how long the engine's text was when it began.
    private struct Composition {
        var start: Int
        var unchangedAfter: Int
        let engineLength: Int
    }

    private var composition: Composition?
    /// True while an input-method call runs, so the composition is ended only
    /// after the whole call, not in the middle of it.
    private var inInputMethodCall = false

    var isComposing: Bool { composition != nil }

    /// Whether the delegate should let a change through without sending it
    /// to the engine, because this view sends it when the composition ends.
    var letsChangesThrough: Bool { isComposing }

    override func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        if composition == nil, !Self.plain(string).isEmpty {
            let length = (self.string as NSString).length
            composition = Composition(start: length, unchangedAfter: length, engineLength: length)
        }
        inputMethodCall {
            super.setMarkedText(string, selectedRange: selectedRange, replacementRange: replacementRange)
        }
    }

    override func insertText(_ string: Any, replacementRange: NSRange) {
        inputMethodCall { super.insertText(string, replacementRange: replacementRange) }
    }

    override func unmarkText() {
        inputMethodCall { super.unmarkText() }
    }

    /// Every change to the text passes here before it is applied, so the
    /// region grows to cover it.
    override func shouldChangeText(inRanges affectedRanges: [NSValue], replacementStrings: [String]?) -> Bool {
        if var current = composition, replacementStrings != nil {
            let length = (string as NSString).length
            for range in affectedRanges.map(\.rangeValue) {
                current.start = min(current.start, range.location)
                current.unchangedAfter = min(current.unchangedAfter, length - NSMaxRange(range))
            }
            composition = current
        }
        return super.shouldChangeText(inRanges: affectedRanges, replacementStrings: replacementStrings)
    }

    /// Ends a composition that a change outside the input-method calls left
    /// without marked text, such as Backspace over the last marked character.
    override func didChangeText() {
        super.didChangeText()
        if !inInputMethodCall {
            endIfUnmarked()
        }
    }

    /// Commits marked text when the view loses focus, so a composition never
    /// outlives the editing session it belongs to.
    override func resignFirstResponder() -> Bool {
        endComposition()
        return super.resignFirstResponder()
    }

    /// Ends the composition in progress, committing its marked text, before
    /// the app sends the engine another command for this document.
    func endComposition() {
        if hasMarkedText() {
            unmarkText()
        }
        endIfUnmarked()
    }

    /// Undo and Redo while this view has focus: the engine owns the
    /// document's history, as ADR 0004 describes. They are not offered
    /// during a composition, whose text the engine has not seen yet.
    @objc func undo(_ sender: Any?) {
        if offers(.undo) { runHistory(.undo) }
    }

    @objc func redo(_ sender: Any?) {
        if offers(.redo) { runHistory(.redo) }
    }

    /// Menus validate through this too: `NSTextView` passes menu items on.
    override func validateUserInterfaceItem(_ item: any NSValidatedUserInterfaceItem) -> Bool {
        guard let command = Self.historyCommand(item.action) else { return super.validateUserInterfaceItem(item) }
        // Another field's undo may have named the item after its own step.
        (item as? NSMenuItem)?.title = command == .undo ? "Undo" : "Redo"
        return offers(command)
    }

    private func offers(_ command: HistoryCommand) -> Bool {
        !isComposing && canRunHistory(command)
    }

    private static func historyCommand(_ action: Selector?) -> HistoryCommand? {
        switch action {
        case #selector(undo(_:)): .undo
        case #selector(redo(_:)): .redo
        default: nil
        }
    }

    private func inputMethodCall(_ call: () -> Void) {
        let outer = !inInputMethodCall
        inInputMethodCall = true
        call()
        if outer {
            inInputMethodCall = false
            endIfUnmarked()
        }
    }

    /// Sends the region's text to the engine once no marked text remains.
    /// Whether the engine applies it or not, the view then shows the engine's
    /// text if the two differ.
    private func endIfUnmarked() {
        guard let ended = composition, !hasMarkedText() else { return }
        composition = nil
        let length = (string as NSString).length
        let viewRegion = NSRange(location: ended.start, length: length - ended.unchangedAfter - ended.start)
        let engineRegion = NSRange(
            location: ended.start, length: ended.engineLength - ended.unchangedAfter - ended.start)
        _ = commit(engineRegion, (string as NSString).substring(with: viewRegion))
        restore()
    }

    private static func plain(_ string: Any) -> String {
        (string as? NSAttributedString)?.string ?? string as? String ?? ""
    }
}
