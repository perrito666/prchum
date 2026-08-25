import AppKit
import PrchumKit
import SwiftUI

/// One review window: changed-files sidebar on the left, the selected
/// file's diff on the right, draft comments and host threads inline.
///
/// Navigation and review are action-driven (menu items with key
/// equivalents, see `Keymap`); the caret is the position. The mouse is the
/// secondary path — click a file, click or drag in the diff to place and
/// extend the selection.
@MainActor
final class ReviewWindowController: NSWindowController, NSWindowDelegate {
    private let session: CoreSession
    private let files: [DiffFile]
    private let sidebarModel: SidebarModel
    private let diffTextView: NSTextView
    private let diffScrollView: NSScrollView
    private var rendered: RenderedDiff?
    private var comments: [DraftComment] = []
    private var threads: [ReviewThread] = []
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

        if let warning = session.attachStore() {
            NSLog("drafts: %@ — starting fresh, the saved file is untouched", warning)
        }
        session.setAuthor(NSUserName())
        comments = session.comments()
        threads = session.threads()

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
        updateBadges()
        showFile(at: 0)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    func windowWillClose(_ notification: Notification) {
        onClose?(self)
    }

    // MARK: - Navigation actions

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

    @objc func findInDiff(_ sender: Any?) {
        window?.makeFirstResponder(diffTextView)
        let item = NSMenuItem()
        item.tag = NSTextFinder.Action.showFindInterface.rawValue
        diffTextView.performTextFinderAction(item)
    }

    // MARK: - Review actions

    /// Comment on the selected lines (or the caret's line).
    @objc func addComment(_ sender: Any?) {
        guard let rendered else { return }
        let resolved = SelectionResolver.resolve(
            lineRefs: rendered.lineRefs, selection: diffTextView.selectedRange())
        switch resolved {
        case .failure(let error):
            presentInfo(error.message)
        case .success(let target):
            let lines = target.startLine == target.endLine
                ? "line \(target.startLine)"
                : "lines \(target.startLine)–\(target.endLine)"
            promptForText(
                title: "Comment on \(lines) (\(target.side.rawValue))",
                button: "Comment"
            ) { [weak self] body in
                guard let self else { return }
                do {
                    try self.session.addComment(
                        fileIndex: self.sidebarModel.selected,
                        side: target.side,
                        startLine: target.startLine,
                        endLine: target.endLine,
                        body: body)
                    self.refreshReviewState()
                } catch {
                    self.presentInfo("\(error)")
                }
            }
        }
    }

    /// Edit the draft comment at the caret.
    @objc func editComment(_ sender: Any?) {
        guard let comment = draftAtCaret() else {
            presentInfo("No draft comment here.")
            return
        }
        promptForText(
            title: "Edit comment", button: "Save", initial: comment.body
        ) { [weak self] body in
            guard let self else { return }
            if !self.session.updateComment(localID: comment.localID, body: body) {
                self.presentInfo("Could not update the comment.")
            }
            self.refreshReviewState()
        }
    }

    @objc func deleteComment(_ sender: Any?) {
        guard let comment = draftAtCaret() else {
            presentInfo("No draft comment here.")
            return
        }
        if session.deleteComment(localID: comment.localID) {
            refreshReviewState()
        }
    }

    /// Dismiss ↔ restore the draft at the caret (kept, never submitted).
    @objc func dismissComment(_ sender: Any?) {
        guard let comment = draftAtCaret() else {
            presentInfo("No draft comment here.")
            return
        }
        if session.toggleDismiss(localID: comment.localID) {
            refreshReviewState()
        }
    }

    /// Reply: to the host thread at the caret (PR mode), else to the draft
    /// conversation at the caret.
    @objc func replyAtCursor(_ sender: Any?) {
        if let thread = threadAtCaret() {
            promptForText(
                title: "Reply to @\(thread.comments.first?.author ?? "thread")",
                button: "Stage Reply"
            ) { [weak self] body in
                guard let self else { return }
                do {
                    // A thread reply anchors where the thread lives and
                    // posts individually on submit.
                    try self.session.addComment(
                        fileIndex: self.sidebarModel.selected,
                        side: thread.side,
                        startLine: thread.line ?? thread.originalLine ?? 1,
                        endLine: thread.line ?? thread.originalLine ?? 1,
                        body: body,
                        replyTo: thread.id)
                    self.refreshReviewState()
                } catch {
                    self.presentInfo("\(error)")
                }
            }
            return
        }
        guard let comment = draftAtCaret() else {
            presentInfo("No comment or thread here to reply to.")
            return
        }
        promptForText(title: "Reply", button: "Reply") { [weak self] body in
            guard let self else { return }
            _ = self.session.addReply(localID: comment.localID, body: body)
            self.refreshReviewState()
        }
    }

