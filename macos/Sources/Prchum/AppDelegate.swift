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
    /// Sessions being fetched (PR opens run off-main). While one is in
    /// flight the app has no window, and closing the last dialog must not
    /// quit it out from under the fetch.
    private var pendingOpens = 0

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

        // A command-line target opens directly: `Prchum change.diff`, or a
        // PR reference (`418`, `owner/repo#418`, a URL). A file on disk
        // always wins over a PR interpretation of the same argument.
        let cliTargets = CommandLine.arguments.dropFirst().filter { !$0.hasPrefix("-") }
        // AppKit also hands CLI arguments to application(_:openFile:), so
        // the same target arrives twice; keep first occurrences only.
        var seen = Set<String>()
        let targets = (pendingPaths + cliTargets).filter { seen.insert($0).inserted }
        pendingPaths = []
        if targets.isEmpty {
            showHome()
        } else {
            for target in targets {
                if FileManager.default.fileExists(atPath: target) {
                    openReview(atPath: target)
                } else {
                    openPullRequest(reference: target)
                }
            }
        }
        NSApp.activate(ignoringOtherApps: true)
    }

    private var home: HomeWindowController?

    /// The landing screen: the review history plus the doors in. Also the
    /// place a finished review returns to.
    @objc func showHome(_ sender: Any? = nil) {
        if home == nil {
            let controller = HomeWindowController()
            controller.onOpenEntry = { [weak self] entry in
                self?.reopen(entry)
            }
            controller.onClose = { [weak self] in self?.home = nil }
            home = controller
        }
        home?.refresh()
        home?.showWindow(nil)
        home?.window?.makeKeyAndOrderFront(nil)
    }

    /// Reopens a history entry the way it was first opened.
    private func reopen(_ entry: HistoryEntry) {
        switch entry.kind {
        case "pr":
            openPullRequest(reference: entry.reopen)
        case "git":
            let parts = entry.reopen.split(
                separator: "\u{1F}", omittingEmptySubsequences: false
            ).map(String.init)
            guard parts.count == 4 else {
                presentReopenFailure(entry)
                return
            }
            let comparison: GitComparison
            switch parts[1] {
            case "staged": comparison = .staged
            case "base": comparison = .base(parts[2])
            case "range": comparison = .range(parts[2], parts[3])
            default: comparison = .workingTree
            }
            do {
                adopt(session: try CoreSession(gitRepo: parts[0], comparison: comparison))
            } catch {
                presentOpenFailure("Could not reopen \(entry.title)", error)
            }
        default:
            guard FileManager.default.fileExists(atPath: entry.reopen) else {
                presentReopenFailure(entry)
                return
            }
            openReview(atPath: entry.reopen)
        }
    }

    private func presentReopenFailure(_ entry: HistoryEntry) {
        let alert = NSAlert()
        alert.messageText = "Could not reopen \(entry.title)"
        alert.informativeText = "The source is gone. Remove the entry with ⌫."
        alert.runModal()
    }

    private var queueController: ReviewQueueWindowController?

    /// Fetches the review queue off-main and shows it; Return or a
    /// double-click reviews the selected request.
    @objc func showReviewQueue(_ sender: Any?) {
        pendingOpens += 1
        let progress = makeProgressWindow(text: "Fetching your review queue…")
        progress.makeKeyAndOrderFront(nil)
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let requests = try CoreDiscovery.listRequests()
                DispatchQueue.main.async {
                    self.pendingOpens -= 1
                    progress.orderOut(nil)
                    guard !requests.isEmpty else {
                        let empty = NSAlert()
                        empty.messageText = "Nothing waiting on you"
                        empty.informativeText =
                            "The queue filter found no open requests. list_filter in config.json adjusts it."
                        empty.runModal()
                        self.returnToHomeIfEmpty()
                        return
                    }
                    let controller = ReviewQueueWindowController(requests: requests) {
                        request in
                        self.openPullRequest(reference: request.url)
                    }
                    controller.onClose = { [weak self] in
                        self?.queueController = nil
                        // Closing the queue without picking is a cancel.
                        DispatchQueue.main.async { self?.returnToHomeIfEmpty() }
                    }
                    self.queueController = controller
                    controller.showWindow(nil)
                }
            } catch {
                DispatchQueue.main.async {
                    self.pendingOpens -= 1
                    progress.orderOut(nil)
                    let failure = NSAlert()
                    failure.messageText = "Could not fetch the review queue"
                    failure.informativeText = "\(error)"
                    failure.runModal()
                    self.returnToHomeIfEmpty()
                }
            }
        }
    }

    /// Back home when a dialog was cancelled or a window closed and there
    /// is nothing else on screen — and nothing on its way to the screen.
    private func returnToHomeIfEmpty() {
        if windows.isEmpty && pendingOpens == 0 && queueController == nil {
            showHome()
        }
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
        pendingOpens == 0
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
            returnToHomeIfEmpty()
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
        session.recordHistory()
        let controller = ReviewWindowController(session: session)
        controller.onClose = { [weak self] closed in
            self?.windows.removeAll { $0 === closed }
            // A closed review lands back on the home screen — synchronously,
            // so the quit-on-last-window check already sees home open.
            self?.returnToHomeIfEmpty()
        }
        windows.append(controller)
        home?.close()
        controller.showWindow(nil)
    }

    private func presentOpenFailure(_ title: String, _ error: Error) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = "\(error)"
        alert.alertStyle = .warning
        alert.runModal()
        returnToHomeIfEmpty()
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
        // A PR reference already on the clipboard is almost certainly what
        // the user came here to paste; save them the keystroke.
        if let pasted = NSPasteboard.general.string(forType: .string)?
            .trimmingCharacters(in: .whitespacesAndNewlines),
            Self.looksLikePullRequestReference(pasted)
        {
            field.stringValue = pasted
        }
        alert.accessoryView = field
        alert.window.initialFirstResponder = field
        alert.addButton(withTitle: "Open")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else {
            returnToHomeIfEmpty()
            return
        }
        let reference = field.stringValue.trimmingCharacters(in: .whitespaces)
        guard !reference.isEmpty else {
            returnToHomeIfEmpty()
            return
        }
        openPullRequest(reference: reference)
    }

    /// Does this text look like a pull/merge-request reference worth
    /// prefilling? URLs with a request path, or explicit owner/repo#N /
    /// group/repo!N — never bare numbers (too easy to false-positive on
    /// an unrelated clipboard).
    static func looksLikePullRequestReference(_ text: String) -> Bool {
        guard !text.isEmpty, text.count < 300, !text.contains("\n") else { return false }
        if text.hasPrefix("https://") || text.hasPrefix("http://") {
            return text.contains("/pull/") || text.contains("/pulls/")
                || text.contains("/-/merge_requests/")
        }
        for marker in ["#", "!"] {
            if let range = text.range(of: marker, options: .backwards),
                text[range.upperBound...].allSatisfy(\.isNumber),
                !text[range.upperBound...].isEmpty,
                text[..<range.lowerBound].contains("/"),
                !text.contains(" ")
            {
                return true
            }
        }
        return false
    }

    /// Fetches a PR reference off-main and opens it; a progress window
    /// covers the wait, and failures report and fall back to the chooser
    /// when nothing else is open.
    private func openPullRequest(reference: String) {
        pendingOpens += 1
        let progress = makeProgressWindow(text: "Opening \(reference)…")
        progress.makeKeyAndOrderFront(nil)
        let hint = FileManager.default.currentDirectoryPath
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                // Built off-main, then confined to the main thread forever.
                let session = try CoreSession(pullRequest: reference, repoHint: hint)
                DispatchQueue.main.async {
                    self.pendingOpens -= 1
                    progress.orderOut(nil)
                    self.adopt(session: session)
                }
            } catch {
                DispatchQueue.main.async {
                    self.pendingOpens -= 1
                    progress.orderOut(nil)
                    let failure = NSAlert()
                    failure.messageText = "Could not open \(reference)"
                    failure.informativeText = "\(error)"
                    failure.runModal()
                    self.returnToHomeIfEmpty()
                }
            }
        }
    }

    private func makeProgressWindow(text: String) -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 320, height: 64),
            styleMask: [.titled],
            backing: .buffered,
            defer: false)
        window.title = "Prchum"
        window.isReleasedWhenClosed = false
        window.center()
        let spinner = NSProgressIndicator(frame: NSRect(x: 20, y: 22, width: 20, height: 20))
        spinner.style = .spinning
        spinner.startAnimation(nil)
        let label = NSTextField(labelWithString: text)
        label.frame = NSRect(x: 52, y: 24, width: 250, height: 18)
        label.lineBreakMode = .byTruncatingMiddle
        window.contentView?.addSubview(spinner)
        window.contentView?.addSubview(label)
        return window
    }

    /// Review a local repository: pick the folder, then the comparison.
    @objc func openGitComparison(_ sender: Any?) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.message = "Choose a git repository to review"
        guard panel.runModal() == .OK, let url = panel.url else {
            returnToHomeIfEmpty()
            return
        }

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
        guard alert.runModal() == .alertFirstButtonReturn else {
            returnToHomeIfEmpty()
            return
        }

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
        fileMenu.addItem(keymap.menuItem(for: .reviewQueue))
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
            withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        editMenu.addItem(
            withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "Z")
        editMenu.addItem(.separator())
        editMenu.addItem(
            withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        editMenu.addItem(
            withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(
            withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
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
        reviewMenu.addItem(keymap.menuItem(for: .suggest))
        reviewMenu.addItem(keymap.menuItem(for: .reply))
        reviewMenu.addItem(.separator())
        reviewMenu.addItem(keymap.menuItem(for: .editComment))
        reviewMenu.addItem(keymap.menuItem(for: .deleteComment))
        reviewMenu.addItem(keymap.menuItem(for: .dismissComment))
        reviewMenu.addItem(.separator())
        reviewMenu.addItem(keymap.menuItem(for: .commentList))
        reviewMenu.addItem(keymap.menuItem(for: .prInfo))
        reviewMenu.addItem(keymap.menuItem(for: .submit))

        let viewItem = NSMenuItem()
        mainMenu.addItem(viewItem)
        let viewMenu = NSMenu(title: "View")
        viewItem.submenu = viewMenu
        viewMenu.addItem(keymap.menuItem(for: .toggleSidebar))
        viewMenu.addItem(keymap.menuItem(for: .toggleLayout))
        viewMenu.addItem(keymap.menuItem(for: .toggleContext))
        viewMenu.addItem(keymap.menuItem(for: .toggleWrap))
        viewMenu.addItem(keymap.menuItem(for: .toggleSyntax))
        viewMenu.addItem(.separator())
        viewMenu.addItem(keymap.menuItem(for: .toggleFold))
        viewMenu.addItem(keymap.menuItem(for: .expandAll))
        viewMenu.addItem(keymap.menuItem(for: .collapseAll))

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
