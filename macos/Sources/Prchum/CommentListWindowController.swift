import AppKit
import PrchumKit

/// The review navigator: every draft comment and host thread in one list.
/// Return or a double-click jumps to the entry's anchor.
@MainActor
final class CommentListWindowController: NSWindowController, NSWindowDelegate,
    NSTableViewDataSource, NSTableViewDelegate
{
    struct Entry {
        let location: String
        let kind: String
        let preview: String
        let path: String
        let side: DiffSide
        let line: Int
    }

    private let entries: [Entry]
    private let onJump: (Entry) -> Void
    private let table = NSTableView()

    var onClose: (() -> Void)?

    init(title: String, entries: [Entry], onJump: @escaping (Entry) -> Void) {
        self.entries = entries
        self.onJump = onJump

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 720, height: 340),
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false)
        window.title = title
        super.init(window: window)
        window.delegate = self

        for (identifier, columnTitle, width) in [
            ("location", "Where", 220), ("kind", "Kind", 90), ("preview", "Comment", 380),
        ] {
            let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(identifier))
            column.title = columnTitle
            column.width = CGFloat(width)
            table.addTableColumn(column)
        }
        table.dataSource = self
        table.delegate = self
        table.doubleAction = #selector(jumpToSelected(_:))
        table.target = self
        table.usesAlternatingRowBackgroundColors = true

        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.documentView = table
        window.contentView = scroll
        window.center()
        if !entries.isEmpty {
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

    @objc func jumpToSelected(_ sender: Any?) {
        let row = table.selectedRow
        guard entries.indices.contains(row) else { return }
        close()
        onJump(entries[row])
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 36 /* return */ {
            jumpToSelected(nil)
        } else {
            super.keyDown(with: event)
        }
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        entries.count
    }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        let entry = entries[row]
        let text: String
        switch tableColumn?.identifier.rawValue {
        case "location": text = entry.location
        case "kind": text = entry.kind
        default: text = entry.preview
        }
        let label = NSTextField(labelWithString: text)
        label.lineBreakMode = .byTruncatingTail
        return label
    }
}
