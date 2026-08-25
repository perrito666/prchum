import AppKit
import PrchumKit
import UniformTypeIdentifiers

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var windows: [ReviewWindowController] = []
    /// Files handed to us (Finder, `open`) before the app finished launching.
    private var pendingPaths: [String] = []
    private var launched = false
    private let config = CoreConfig()
    private var keymap = Keymap(overrides: [:])

    func applicationDidFinishLaunching(_ notification: Notification) {
        keymap = Keymap(overrides: config.keyOverrides)
        if let warning = config.loadWarning {
            NSLog("config: %@ — defaults are in effect", warning)
        }
        for problem in keymap.problems {
            NSLog("config: %@ — the default binding stays", problem)
        }
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
            adopt(session: session)
        } catch {
            presentOpenFailure("Could not open \((path as NSString).lastPathComponent)", error)
        }
    }

    private func adopt(session: CoreSession) {
        let controller = ReviewWindowController(session: session)
        controller.onClose = { [weak self] closed in
            self?.windows.removeAll { $0 === closed }
        }
        windows.append(controller)
        controller.showWindow(nil)
    }

    private func presentOpenFailure(_ title: String, _ error: Error) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = "\(error)"
        alert.alertStyle = .warning
        alert.runModal()
        if windows.isEmpty { NSApp.terminate(nil) }
    }

    /// Open a PR by URL, `owner/repo#N`, or bare number (inferred from the
    /// current directory's origin). The fetch runs off the main thread; the
    /// session is handed over once built.
    @objc func openPullRequest(_ sender: Any?) {
        let alert = NSAlert()
        alert.messageText = "Open Pull Request"
        alert.informativeText =
            "A URL, owner/repo#N, or a bare number (repository inferred from the current directory)."
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 380, height: 24))
        field.placeholderString = "https://github.com/owner/repo/pull/418"
        alert.accessoryView = field
        alert.window.initialFirstResponder = field
        alert.addButton(withTitle: "Open")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let reference = field.stringValue.trimmingCharacters(in: .whitespaces)
        guard !reference.isEmpty else { return }

        let hint = FileManager.default.currentDirectoryPath
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                // Built off-main, then confined to the main thread forever.
                let session = try CoreSession(pullRequest: reference, repoHint: hint)
                DispatchQueue.main.async { self.adopt(session: session) }
            } catch {
                DispatchQueue.main.async {
                    let failure = NSAlert()
                    failure.messageText = "Could not open \(reference)"
                    failure.informativeText = "\(error)"
                    failure.runModal()
                }
            }
        }
    }

    /// Review a local repository: pick the folder, then the comparison.
    @objc func openGitComparison(_ sender: Any?) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.message = "Choose a git repository to review"
        guard panel.runModal() == .OK, let url = panel.url else { return }

        let alert = NSAlert()
        alert.messageText = "What to compare"
        let picker = NSPopUpButton(frame: NSRect(x: 0, y: 30, width: 380, height: 24))
        picker.addItems(withTitles: [
            "Working tree vs HEAD", "Staged (index) vs HEAD", "Against a base ref…",
        ])
        let baseField = NSTextField(frame: NSRect(x: 0, y: 0, width: 380, height: 24))
        baseField.placeholderString = "base ref (e.g. main) — for the third option"
        let stack = NSView(frame: NSRect(x: 0, y: 0, width: 380, height: 58))
        stack.addSubview(picker)
        stack.addSubview(baseField)
        alert.accessoryView = stack
        alert.addButton(withTitle: "Review")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        let comparison: GitComparison
        switch picker.indexOfSelectedItem {
        case 1: comparison = .staged
        case 2:
            let base = baseField.stringValue.trimmingCharacters(in: .whitespaces)
            comparison = .base(base.isEmpty ? "main" : base)
        default: comparison = .workingTree
        }
        do {
            let session = try CoreSession(gitRepo: url.path, comparison: comparison)
            adopt(session: session)
        } catch {
            presentOpenFailure("Could not review \(url.lastPathComponent)", error)
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
        fileMenu.addItem(keymap.menuItem(for: .open))
        fileMenu.addItem(keymap.menuItem(for: .openPullRequest))
        fileMenu.addItem(keymap.menuItem(for: .openGitComparison))
        fileMenu.addItem(.separator())
        fileMenu.addItem(keymap.menuItem(for: .exportNotes))
        fileMenu.addItem(.separator())
        fileMenu.addItem(
            withTitle: "Close",
            action: #selector(NSWindow.performClose(_:)),
            keyEquivalent: "w")

        // Edit exists so the standard selection/copy machinery works in the
        // diff view (mouse selection is supported, but secondary).
        let editItem = NSMenuItem()
        mainMenu.addItem(editItem)
        let editMenu = NSMenu(title: "Edit")
        editItem.submenu = editMenu
        editMenu.addItem(
            withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(
            withTitle: "Select All",
            action: #selector(NSText.selectAll(_:)),
            keyEquivalent: "a")
        editMenu.addItem(.separator())
        editMenu.addItem(keymap.menuItem(for: .find))

        let reviewItem = NSMenuItem()
        mainMenu.addItem(reviewItem)
        let reviewMenu = NSMenu(title: "Review")
        reviewItem.submenu = reviewMenu
        reviewMenu.addItem(keymap.menuItem(for: .comment))
        reviewMenu.addItem(keymap.menuItem(for: .reply))
        reviewMenu.addItem(.separator())
        reviewMenu.addItem(keymap.menuItem(for: .editComment))
        reviewMenu.addItem(keymap.menuItem(for: .deleteComment))
        reviewMenu.addItem(keymap.menuItem(for: .dismissComment))
        reviewMenu.addItem(.separator())
        reviewMenu.addItem(keymap.menuItem(for: .prInfo))
        reviewMenu.addItem(keymap.menuItem(for: .submit))

        let viewItem = NSMenuItem()
        mainMenu.addItem(viewItem)
        let viewMenu = NSMenu(title: "View")
        viewItem.submenu = viewMenu
        viewMenu.addItem(keymap.menuItem(for: .toggleSidebar))
        viewMenu.addItem(keymap.menuItem(for: .toggleWrap))

        let goItem = NSMenuItem()
        mainMenu.addItem(goItem)
        let goMenu = NSMenu(title: "Go")
        goItem.submenu = goMenu
        goMenu.addItem(keymap.menuItem(for: .nextChange))
        goMenu.addItem(keymap.menuItem(for: .previousChange))
        goMenu.addItem(.separator())
        goMenu.addItem(keymap.menuItem(for: .nextHunk))
        goMenu.addItem(keymap.menuItem(for: .previousHunk))
        goMenu.addItem(.separator())
        goMenu.addItem(keymap.menuItem(for: .nextFile))
        goMenu.addItem(keymap.menuItem(for: .previousFile))

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
