import AppKit
import Foundation
import PrchumKit

/// Headless verification that the shell and the core actually talk to each
/// other: a diff parses through the C boundary into typed Swift values, bad
/// input is rejected with a message instead of a crash, and an asynchronous
/// event makes it from a core thread back to the main queue.
///
/// Returns a process exit code: 0 on success.
@MainActor
func runSmokeTest() -> Int32 {
    print("prchum core \(Core.version)")

    let patch = """
        diff --git a/src/lib.rs b/src/lib.rs
        --- a/src/lib.rs
        +++ b/src/lib.rs
        @@ -1,3 +1,4 @@
         fn main() {
        -    old();
        +    new();
        +    extra();
         }
        diff --git a/gone.txt b/gone.txt
        deleted file mode 100644
        --- a/gone.txt
        +++ /dev/null
        @@ -1 +0,0 @@
        -bye
        """

    // Session round trip: parse in the core, read back typed values.
    do {
        let session = try CoreSession(title: "smoke", patch: patch)
        guard session.title == "smoke", session.fileCount == 2 else {
            print("FAIL: session shape: \(session.title), \(session.fileCount) files")
            return 1
        }
        let files = try session.files()
        guard files[0].displayPath == "src/lib.rs", files[0].status == .modified,
            files[1].displayPath == "gone.txt", files[1].status == .deleted
        else {
            print("FAIL: file identities: \(files.map(\.displayPath))")
            return 1
        }
        let counts = files[0].changeCounts
        guard counts.added == 2, counts.deleted == 1 else {
            print("FAIL: change counts: \(counts)")
            return 1
        }
        let lines = files[0].hunks[0].lines
        guard lines[0].kind == .context, lines[0].oldLine == 1, lines[0].newLine == 1,
            lines[1].kind == .deletion, lines[1].oldLine == 2, lines[1].newLine == nil,
            lines[2].kind == .addition, lines[2].newLine == 2,
            lines[2].patchPosition == 3
        else {
            print("FAIL: line model: \(lines)")
            return 1
        }
        print("session round trip ok (\(session.fileCount) files)")
    } catch {
        print("FAIL: session over a valid diff: \(error)")
        return 1
    }

    // Invalid input must be rejected with a message, not crash.
    do {
        _ = try CoreSession(title: "bad", patch: "this is not a diff")
        print("FAIL: core accepted garbage as a diff")
        return 1
    } catch let error as CoreError {
        guard !error.message.isEmpty else {
            print("FAIL: rejection carried no message")
            return 1
        }
        print("input validation ok (\(error.message))")
    } catch {
        print("FAIL: unexpected error type: \(error)")
        return 1
    }

    // Configuration: missing file = defaults, overrides load, broken file
    // warns and never aborts — all through the C boundary.
    do {
        let configDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("prchum-smoke-cfg-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(at: configDir, withIntermediateDirectories: true)
        let configPath = configDir.appendingPathComponent("config.json").path

        let fresh = CoreConfig(path: configPath)
        guard fresh.loadWarning == nil, fresh.keyOverrides.isEmpty else {
            print("FAIL: missing config was not clean defaults")
            return 1
        }

        try #"{"keys": {"next-hunk": "cmd+alt+n", "toggle-wrap": ""}, "future": 1}"#
            .write(toFile: configPath, atomically: true, encoding: .utf8)
        let loaded = CoreConfig(path: configPath)
        guard loaded.loadWarning == nil,
            loaded.keyOverrides == ["next-hunk": "cmd+alt+n", "toggle-wrap": ""]
        else {
            print("FAIL: overrides did not load: \(String(describing: loaded.loadWarning))")
            return 1
        }

        try "{ broken".write(toFile: configPath, atomically: true, encoding: .utf8)
        let broken = CoreConfig(path: configPath)
        guard broken.loadWarning != nil, broken.keyOverrides.isEmpty else {
            print("FAIL: broken config not detected or defaults not applied")
            return 1
        }
        print("configuration ok (defaults, overrides, breakage recovery)")
        try? FileManager.default.removeItem(at: configDir)
    } catch {
        print("FAIL: configuration: \(error)")
        return 1
    }

    // Keymap: chord parsing, override resolution, unbinding, bad specs.
    guard KeyChord.parse("cmd+alt+down")
        == KeyChord(
            keyEquivalent: String(Character(UnicodeScalar(NSDownArrowFunctionKey)!)),
            modifiers: [.command, .option]),
        KeyChord.parse("shift+cmd+s")
            == KeyChord(keyEquivalent: "s", modifiers: [.shift, .command]),
        KeyChord.parse("meta+x") == nil,
        KeyChord.parse("cmd+nosuchkey") == nil
    else {
        print("FAIL: key chord parsing")
        return 1
    }
    let keymap = Keymap(overrides: [
        "next-hunk": "cmd+alt+n",
        "toggle-wrap": "",
        "no-such-action": "cmd+z",
        "next-file": "cmd+???",
    ])
    guard keymap.chord(for: .nextHunk) == KeyChord.parse("cmd+alt+n"),
        keymap.chord(for: .toggleWrap) == nil,
        keymap.chord(for: .nextFile) == ActionID.nextFile.defaultChord,
        keymap.problems.count == 2
    else {
        print("FAIL: keymap resolution: \(keymap.problems)")
        return 1
    }
    guard keymap.menuItem(for: .nextHunk).keyEquivalent == "n" else {
        print("FAIL: menu item did not adopt the override")
        return 1
    }
    // Two defaults on one chord would leave one action unreachable.
    let defaults = ActionID.allCases.compactMap { action in
        action.defaultChord.map { (action, $0) }
    }
    for (index, (action, chord)) in defaults.enumerated() {
        if let clash = defaults[(index + 1)...].first(where: { $0.1 == chord }) {
            print("FAIL: \(action.rawValue) and \(clash.0.rawValue) share a default key")
            return 1
        }
    }
    print("keymap ok (overrides, unbind, defaults kept on bad specs)")

    // Rendering: navigable block ranges line up with the model.
    do {
        let session = try CoreSession(title: "blocks", patch: patch)
        let rendered = DiffRenderer.render(file: try session.file(at: 0))
        // One hunk; two change runs would merge — here the -/+/+ run is one.
        guard rendered.hunkRanges.count == 1, rendered.changeRanges.count == 1 else {
            print(
                "FAIL: block ranges: \(rendered.hunkRanges.count) hunks, \(rendered.changeRanges.count) changes"
            )
            return 1
        }
        let text = rendered.text.string as NSString
        let change = text.substring(with: rendered.changeRanges[0])
        guard change.contains("-    old();"), change.contains("+    extra();"),
            !change.contains("fn main")
        else {
            print("FAIL: change block content: \(change.debugDescription)")
            return 1
        }
        print("block ranges ok (hunks and change runs)")
    } catch {
        print("FAIL: block ranges: \(error)")
        return 1
    }

    // Comment lifecycle through the C boundary: add on a selection,
    // validate rejection, edit, dismiss, reply, export, delete — with
    // drafts persisting across session lifetimes.
    do {
        let stateDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("prchum-smoke-drafts-\(ProcessInfo.processInfo.processIdentifier)")
            .path

        let session = try CoreSession(title: "review", patch: patch)
        _ = session.attachStore(directory: stateDir)
        session.setAuthor("smoke")

        let id = try session.addComment(
            fileIndex: 0, side: .right, startLine: 2, endLine: 3, body: "why?")
        guard session.comments().count == 1 else {
            print("FAIL: comment not recorded")
            return 1
        }
        // Cross-side and out-of-diff ranges must be rejected with a message.
        do {
            _ = try session.addComment(
                fileIndex: 0, side: .left, startLine: 99, endLine: 99, body: "x")
            print("FAIL: core accepted an impossible location")
            return 1
        } catch {}

        guard session.updateComment(localID: id, body: "why though?"),
            session.addReply(localID: id, body: "checking"),
            session.toggleDismiss(localID: id)
        else {
            print("FAIL: comment mutation refused")
            return 1
        }
        guard session.comments()[0].state == .dismissed,
            session.comments()[0].replies?.count == 1
        else {
            print("FAIL: comment state after mutations: \(session.comments())")
            return 1
        }

        // A fresh session over the same content resumes the draft.
        let resumed = try CoreSession(title: "review", patch: patch)
        _ = resumed.attachStore(directory: stateDir)
        guard resumed.comments().count == 1, resumed.comments()[0].body == "why though?" else {
            print("FAIL: draft did not persist across sessions")
            return 1
        }

        // Export: markdown and exchange, by extension.
        let exportBase = FileManager.default.temporaryDirectory
            .appendingPathComponent("prchum-smoke-export-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(at: exportBase, withIntermediateDirectories: true)
        let markdownPath = exportBase.appendingPathComponent("notes.md").path
        try resumed.export(to: markdownPath)
        let markdown = try String(contentsOfFile: markdownPath, encoding: .utf8)
        guard markdown.contains("## src/lib.rs"), markdown.contains("> why though?") else {
            print("FAIL: markdown export: \(markdown)")
            return 1
        }
        let jsonPath = exportBase.appendingPathComponent("notes.json").path
        try resumed.export(to: jsonPath)
        let exchange = try String(contentsOfFile: jsonPath, encoding: .utf8)
        guard exchange.hasPrefix("{\n  \"leanreview_review\": 1") else {
            print("FAIL: exchange export: \(exchange.prefix(60))")
            return 1
        }

        guard resumed.deleteComment(localID: id), resumed.comments().isEmpty else {
            print("FAIL: delete")
            return 1
        }
        try? FileManager.default.removeItem(atPath: stateDir)
        try? FileManager.default.removeItem(at: exportBase)
        print("comment lifecycle ok (add/validate/edit/reply/dismiss/persist/export/delete)")
    } catch {
        print("FAIL: comment lifecycle: \(error)")
        return 1
    }

    // Reviewed marks: mark, persist, the sidebar's progress, the next
    // unreviewed file, and a changed file losing its mark. A patch file
    // keys its draft by path, so rewriting it reopens the same draft over
    // different changes.
    do {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("prchum-smoke-reviewed-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let stateDir = dir.appendingPathComponent("drafts").path
        let patchPath = dir.appendingPathComponent("change.diff").path
        try patch.write(toFile: patchPath, atomically: true, encoding: .utf8)

        let session = try CoreSession(contentsOf: patchPath)
        _ = session.attachStore(directory: stateDir)
        let sidebar = SidebarModel(files: try session.files())
        sidebar.updateReviewed(session.reviewedFiles())
        guard sidebar.progress == "0 of 2 reviewed",
            session.nextUnreviewedFile(from: 0) == 1,
            session.nextUnreviewedFile(from: 0, forward: false) == 1
        else {
            print("FAIL: fresh reviewed state: \(sidebar.progress)")
            return 1
        }
        guard session.setFileReviewed(at: 0, true), session.isFileReviewed(at: 0) else {
            print("FAIL: marking a file reviewed")
            return 1
        }
        guard !session.setFileReviewed(at: 9, true) else {
            print("FAIL: marked a file that does not exist")
            return 1
        }
        sidebar.updateReviewed(session.reviewedFiles())
        guard sidebar.progress == "1 of 2 reviewed", sidebar.rows[0].reviewed,
            session.nextUnreviewedFile(from: 0) == 1,
            session.nextUnreviewedFile(from: 1) == 1
        else {
            print("FAIL: after marking: \(sidebar.progress)")
            return 1
        }

        let resumed = try CoreSession(contentsOf: patchPath)
        _ = resumed.attachStore(directory: stateDir)
        guard resumed.reviewedFiles() == [true, false] else {
            print("FAIL: marks did not persist: \(resumed.reviewedFiles())")
            return 1
        }
        guard resumed.setFileReviewed(at: 1, true), resumed.nextUnreviewedFile(from: 0) == nil
        else {
            print("FAIL: all reviewed still reports a next file")
            return 1
        }

        // The deleted file's content changes; its mark lapses, the other
        // file's survives.
        try patch.replacingOccurrences(of: "-bye", with: "-farewell")
            .write(toFile: patchPath, atomically: true, encoding: .utf8)
        let changed = try CoreSession(contentsOf: patchPath)
        _ = changed.attachStore(directory: stateDir)
        guard changed.reviewedFiles() == [true, false],
            changed.nextUnreviewedFile(from: 0) == 1
        else {
            print("FAIL: fingerprint invalidation: \(changed.reviewedFiles())")
            return 1
        }
        try? FileManager.default.removeItem(at: dir)
        print("reviewed files ok (mark/persist/progress/next/invalidation)")
    } catch {
        print("FAIL: reviewed files: \(error)")
        return 1
    }

    // An exchange document opens as an exchange session (sniffed by
    // content) and triage rewrites it in place.
    do {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("prchum-smoke-exch-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let path = dir.appendingPathComponent("loop.review.json").path
        try #"{"leanreview_review": 1, "title": "loop", "patch": ["--- a/x.rs", "+++ b/x.rs", "@@ -1,2 +1,2 @@", " context", "-a", "+b"], "comments": [{"id": "c1", "author": "assistant", "path": "x.rs", "side": "RIGHT", "start_line": 2, "end_line": 2, "body": "why b?", "state": "active"}]}"#
            .write(toFile: path, atomically: true, encoding: .utf8)

        let session = try CoreSession(contentsOf: path)
        session.setAuthor("smoke")
        guard session.title == "loop", session.comments().count == 1 else {
            print("FAIL: exchange session shape")
            return 1
        }
        guard session.addReply(localID: "c1", body: "because a was wrong") else {
            print("FAIL: exchange reply refused")
            return 1
        }
        let rewritten = try String(contentsOfFile: path, encoding: .utf8)
        guard rewritten.contains("because a was wrong") else {
            print("FAIL: exchange writeback did not happen")
            return 1
        }
        try? FileManager.default.removeItem(at: dir)
        print("exchange session ok (content sniffing, triage, in-place writeback)")
    } catch {
        print("FAIL: exchange session: \(error)")
        return 1
    }

    // A local git comparison through the C boundary.
    do {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("prchum-smoke-git-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        func git(_ arguments: [String]) throws {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = ["git", "-C", dir.path] + arguments
            process.standardOutput = FileHandle.nullDevice
            process.standardError = FileHandle.nullDevice
            try process.run()
            process.waitUntilExit()
        }
        try git(["init", "-q", "-b", "main"])
        try git(["config", "user.name", "Smoke"])
        try git(["config", "user.email", "smoke@example.com"])
        try "one\ntwo\n".write(
            to: dir.appendingPathComponent("f.txt"), atomically: true, encoding: .utf8)
        try git(["add", "."])
        try git(["commit", "-q", "-m", "init"])
        try "one\nchanged\n".write(
            to: dir.appendingPathComponent("f.txt"), atomically: true, encoding: .utf8)

        let session = try CoreSession(gitRepo: dir.path, comparison: .workingTree)
        guard session.fileCount == 1,
            try session.file(at: 0).displayPath == "f.txt",
            session.title.contains("working tree")
        else {
            print("FAIL: git session shape: \(session.title)")
            return 1
        }

        // The context view: the whole file with the hunks overlaid, gap
        // lines carrying both numbers, content verified against the diff.
        try "zero\none\nchanged\ntail\n".write(
            to: dir.appendingPathComponent("f.txt"), atomically: true, encoding: .utf8)
        let fresh = try CoreSession(gitRepo: dir.path, comparison: .workingTree)
        let context = try fresh.contextFile(at: 0)
        let allLines = context.hunks.flatMap(\.lines)
        guard allLines.contains(where: { $0.text == "zero" }),
            allLines.contains(where: { $0.text == "tail" }),
            allLines.contains(where: { $0.kind == .deletion })
        else {
            print("FAIL: context projection: \(allLines.map(\.text))")
            return 1
        }

        // The projection highlights over its own hunks: gap lines (zero
        // and tail, outside the diff) carry spans too.
        // Long enough that -U3 leaves gap regions above and below the
        // hunk.
        let before = (1...12).map { "let line\($0) = \($0);" }
        try (before.joined(separator: "\n") + "\n").write(
            to: dir.appendingPathComponent("f.rs"), atomically: true, encoding: .utf8)
        try git(["add", "."])
        try git(["commit", "-q", "-m", "rust file"])
        var after = before
        after[5] = "let altered = 6;"
        try (after.joined(separator: "\n") + "\n").write(
            to: dir.appendingPathComponent("f.rs"), atomically: true, encoding: .utf8)
        let rustSession = try CoreSession(gitRepo: dir.path, comparison: .workingTree)
        let rustIndex = try (0..<rustSession.fileCount).first {
            try rustSession.file(at: $0).displayPath == "f.rs"
        }!
        let projection = try rustSession.contextFile(at: rustIndex)
        guard let contextSpans = rustSession.contextHighlights(at: rustIndex) else {
            print("FAIL: no context highlights for a rust file")
            return 1
        }
        guard contextSpans.count == projection.hunks.count else {
            print("FAIL: context spans shape: \(contextSpans.count) vs \(projection.hunks.count)")
            return 1
        }
        // A gap hunk (empty header) must carry spans — `let` is a keyword.
        guard
            let gapIndex = projection.hunks.firstIndex(where: { $0.header.isEmpty }),
            contextSpans[gapIndex].contains(where: { !$0.isEmpty })
        else {
            print("FAIL: gap regions carry no highlights")
            return 1
        }

        // A patch session has no content to fetch — a plain error, not a
        // crash.
        let patchOnly = try CoreSession(title: "p", patch: patch)
        do {
            _ = try patchOnly.contextFile(at: 0)
            print("FAIL: patch session offered a context view")
            return 1
        } catch {}

        try? FileManager.default.removeItem(at: dir)
        print("git session ok (worktree comparison, context projection)")
    } catch {
        print("FAIL: git session: \(error)")
        return 1
    }

    // Selection resolution: one continuous range on one side, enforced
    // before any editor opens.
    do {
        let session = try CoreSession(title: "sel", patch: patch)
        let rendered = DiffRenderer.render(file: try session.file(at: 0))
        let deletion = rendered.lineRefs.first { $0.kind == .deletion }!
        let addition = rendered.lineRefs.first { $0.kind == .addition }!
        let context = rendered.lineRefs.first { $0.kind == .context }!

        guard case .success(let single) = SelectionResolver.resolve(
            lineRefs: rendered.lineRefs,
            selection: NSRange(location: addition.range.location, length: 0)),
            single.side == .right, single.startLine == 2
        else {
            print("FAIL: caret resolution")
            return 1
        }
        guard case .success(let left) = SelectionResolver.resolve(
            lineRefs: rendered.lineRefs,
            selection: NSRange(location: deletion.range.location, length: 1)),
            left.side == .left, left.startLine == 2
        else {
            print("FAIL: deletion resolves LEFT")
            return 1
        }
        // Context + addition selection spans both rows on the RIGHT side.
        let union = NSUnionRange(context.range, addition.range)
        guard case .success(let multi) = SelectionResolver.resolve(
            lineRefs: rendered.lineRefs, selection: union),
            multi.side == .right, multi.startLine == 1
        else {
            print("FAIL: multi-line resolution")
            return 1
        }
        // A changed block (deletion + addition) anchors RIGHT — the
        // deletion is not part of that side, GitHub-style.
        let block = NSUnionRange(deletion.range, addition.range)
        guard case .success(let blockTarget) = SelectionResolver.resolve(
            lineRefs: rendered.lineRefs, selection: block),
            blockTarget.side == .right, blockTarget.startLine == 2
        else {
            print("FAIL: changed-block resolution")
            return 1
        }
        // Context + deletion (no additions) anchors LEFT.
        let leftSpan = NSUnionRange(context.range, deletion.range)
        guard case .success(let leftTarget) = SelectionResolver.resolve(
            lineRefs: rendered.lineRefs, selection: leftSpan),
            leftTarget.side == .left, leftTarget.startLine == 1, leftTarget.endLine == 2
        else {
            print("FAIL: left-span resolution")
            return 1
        }
        print("selection resolution ok (caret, sides, changed blocks)")
    } catch {
        print("FAIL: selection resolution: \(error)")
        return 1
    }

    // Rendering with review state: markers in the gutter, framed boxes
    // with Markdown bodies, threaded replies, and action links.
    do {
        let session = try CoreSession(title: "marks", patch: patch)
        session.setAuthor("smoke")
        let id = try session.addComment(
            fileIndex: 0, side: .right, startLine: 2, endLine: 2,
            body: "note **here** with `code`")
        _ = session.addReply(localID: id, body: "threaded answer")
        let threadJSON = #"""
            {"id": 7, "path": "src/lib.rs", "side": "RIGHT", "line": 3,
             "outdated": false, "comments": [
               {"id": 7, "author": "alice", "body": "root question",
                "created_at": "2026-01-02T03:04:05Z", "url": ""},
               {"id": 8, "author": "bob", "body": "root answer",
                "created_at": "2026-01-03T03:04:05Z", "url": ""}]}
            """#
        let thread = try JSONDecoder().decode(
            ReviewThread.self, from: Data(threadJSON.utf8))
        let rendered = DiffRenderer.render(
            file: try session.file(at: 0),
            comments: session.comments(),
            threads: [thread])
        let text = rendered.text.string
        guard text.contains("●"), text.contains("@smoke"),
            text.contains("threaded answer"),
            text.contains("◆ @alice"), text.contains("↳ @bob"),
            text.contains("root answer"),
            rendered.annotations.count == 2
        else {
            print("FAIL: markers/boxes/threading missing")
            return 1
        }
        // Markdown applied: the fence markers are gone from the display.
        guard !text.contains("**here**"), text.contains("here") else {
            print("FAIL: Markdown not rendered in the box")
            return 1
        }
        // Action links present: reply on both kinds, edit on the draft.
        var links: [String] = []
        rendered.text.enumerateAttribute(
            .link, in: NSRange(location: 0, length: rendered.text.length)
        ) { value, _, _ in
            if let value = value as? String { links.append(value) }
        }
        guard links.contains("prchum-act://reply-thread/7"),
            links.contains("prchum-act://reply-draft/\(id)"),
            links.contains("prchum-act://edit-draft/\(id)")
        else {
            print("FAIL: action links: \(links)")
            return 1
        }
        print("annotated rendering ok (framed boxes, Markdown, threads, links)")

        // Outdated and resolved threads stay readable: the outdated one
        // heads the file (its line is gone) and opens in the reader; the
        // resolved one collapses to a line under its anchor and expands.
        let moreJSON = #"""
            [{"id": 9, "path": "src/lib.rs", "side": "RIGHT", "line": null,
              "original_line": 40, "outdated": true, "resolved": false,
              "placement": "list_only", "comments": [
               {"id": 9, "author": "dana", "body": "stale remark",
                "created_at": "2026-01-01T00:00:00Z", "url": ""}]},
             {"id": 10, "path": "src/lib.rs", "side": "RIGHT", "line": 1,
              "outdated": false, "resolved": true, "placement": "collapsed",
              "comments": [
               {"id": 10, "author": "carol", "body": "settled question",
                "created_at": "2026-01-01T00:00:00Z", "url": ""},
               {"id": 11, "author": "alice", "body": "settled answer",
                "created_at": "2026-01-01T00:00:00Z", "url": ""}]}]
            """#
        let more = try JSONDecoder().decode([ReviewThread].self, from: Data(moreJSON.utf8))
        guard thread.placement == .inline, !thread.resolved,
            more[0].placement == .listOnly, more[1].placement == .collapsed,
            more[0].stateLabels == ["outdated"], more[1].stateLabels == ["resolved"]
        else {
            print("FAIL: thread placement/state decoding")
            return 1
        }
        func linkTargets(in rendered: RenderedDiff) -> [String] {
            var found: [String] = []
            rendered.text.enumerateAttribute(
                .link, in: NSRange(location: 0, length: rendered.text.length)
            ) { value, _, _ in
                if let value = value as? String { found.append(value) }
            }
            return found
        }
        let folded = DiffRenderer.render(
            file: try session.file(at: 0), threads: [thread] + more)
        let foldedText = folded.text.string
        let listed = folded.annotations.first { $0.threadID == 9 }
        let collapsed = folded.annotations.first { $0.threadID == 10 }
        guard foldedText.hasPrefix("1 outdated thread"),
            foldedText.contains("◆ outdated · @dana · was L40 · 1 comment — stale remark"),
            foldedText.contains("◆ resolved · @carol · 2 comments — settled question"),
            !foldedText.contains("settled answer"),
            let listed, listed.target == nil,
            let collapsed, collapsed.target?.side == .right, collapsed.target?.line == 1,
            // The collapsed box sits under its line, after the file's head.
            collapsed.range.location > listed.range.location,
            linkTargets(in: folded).contains("prchum-act://read-thread/9"),
            linkTargets(in: folded).contains("prchum-act://expand-thread/10")
        else {
            print("FAIL: outdated/resolved threads: \(foldedText)")
            return 1
        }
        let opened = DiffRenderer.render(
            file: try session.file(at: 0), threads: [thread] + more, expandedThreads: [10])
        let openedText = opened.text.string
        guard openedText.contains("◆ @carol · 2026-01-01 · resolved"),
            openedText.contains("settled answer"),
            linkTargets(in: opened).contains("prchum-act://collapse-thread/10"),
            linkTargets(in: opened).contains("prchum-act://reply-thread/10"),
            // The outdated thread never lands on a line of today's diff.
            !openedText.contains("stale remark\n")
        else {
            print("FAIL: expanded resolved thread: \(openedText)")
            return 1
        }
        print("outdated and resolved threads ok (listed, collapsed, expandable)")
    } catch {
        print("FAIL: annotated rendering: \(error)")
        return 1
    }

    // Syntax highlighting: the style table crosses the FFI, a rust diff
    // gets spans on both sides, offsets convert to UTF-16, and unknown
    // languages report nil instead of empty.
    do {
        guard !CoreSyntax.styles.isEmpty else {
            print("FAIL: style table is empty")
            return 1
        }
        let rust = try CoreSession(
            title: "hl",
            patch: "--- a/x.rs\n+++ b/x.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    let a = \"x\";\n+    let b = \"y\";\n }\n"
        )
        guard let highlights = rust.fileHighlights(at: 0), highlights.count == 1 else {
            print("FAIL: no highlights for a rust file")
            return 1
        }
        let lines = highlights[0]
        guard lines.count == 4,
            !lines[0].isEmpty,  // context `fn main() {`
            !lines[1].isEmpty,  // deletion (LEFT pass)
            !lines[2].isEmpty   // addition (RIGHT pass)
        else {
            print("FAIL: highlight coverage: \(lines.map(\.count))")
            return 1
        }
        for line in lines {
            for span in line {
                guard span.styleIndex < CoreSyntax.styles.count, span.startByte < span.endByte
                else {
                    print("FAIL: span out of table bounds")
                    return 1
                }
            }
        }
        guard DiffRenderer.utf16Range(ofUTF8: 0..<4, in: "🎉ab") == NSRange(location: 0, length: 2),
            DiffRenderer.utf16Range(ofUTF8: 4..<6, in: "🎉ab") == NSRange(location: 2, length: 2)
        else {
            print("FAIL: UTF-8 → UTF-16 conversion")
            return 1
        }

        let unknown = try CoreSession(
            title: "hl2", patch: "--- a/d.bin\n+++ b/d.bin\n@@ -1 +1 @@\n-a\n+b\n")
        guard unknown.fileHighlights(at: 0) == nil else {
            print("FAIL: unknown language should have no highlights")
            return 1
        }
        print("syntax highlighting ok (\(CoreSyntax.styles.count) styles, both sides)")
    } catch {
        print("FAIL: syntax highlighting: \(error)")
        return 1
    }

    // Folding: a folded hunk collapses to its annotated header, its lines
    // leave the row model, and other hunks keep their identity.
    do {
        let session = try CoreSession(title: "fold", patch: patch)
        let file = try session.file(at: 0)
        let folded = DiffRenderer.render(file: file, foldedHunks: [0])
        guard folded.text.string.contains("▸"),
            folded.text.string.contains("(5 lines)"),
            folded.hunkRanges.count == file.hunks.count,
            folded.lineRefs.isEmpty
        else {
            print("FAIL: folded rendering: \(folded.text.string.prefix(80))")
            return 1
        }
        let expanded = DiffRenderer.render(file: file)
        guard expanded.text.string.contains("▾"), !expanded.lineRefs.isEmpty else {
            print("FAIL: expanded rendering")
            return 1
        }
        print("folding ok (collapse to header, row model shrinks)")
    } catch {
        print("FAIL: folding: \(error)")
        return 1
    }

    // Split layout: paired rows, per-panel line refs, sides preserved.
    do {
        let session = try CoreSession(title: "split", patch: patch)
        let file = try session.file(at: 0)
        let split = DiffRenderer.render(file: file, layout: .split)
        guard split.text.string.contains("│") else {
            print("FAIL: no split divider")
            return 1
        }
        // First hunk: ctx, -old, +new, +extra, ctx → rows: ctx, (-old|+new),
        // (blank|+extra), ctx. Refs: 2 per full row, 1 for the half row.
        let hunkRefs = split.lineRefs.filter {
            split.hunkRanges[0].contains($0.range.location)
        }
        guard hunkRefs.count == 7 else {
            print("FAIL: split refs: \(hunkRefs.count)")
            return 1
        }
        let deletions = hunkRefs.filter { $0.kind == .deletion }
        let additions = hunkRefs.filter { $0.kind == .addition }
        guard deletions.count == 1, additions.count == 2 else {
            print("FAIL: split pairing")
            return 1
        }
        // The deletion's ref sits left of its paired addition's.
        guard deletions[0].range.location < additions[0].range.location,
            additions[0].range.location < deletions[0].range.location + 200
        else {
            print("FAIL: split panel geometry")
            return 1
        }
        // A caret in the deletion half resolves LEFT; in the addition
        // half, RIGHT.
        guard case .success(let leftHit) = SelectionResolver.resolve(
            lineRefs: split.lineRefs,
            selection: NSRange(location: deletions[0].range.location + 3, length: 0)),
            leftHit.side == .left, leftHit.startLine == 2,
            case .success(let rightHit) = SelectionResolver.resolve(
                lineRefs: split.lineRefs,
                selection: NSRange(location: additions[0].range.location + 3, length: 0)),
            rightHit.side == .right, rightHit.startLine == 2
        else {
            print("FAIL: split side resolution")
            return 1
        }
        print("split layout ok (pairing, per-panel refs, side resolution)")
    } catch {
        print("FAIL: split layout: \(error)")
        return 1
    }

    // Clipboard prefill: PR-looking references only, never random text.
    guard AppDelegate.looksLikePullRequestReference("https://github.com/o/r/pull/418"),
        AppDelegate.looksLikePullRequestReference(
            "https://gitlab.com/g/r/-/merge_requests/42"),
        AppDelegate.looksLikePullRequestReference("owner/repo#7"),
        AppDelegate.looksLikePullRequestReference("group/sub/repo!9"),
        !AppDelegate.looksLikePullRequestReference("418"),
        !AppDelegate.looksLikePullRequestReference("https://github.com/o/r"),
        !AppDelegate.looksLikePullRequestReference("see issue #7 for details"),
        !AppDelegate.looksLikePullRequestReference("a\nb#3")
    else {
        print("FAIL: PR-reference sniffing")
        return 1
    }
    print("clipboard prefill sniffing ok")

    // Themes: built-ins switch the table, user JSON applies over the
    // default, breakage keeps the default, and config writes preserve
    // unknown keys.
    do {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("prchum-smoke-theme-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(
            at: dir.appendingPathComponent("themes"), withIntermediateDirectories: true)
        let configPath = dir.appendingPathComponent("config.json").path
        let defaultStyles = CoreSyntax.styles
        guard !defaultStyles.isEmpty else {
            print("FAIL: empty style table")
            return 1
        }

        try #"{"future": true, "theme": "high-contrast"}"#
            .write(toFile: configPath, atomically: true, encoding: .utf8)
        guard CoreSyntax.applyTheme(configPath: configPath) == nil,
            CoreSyntax.styles[0].light != defaultStyles[0].light
        else {
            print("FAIL: high-contrast did not switch the table")
            return 1
        }

        try ##"{"styles": {"keyword": {"light": "#123456"}}}"##
            .write(
                toFile: dir.appendingPathComponent("themes/mine.json").path,
                atomically: true, encoding: .utf8)
        _ = CoreConfig.setString("theme", "mine", path: configPath)
        guard CoreSyntax.applyTheme(configPath: configPath) == nil else {
            print("FAIL: user theme did not apply")
            return 1
        }
        let written = try String(contentsOfFile: configPath, encoding: .utf8)
        guard written.contains("future") else {
            print("FAIL: config write dropped unknown keys")
            return 1
        }

        _ = CoreConfig.setString("theme", "no-such-theme", path: configPath)
        guard CoreSyntax.applyTheme(configPath: configPath) != nil else {
            print("FAIL: a missing theme applied silently")
            return 1
        }

        _ = CoreConfig.setString("theme", "default", path: configPath)
        guard CoreSyntax.applyTheme(configPath: configPath) == nil,
            CoreSyntax.styles[0].light == defaultStyles[0].light
        else {
            print("FAIL: returning to the default palette")
            return 1
        }
        try? FileManager.default.removeItem(at: dir)
        print("themes ok (built-ins, user JSON, breakage, config writes)")
    } catch {
        print("FAIL: themes: \(error)")
        return 1
    }

    // Discovery filters: named filters load, map entries write through
    // preserving the rest of the file, removal works.
    do {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("prchum-smoke-flt-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let path = dir.appendingPathComponent("config.json").path
        try #"{"future": 1, "list_filter": "is:open review-requested:@me"}"#
            .write(toFile: path, atomically: true, encoding: .utf8)

        _ = CoreConfig.setMapEntry("list_filters", "bugs", "is:open label:bug", path: path)
        _ = CoreConfig.setMapEntry("list_filters", "gone", "x", path: path)
        _ = CoreConfig.setMapEntry("list_filters", "gone", "", path: path)
        let config = CoreConfig(path: path)
        guard config.listFilters == ["bugs": "is:open label:bug"],
            config.listFilter == "is:open review-requested:@me",
            try String(contentsOfFile: path, encoding: .utf8).contains("future")
        else {
            print("FAIL: filter config round trip: \(config.listFilters)")
            return 1
        }
        try? FileManager.default.removeItem(at: dir)
        print("discovery filters ok (named, default, removal, preservation)")
    } catch {
        print("FAIL: discovery filters: \(error)")
        return 1
    }

    // Local editing: the editor invocation for both template kinds, and
    // a git comparison answering with its own checkout.
    do {
        guard case .url(let url)? = CoreEditor.invocation(
            template: "", path: "/tmp/a b.rs", line: 42, directory: "/tmp"),
            url.hasPrefix("textchum://open?path="),
            url.contains("%2Ftmp%2Fa%20b.rs"),
            url.hasSuffix("&line=42")
        else {
            print("FAIL: default editor invocation")
            return 1
        }
        guard case .command(let program, let args)? = CoreEditor.invocation(
            template: "code -g {path}:{line}", path: "/tmp/a.rs", line: 7, directory: "/tmp"),
            program == "code", args == ["-g", "/tmp/a.rs:7"]
        else {
            print("FAIL: command editor invocation")
            return 1
        }

        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("prchum-smoke-wt-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        func git(_ arguments: [String]) throws {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = ["git", "-C", dir.path] + arguments
            process.standardOutput = FileHandle.nullDevice
            process.standardError = FileHandle.nullDevice
            try process.run()
            process.waitUntilExit()
        }
        try git(["init", "-q", "-b", "main"])
        try git(["config", "user.name", "Smoke"])
        try git(["config", "user.email", "smoke@example.com"])
        try "one\ntwo\n".write(
            to: dir.appendingPathComponent("f.txt"), atomically: true, encoding: .utf8)
        try git(["add", "."])
        try git(["commit", "-q", "-m", "init"])
        try "one\nchanged\n".write(
            to: dir.appendingPathComponent("f.txt"), atomically: true, encoding: .utf8)

        // A comparison is already a checkout: its own root, unmanaged.
        let gitSession = try CoreSession(gitRepo: dir.path, comparison: .workingTree)
        let worktree = try gitSession.localWorktree(clone: "")
        guard !worktree.created,
            try FileManager.default.contentsOfDirectory(atPath: worktree.path)
                .contains("f.txt")
        else {
            print("FAIL: git comparison worktree: \(worktree)")
            return 1
        }
        guard gitSession.repoSlug.isEmpty else {
            print("FAIL: a git comparison has no forge slug")
            return 1
        }

        // A patch has no repository at all — an error, not a crash.
        let patchSession = try CoreSession(title: "p", patch: patch)
        do {
            _ = try patchSession.localWorktree(clone: dir.path)
            print("FAIL: a patch offered a worktree")
            return 1
        } catch {}

        try? FileManager.default.removeItem(at: dir)
        print("local editing ok (invocations, comparison checkout, refusals)")
    } catch {
        print("FAIL: local editing: \(error)")
        return 1
    }

    // Commit review: a pull request opened through a scripted `gh` on the
    // PATH (no network), one of its commits opened as a review of its
    // own, drafts kept apart, and the submission pinned to the commit.
    do {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("prchum-smoke-commit-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let first = String(repeating: "1", count: 40)
        let second = String(repeating: "2", count: 40)
        func write(_ name: String, _ text: String) throws {
            try text.write(
                to: dir.appendingPathComponent(name), atomically: true, encoding: .utf8)
        }
        let commitDiff = """
            diff --git a/b.txt b/b.txt
            --- a/b.txt
            +++ b/b.txt
            @@ -1,2 +1,2 @@
             keep
            -old
            +new

            """
        try write("commit.diff", commitDiff)
        try write(
            "pr.diff",
            """
            diff --git a/a.txt b/a.txt
            --- a/a.txt
            +++ b/a.txt
            @@ -1 +1 @@
            -x
            +y

            """ + commitDiff)
        try write(
            "pr.json",
            """
            {"number": 7, "state": "open", "title": "Smoke request", "body": "",
             "user": {"login": "al"}, "html_url": "https://github.com/o/r/pull/7",
             "head": {"sha": "\(second)", "ref": "feat"}, "base": {"ref": "main"}}
            """)
        // Newest first on purpose: the core puts them in history order.
        try write(
            "commits.json",
            """
            [{"sha": "\(second)", "parents": [{"sha": "\(first)"}],
              "commit": {"message": "Second\\n\\nbody", "author": {"name": "Bo", "date": "d2"}}},
             {"sha": "\(first)", "parents": [{"sha": "\(String(repeating: "0", count: 40))"}],
              "commit": {"message": "First", "author": {"name": "Al", "date": "d1"}}}]
            """)
        try write(
            "gh",
            """
            #!/bin/sh
            dir="$(dirname "$0")"
            case "$*" in
              *pulls/7/commits*) cat "$dir/commits.json" ;;
              *repos/o/r/commits/*) cat "$dir/commit.diff" ;;
              *pulls/7/comments*|*issues/7/comments*) echo '[]' ;;
              *pulls/7/reviews*) cat > "$dir/review.json"; echo '{}' ;;
              *vnd.github.v3.diff*) cat "$dir/pr.diff" ;;
              *pulls/7*) cat "$dir/pr.json" ;;
              *) echo "unexpected: $*" >&2; exit 1 ;;
            esac

            """)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755], ofItemAtPath: dir.appendingPathComponent("gh").path)
        let configPath = dir.appendingPathComponent("config.json").path
        try "{}".write(toFile: configPath, atomically: true, encoding: .utf8)
        let drafts = dir.appendingPathComponent("drafts").path

        let savedPath = ProcessInfo.processInfo.environment["PATH"] ?? ""
        setenv("PATH", "\(dir.path):\(savedPath)", 1)
        defer { setenv("PATH", savedPath, 1) }

        let whole = try CoreSession(pullRequest: "o/r#7", configPath: configPath)
        whole.attachStore(directory: drafts)
        let listing = try whole.commits()
        guard listing.commits.map(\.sha) == [first, second],
            listing.commits[1].title == "Second", listing.notice.isEmpty,
            listing.current.isEmpty, whole.reviewedCommit == nil
        else {
            print("FAIL: commit listing: \(listing)")
            return 1
        }

        let one = try CoreSession(commit: second, of: whole)
        one.attachStore(directory: drafts)
        guard one.title == "o/r#7 @ 2222222: Second", one.reviewedCommit?.sha == second,
            try one.files().map(\.displayPath) == ["b.txt"], one.threads().isEmpty,
            one.isPullRequest
        else {
            print("FAIL: commit session: \(one.title)")
            return 1
        }
        try one.addComment(fileIndex: 0, side: .right, startLine: 2, endLine: 2, body: "here")
        let counted = try whole.commits()
        guard counted.commits[1].drafts == 1, counted.commits[0].drafts == 0,
            counted.drafts == 0, whole.comments().isEmpty
        else {
            print("FAIL: drafts are not kept per commit: \(counted)")
            return 1
        }

        // Back to the whole request, from the commit.
        let again = try CoreSession(commit: "", of: one)
        guard again.reviewedCommit == nil, again.title == "o/r#7: Smoke request",
            again.fileCount == 2
        else {
            print("FAIL: back to all changes: \(again.title)")
            return 1
        }

        let result = try one.submit()
        let posted = try JSONSerialization.jsonObject(
            with: Data(contentsOf: dir.appendingPathComponent("review.json")))
            as? [String: Any]
        let comments = posted?["comments"] as? [[String: Any]]
        guard result.posted == 1, result.error == nil,
            posted?["commit_id"] as? String == second,
            comments?.first?["line"] as? Int == 2, comments?.first?["path"] as? String == "b.txt"
        else {
            print("FAIL: commit submission: \(result), \(String(describing: posted))")
            return 1
        }
        let resumed = try CoreSession(commit: second, of: whole)
        resumed.attachStore(directory: drafts)
        guard resumed.comments().isEmpty else {
            print("FAIL: a submitted commit draft came back")
            return 1
        }

        do {
            _ = try CoreSession(commit: String(repeating: "9", count: 40), of: whole)
            print("FAIL: a sha outside the request opened")
            return 1
        } catch {}
        print("commit review ok (listing, own drafts, back to all, pinned submission)")
    } catch {
        print("FAIL: commit review: \(error)")
        return 1
    }

    // Async event round trip: core dispatch thread → main queue.
    var receivedSequence: UInt64?
    let coreApp = CoreApp { event in
        if case let .pong(sequence) = event {
            receivedSequence = sequence
            CFRunLoopStop(CFRunLoopGetMain())
        }
    }
    coreApp.ping(sequence: 42)
    // Give the main run loop up to five seconds to receive the pong.
    let outcome = CFRunLoopRunInMode(.defaultMode, 5.0, false)
    guard receivedSequence == 42, outcome == .stopped else {
        print("FAIL: pong not delivered (got \(String(describing: receivedSequence)))")
        return 1
    }
    print("event round trip ok (pong 42)")

    print("smoke test passed")
    return 0
}
