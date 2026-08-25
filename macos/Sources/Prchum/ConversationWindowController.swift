import AppKit
import PrchumKit

/// The pull request's conversation: the host's discussion plus the
/// comments staged to post on submit. Both forges' conversations are
/// flat, so replying means quoting.
@MainActor
final class ConversationWindowController: NSWindowController, NSWindowDelegate,
    NSTableViewDataSource, NSTableViewDelegate
{
    struct Item {
        let author: String
        let date: String
        let body: String
        /// Set for staged drafts — the deletable ones.
        let draftID: String?
    }

    private var items: [Item] = []
    private let table = NSTableView()
    private let detail = NSTextView.scrollableTextView()
    private let reload: () -> [Item]
    private let onAdd: (String) -> Void
    private let onDelete: (String) -> Void

    var onClose: (() -> Void)?

    init(
        title: String,
        reload: @escaping () -> [Item],
        onAdd: @escaping (String) -> Void,
        onDelete: @escaping (String) -> Void
    ) {
        self.reload = reload
        self.onAdd = onAdd
        self.onDelete = onDelete

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 700, height: 480),
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false)
        window.title = title
        super.init(window: window)
        window.delegate = self

        let buttons = NSStackView()
        buttons.orientation = .horizontal
        buttons.spacing = 8
        buttons.addArrangedSubview(
            NSButton(
                title: "Add Comment…", target: self, action: #selector(addComment(_:))))
        buttons.addArrangedSubview(
            NSButton(
                title: "Quote & Reply…", target: self, action: #selector(quoteReply(_:))))
        buttons.addArrangedSubview(
            NSButton(
                title: "Delete Staged", target: self, action: #selector(deleteStaged(_:))))

        for (identifier, columnTitle, width) in [
            ("who", "Who", 180), ("when", "When", 90), ("preview", "Comment", 360),
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
        stack.addArrangedSubview(buttons)
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
        table.reloadData()
        if !items.isEmpty {
            table.selectRowIndexes(
                [min(max(selected, 0), items.count - 1)], byExtendingSelection: false)
        }
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
                header: "@\(item.author)  \(item.date)"
                    + (item.draftID != nil ? "  (staged — posts on submit)" : "")))
    }

    @objc private func addComment(_ sender: Any?) {
        promptForBody(title: "Conversation comment", initial: "")
    }

    /// Replying to a flat conversation = quoting the selected comment.
    @objc private func quoteReply(_ sender: Any?) {
        guard let item = selectedItem else { return }
        let quoted = item.body
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map { "> \($0)" }
            .joined(separator: "\n")
        promptForBody(
            title: "Reply to @\(item.author)", initial: "\(quoted)\n\n")
    }

    @objc private func deleteStaged(_ sender: Any?) {
        guard let item = selectedItem, let draftID = item.draftID else {
            NSSound.beep()
            return
        }
        onDelete(draftID)
        refresh()
    }

    private func promptForBody(title: String, initial: String) {
        guard let window else { return }
        let alert = NSAlert()
        alert.messageText = title
        let scroll = NSTextView.scrollableTextView()
        scroll.frame = NSRect(x: 0, y: 0, width: 440, height: 140)
        let textView = scroll.documentView as! NSTextView
        textView.string = initial
        textView.font = .systemFont(ofSize: NSFont.systemFontSize)
        textView.isRichText = false
        alert.accessoryView = scroll
        alert.window.initialFirstResponder = textView
        alert.addButton(withTitle: "Stage")
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { [weak self] response in
            guard response == .alertFirstButtonReturn else { return }
            let body = textView.string.trimmingCharacters(in: .whitespacesAndNewlines)
            if !body.isEmpty {
                self?.onAdd(body)
                self?.refresh()
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
            text = (item.draftID != nil ? "● " : "") + "@\(item.author)"
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

/// Markdown → attributed text for comment bodies and PR descriptions.
enum MarkdownRenderer {
    static func render(markdown: String, header: String? = nil) -> NSAttributedString {
        let result = NSMutableAttributedString()
        if let header {
            result.append(
                NSAttributedString(
                    string: header + "\n\n",
                    attributes: [
                        .font: NSFont.boldSystemFont(ofSize: NSFont.smallSystemFontSize),
                        .foregroundColor: NSColor.secondaryLabelColor,
                    ]))
        }
        let options = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace)
        let body: NSAttributedString
        if let parsed = try? AttributedString(markdown: markdown, options: options) {
            let mutable = NSMutableAttributedString(parsed)
            // The parser leaves fonts unset; give everything a base and
            // keep the traits (bold/italic/code) it added.
            mutable.addAttribute(
                .foregroundColor, value: NSColor.labelColor,
                range: NSRange(location: 0, length: mutable.length))
            mutable.enumerateAttribute(
                .font, in: NSRange(location: 0, length: mutable.length)
            ) { value, range, _ in
                if value == nil {
                    mutable.addAttribute(
                        .font, value: NSFont.systemFont(ofSize: NSFont.systemFontSize),
                        range: range)
                }
            }
            body = mutable
        } else {
            body = NSAttributedString(
                string: markdown,
                attributes: [
                    .font: NSFont.systemFont(ofSize: NSFont.systemFontSize),
                    .foregroundColor: NSColor.labelColor,
                ])
        }
        result.append(body)
        return result
    }
}
