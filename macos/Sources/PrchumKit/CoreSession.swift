import CPrchum
import Foundation

/// Which side of the change a line or comment belongs to.
public enum DiffSide: String, Codable, Sendable {
    case left = "LEFT"
    case right = "RIGHT"

    var raw: UInt32 {
        self == .left ? UInt32(PC_SIDE_LEFT) : UInt32(PC_SIDE_RIGHT)
    }
}

public enum DiffLineKind: String, Codable, Sendable {
    case context
    case addition
    case deletion
    case meta
}

public enum DiffFileStatus: String, Codable, Sendable {
    case modified
    case added
    case deleted
    case renamed
    case copied
    case binary

    /// The one-letter glyph the sidebar shows.
    public var glyph: String {
        switch self {
        case .modified: return "M"
        case .added: return "A"
        case .deleted: return "D"
        case .renamed: return "R"
        case .copied: return "C"
        case .binary: return "B"
        }
    }
}

/// One line of a hunk. `text` is tab-expanded for display; `rawText` is the
/// verbatim form for anything leaving the app as code.
public struct DiffLine: Codable, Sendable {
    public let kind: DiffLineKind
    public let text: String
    public let raw: String?
    public let oldLine: Int?
    public let newLine: Int?
    public let patchPosition: Int?

    public var rawText: String { raw ?? text }

    enum CodingKeys: String, CodingKey {
        case kind, text, raw
        case oldLine = "old_line"
        case newLine = "new_line"
        case patchPosition = "patch_position"
    }
}

public struct DiffHunk: Codable, Sendable {
    public let header: String
    public let lines: [DiffLine]
}

public struct DiffFile: Codable, Sendable {
    public let oldPath: String
    public let newPath: String
    public let status: DiffFileStatus
    public let hunks: [DiffHunk]
    public let isBinary: Bool

    /// The path a reviewer refers to this file by.
    public var displayPath: String {
        status == .deleted ? oldPath : newPath
    }

    /// (additions, deletions) across all hunks.
    public var changeCounts: (added: Int, deleted: Int) {
        var added = 0
        var deleted = 0
        for hunk in hunks {
            for line in hunk.lines {
                switch line.kind {
                case .addition: added += 1
                case .deletion: deleted += 1
                default: break
                }
            }
        }
        return (added, deleted)
    }

    enum CodingKeys: String, CodingKey {
        case status, hunks
        case oldPath = "old_path"
        case newPath = "new_path"
        case isBinary = "is_binary"
    }
}

// MARK: - Review state

public enum DraftCommentState: String, Codable, Sendable {
    case active
    case stale
    case orphaned
    case dismissed
}

public struct CommentLocation: Codable, Sendable {
    public let path: String
    public let side: DiffSide
    public let startLine: Int
    public let endLine: Int

    enum CodingKeys: String, CodingKey {
        case path, side
        case startLine = "start_line"
        case endLine = "end_line"
    }
}

public struct ReviewReply: Codable, Sendable {
    public let author: String
    public let body: String
    public let at: String
}

public struct DraftComment: Codable, Sendable {
    public let localID: String
    public let location: CommentLocation
    public let body: String
    public let snippet: String
    public let state: DraftCommentState
    public let author: String?
    public let at: String?
    public let replyTo: Int64?
    public let replies: [ReviewReply]?

    enum CodingKeys: String, CodingKey {
        case location, body, snippet, state, author, at, replies
        case localID = "local_id"
        case replyTo = "reply_to"
    }
}

/// One comment as the host stores it.
public struct HostComment: Codable, Sendable {
    public let id: Int64
    public let author: String
    public let body: String
    public let createdAt: String
    public let url: String

    enum CodingKeys: String, CodingKey {
        case id, author, body, url
        case createdAt = "created_at"
    }
}

/// A host review thread anchored to a diff position.
public struct ReviewThread: Codable, Sendable {
    public let id: Int64
    public let path: String
    public let side: DiffSide
    public let line: Int?
    public let startLine: Int?
    public let originalLine: Int?
    public let outdated: Bool
    /// Root first, replies after.
    public let comments: [HostComment]

    enum CodingKeys: String, CodingKey {
        case id, path, side, line, outdated, comments
        case startLine = "start_line"
        case originalLine = "original_line"
    }
}

/// A staged conversation-level comment.
public struct GeneralDraft: Codable, Sendable {
    public let localID: String
    public let body: String
    public let at: String

