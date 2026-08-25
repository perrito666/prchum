import AppKit
import PrchumKit
import SwiftUI

/// One review window: changed-files sidebar on the left, the selected
/// file's diff on the right.
///
/// Phase 0 renders the diff read-only into a text view; the custom
/// line-oriented view with gutters and comment rows replaces it in Phase 1.
@MainActor
final class ReviewWindowController: NSWindowController, NSWindowDelegate {
    private let session: CoreSession
    private let files: [DiffFile]
    private let sidebarModel: SidebarModel
    private let diffTextView: NSTextView

    var onClose: ((ReviewWindowController) -> Void)?

    init(session: CoreSession) {
        self.session = session
        self.files = (try? session.files()) ?? []
        self.sidebarModel = SidebarModel(files: files)
        self.diffTextView = Self.makeDiffTextView()

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

        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.documentView = diffTextView
        let content = NSViewController()
        content.view = scroll
        split.addSplitViewItem(NSSplitViewItem(viewController: content))

        window.contentViewController = split
        showFile(at: 0)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    func windowWillClose(_ notification: Notification) {
        onClose?(self)
    }

    private func showFile(at index: Int) {
        guard files.indices.contains(index) else { return }
        sidebarModel.selected = index
        let rendered = DiffRenderer.render(file: files[index])
        diffTextView.textStorage?.setAttributedString(rendered)
        diffTextView.scroll(.zero)
    }

    private static func makeDiffTextView() -> NSTextView {
        let view = NSTextView()
        view.isEditable = false
        view.isRichText = false
        view.autoresizingMask = [.width]
        view.isVerticallyResizable = true
        view.isHorizontallyResizable = false
        view.textContainer?.widthTracksTextView = true
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

/// Builds the attributed text for one file's unified diff.
enum DiffRenderer {
    static func render(file: DiffFile) -> NSAttributedString {
        let font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        let result = NSMutableAttributedString()

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

        if file.isBinary {
            append("Binary file — nothing to show.\n", color: .secondaryLabelColor)
            return result
        }
        if file.status == .renamed {
            append("renamed \(file.oldPath) → \(file.newPath)\n\n", color: .secondaryLabelColor)
        }

        for hunk in file.hunks {
            append("\(hunk.header)\n", color: .secondaryLabelColor,
                   background: NSColor.separatorColor.withAlphaComponent(0.25))
            for line in hunk.lines {
                if line.kind == .meta {
                    append("        \(line.text)\n", color: .tertiaryLabelColor)
                    continue
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
            append("\n", color: .labelColor)
        }
        return result
    }

    private static func pad(_ number: Int?) -> String {
        guard let number else { return String(repeating: " ", count: 5) }
        return String(format: "%5d", number)
    }
}
