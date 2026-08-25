import AppKit
import PrchumKit

/// The review queue: the open requests waiting on the user, one row each.
/// Return or a double-click opens the selected request.
@MainActor
final class ReviewQueueWindowController: NSWindowController, NSWindowDelegate,
    NSTableViewDataSource, NSTableViewDelegate
{
    private let requests: [ListedRequest]
    private let onOpen: (ListedRequest) -> Void
    private let table = NSTableView()

    var onClose: (() -> Void)?

    init(requests: [ListedRequest], onOpen: @escaping (ListedRequest) -> Void) {
        self.requests = requests
        self.onOpen = onOpen

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 720, height: 380),
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false)
        window.title = "Review Queue"
        super.init(window: window)
        window.delegate = self

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
        window.contentView = scroll
        window.center()
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

    @objc func openSelected(_ sender: Any?) {
        let row = table.selectedRow
        guard requests.indices.contains(row) else { return }
        close()
        onOpen(requests[row])
    }

    // Return opens; the table is the window's whole content, so plain key
    // handling on the window works.
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
