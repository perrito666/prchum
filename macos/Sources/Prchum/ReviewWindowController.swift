import AppKit
import PrchumKit
import SwiftUI

/// One review window: changed-files sidebar on the left, the selected
/// file's diff on the right.
///
/// Navigation is action-driven (menu items with key equivalents, see
/// `Keymap`): the caret is the position, and next/previous change, hunk,
/// and file move it between blocks. The mouse is the secondary path —
/// click a file in the sidebar, click or drag in the diff to place and
/// extend the selection.
@MainActor
final class ReviewWindowController: NSWindowController, NSWindowDelegate {
    private let session: CoreSession
    private let files: [DiffFile]
    private let sidebarModel: SidebarModel
    private let diffTextView: NSTextView
    private let diffScrollView: NSScrollView
    private var rendered: RenderedDiff?
    private var wrapEnabled = true

    var onClose: ((ReviewWindowController) -> Void)?

    init(session: CoreSession) {
        self.session = session
        self.files = (try? session.files()) ?? []
        self.sidebarModel = SidebarModel(files: files)
        self.diffTextView = Self.makeDiffTextView()
        self.diffScrollView = NSScrollView()

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 720),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false)
        window.title = session.title
        window.center()
        window.setFrameAutosaveName("ReviewWindow")
        super.init(window: window)
        window.delegate = self

        let split = NSSplitViewController()

        let sidebar = NSHostingController(
            rootView: SidebarView(model: sidebarModel) { [weak self] index in
                self?.showFile(at: index)
            })
        let sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebar)
        sidebarItem.minimumThickness = 220
        sidebarItem.maximumThickness = 400
        split.addSplitViewItem(sidebarItem)

        diffScrollView.hasVerticalScroller = true
        diffScrollView.documentView = diffTextView
        let content = NSViewController()
        content.view = diffScrollView
        split.addSplitViewItem(NSSplitViewItem(viewController: content))

        window.contentViewController = split
        applyWrap()
        showFile(at: 0)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    func windowWillClose(_ notification: Notification) {
        onClose?(self)
    }

    // MARK: - Actions (nil-target, reached through the responder chain)

    @objc func nextChange(_ sender: Any?) {
        move(to: nextBlock(in: rendered?.changeRanges ?? []))
    }

    @objc func previousChange(_ sender: Any?) {
        move(to: previousBlock(in: rendered?.changeRanges ?? []))
    }

    @objc func nextHunk(_ sender: Any?) {
        move(to: nextBlock(in: rendered?.hunkRanges ?? []))
    }

    @objc func previousHunk(_ sender: Any?) {
        move(to: previousBlock(in: rendered?.hunkRanges ?? []))
    }

    @objc func nextFile(_ sender: Any?) {
        showFile(at: sidebarModel.selected + 1)
    }

    @objc func previousFile(_ sender: Any?) {
        showFile(at: sidebarModel.selected - 1)
    }

    @objc func toggleWrap(_ sender: Any?) {
        wrapEnabled.toggle()
        applyWrap()
    }

    @objc func validateMenuItem(_ item: NSMenuItem) -> Bool {
        switch item.action {
        case #selector(nextChange(_:)), #selector(previousChange(_:)):
            return !(rendered?.changeRanges.isEmpty ?? true)
        case #selector(nextHunk(_:)), #selector(previousHunk(_:)):
            return !(rendered?.hunkRanges.isEmpty ?? true)
        case #selector(nextFile(_:)):
            return sidebarModel.selected + 1 < files.count
        case #selector(previousFile(_:)):
            return sidebarModel.selected > 0
        case #selector(toggleWrap(_:)):
            item.state = wrapEnabled ? .on : .off
            return true
        default:
            return true
        }
    }

    // MARK: - Block navigation

    /// The caret: where the selection starts.
    private var caret: Int {
        diffTextView.selectedRange().location
    }

    private func nextBlock(in ranges: [NSRange]) -> NSRange? {
        ranges.first { $0.location > caret }
    }

    private func previousBlock(in ranges: [NSRange]) -> NSRange? {
        ranges.last { $0.location < caret }
    }

    private func move(to range: NSRange?) {
        guard let range else {
            NSSound.beep()
            return
        }
        // Selecting the block both places the caret for the next motion and
        // makes the landing spot visible.
        diffTextView.setSelectedRange(range)
        diffTextView.scrollRangeToVisible(range)
    }

    // MARK: - Content

    private func showFile(at index: Int) {
        guard files.indices.contains(index) else { return }
        sidebarModel.selected = index
        let rendered = DiffRenderer.render(file: files[index])
        self.rendered = rendered
        diffTextView.textStorage?.setAttributedString(rendered.text)
        diffTextView.setSelectedRange(NSRange(location: 0, length: 0))
        diffTextView.scroll(.zero)
    }

    private func applyWrap() {
        let container = diffTextView.textContainer
        if wrapEnabled {
            diffScrollView.hasHorizontalScroller = false
            diffTextView.isHorizontallyResizable = false
            diffTextView.autoresizingMask = [.width]
            container?.widthTracksTextView = true
            container?.size = NSSize(
                width: diffScrollView.contentSize.width, height: .greatestFiniteMagnitude)
            diffTextView.frame.size.width = diffScrollView.contentSize.width
        } else {
            diffScrollView.hasHorizontalScroller = true
            diffTextView.isHorizontallyResizable = true
            diffTextView.autoresizingMask = []
            container?.widthTracksTextView = false
            container?.size = NSSize(
                width: CGFloat.greatestFiniteMagnitude,
                height: CGFloat.greatestFiniteMagnitude)
        }
        diffTextView.needsLayout = true
    }

    private static func makeDiffTextView() -> NSTextView {
        let view = NSTextView()
        view.isEditable = false
        view.isSelectable = true
        view.isRichText = false
        view.isVerticallyResizable = true
        view.minSize = .zero
        view.maxSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude)
        view.textContainerInset = NSSize(width: 8, height: 8)
        view.backgroundColor = .textBackgroundColor
        return view
    }
}

