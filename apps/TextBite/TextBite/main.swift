import AppKit

// TextBite is an AppKit document app, as ADR 0006 describes: AppKit's
// document controller drives New, Save, Close, and Quit, and each window's
// content is a SwiftUI view. The app starts on the main thread.
MainActor.assumeIsolated {
    // The first `shared` call makes the app, so it is an `Application`.
    let app = Application.shared
    // NSApplication keeps only a weak reference to its delegate.
    let delegate = AppDelegate()
    app.delegate = delegate
    withExtendedLifetime(delegate) {
        _ = NSApplicationMain(CommandLine.argc, CommandLine.unsafeArgv)
    }
}
