import CPrchum
import Foundation

/// Which side of the change a line or comment belongs to.
public enum DiffSide: String, Codable, Sendable {
    case left = "LEFT"
    case right = "RIGHT"
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

/// A review session owned by the core.
///
/// The diff under review is immutable; this class is the shell's only way
/// to read it. Not thread-safe: use from the main thread.
public final class CoreSession {
    private let handle: OpaquePointer

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

    /// Opens a session over a patch file on disk.
    public convenience init(contentsOf path: String) throws {
        let patch: String
        do {
            patch = try String(contentsOfFile: path, encoding: .utf8)
        } catch {
            throw CoreError(message: "could not read \(path): \(error.localizedDescription)")
        }
        let title = (path as NSString).lastPathComponent
        try self.init(title: title, patch: patch)
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
        do {
            return try JSONDecoder().decode(DiffFile.self, from: Data(json.utf8))
        } catch {
            throw CoreError(message: "malformed file payload from core: \(error)")
        }
    }

    /// All changed files, in diff order.
    public func files() throws -> [DiffFile] {
        try (0..<fileCount).map { try file(at: $0) }
    }
}