    @objc func exportNotes(_ sender: Any?) {
        guard let window else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "review.md"
        panel.message = "Markdown by default; a .json name writes a review-exchange document"
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let url = panel.url, let self else { return }
            do {
                try self.session.export(to: url.path)
            } catch {
                self.presentInfo("\(error)")
            }
        }
    }

    @objc func showPRInfo(_ sender: Any?) {
        guard let info = session.pullRequestInfo else {
            presentInfo("This session is not a pull request.")
            return
        }
        let alert = NSAlert()
        alert.messageText = info.title
        var details = "@\(info.author)  \(info.baseRef) ← \(info.headRef)\n\(info.url)"
        if !info.body.isEmpty {
            details += "\n\n" + info.body.prefix(2000)
        }
        alert.informativeText = details
        alert.addButton(withTitle: "OK")
        alert.addButton(withTitle: "Open in Browser")
        guard let window else { return }
        alert.beginSheetModal(for: window) { response in
            if response == .alertSecondButtonReturn, let url = URL(string: info.url) {
                NSWorkspace.shared.open(url)
            }
        }
    }

    /// The submit sheet: counts, event, summary — nothing is sent before
    /// this confirmation.
    @objc func submitReview(_ sender: Any?) {
        guard session.isPullRequest, let window else {
            presentInfo("Submitting needs a pull-request session.")
            return
        }
        let drafts = session.comments()
        let newComments = drafts.filter { $0.replyTo == nil && $0.state == .active }.count
        let replies = drafts.filter { $0.replyTo != nil && $0.state == .active }.count
        let orphaned = drafts.filter { $0.state == .orphaned }.count
        let dismissed = drafts.filter { $0.state == .dismissed }.count

        let alert = NSAlert()
        alert.messageText = "Submit review"
        var counts = "\(newComments) comment\(newComments == 1 ? "" : "s"), \(replies) repl\(replies == 1 ? "y" : "ies")"
        if dismissed > 0 { counts += "; \(dismissed) dismissed stay local" }
        if orphaned > 0 {
            counts += "\n⚠ \(orphaned) orphaned comment\(orphaned == 1 ? "" : "s") will NOT be submitted"
        }
        alert.informativeText = counts

        let accessory = NSStackView()
        accessory.orientation = .vertical
        accessory.alignment = .leading
        accessory.frame = NSRect(x: 0, y: 0, width: 420, height: 150)
        let eventPicker = NSPopUpButton(frame: .zero, pullsDown: false)
        eventPicker.addItems(withTitles: ["Comment", "Approve", "Request changes"])
        let summaryScroll = NSTextView.scrollableTextView()
        summaryScroll.frame = NSRect(x: 0, y: 0, width: 420, height: 110)
        let summaryView = summaryScroll.documentView as! NSTextView
        summaryView.string = session.summary
        summaryView.font = .systemFont(ofSize: NSFont.systemFontSize)
        accessory.addArrangedSubview(eventPicker)
        accessory.addArrangedSubview(summaryScroll)
        summaryScroll.translatesAutoresizingMaskIntoConstraints = false
        summaryScroll.heightAnchor.constraint(equalToConstant: 110).isActive = true
        summaryScroll.widthAnchor.constraint(equalToConstant: 420).isActive = true
        alert.accessoryView = accessory

        alert.addButton(withTitle: "Submit")
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { [weak self] response in
            guard response == .alertFirstButtonReturn, let self else { return }
            self.session.summary = summaryView.string
            let events: [ReviewSubmitEvent] = [.comment, .approve, .requestChanges]
            self.session.setEvent(events[eventPicker.indexOfSelectedItem])
            do {
                let result = try self.session.submit()
                self.refreshReviewState()
                if let error = result.error {
                    self.presentInfo(
                        "Posted \(result.posted); \(result.remaining) kept as drafts.\n\(error)")
                } else {
                    self.presentInfo("Review submitted (\(result.posted) item\(result.posted == 1 ? "" : "s") posted).")
                }
            } catch {
                self.presentInfo("\(error)")
            }
        }
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
        case #selector(editComment(_:)), #selector(deleteComment(_:)),
            #selector(dismissComment(_:)):
            return draftAtCaret() != nil
        case #selector(replyAtCursor(_:)):
            return threadAtCaret() != nil || draftAtCaret() != nil
        case #selector(showPRInfo(_:)), #selector(submitReview(_:)):
            return session.isPullRequest
        default:
            return true
        }
    }

    // MARK: - Caret resolution

    private var caret: Int {
        diffTextView.selectedRange().location
    }

    /// The (side, line) under the caret, from the line map or an inline box.
    private func caretTarget() -> (side: DiffSide, line: Int)? {
        guard let rendered else { return nil }
        if let annotation = rendered.annotations.first(where: { $0.range.contains(caret) }) {
            return annotation.target
        }
        guard
            let ref = rendered.lineRefs.first(where: {
                $0.range.contains(caret) || $0.range.upperBound == caret
            })
        else { return nil }
        if let newLine = ref.newLine { return (.right, newLine) }
        if let oldLine = ref.oldLine { return (.left, oldLine) }
        return nil
    }

    private func draftAtCaret() -> DraftComment? {
        guard let rendered else { return nil }
        // Inside a draft's own box, that draft wins.
        if let annotation = rendered.annotations.first(where: {
            $0.range.contains(caret) && $0.commentID != nil
        }) {
            return comments.first { $0.localID == annotation.commentID }
        }
        guard let target = caretTarget() else { return nil }
        let path = files[sidebarModel.selected].displayPath
        // Most recent first when several share a line.
        return comments.last {
            $0.location.path == path && $0.location.side == target.side
                && $0.location.startLine <= target.line && target.line <= $0.location.endLine
        }
    }

    private func threadAtCaret() -> ReviewThread? {
        guard let rendered else { return nil }
        if let annotation = rendered.annotations.first(where: {
            $0.range.contains(caret) && $0.threadID != nil
        }) {
            return threads.first { $0.id == annotation.threadID }
        }
        guard let target = caretTarget() else { return nil }
        let path = files[sidebarModel.selected].displayPath
        return threads.first {
            $0.path == path && $0.side == target.side && $0.line == target.line
        }
    }

    // MARK: - Block navigation

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
        diffTextView.setSelectedRange(range)
        diffTextView.scrollRangeToVisible(range)
    }

    // MARK: - Content

    /// Reloads review state and re-renders, keeping the caret in place.
    private func refreshReviewState() {
        comments = session.comments()
        threads = session.threads()
        updateBadges()
        let saved = diffTextView.selectedRange()
        renderCurrentFile()
        let length = diffTextView.textStorage?.length ?? 0
        let location = min(saved.location, length)
        diffTextView.setSelectedRange(NSRange(location: location, length: 0))
        diffTextView.scrollRangeToVisible(NSRange(location: location, length: 0))
    }

    private func updateBadges() {
        var counts: [String: Int] = [:]
        for comment in comments {
            counts[comment.location.path, default: 0] += 1
        }
        sidebarModel.updateDraftCounts(counts)
    }

    private func showFile(at index: Int) {
        guard files.indices.contains(index) else { return }
        sidebarModel.selected = index
        renderCurrentFile()
        diffTextView.setSelectedRange(NSRange(location: 0, length: 0))
        diffTextView.scroll(.zero)
    }

    private func renderCurrentFile() {
        let file = files[sidebarModel.selected]
        let path = file.displayPath
        let rendered = DiffRenderer.render(
            file: file,
            comments: comments.filter { $0.location.path == path },
            threads: threads.filter { $0.path == path })
        self.rendered = rendered
        diffTextView.textStorage?.setAttributedString(rendered.text)
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
        view.usesFindBar = true
        view.isIncrementalSearchingEnabled = true
        return view
    }

    // MARK: - Sheets

    /// A one-field text sheet (comment editing) with Save/Cancel.
    private func promptForText(
        title: String,
        button: String,
        initial: String = "",
        onSave: @escaping (String) -> Void
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
        alert.addButton(withTitle: button)
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { response in
            guard response == .alertFirstButtonReturn else { return }
            let body = textView.string.trimmingCharacters(in: .whitespacesAndNewlines)
            // An empty body discards, never posts an empty comment.
            if !body.isEmpty {
                onSave(body)
            }
        }
    }

    private func presentInfo(_ message: String) {
        guard let window else { return }
        let alert = NSAlert()
        alert.messageText = message
        alert.beginSheetModal(for: window)
    }
}

