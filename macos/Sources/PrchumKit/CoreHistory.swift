import CPrchum
import Foundation

/// One review the user has opened before.
public struct HistoryEntry: Codable, Sendable {
    public let key: String
    /// `pr` | `patch` | `exchange` | `git`.
    public let kind: String
    public let title: String
    public let display: String
    public let reopen: String
    public let lastOpened: String
    public let submittedAt: String?

    enum CodingKeys: String, CodingKey {
        case key, kind, title, display, reopen
        case lastOpened = "last_opened"
        case submittedAt = "submitted_at"
    }
}

public enum CoreHistory {
    /// Where the history lives (next to the drafts).
    public static var defaultDirectory: String {
        let base = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("Prchum").path
    }

    public static func list(directory: String = defaultDirectory) -> [HistoryEntry] {
        withUTF8Pointer(directory) { pointer, length in
            decode(takeString(pc_history_list_json(pointer, UInt(length))))
        }
    }

    @discardableResult
    public static func remove(key: String, directory: String = defaultDirectory) -> Bool {
        withUTF8Pointer(directory) { dirPtr, dirLen in
            withUTF8Pointer(key) { keyPtr, keyLen in
                pc_history_remove(dirPtr, UInt(dirLen), keyPtr, UInt(keyLen))
            }
        }
    }

    /// Drops entries whose pull request is merged, closed, or gone —
    /// blocking (one forge call per PR entry); run off the main thread.
    /// Returns the surviving entries, newest first.
    public static func prune(
        directory: String = defaultDirectory,
        configPath: String = CoreConfig.defaultPath
    ) -> [HistoryEntry] {
        withUTF8Pointer(directory) { dirPtr, dirLen in
            withUTF8Pointer(configPath) { cfgPtr, cfgLen in
                decode(
                    takeString(
                        pc_history_prune_json(dirPtr, UInt(dirLen), cfgPtr, UInt(cfgLen))))
            }
        }
    }

    private static func decode(_ json: String?) -> [HistoryEntry] {
        guard let json, let data = json.data(using: .utf8),
            let entries = try? JSONDecoder().decode([HistoryEntry].self, from: data)
        else { return [] }
        return entries
    }
}

extension CoreSession {
    /// Records (or refreshes) this session in the review history;
    /// `submitted` also stamps the submission time.
    @discardableResult
    public func recordHistory(
        directory: String = CoreHistory.defaultDirectory,
        submitted: Bool = false
    ) -> Bool {
        withUTF8Pointer(directory) { pointer, length in
            pc_session_record_history(handle, pointer, UInt(length), submitted)
        }
    }
}
