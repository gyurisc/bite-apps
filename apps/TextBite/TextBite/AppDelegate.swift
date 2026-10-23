import AppKit

/// Ends every document's composition when AppKit checks for unsaved
/// changes before quitting. Every way of quitting reads
/// `hasEditedDocuments` first: the Quit menu item, and the quit Apple Event
/// that the Dock, logging out, and shutting down send, which skips
/// `terminate(_:)`. A composition is not in the engine yet, so without this
/// a saved document with one would quit without asking and lose it, as
/// ADR 0006 describes. AppKit reads this only when quitting.
final class DocumentController: NSDocumentController {
    override var hasEditedDocuments: Bool {
        finishEditing()
        return super.hasEditedDocuments
    }

    /// Ends the composition in every document's window.
    func finishEditing() {
        for case let document as TextDocument in documents {
            document.model.finishEditing()
        }
    }
}

/// Sets up the app's menus. AppKit's document controller does the rest: it
/// opens an untitled document at launch and reviews unsaved documents when
/// the app quits.
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationWillFinishLaunching(_ notification: Notification) {
        // The first document controller made becomes the shared one.
        _ = DocumentController()
        NSApp.mainMenu = Self.mainMenu()
    }

    /// The menu bar. Each item sends its standard action along the responder
    /// chain, so AppKit enables it only where something handles it.
    static func mainMenu() -> NSMenu {
        let name = ProcessInfo.processInfo.processName
        let services = NSMenu(title: "Services")
        NSApp.servicesMenu = services
        let window = NSMenu(title: "Window")
        NSApp.windowsMenu = window

        let main = NSMenu()
        for menu in [
            menu(
                name,
                item("About \(name)", #selector(NSApplication.orderFrontStandardAboutPanel(_:))),
                .separator(),
                submenu(services),
                .separator(),
                item("Hide \(name)", #selector(NSApplication.hide(_:)), "h"),
                item("Hide Others", #selector(NSApplication.hideOtherApplications(_:)), "h", [.command, .option]),
                item("Show All", #selector(NSApplication.unhideAllApplications(_:))),
                .separator(),
                item("Quit \(name)", #selector(NSApplication.terminate(_:)), "q")),
            menu(
                "File",
                item("New", #selector(NSDocumentController.newDocument(_:)), "n"),
                item("Open…", #selector(NSDocumentController.openDocument(_:)), "o"),
                // AppKit fills in a submenu that holds Clear Menu.
                submenu(
                    menu(
                        "Open Recent",
                        item("Clear Menu", #selector(NSDocumentController.clearRecentDocuments(_:))))),
                .separator(),
                item("Close", #selector(NSWindow.performClose(_:)), "w"),
                item("Save…", #selector(NSDocument.save(_:)), "s"),
                item("Save As…", #selector(NSDocument.saveAs(_:)), "s", [.command, .shift]),
                item("Revert to Saved", #selector(NSDocument.revertToSaved(_:)))),
            menu(
                "Edit",
                item("Undo", Selector(("undo:")), "z"),
                item("Redo", Selector(("redo:")), "z", [.command, .shift]),
                .separator(),
                item("Cut", #selector(NSText.cut(_:)), "x"),
                item("Copy", #selector(NSText.copy(_:)), "c"),
                item("Paste", #selector(NSText.paste(_:)), "v"),
                item("Delete", #selector(NSText.delete(_:))),
                item("Select All", #selector(NSText.selectAll(_:)), "a")),
            fill(
                window,
                item("Minimize", #selector(NSWindow.performMiniaturize(_:)), "m"),
                item("Zoom", #selector(NSWindow.performZoom(_:))),
                .separator(),
                item("Bring All to Front", #selector(NSApplication.arrangeInFront(_:)))),
        ] {
            main.addItem(submenu(menu))
        }
        return main
    }

    private static func item(
        _ title: String, _ action: Selector, _ key: String = "",
        _ modifiers: NSEvent.ModifierFlags = .command
    ) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.keyEquivalentModifierMask = modifiers
        return item
    }

    private static func menu(_ title: String, _ items: NSMenuItem...) -> NSMenu {
        fill(NSMenu(title: title), items)
    }

    private static func fill(_ menu: NSMenu, _ items: NSMenuItem...) -> NSMenu {
        fill(menu, items)
    }

    private static func fill(_ menu: NSMenu, _ items: [NSMenuItem]) -> NSMenu {
        items.forEach(menu.addItem)
        return menu
    }

    private static func submenu(_ menu: NSMenu) -> NSMenuItem {
        let item = NSMenuItem(title: menu.title, action: nil, keyEquivalent: "")
        item.submenu = menu
        return item
    }
}
