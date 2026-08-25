import AppKit
import PrchumKit

/// The review queue: the open requests a filter finds, one row each.
/// The filter picker offers the default and every named filter from
/// config.json, plus a one-off custom filter typed on the spot. Return
/// or a double-click opens the selected request.
@MainActor
final class ReviewQueueWindowController: NSWindowController, NSWindowDelegate,
    NSTableViewDataSource, NSTableViewDelegate
{
    private var requests: [ListedRequest] = []
    private let onOpen: (ListedRequest) -> Void
    private let table = NSTableView()
    private let filterPicker = NSPopUpButton(frame: .zero, pullsDown: false)
    private let statusLabel = NSTextField(labelWithString: "")
    private let spinner = NSProgressIndicator()
    /// Named filters from config, in menu order.
    private var filterNames: [String] = []
    /// The last one-off filter typed, kept in the menu for the session.
    private var customFilter: String?
    private var loading = false

    var onClose: (() -> Void)?

    init(requests: [ListedRequest], onOpen: @escaping (ListedRequest) -> Void) {
        self.requests = requests
        self.onOpen = onOpen

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 760, height: 420),
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false)
        window.title = "Review Queue"
        super.init(window: window)
        window.delegate = self

        filterPicker.target = self
        filterPicker.action = #selector(filterChanged(_:))
        rebuildFilterMenu()

        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isDisplayedWhenStopped = false
        statusLabel.textColor = .secondaryLabelColor
        statusLabel.font = .systemFont(ofSize: NSFont.smallSystemFontSize)

        let bar = NSStackView()
        bar.orientation = .horizontal
        bar.spacing = 8
        bar.addArrangedSubview(NSTextField(labelWithString: "Filter:"))
        bar.addArrangedSubview(filterPicker)
        bar.addArrangedSubview(spinner)
        bar.addArrangedSubview(statusLabel)

        let columns: [(String, String, CGFloat)] = [
            ("request", "Request", 180),
            ("title", "Title", 320),
            ("author", "Author", 100),
            ("updated", "Updated", 90),
        ]
        for (identifier, title, width) in columns {
            let column = NSTableColumn(
                identifier: NSUserInterfaceItemIdentifier(identifier))
            column.title = title
            column.width = width
            table.addTableColumn(column)
        }
        table.dataSource = self
        table.delegate = self
        table.allowsMultipleSelection = false
        table.doubleAction = #selector(openSelected(_:))
        table.target = self
        table.usesAlternatingRowBackgroundColors = true

        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.documentView = table

        let stack = NSStackView()
        stack.orientation = .vertical
        stack.spacing = 8
        stack.edgeInsets = NSEdgeInsets(top: 10, left: 10, bottom: 10, right: 10)
        stack.addArrangedSubview(bar)
        stack.addArrangedSubview(scroll)
        window.contentView = stack
        window.center()
        updateStatus()
        if !requests.isEmpty {
            table.selectRowIndexes([0], byExtendingSelection: false)
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    func windowWillClose(_ notification: Notification) {
        onClose?()
    }

    // MARK: - Filters

    /// Menu: Default (the config's fallback or the engine's), each named
    /// filter, the session's last custom one, and "Custom…".
    private func rebuildFilterMenu(keepSelection: String? = nil) {
        let config = CoreConfig()
        let fallback = config.listFilter
        filterNames = config.listFilters.keys.sorted()
        filterPicker.removeAllItems()
        filterPicker.addItem(
            withTitle: "Default" + (fallback.isEmpty ? "" : " — \(fallback)"))
        for name in filterNames {
            filterPicker.addItem(withTitle: name)
        }
        if let customFilter {
            filterPicker.addItem(withTitle: "Custom — \(customFilter)")
        }
        filterPicker.menu?.addItem(.separator())
        filterPicker.addItem(withTitle: "Custom…")
        if let keepSelection {
            filterPicker.selectItem(withTitle: keepSelection)
        }
    }

    /// The filter string the current selection stands for ("" = default).
    private var selectedFilter: String {
        let index = filterPicker.indexOfSelectedItem
        if index == 0 {
            return ""
        }
        let namedEnd = filterNames.count
        if index - 1 < namedEnd {
            return CoreConfig().listFilters[filterNames[index - 1]] ?? ""
        }
        return customFilter ?? ""
    }

    @objc private func filterChanged(_ sender: Any?) {
        let title = filterPicker.titleOfSelectedItem ?? ""
        if title == "Custom…" {
            promptForCustomFilter()
            return
        }
        reload()
    }

    private func promptForCustomFilter() {
        guard let window else { return }
        let alert = NSAlert()
        alert.messageText = "Custom filter"
        alert.informativeText =
            "A GitHub search query (gh engine) or query-string qualifiers (forgejo). "
            + "Save it for good in Settings."
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 380, height: 24))
        field.placeholderString = "is:open label:bug review-requested:@me"
        field.stringValue = customFilter ?? ""
        alert.accessoryView = field
        alert.window.initialFirstResponder = field
        alert.addButton(withTitle: "Search")
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { [weak self] response in
            guard let self else { return }
            let filter = field.stringValue.trimmingCharacters(in: .whitespaces)
            guard response == .alertFirstButtonReturn, !filter.isEmpty else {
                self.filterPicker.selectItem(at: 0)
                return
            }
            self.customFilter = filter
            self.rebuildFilterMenu(keepSelection: "Custom — \(filter)")
            self.reload()
        }
    }

    /// Refetches with the selected filter, off-main behind the spinner.
    private func reload() {
        guard !loading else { return }
        loading = true
        spinner.startAnimation(nil)
        statusLabel.stringValue = "Searching…"
        let filter = selectedFilter
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let outcome = Result { try CoreDiscovery.listRequests(filter: filter) }
            DispatchQueue.main.async {
                guard let self else { return }
                self.loading = false
                self.spinner.stopAnimation(nil)
                switch outcome {
                case .success(let found):
                    self.requests = found
                    self.table.reloadData()
                    if !found.isEmpty {
                        self.table.selectRowIndexes([0], byExtendingSelection: false)
                    }
                    self.updateStatus()
                case .failure(let error):
                    self.statusLabel.stringValue = "\(error)"
                }
            }
        }
    }

    private func updateStatus() {
        statusLabel.stringValue =
            requests.isEmpty
            ? "Nothing matches this filter."
            : "\(requests.count) request\(requests.count == 1 ? "" : "s")"
    }

    // MARK: - Opening

    @objc func openSelected(_ sender: Any?) {
        let row = table.selectedRow
        guard requests.indices.contains(row) else { return }
        close()
        onOpen(requests[row])
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 36 /* return */ {
            openSelected(nil)
        } else {
            super.keyDown(with: event)
        }
    }

    // MARK: NSTableViewDataSource / Delegate

    func numberOfRows(in tableView: NSTableView) -> Int {
        requests.count
    }

    func tableView(
        _ tableView: NSTableView,
        viewFor tableColumn: NSTableColumn?,
        row: Int
    ) -> NSView? {
        let request = requests[row]
        let text: String
        switch tableColumn?.identifier.rawValue {
        case "request": text = request.reference
        case "title": text = request.title
        case "author": text = "@\(request.author)"
        default: text = String(request.updatedAt.prefix(10))
        }
        let label = NSTextField(labelWithString: text)
        label.lineBreakMode = .byTruncatingTail
        return label
    }
}