    enum CodingKeys: String, CodingKey {
        case body, at
        case localID = "local_id"
    }
}

public struct PullRequestInfo: Codable, Sendable {
    public let number: UInt64
    public let title: String
    public let body: String
    public let author: String
    public let url: String
    public let headOid: String
    public let baseRef: String
    public let headRef: String

    enum CodingKeys: String, CodingKey {
        case number, title, body, author, url
        case headOid = "head_oid"
        case baseRef = "base_ref"
        case headRef = "head_ref"
    }
}

public enum ReviewSubmitEvent: UInt32, Sendable {
    case comment = 0
    case approve = 1
    case requestChanges = 2
}

public struct SubmitResult: Codable, Sendable {
    public let posted: Int
    public let remaining: Int
    public let skippedDismissed: Int
    public let skippedOrphaned: Int
    public let error: String?

    enum CodingKeys: String, CodingKey {
        case posted, remaining, error
        case skippedDismissed = "skipped_dismissed"
        case skippedOrphaned = "skipped_orphaned"
    }
}

/// A local git comparison to open.
public enum GitComparison {
    case workingTree
    case staged
    case base(String)
    case range(String, String)
}

// MARK: - The session

/// A review session owned by the core.
///
/// The diff under review is immutable; the mutable part is the draft
/// review, and every mutation persists it. Calls serialize on the core's
/// internal lock, so slow operations (submission, the first context
/// fetch) may run off the main thread while the UI stays hands-off.
public final class CoreSession: @unchecked Sendable {
    // Unchecked because safety lives on the other side of the boundary:
    // the core handle is internally synchronized (every FFI call locks),
    // so cross-thread use serializes there.
    let handle: OpaquePointer

    /// The default persistence directory for drafts.
    public static var defaultDraftsDirectory: String {
        let base = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("Prchum/drafts").path
    }

    /// Opens a session over a literal unified-diff text.
    public init(title: String, patch: String) throws {
        var errorOut: UnsafeMutablePointer<CChar>?
        let handle = withUTF8Pointer(title) { titlePtr, titleLen in
            withUTF8Pointer(patch) { patchPtr, patchLen in
                pc_session_new_from_patch(
                    titlePtr, UInt(titleLen), patchPtr, UInt(patchLen), &errorOut)
            }
        }
        guard let handle else {
            throw CoreError(message: takeString(errorOut) ?? "could not parse the diff")
        }
        self.handle = handle
    }

    /// Opens a session over a file: a patch, or a review-exchange document
    /// (detected by content — exchange sessions rewrite their file on save).
    public init(contentsOf path: String) throws {
        var errorOut: UnsafeMutablePointer<CChar>?
        let handle = withUTF8Pointer(path) { pointer, length in
            pc_session_new_from_file(pointer, UInt(length), &errorOut)
        }
        guard let handle else {
            throw CoreError(message: takeString(errorOut) ?? "could not open \(path)")
        }
        self.handle = handle
    }

    /// Opens a session over a local git comparison.
    public init(gitRepo repo: String, comparison: GitComparison, context: UInt32 = 3) throws {
        var errorOut: UnsafeMutablePointer<CChar>?
        let kind: UInt32
        var arg1 = ""
        var arg2 = ""
        switch comparison {
        case .workingTree:
            kind = UInt32(PC_GIT_WORKTREE)
        case .staged:
            kind = UInt32(PC_GIT_STAGED)
        case .base(let base):
            kind = UInt32(PC_GIT_BASE)
            arg1 = base
        case .range(let a, let b):
            kind = UInt32(PC_GIT_RANGE)
            arg1 = a
            arg2 = b
        }
        let handle = withUTF8Pointer(repo) { repoPtr, repoLen in
            withUTF8Pointer(arg1) { arg1Ptr, arg1Len in
                withUTF8Pointer(arg2) { arg2Ptr, arg2Len in
                    pc_session_new_from_git(
                        repoPtr, UInt(repoLen), kind,
                        arg1Ptr, UInt(arg1Len), arg2Ptr, UInt(arg2Len),
                        context, &errorOut)
                }
            }
        }
        guard let handle else {
            throw CoreError(message: takeString(errorOut) ?? "could not run the git comparison")
        }
        self.handle = handle
    }

