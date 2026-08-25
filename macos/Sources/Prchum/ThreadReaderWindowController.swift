import AppKit
import PrchumKit

/// A conversation reader over one anchored thread: a draft's travelling
/// conversation (every item editable), or a host thread (read-only, with
/// reply staging). One row per item, Markdown detail below.
@MainActor
final class ThreadReaderWindowController: NSWindowController, NSWindowDelegate,
    NSTableViewDataSource, NSTableViewDelegate
{
    struct Item {
        let author: String
        let date: String
        let body: String
        /// Root = nil; replies carry their index.
        let replyIndex: Int?
        let editable: Bool
        var imageMap: [String: String] = [:]
    }

    private var items: [Item] = []
    private let table = NSTableView()
    private let detail = NSTextView.scrollableTextView()
    private let reload: () -> [Item]
    private let onReply: ((String) -> Void)?
    private let onEdit: ((Item, String) -> Void)?
    private let onDelete: ((Item) -> Void)?

    var onClose: (() -> Void)?

    init(
        title: String,
        snippet: String,
        reload: @escaping () -> [Item],
        onReply: ((String) -> Void)?,
        onEdit: ((Item, String) -> Void)?,
        onDelete: ((Item) -> Void)?
    ) {
        self.reload = reload
        self.onReply = onReply
        self.onEdit = onEdit
        self.onDelete = onDelete

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 640, height: 460),
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false)
        window.title = title
        super.init(window: window)
        window.delegate = self

        let buttons = NSStackView()
        buttons.orientation = .horizontal
        buttons.spacing = 8
        if onReply != nil {
            buttons.addArrangedSubview(
                NSButton(title: "Reply…", target: self, action: #selector(reply(_:))))
        }
        if onEdit != nil {
            buttons.addArrangedSubview(
                NSButton(title: "Edit…", target: self, action: #selector(edit(_:))))
        }
        if onDelete != nil {
            buttons.addArrangedSubview(
                NSButton(title: "Delete", target: self, action: #selector(delete(_:))))
        }

        for (identifier, columnTitle, width) in [
            ("who", "Who", 200), ("when", "When", 90), ("preview", "Text", 300),
        ] {
            let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(identifier))
            column.title = columnTitle
            column.width = CGFloat(width)
            table.addTableColumn(column)
        }
        table.dataSource = self
        table.delegate = self
        table.usesAlternatingRowBackgroundColors = true

        let tableScroll = NSScrollView()
        tableScroll.hasVerticalScroller = true
        tableScroll.documentView = table

        let detailView = detail.documentView as! NSTextView
        detailView.isEditable = false
        detailView.textContainerInset = NSSize(width: 8, height: 8)
        detail.hasVerticalScroller = true

        let splitView = NSSplitView()
        splitView.isVertical = false
        splitView.dividerStyle = .thin
        splitView.addArrangedSubview(tableScroll)
        splitView.addArrangedSubview(detail)

        let stack = NSStackView()
        stack.orientation = .vertical
        stack.spacing = 8
        stack.edgeInsets = NSEdgeInsets(top: 10, left: 10, bottom: 10, right: 10)
        if !snippet.isEmpty {
            let code = NSTextField(wrappingLabelWithString: snippet)
            code.font = .monospacedSystemFont(
                ofSize: NSFont.smallSystemFontSize, weight: .regular)
            code.textColor = .secondaryLabelColor
            code.maximumNumberOfLines = 4
            stack.addArrangedSubview(code)
        }
        if !buttons.arrangedSubviews.isEmpty {
            stack.addArrangedSubview(buttons)
        }
        stack.addArrangedSubview(splitView)
        window.contentView = stack
        window.center()
        refresh()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    func windowWillClose(_ notification: Notification) {
        onClose?()
    }

    func refresh() {
        let selected = table.selectedRow
        items = reload()
        // The conversation may have been deleted out from under us.
        if items.isEmpty {
            close()
            return
        }
        table.reloadData()
        table.selectRowIndexes(
            [min(max(selected, 0), items.count - 1)], byExtendingSelection: false)
        showDetail()
    }

    private var selectedItem: Item? {
        items.indices.contains(table.selectedRow) ? items[table.selectedRow] : nil
    }

    private func showDetail() {
        let view = detail.documentView as! NSTextView
        guard let item = selectedItem else {
            view.string = ""
            return
        }
        view.textStorage?.setAttributedString(
            MarkdownRenderer.render(
                markdown: item.body,
                header: "@\(item.author)  \(item.date)",
                imageMap: item.imageMap,
                onImagesLoaded: { [weak self] in self?.showDetail() }))
    }

    @objc private func reply(_ sender: Any?) {
        promptForBody(title: "Reply", initial: "") { [weak self] body in
            self?.onReply?(body)
            self?.refresh()
        }
    }

    @objc private func edit(_ sender: Any?) {
        guard let item = selectedItem else { return }
        guard item.editable else {
            NSSound.beep()
            return
        }
        promptForBody(title: "Edit", initial: item.body) { [weak self] body in
            self?.onEdit?(item, body)
            self?.refresh()
        }
    }

    @objc private func delete(_ sender: Any?) {
        guard let item = selectedItem, item.editable else {
            NSSound.beep()
            return
        }
        onDelete?(item)
        refresh()
    }

    private func promptForBody(
        title: String, initial: String, onSave: @escaping (String) -> Void
    ) {
        guard let window else { return }
        let alert = NSAlert()
        alert.messageText = title
        let scroll = NSTextView.scrollableTextView()
        scroll.frame = NSRect(x: 0, y: 0, width: 420, height: 120)
        let textView = scroll.documentView as! NSTextView
        textView.string = initial
        textView.font = .systemFont(ofSize: NSFont.systemFontSize)
        textView.isRichText = false
        alert.accessoryView = scroll
        alert.window.initialFirstResponder = textView
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { response in
            guard response == .alertFirstButtonReturn else { return }
            let body = textView.string.trimmingCharacters(in: .whitespacesAndNewlines)
            if !body.isEmpty {
                onSave(body)
            }
        }
    }

    // MARK: table

    func numberOfRows(in tableView: NSTableView) -> Int {
        items.count
    }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        let item = items[row]
        let text: String
        switch tableColumn?.identifier.rawValue {
        case "who":
            text = (item.replyIndex == nil ? "" : "  ↳ ") + "@\(item.author)"
        case "when":
            text = String(item.date.prefix(10))
        default:
            text = item.body.split(separator: "\n").first.map(String.init) ?? ""
        }
        let label = NSTextField(labelWithString: text)
        label.lineBreakMode = .byTruncatingTail
        return label
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        showDetail()
    }
}