// MARK: - Selection resolution

/// Maps a text selection onto diff semantics: one continuous range on one
/// side (host requirements), rejected before any editor opens.
enum SelectionResolver {
    struct Target: Equatable {
        let side: DiffSide
        let startLine: Int
        let endLine: Int
    }

    struct ResolveError: Error {
        let message: String
    }

    static func resolve(
        lineRefs: [DiffRenderer.LineRef], selection: NSRange
    ) -> Result<Target, ResolveError> {
        let touched: [DiffRenderer.LineRef]
        if selection.length == 0 {
            // A caret is exactly one line: the one containing it, or —
            // at the very end of the text — the one just closed.
            if let hit = lineRefs.first(where: { $0.range.contains(selection.location) }) {
                touched = [hit]
            } else if let hit = lineRefs.last(where: {
                $0.range.upperBound == selection.location
            }) {
                touched = [hit]
            } else {
                touched = []
            }
        } else {
            touched = lineRefs.filter { NSIntersectionRange($0.range, selection).length > 0 }
        }
        guard !touched.isEmpty else {
            return .failure(ResolveError(message: "Place the cursor on a diff line to comment."))
        }
        // GitHub semantics: the comment lives on one side. A selection with
        // any addition (or only context) anchors RIGHT, using the lines
        // visible there — an intervening deletion is simply not part of
        // that side. Deletions-only selections anchor LEFT.
        let hasAddition = touched.contains { $0.kind == .addition }
        let hasDeletion = touched.contains { $0.kind == .deletion }
        let side: DiffSide = (hasDeletion && !hasAddition) ? .left : .right
        let lines = touched.compactMap { side == .left ? $0.oldLine : $0.newLine }
        guard let first = lines.min(), let last = lines.max() else {
            return .failure(
                ResolveError(
                    message: "The selection has no lines on the \(side.rawValue) side."))
        }
        return .success(Target(side: side, startLine: first, endLine: last))
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
        var drafts: Int = 0
    }

