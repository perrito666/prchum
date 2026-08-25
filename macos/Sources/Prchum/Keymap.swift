import AppKit
import PrchumKit

/// A menu key equivalent: the character plus its modifier mask.
struct KeyChord: Equatable {
    let keyEquivalent: String
    let modifiers: NSEvent.ModifierFlags

    static func == (lhs: KeyChord, rhs: KeyChord) -> Bool {
        lhs.keyEquivalent == rhs.keyEquivalent && lhs.modifiers == rhs.modifiers
    }

    /// Parses a config key spec like `"cmd+alt+down"` or `"shift+cmd+s"`.
    ///
    /// Modifiers: `cmd`/`command`, `alt`/`opt`/`option`, `ctrl`/`control`,
    /// `shift`. The last token is the key: a single character or a named
    /// key (`up`, `down`, `left`, `right`, `pageup`, `pagedown`, `home`,
    /// `end`, `return`, `space`, `tab`, `esc`, `delete`).
    static func parse(_ spec: String) -> KeyChord? {
        let tokens = spec.lowercased().split(separator: "+").map(String.init)
        guard let keyToken = tokens.last, !keyToken.isEmpty else { return nil }

        var modifiers: NSEvent.ModifierFlags = []
        for token in tokens.dropLast() {
            switch token {
            case "cmd", "command": modifiers.insert(.command)
            case "alt", "opt", "option": modifiers.insert(.option)
            case "ctrl", "control": modifiers.insert(.control)
            case "shift": modifiers.insert(.shift)
            default: return nil
            }
        }

        let named: [String: Int] = [
            "up": NSUpArrowFunctionKey,
            "down": NSDownArrowFunctionKey,
            "left": NSLeftArrowFunctionKey,
            "right": NSRightArrowFunctionKey,
            "pageup": NSPageUpFunctionKey,
            "pagedown": NSPageDownFunctionKey,
            "home": NSHomeFunctionKey,
            "end": NSEndFunctionKey,
        ]
        let literal: [String: String] = [
            "return": "\r",
            "enter": "\r",
            "space": " ",
            "tab": "\t",
            "esc": "\u{1B}",
            "escape": "\u{1B}",
            "delete": "\u{7F}",
        ]
        if let code = named[keyToken], let scalar = UnicodeScalar(code) {
            return KeyChord(keyEquivalent: String(Character(scalar)), modifiers: modifiers)
        }
        if let character = literal[keyToken] {
            return KeyChord(keyEquivalent: character, modifiers: modifiers)
        }
        guard keyToken.count == 1 else { return nil }
        return KeyChord(keyEquivalent: keyToken, modifiers: modifiers)
    }
}

/// Every user-invokable operation, by its stable config name.
///
/// The registry is the contract with config.json: `keys` maps these names
/// to key specs, and every action is also a menu item, so nothing is
/// keyboard-only or mouse-only.
enum ActionID: String, CaseIterable {
    case open = "open"
    case openPullRequest = "open-pr"
    case openGitComparison = "open-git"
    case reviewQueue = "review-queue"
    case exportNotes = "export"
    case nextChange = "next-change"
    case previousChange = "prev-change"
    case nextHunk = "next-hunk"
    case previousHunk = "prev-hunk"
    case nextFile = "next-file"
    case previousFile = "prev-file"
    case toggleSidebar = "toggle-sidebar"
    case toggleWrap = "toggle-wrap"
    case toggleLayout = "toggle-layout"
    case toggleContext = "toggle-context"
    case toggleSyntax = "toggle-syntax"
    case toggleFold = "toggle-fold"
    case expandAll = "expand-all"
    case collapseAll = "collapse-all"
    case find = "find"
    case comment = "comment"
    case editComment = "edit-comment"
    case deleteComment = "delete-comment"
    case dismissComment = "dismiss-comment"
    case reply = "reply"
    case suggest = "suggest"
    case commentList = "comments"
    case prInfo = "pr-info"
    case submit = "submit"

    var title: String {
        switch self {
        case .open: return "Open…"
        case .openPullRequest: return "Open Pull Request…"
        case .openGitComparison: return "Review Git Repository…"
        case .reviewQueue: return "My Review Queue"
        case .exportNotes: return "Export Notes…"
        case .nextChange: return "Next Change"
        case .previousChange: return "Previous Change"
        case .nextHunk: return "Next Hunk"
        case .previousHunk: return "Previous Hunk"
        case .nextFile: return "Next File"
        case .previousFile: return "Previous File"
        case .toggleSidebar: return "Toggle Sidebar"
        case .toggleWrap: return "Wrap Lines"
        case .toggleLayout: return "Toggle Split View"
        case .toggleContext: return "Toggle Full-File Context"
        case .toggleSyntax: return "Cycle Syntax Coloring"
        case .toggleFold: return "Fold / Unfold Hunk"
        case .expandAll: return "Expand All Hunks"
        case .collapseAll: return "Collapse All Hunks"
        case .find: return "Find in Diff"
        case .comment: return "Add Comment…"
        case .editComment: return "Edit Comment…"
        case .deleteComment: return "Delete Comment"
        case .dismissComment: return "Dismiss / Restore Comment"
        case .reply: return "Reply…"
        case .suggest: return "Suggest a Change…"
        case .commentList: return "Review Navigator"
        case .prInfo: return "Pull Request Info"
        case .submit: return "Submit Review…"
        }
    }

