import AppKit

// TextBite is an AppKit document app, as ADR 0006 describes: AppKit's
// document controller drives New, Save, Close, and Quit, and each window's
// content is a SwiftUI view. The app starts on the main thread.
MainActor.assumeIsolated {
    if ProcessInfo.processInfo.arguments.contains("--ui-testing") {
        forgetSavedWindows()
    }
    // NSApplication keeps only a weak reference to its delegate.
    let delegate = AppDelegate()
    NSApplication.shared.delegate = delegate
    withExtendedLifetime(delegate) {
        _ = NSApplicationMain(CommandLine.argc, CommandLine.unsafeArgv)
    }
}

/// Deletes the window state AppKit saved when the app last quit, so a UI
/// test starts with one new document. Otherwise AppKit reopens the
/// documents an earlier test left open; when their files are gone, it opens
/// no window at all.
func forgetSavedWindows() {
    guard let library = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask).first,
        let bundle = Bundle.main.bundleIdentifier
    else { return }
    let state = library.appendingPathComponent("Saved Application State/\(bundle).savedState")
    try? FileManager.default.removeItem(at: state)
}