    @Published var rows: [Row]
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

    func updateDraftCounts(_ counts: [String: Int]) {
        for index in rows.indices {
            rows[index].drafts = counts[rows[index].path] ?? 0
        }
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
                if row.drafts > 0 {
                    Text("●\(row.drafts)")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.orange)
                }
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

/// One file's diff laid out as attributed text, plus the maps navigation
/// and commenting need.
struct RenderedDiff {
    let text: NSAttributedString
    /// Each hunk, from its `@@` header through its last line.
    let hunkRanges: [NSRange]
    /// Each contiguous run of added/deleted lines.
    let changeRanges: [NSRange]
    /// Every content line's character range and diff identity.
    let lineRefs: [DiffRenderer.LineRef]
    /// Inline comment/thread boxes: their ranges and what they belong to.
    let annotations: [DiffRenderer.Annotation]
}

/// Builds the attributed text for one file's unified diff, with marker
/// gutters (● drafts, ◆ threads) and inline preview boxes.
enum DiffRenderer {
    struct LineRef {
        let range: NSRange
        let kind: DiffLineKind
        let oldLine: Int?
        let newLine: Int?
    }

    struct Annotation {
        let range: NSRange
        let commentID: String?
        let threadID: Int64?
        /// The (side, line) the box anchors to, so caret actions work
        /// inside it.
        let target: (side: DiffSide, line: Int)
    }