    /// Nil-target selector resolved through the responder chain, so the
    /// frontmost review window answers.
    var selector: Selector {
        switch self {
        case .open: return #selector(AppDelegate.openDocument(_:))
        case .openPullRequest: return #selector(AppDelegate.openPullRequest(_:))
        case .openGitComparison: return #selector(AppDelegate.openGitComparison(_:))
        case .reviewQueue: return #selector(AppDelegate.showReviewQueue(_:))
        case .exportNotes: return #selector(ReviewWindowController.exportNotes(_:))
        case .nextChange: return #selector(ReviewWindowController.nextChange(_:))
        case .previousChange: return #selector(ReviewWindowController.previousChange(_:))
        case .nextHunk: return #selector(ReviewWindowController.nextHunk(_:))
        case .previousHunk: return #selector(ReviewWindowController.previousHunk(_:))
        case .nextFile: return #selector(ReviewWindowController.nextFile(_:))
        case .previousFile: return #selector(ReviewWindowController.previousFile(_:))
        case .toggleSidebar: return #selector(NSSplitViewController.toggleSidebar(_:))
        case .toggleWrap: return #selector(ReviewWindowController.toggleWrap(_:))
        case .toggleLayout: return #selector(ReviewWindowController.toggleLayout(_:))
        case .toggleContext: return #selector(ReviewWindowController.toggleContext(_:))
        case .toggleSyntax: return #selector(ReviewWindowController.toggleSyntax(_:))
        case .toggleFold: return #selector(ReviewWindowController.toggleFold(_:))
        case .expandAll: return #selector(ReviewWindowController.expandAllHunks(_:))
        case .collapseAll: return #selector(ReviewWindowController.collapseAllHunks(_:))
        case .find: return #selector(ReviewWindowController.findInDiff(_:))
        case .comment: return #selector(ReviewWindowController.addComment(_:))
        case .editComment: return #selector(ReviewWindowController.editComment(_:))
        case .deleteComment: return #selector(ReviewWindowController.deleteComment(_:))
        case .dismissComment: return #selector(ReviewWindowController.dismissComment(_:))
        case .reply: return #selector(ReviewWindowController.replyAtCursor(_:))
        case .suggest: return #selector(ReviewWindowController.suggestChange(_:))
        case .commentList: return #selector(ReviewWindowController.showCommentList(_:))
        case .prInfo: return #selector(ReviewWindowController.showPRInfo(_:))
        case .submit: return #selector(ReviewWindowController.submitReview(_:))
        }
    }

    var defaultChord: KeyChord? {
        switch self {
        case .open: return KeyChord.parse("cmd+o")
        case .openPullRequest: return KeyChord.parse("cmd+shift+o")
        case .openGitComparison: return KeyChord.parse("cmd+ctrl+o")
        case .reviewQueue: return KeyChord.parse("cmd+shift+l")
        case .exportNotes: return KeyChord.parse("cmd+shift+e")
        case .nextChange: return KeyChord.parse("cmd+down")
        case .previousChange: return KeyChord.parse("cmd+up")
        case .nextHunk: return KeyChord.parse("cmd+alt+down")
        case .previousHunk: return KeyChord.parse("cmd+alt+up")
        case .nextFile: return KeyChord.parse("cmd+shift+down")
        case .previousFile: return KeyChord.parse("cmd+shift+up")
        case .toggleSidebar: return KeyChord.parse("cmd+ctrl+s")
        case .toggleWrap: return KeyChord.parse("cmd+alt+w")
        case .toggleLayout: return KeyChord.parse("cmd+alt+t")
        case .toggleContext: return KeyChord.parse("cmd+alt+c")
        case .toggleSyntax: return KeyChord.parse("cmd+alt+s")
        case .toggleFold: return KeyChord.parse("cmd+alt+left")
        case .expandAll: return KeyChord.parse("cmd+alt+shift+right")
        case .collapseAll: return KeyChord.parse("cmd+alt+shift+left")
        case .find: return KeyChord.parse("cmd+f")
        case .comment: return KeyChord.parse("cmd+return")
        case .editComment: return KeyChord.parse("cmd+e")
        case .deleteComment: return KeyChord.parse("cmd+delete")
        case .dismissComment: return KeyChord.parse("cmd+shift+x")
        case .reply: return KeyChord.parse("cmd+r")
        case .suggest: return KeyChord.parse("cmd+alt+return")
        case .commentList: return KeyChord.parse("cmd+l")
        case .prInfo: return KeyChord.parse("cmd+i")
        case .submit: return KeyChord.parse("cmd+shift+return")
        }
    }
}

/// Defaults overlaid with the config's `keys` map.
struct Keymap {
    private var chords: [ActionID: KeyChord] = [:]
    /// Overrides that did not parse, reported once at startup.
    let problems: [String]

    init(overrides: [String: String]) {
        var problems: [String] = []
        for action in ActionID.allCases {
            if let chord = action.defaultChord {
                chords[action] = chord
            }
        }
        for (name, spec) in overrides {
            guard let action = ActionID(rawValue: name) else {
                problems.append("keys.\(name): unknown action")
                continue
            }
            if spec.isEmpty {
                chords.removeValue(forKey: action)
                continue
            }
            guard let chord = KeyChord.parse(spec) else {
                problems.append("keys.\(name): could not parse \(spec)")
                continue
            }
            chords[action] = chord
        }
        self.problems = problems
    }

    func chord(for action: ActionID) -> KeyChord? {
        chords[action]
    }

    /// A menu item wired to the action's selector and chord.
    func menuItem(for action: ActionID) -> NSMenuItem {
        let item = NSMenuItem(
            title: action.title,
            action: action.selector,
            keyEquivalent: chord(for: action)?.keyEquivalent ?? "")
        if let chord = chord(for: action) {
            item.keyEquivalentModifierMask = chord.modifiers
        }
        return item
    }
}
