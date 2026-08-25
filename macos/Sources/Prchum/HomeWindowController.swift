import AppKit
import PrchumKit

/// The landing screen: what you have been reviewing, and the doors in.
///
/// The list holds every source opened before — pull requests, patches,
/// git comparisons, exchange documents. Return or a double-click reopens
/// one; ⌫ removes one by hand; and a periodic prune quietly drops pull
/// requests that have since merged, closed, or vanished (a network
/// failure never prunes — only a definite answer does).
@MainActor
final class HomeWindowController: NSWindowController, NSWindowDelegate,
    NSTableViewDataSource, NSTableViewDelegate
{
    private var entries: [HistoryEntry] = []
    private let table = NSTableView()
    private let emptyLabel = NSTextField(
        wrappingLabelWithString:
            "Nothing reviewed yet — open a pull request, your review queue, a patch, or a repository.")
    private var lastPrune = Date.distantPast
    private var pruneTimer: Timer?

    var onOpenEntry: ((HistoryEntry) -> Void)?
    var onClose: (() -> Void)?

    init() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 760, height: 440),
            styleMask: [.titled, .closable, .resizable, .miniaturizable],
            backing: .buffered,
            defer: false)
        window.title = "Prchum"
        super.init(window: window)
        window.delegate = self

        let buttons = NSStackView()
        buttons.orientation = .horizontal
        buttons.spacing = 8
        for (title, selector, isDefault) in [
            ("Open Pull Request…", #selector(AppDelegate.openPullRequest(_:)), true),
            ("Review Queue", #selector(AppDelegate.showReviewQueue(_:)), false),
            ("Patch File…", #selector(AppDelegate.openDocument(_:)), false),
            ("Git Repository…", #selector(AppDelegate.openGitComparison(_:)), false),
        ] {
            let button = NSButton(title: title, target: nil, action: selector)
            button.bezelStyle = .rounded
            if isDefault {
                button.keyEquivalent = "\r"
                button.keyEquivalentModifierMask = [.command]
            }
            buttons.addArrangedSubview(button)
        }

        for (identifier, columnTitle, width) in [
            ("title", "Reviewed", 300), ("display", "Where", 200),
            ("kind", "Kind", 70), ("opened", "Opened", 90), ("submitted", "Submitted", 90),
        ] {
            let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(identifier))
            column.title = columnTitle
            column.width = CGFloat(width)
            table.addTableColumn(column)
        }
        table.dataSource = self
        table.delegate = self
        table.doubleAction = #selector(openSelected(_:))
        table.target = self
        table.usesAlternatingRowBackgroundColors = true

        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.documentView = table

        emptyLabel.textColor = .secondaryLabelColor
        emptyLabel.alignment = .center

        let stack = NSStackView()
        stack.orientation = .vertical
        stack.spacing = 10
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 12, bottom: 12, right: 12)
        stack.addArrangedSubview(buttons)
        stack.addArrangedSubview(emptyLabel)
        stack.addArrangedSubview(scroll)
        window.contentView = stack
        window.setFrameAutosaveName("HomeWindow")
        if !window.setFrameUsingName("HomeWindow") {
            window.center()
        }
        reload()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    func windowWillClose(_ notification: Notification) {
        pruneTimer?.invalidate()
        pruneTimer = nil
        onClose?()
    }

    /// Refreshes from disk, then — every now and then — prunes finished
    /// pull requests off the main thread.
    func refresh() {
        reload()
        if Date().timeIntervalSince(lastPrune) > 600 {
            pruneNow()
        }
        if pruneTimer == nil {
            pruneTimer = Timer.scheduledTimer(withTimeInterval: 900, repeats: true) {
                [weak self] _ in
                Task { @MainActor in self?.pruneNow() }
            }
        }
    }

    private func pruneNow() {
        lastPrune = Date()
        DispatchQueue.global(qos: .utility).async {
            let kept = CoreHistory.prune()
            DispatchQueue.main.async { [weak self] in
                self?.entries = kept
                self?.applyEntries()
            }
        }
    }

    private func reload() {
        entries = CoreHistory.list()
        applyEntries()
    }

    private func applyEntries() {
        emptyLabel.isHidden = !entries.isEmpty
        table.reloadData()
    }

    @objc func openSelected(_ sender: Any?) {
        let row = table.selectedRow
        guard entries.indices.contains(row) else { return }
        onOpenEntry?(entries[row])
    }

    private func removeSelected() {
        let row = table.selectedRow
        guard entries.indices.contains(row) else { return }
        CoreHistory.remove(key: entries[row].key)
        reload()
        if !entries.isEmpty {
            table.selectRowIndexes([min(row, entries.count - 1)], byExtendingSelection: false)
        }
    }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 36:  // return
            openSelected(nil)
        case 51, 117:  // delete, forward delete
            removeSelected()
        default:
            super.keyDown(with: event)
        }
    }

    // MARK: table

    func numberOfRows(in tableView: NSTableView) -> Int {
        entries.count
    }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        let entry = entries[row]
        let text: String
        switch tableColumn?.identifier.rawValue {
        case "title": text = entry.title
        case "display": text = entry.display
        case "kind": text = entry.kind
        case "opened": text = String(entry.lastOpened.prefix(10))
        default:
            let submitted = entry.submittedAt ?? ""
            text = submitted.isEmpty ? "" : "✓ \(submitted.prefix(10))"
        }
        let label = NSTextField(labelWithString: text)
        label.lineBreakMode = .byTruncatingTail
        return label
    }
}