    /// Opens a session over a pull request — a blocking network call
    /// through the forge CLI; create off the main thread, then hand over.
    /// `configPath` supplies forge-kind overrides and the Forgejo transport
    /// template for self-hosted instances.
    public init(
        pullRequest reference: String,
        repoHint: String = "",
        configPath: String = CoreConfig.defaultPath
    ) throws {
        var errorOut: UnsafeMutablePointer<CChar>?
        let handle = withUTF8Pointer(reference) { refPtr, refLen in
            withUTF8Pointer(repoHint) { hintPtr, hintLen in
                withUTF8Pointer(configPath) { cfgPtr, cfgLen in
                    pc_session_new_from_pr(
                        refPtr, UInt(refLen), hintPtr, UInt(hintLen),
                        cfgPtr, UInt(cfgLen), &errorOut)
                }
            }
        }
        guard let handle else {
            throw CoreError(
                message: takeString(errorOut) ?? "could not open the pull request")
        }
        self.handle = handle
    }

    deinit {
        pc_session_free(handle)
    }

    public var title: String {
        takeString(pc_session_title(handle)) ?? ""
    }

    public var fileCount: Int {
        Int(pc_session_file_count(handle))
    }

    /// The changed file at `index`, hunks and lines included.
    public func file(at index: Int) throws -> DiffFile {
        guard let json = takeString(pc_session_file_json(handle, UInt(index))) else {
            throw CoreRejectedOperation(operation: "file at index \(index)")
        }
        return try decode(DiffFile.self, from: json)
    }

    /// All changed files, in diff order.
    public func files() throws -> [DiffFile] {
        try (0..<fileCount).map { try file(at: $0) }
    }

    /// The whole-file projection of one file — the context view. Content
    /// is fetched on first use (a network call in PR mode), verified
    /// against the diff, and cached; hunks come back overlaid on the full
    /// file, gap lines carrying both line numbers.
    public func contextFile(at index: Int) throws -> DiffFile {
        var errorOut: UnsafeMutablePointer<CChar>?
        guard
            let json = takeString(
                pc_session_context_file_json(handle, UInt(index), &errorOut))
        else {
            throw CoreError(message: takeString(errorOut) ?? "no context available")
        }
        return try decode(DiffFile.self, from: json)
    }

    /// Attaches persistence: loads any saved draft (re-anchored if the head
    /// moved) and saves every later change. Returns a warning when the
    /// saved draft was unreadable.
    @discardableResult
    public func attachStore(directory: String = CoreSession.defaultDraftsDirectory) -> String? {
        withUTF8Pointer(directory) { pointer, length in
            takeString(pc_session_attach_store(handle, pointer, UInt(length)))
        }
    }

    public func setAuthor(_ author: String) {
        withUTF8Pointer(author) { pointer, length in
            pc_session_set_author(handle, pointer, UInt(length))
        }
    }

    // MARK: Comments

    /// Adds a draft comment; `replyTo` is a host thread's root comment id.
    /// Returns the new comment's local id.
    @discardableResult
    public func addComment(
        fileIndex: Int,
        side: DiffSide,
        startLine: Int,
        endLine: Int,
        body: String,
        replyTo: Int64 = 0
    ) throws -> String {
        var errorOut: UnsafeMutablePointer<CChar>?
        let id = withUTF8Pointer(body) { pointer, length in
            takeString(
                pc_session_add_comment(
                    handle, UInt(fileIndex), side.raw,
                    UInt32(startLine), UInt32(endLine),
                    pointer, UInt(length), replyTo, &errorOut))
        }
        guard let id else {
            throw CoreError(message: takeString(errorOut) ?? "could not add the comment")
        }
        return id
    }

    public func updateComment(localID: String, body: String) -> Bool {
        withUTF8Pointer(localID) { idPtr, idLen in
            withUTF8Pointer(body) { bodyPtr, bodyLen in
                pc_session_update_comment(
                    handle, idPtr, UInt(idLen), bodyPtr, UInt(bodyLen))
            }
        }
    }

    public func deleteComment(localID: String) -> Bool {
        withUTF8Pointer(localID) { pointer, length in
            pc_session_delete_comment(handle, pointer, UInt(length))
        }
    }

    /// Dismiss ↔ restore (kept, never submitted while dismissed).
    public func toggleDismiss(localID: String) -> Bool {
        withUTF8Pointer(localID) { pointer, length in
            pc_session_toggle_dismiss(handle, pointer, UInt(length))
        }
    }

