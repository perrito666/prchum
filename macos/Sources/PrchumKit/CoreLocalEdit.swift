import AppKit
import CPrchum
import Foundation

/// Where a session's files live on disk for editing.
public struct WorktreeInfo: Codable, Sendable {
    public let path: String
    /// The branch checked out there, empty when detached.
    public let branch: String
    /// True when prchum created it — the only ones it removes later.
    public let created: Bool
}

public enum CoreWorktree {
    /// Removes the worktree prchum created for `key`, if any. Worktrees
    /// it did not create are never touched.
    @discardableResult
    public static func remove(
        key: String, directory: String = CoreHistory.defaultDirectory
    ) -> Bool {
        withUTF8Pointer(directory) { dirPtr, dirLen in
            withUTF8Pointer(key) { keyPtr, keyLen in
                pc_worktree_remove(dirPtr, UInt(dirLen), keyPtr, UInt(keyLen))
            }
        }
    }
}

extension CoreSession {
    /// The repository as `owner/repo`, or empty when the source has none.
    public var repoSlug: String {
        takeString(pc_session_repo_slug(handle)) ?? ""
    }

    /// Finds or creates the local worktree to edit this session's files
    /// in. A pull request checks its branch out of `clone` (reusing an
    /// existing checkout when there is one); a git comparison answers
    /// with its own repository root. Blocking — call off the main thread.
    public func localWorktree(
        clone: String,
        directory: String = CoreHistory.defaultDirectory
    ) throws -> WorktreeInfo {
        var errorOut: UnsafeMutablePointer<CChar>?
        let json = withUTF8Pointer(directory) { dirPtr, dirLen in
            withUTF8Pointer(clone) { clonePtr, cloneLen in
                takeString(
                    pc_session_worktree_json(
                        handle, dirPtr, UInt(dirLen), clonePtr, UInt(cloneLen), &errorOut))
            }
        }
        guard let json else {
            throw CoreError(message: takeString(errorOut) ?? "could not prepare a worktree")
        }
        do {
            return try JSONDecoder().decode(WorktreeInfo.self, from: Data(json.utf8))
        } catch {
            throw CoreError(message: "malformed worktree payload: \(error)")
        }
    }
}

/// Opening a file in the user's editor.
public enum CoreEditor {
    /// What the platform should do to open a file.
    public enum Invocation: Sendable {
        case url(String)
        case command(program: String, args: [String])
    }

    /// Builds the invocation from the configured template. An empty
    /// template means textchum's `textchum://open` URL.
    public static func invocation(
        template: String, path: String, line: Int, directory: String
    ) -> Invocation? {
        let json = withUTF8Pointer(template) { templatePtr, templateLen in
            withUTF8Pointer(path) { pathPtr, pathLen in
                withUTF8Pointer(directory) { dirPtr, dirLen in
                    takeString(
                        pc_editor_invocation_json(
                            templatePtr, UInt(templateLen),
                            pathPtr, UInt(pathLen),
                            UInt32(max(line, 0)),
                            dirPtr, UInt(dirLen)))
                }
            }
        }
        guard let json, let data = json.data(using: .utf8),
            let decoded = try? JSONDecoder().decode(RawInvocation.self, from: data)
        else { return nil }
        if decoded.kind == "url", let url = decoded.url {
            return .url(url)
        }
        guard let program = decoded.program, !program.isEmpty else { return nil }
        return .command(program: program, args: decoded.args ?? [])
    }

    /// Builds and performs the invocation: a URL goes to the platform's
    /// opener (which is how textchum and other scheme-registering editors
    /// are reached), a command is spawned. Throws when the editor could
    /// not be launched — a missing binary, or a scheme nothing handles.
    public static func open(
        template: String, path: String, line: Int, directory: String
    ) throws {
        guard let invocation = invocation(
            template: template, path: path, line: line, directory: directory)
        else {
            throw CoreError(message: "the editor command is empty")
        }
        switch invocation {
        case .url(let text):
            guard let url = URL(string: text) else {
                throw CoreError(message: "the editor produced an unusable URL: \(text)")
            }
            guard NSWorkspace.shared.open(url) else {
                throw CoreError(
                    message:
                        "nothing on this Mac opens \(url.scheme ?? "that") links — "
                        + "install the editor, or set editor_command in Settings")
            }
        case .command(let program, let args):
            let process = Process()
            // A bare name is resolved on PATH the way a shell would.
            if program.contains("/") {
                process.executableURL = URL(fileURLWithPath: program)
                process.arguments = args
            } else {
                process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
                process.arguments = [program] + args
            }
            process.currentDirectoryURL = URL(fileURLWithPath: directory)
            do {
                try process.run()
            } catch {
                throw CoreError(
                    message: "could not run \(program): \(error.localizedDescription)")
            }
        }
    }

    private struct RawInvocation: Decodable {
        let kind: String
        let url: String?
        let program: String?
        let args: [String]?
    }
}

extension CoreConfig {
    /// Configured clones: `owner/repo` → local path.
    public var clones: [String: String] {
        guard let json = takeString(pc_config_clones_json(handle)),
            let data = json.data(using: .utf8),
            let decoded = try? JSONDecoder().decode([String: String].self, from: data)
        else { return [:] }
        return decoded
    }

    /// The clone configured for `owner/repo`, case-insensitively.
    public func clone(for slug: String) -> String? {
        let needle = slug.lowercased()
        return clones.first { $0.key.lowercased() == needle }?.value
    }

    /// The editor template; empty means textchum's URL scheme.
    public var editorCommand: String {
        takeString(pc_config_editor_command(handle)) ?? ""
    }
}
