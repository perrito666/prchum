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
        try? FileManager.default.removeItem(at: dir)
        print("git session ok (worktree comparison through the core)")
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

    // Rendering with review state: markers in the gutter, inline boxes.
    do {
        let session = try CoreSession(title: "marks", patch: patch)
        _ = try session.addComment(
            fileIndex: 0, side: .right, startLine: 2, endLine: 2, body: "note here")
        let rendered = DiffRenderer.render(
            file: try session.file(at: 0), comments: session.comments())
        let text = rendered.text.string
        guard text.contains("●"), text.contains("note here"),
            rendered.annotations.count == 1,
            rendered.annotations[0].commentID != nil
        else {
            print("FAIL: markers/boxes missing")
            return 1
        }
        print("annotated rendering ok (gutter marker + inline box)")
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