    static func render(
        file: DiffFile,
        comments: [DraftComment] = [],
        threads: [ReviewThread] = []
    ) -> RenderedDiff {
        let font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        let result = NSMutableAttributedString()
        var hunkRanges: [NSRange] = []
        var changeRanges: [NSRange] = []
        var lineRefs: [LineRef] = []
        var annotations: [Annotation] = []
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

        /// Draft boxes and thread boxes under their anchor line.
        func appendAnnotations(side: DiffSide, line: Int) {
            for thread in threads where thread.side == side && thread.line == line {
                let start = result.length
                for (index, comment) in thread.comments.enumerated() {
                    let marker = index == 0 ? "◆" : "  ↳"
                    let date = String(comment.createdAt.prefix(10))
                    append(
                        "        \(marker) @\(comment.author)  \(date)\n",
                        color: .systemPurple)
                    for bodyLine in comment.body.split(
                        separator: "\n", omittingEmptySubsequences: false)
                    {
                        append("          \(bodyLine)\n", color: .secondaryLabelColor)
                    }
                }
                annotations.append(
                    Annotation(
                        range: NSRange(location: start, length: result.length - start),
                        commentID: nil,
                        threadID: thread.id,
                        target: (side, line)))
            }
            for comment in comments
            where comment.location.side == side && comment.location.endLine == line {
                let start = result.length
                let author = comment.author.flatMap { $0.isEmpty ? nil : $0 } ?? "me"
                let state = comment.state == .active ? "" : "  [\(comment.state.rawValue)]"
                let kindMark = comment.replyTo != nil ? "↳" : "●"
                append("        \(kindMark) @\(author)\(state)\n", color: .systemOrange)
                for bodyLine in comment.body.split(
                    separator: "\n", omittingEmptySubsequences: false)
                {
                    append("          \(bodyLine)\n", color: .secondaryLabelColor)
                }
                for reply in comment.replies ?? [] {
                    append("          ↳ @\(reply.author): \(reply.body)\n",
                           color: .secondaryLabelColor)
                }
                annotations.append(
                    Annotation(
                        range: NSRange(location: start, length: result.length - start),
                        commentID: comment.localID,
                        threadID: nil,
                        target: (side, line)))
            }
        }

        func marker(oldLine: Int?, newLine: Int?) -> String {
            let hasDraft = comments.contains { comment in
                let line = comment.location.side == .left ? oldLine : newLine
                guard let line else { return false }
                return comment.location.startLine <= line && line <= comment.location.endLine
            }
            let hasThread = threads.contains { thread in
                let line = thread.side == .left ? oldLine : newLine
                return line != nil && thread.line == line
            }
            if hasDraft { return "●" }
            if hasThread { return "◆" }
            return " "
        }

        if file.isBinary {
            append("Binary file — nothing to show.\n", color: .secondaryLabelColor)
            return RenderedDiff(
                text: result, hunkRanges: [], changeRanges: [], lineRefs: [], annotations: [])
        }
        if file.status == .renamed {
            append("renamed \(file.oldPath) → \(file.newPath)\n\n", color: .secondaryLabelColor)
        }

        for hunk in file.hunks {
            let hunkStart = result.length
            append("  \(hunk.header)\n", color: .secondaryLabelColor,
                   background: NSColor.separatorColor.withAlphaComponent(0.25))
            for line in hunk.lines {
                if line.kind == .meta {
                    closeChangeBlock()
                    append("          \(line.text)\n", color: .tertiaryLabelColor)
                    continue
                }
                let isChange = line.kind == .addition || line.kind == .deletion
                if isChange, changeStart == nil {
                    changeStart = result.length
                } else if !isChange {
                    closeChangeBlock()
                }
                let lineStart = result.length
                let gutterMark = marker(oldLine: line.oldLine, newLine: line.newLine)
                append("\(gutterMark) ", color: .systemOrange)
                append("\(pad(line.oldLine)) \(pad(line.newLine)) ", color: .tertiaryLabelColor)
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
                lineRefs.append(
                    LineRef(
                        range: NSRange(location: lineStart, length: result.length - lineStart),
                        kind: line.kind,
                        oldLine: line.oldLine,
                        newLine: line.newLine))
                // Boxes anchor under the line, RIGHT first (the common side).
                if let newLine = line.newLine, line.kind != .deletion {
                    appendAnnotations(side: .right, line: newLine)
                }
                if let oldLine = line.oldLine, line.kind == .deletion {
                    appendAnnotations(side: .left, line: oldLine)
                }
            }
            closeChangeBlock()
            hunkRanges.append(NSRange(location: hunkStart, length: result.length - hunkStart))
            append("\n", color: .labelColor)
        }
        return RenderedDiff(
            text: result,
            hunkRanges: hunkRanges,
            changeRanges: changeRanges,
            lineRefs: lineRefs,
            annotations: annotations)
    }

    private static func pad(_ number: Int?) -> String {
        guard let number else { return String(repeating: " ", count: 5) }
        return String(format: "%5d", number)
    }
}
