import AppKit
import PrchumKit
import UniformTypeIdentifiers

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var windows: [ReviewWindowController] = []
    /// Files handed to us (Finder, `open`) before the app finished launching.
    private var pendingPaths: [String] = []
    private var launched = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        buildMainMenu()
        launched = true

        // A path on the command line opens directly: `Prchum change.diff`.
        let cliPaths = CommandLine.arguments.dropFirst().filter { !$0.hasPrefix("-") }
        let paths = pendingPaths + cliPaths
        pendingPaths = []
        if paths.isEmpty {
            openDocument(nil)
        } else {
            for path in paths {
                openReview(atPath: path)
            }
        }
        NSApp.activate(ignoringOtherApps: true)
    }

    func application(_ sender: NSApplication, openFile filename: String) -> Bool {
        if launched {
            openReview(atPath: filename)
        } else {
            pendingPaths.append(filename)
        }
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    // MARK: - Actions

    @objc func openDocument(_ sender: Any?) {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.message = "Choose a patch to review"
        var types: [UTType] = [.plainText]
        if let diff = UTType(filenameExtension: "diff") { types.append(diff) }
        if let patch = UTType(filenameExtension: "patch") { types.append(patch) }
        panel.allowedContentTypes = types
        guard panel.runModal() == .OK, let url = panel.url else {
            // Launched empty and declined to open anything: nothing to show.
            if windows.isEmpty { NSApp.terminate(nil) }
            return
        }
        openReview(atPath: url.path)
    }

    private func openReview(atPath path: String) {
        do {
            let session = try CoreSession(contentsOf: path)
            let controller = ReviewWindowController(session: session)
            controller.onClose = { [weak self] closed in
                self?.windows.removeAll { $0 === closed }
            }
            windows.append(controller)
            controller.showWindow(nil)
        } catch {
            let alert = NSAlert()
            alert.messageText = "Could not open \((path as NSString).lastPathComponent)"
            alert.informativeText = "\(error)"
            alert.alertStyle = .warning
            alert.runModal()
            if windows.isEmpty { NSApp.terminate(nil) }
        }
    }

    // MARK: - Menu

    private func buildMainMenu() {
        let mainMenu = NSMenu()

        let appItem = NSMenuItem()
        mainMenu.addItem(appItem)
        let appMenu = NSMenu()
        appItem.submenu = appMenu
        appMenu.addItem(
            withTitle: "About Prchum",
            action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)),
            keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(
            withTitle: "Quit Prchum",
            action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q")

        let fileItem = NSMenuItem()
        mainMenu.addItem(fileItem)
        let fileMenu = NSMenu(title: "File")
        fileItem.submenu = fileMenu
        fileMenu.addItem(
            withTitle: "Open…",
            action: #selector(openDocument(_:)),
            keyEquivalent: "o")
        fileMenu.addItem(
            withTitle: "Close",
            action: #selector(NSWindow.performClose(_:)),
            keyEquivalent: "w")

        let windowItem = NSMenuItem()
        mainMenu.addItem(windowItem)
        let windowMenu = NSMenu(title: "Window")
        windowItem.submenu = windowMenu
        windowMenu.addItem(
            withTitle: "Minimize",
            action: #selector(NSWindow.performMiniaturize(_:)),
            keyEquivalent: "m")
        NSApp.windowsMenu = windowMenu

        NSApp.mainMenu = mainMenu
    }
}