    /// Rewrites one reply of a draft's conversation (authors stay).
    public func updateReply(localID: String, index: Int, body: String) -> Bool {
        withUTF8Pointer(localID) { idPtr, idLen in
            withUTF8Pointer(body) { bodyPtr, bodyLen in
                pc_session_update_reply(
                    handle, idPtr, UInt(idLen), UInt(index), bodyPtr, UInt(bodyLen))
            }
        }
    }

    public func deleteReply(localID: String, index: Int) -> Bool {
        withUTF8Pointer(localID) { pointer, length in
            pc_session_delete_reply(handle, pointer, UInt(length), UInt(index))
        }
    }

    /// Appends to a draft comment's travelling conversation.
    public func addReply(localID: String, body: String) -> Bool {
        withUTF8Pointer(localID) { idPtr, idLen in
            withUTF8Pointer(body) { bodyPtr, bodyLen in
                pc_session_add_reply(handle, idPtr, UInt(idLen), bodyPtr, UInt(bodyLen))
            }
        }
    }

    public func comments() -> [DraftComment] {
        guard let json = takeString(pc_session_comments_json(handle)) else { return [] }
        return (try? decode([DraftComment].self, from: json)) ?? []
    }

    /// Existing host review threads (PR mode).
    public func threads() -> [ReviewThread] {
        guard let json = takeString(pc_session_threads_json(handle)), !json.isEmpty else {
            return []
        }
        return (try? decode([ReviewThread].self, from: json)) ?? []
    }

    /// The host's conversation-level comments (PR mode).
    public func generalComments() -> [HostComment] {
        guard let json = takeString(pc_session_general_json(handle)), !json.isEmpty else {
            return []
        }
        return (try? decode([HostComment].self, from: json)) ?? []
    }

    /// The staged conversation comments (post on submit).
    public func generalDrafts() -> [GeneralDraft] {
        guard let json = takeString(pc_session_general_drafts_json(handle)) else { return [] }
        return (try? decode([GeneralDraft].self, from: json)) ?? []
    }

    /// Stages a conversation-level comment; returns its local id.
    @discardableResult
    public func addGeneral(body: String) -> String? {
        withUTF8Pointer(body) { pointer, length in
            takeString(pc_session_add_general(handle, pointer, UInt(length)))
        }
    }

    public func deleteGeneral(localID: String) -> Bool {
        withUTF8Pointer(localID) { pointer, length in
            pc_session_delete_general(handle, pointer, UInt(length))
        }
    }

    /// Pull-request metadata (PR mode).
    public var pullRequestInfo: PullRequestInfo? {
        guard let json = takeString(pc_session_pr_json(handle)), !json.isEmpty else {
            return nil
        }
        return try? decode(PullRequestInfo.self, from: json)
    }

    public var isPullRequest: Bool {
        pullRequestInfo != nil
    }

    // MARK: Summary, export, submit

    public var summary: String {
        get { takeString(pc_session_summary(handle)) ?? "" }
        set {
            _ = withUTF8Pointer(newValue) { pointer, length in
                pc_session_set_summary(handle, pointer, UInt(length))
            }
        }
    }

    public func setEvent(_ event: ReviewSubmitEvent) {
        pc_session_set_event(handle, event.rawValue)
    }

    /// Exports to `path`: `.json` writes a review-exchange document,
    /// anything else Markdown.
    public func export(to path: String) throws {
        var errorOut: UnsafeMutablePointer<CChar>?
        let accepted = withUTF8Pointer(path) { pointer, length in
            pc_session_export_to_file(handle, pointer, UInt(length), &errorOut)
        }
        guard accepted else {
            throw CoreError(message: takeString(errorOut) ?? "could not export")
        }
    }

    /// Submits to the pull request — blocking; the result reports what the
    /// host accepted even on partial failure (accepted drafts are already
    /// removed and persisted, so a retry never duplicates).
    public func submit() throws -> SubmitResult {
        guard let json = takeString(pc_session_submit(handle)) else {
            throw CoreError(message: "this session has no pull request to submit to")
        }
        return try decode(SubmitResult.self, from: json)
    }

    private func decode<T: Decodable>(_ type: T.Type, from json: String) throws -> T {
        do {
            return try JSONDecoder().decode(type, from: Data(json.utf8))
        } catch {
            throw CoreError(message: "malformed payload from core: \(error)")
        }
    }
}