// MARK: - Sidebar

@MainActor
final class SidebarModel: ObservableObject {
    struct Row: Identifiable {
        let id: Int
        let glyph: String
        let path: String
        let added: Int
        let deleted: Int
    }

    let rows: [Row]
    @Published var selected: Int

    init(files: [DiffFile]) {
        rows = files.enumerated().map { index, file in
            let counts = file.changeCounts
            return Row(
                id: index,
                glyph: file.status.glyph,
                path: file.displayPath,
                added: counts.added,
                deleted: counts.deleted)
        }
        selected = 0
    }
}

struct SidebarView: View {
    @ObservedObject var model: SidebarModel
    let onSelect: (Int) -> Void

    var body: some View {
        List(model.rows, selection: selectionBinding) { row in
            HStack(spacing: 6) {
                Text(row.glyph)
                    .font(.system(.caption, design: .monospaced).bold())
                    .foregroundStyle(glyphColor(row.glyph))
                    .frame(width: 14)
                Text(row.path)
                    .lineLimit(1)
                    .truncationMode(.head)
                Spacer(minLength: 4)
                if row.added > 0 {
                    Text("+\(row.added)")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.green)
                }
                if row.deleted > 0 {
                    Text("−\(row.deleted)")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.red)
                }
            }
            .tag(row.id)
        }
        .listStyle(.sidebar)
    }

    private var selectionBinding: Binding<Int?> {
        Binding(
            get: { model.selected },
            set: { newValue in
                if let newValue { onSelect(newValue) }
            })
    }

    private func glyphColor(_ glyph: String) -> Color {
        switch glyph {
        case "A": return .green
        case "D": return .red
        case "R", "C": return .orange
        case "B": return .secondary
        default: return .accentColor
        }
    }
}

// MARK: - Diff rendering

/// One file's diff laid out as attributed text, plus the character ranges
/// of its navigable blocks.
struct RenderedDiff {
    let text: NSAttributedString
    /// Each hunk, from its `@@` header through its last line.
    let hunkRanges: [NSRange]
    /// Each contiguous run of added/deleted lines.
    let changeRanges: [NSRange]
}

/// Builds the attributed text for one file's unified diff.
enum DiffRenderer {
    static func render(file: DiffFile) -> RenderedDiff {
        let font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        let result = NSMutableAttributedString()
        var hunkRanges: [NSRange] = []
        var changeRanges: [NSRange] = []
        var changeStart: Int?

        func append(_ text: String, color: NSColor, background: NSColor? = nil) {
            var attributes: [NSAttributedString.Key: Any] = [
                .font: font,
                .foregroundColor: color,
            ]
            if let background {
                attributes[.backgroundColor] = background
            }
            result.append(NSAttributedString(string: text, attributes: attributes))
        }

        func closeChangeBlock() {
            if let start = changeStart {
                changeRanges.append(NSRange(location: start, length: result.length - start))
                changeStart = nil
            }
        }

        if file.isBinary {
            append("Binary file — nothing to show.\n", color: .secondaryLabelColor)
            return RenderedDiff(text: result, hunkRanges: [], changeRanges: [])
        }
        if file.status == .renamed {
            append("renamed \(file.oldPath) → \(file.newPath)\n\n", color: .secondaryLabelColor)
        }

        for hunk in file.hunks {
            let hunkStart = result.length
            append("\(hunk.header)\n", color: .secondaryLabelColor,
                   background: NSColor.separatorColor.withAlphaComponent(0.25))
            for line in hunk.lines {
                if line.kind == .meta {
                    closeChangeBlock()
                    append("        \(line.text)\n", color: .tertiaryLabelColor)
                    continue
                }
                let isChange = line.kind == .addition || line.kind == .deletion
                if isChange, changeStart == nil {
                    changeStart = result.length
                } else if !isChange {
                    closeChangeBlock()
                }
                let gutter = "\(pad(line.oldLine)) \(pad(line.newLine)) "
                append(gutter, color: .tertiaryLabelColor)
                switch line.kind {
                case .addition:
                    append("+\(line.text)\n", color: .labelColor,
                           background: NSColor.systemGreen.withAlphaComponent(0.16))
                case .deletion:
                    append("-\(line.text)\n", color: .labelColor,
                           background: NSColor.systemRed.withAlphaComponent(0.16))
                default:
                    append(" \(line.text)\n", color: .labelColor)
                }
            }
            closeChangeBlock()
            hunkRanges.append(NSRange(location: hunkStart, length: result.length - hunkStart))
            append("\n", color: .labelColor)
        }
        return RenderedDiff(text: result, hunkRanges: hunkRanges, changeRanges: changeRanges)
    }

    private static func pad(_ number: Int?) -> String {
        guard let number else { return String(repeating: " ", count: 5) }
        return String(format: "%5d", number)
    }
}
