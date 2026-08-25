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
        var imageMap: [String: String] = [:]
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
                    + (item.draftID != nil ? "  (staged — posts on submit)" : ""),
                imageMap: item.imageMap,
                onImagesLoaded: { [weak self] in self?.showDetail() }))
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

/// Fetches and caches comment images. Public http(s) URLs only; GitHub's
/// session-gated user-attachments links stay links (their signed variants
/// need the HTML body, a later refinement).
@MainActor
enum CommentImageCache {
    private static var images: [String: NSImage] = [:]
    private static var pending: Set<String> = []

    static func image(for url: String) -> NSImage? {
        images[url]
    }

    /// Kicks a fetch when the image is unknown; `onLoad` fires on main
    /// once it arrives (never for failures — a broken URL stays a link).
    static func fetch(
        _ url: String, via signedURL: String? = nil, onLoad: @escaping () -> Void
    ) {
        // Session-gated attachments fetch through their signed variant
        // (cached under the original URL, which is what bodies reference);
        // without one they stay links.
        let fetchURL = signedURL ?? url
        guard images[url] == nil, !pending.contains(url),
            fetchURL.hasPrefix("https://") || fetchURL.hasPrefix("http://"),
            !fetchURL.contains("github.com/user-attachments/"),
            let target = URL(string: fetchURL)
        else { return }
        pending.insert(url)
        URLSession.shared.dataTask(with: target) { data, _, _ in
            DispatchQueue.main.async {
                pending.remove(url)
                // The size cap keeps a hostile URL from ballooning memory.
                guard let data, data.count < 10 * 1024 * 1024,
                    let image = NSImage(data: data)
                else { return }
                images[url] = image
                onLoad()
            }
        }.resume()
    }

    /// Image URLs referenced by a Markdown body: `![alt](url)` and
    /// `<img src="url">`, at most three per comment.
    static func imageURLs(in markdown: String) -> [String] {
        var urls: [String] = []
        for pattern in [
            #"!\[[^\]]*\]\(([^)\s]+)\)"#,
            #"<img[^>]+src="([^"]+)""#,
        ] {
            guard let regex = try? NSRegularExpression(pattern: pattern) else { continue }
            let range = NSRange(markdown.startIndex..., in: markdown)
            regex.enumerateMatches(in: markdown, range: range) { match, _, _ in
                if let match, match.numberOfRanges > 1,
                    let urlRange = Range(match.range(at: 1), in: markdown)
                {
                    urls.append(String(markdown[urlRange]))
                }
            }
        }
        return Array(urls.prefix(3))
    }
}

/// Markdown → attributed text for comment bodies and PR descriptions.
enum MarkdownRenderer {
    @MainActor
    static func render(
        markdown: String,
        header: String? = nil,
        imageMap: [String: String] = [:],
        onImagesLoaded: (() -> Void)? = nil
    ) -> NSAttributedString {
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

        // Referenced images render inline once fetched; until then (and
        // for unfetchable ones) the link in the text stands in.
        if let onImagesLoaded {
            for url in CommentImageCache.imageURLs(in: markdown) {
                if let image = CommentImageCache.image(for: url) {
                    let attachment = NSTextAttachment()
                    attachment.image = image
                    let size = image.size
                    let width = min(size.width, 480)
                    attachment.bounds = CGRect(
                        x: 0, y: 0, width: width,
                        height: size.height * (width / max(size.width, 1)))
                    result.append(NSAttributedString(string: "\n\n"))
                    result.append(NSAttributedString(attachment: attachment))
                } else {
                    CommentImageCache.fetch(url, via: imageMap[url], onLoad: onImagesLoaded)
                }
            }
        }
        return result
    }
}
