import CPrchum
import Foundation

/// One request in the user's review queue.
public struct ListedRequest: Codable, Sendable {
    public let host: String
    public let owner: String
    public let repo: String
    public let number: UInt64
    public let title: String
    public let author: String
    public let updatedAt: String
    public let url: String

    /// The short display reference: `owner/repo#N`.
    public var reference: String {
        "\(owner)/\(repo)#\(number)"
    }

    enum CodingKeys: String, CodingKey {
        case host, owner, repo, number, title, author, url
        case updatedAt = "updated_at"
    }
}

public enum CoreDiscovery {
    /// The open requests waiting for the user's review, through the
    /// config-selected engine. Blocking — call off the main thread.
    public static func listRequests(
        configPath: String = CoreConfig.defaultPath
    ) throws -> [ListedRequest] {
        var errorOut: UnsafeMutablePointer<CChar>?
        let json = withUTF8Pointer(configPath) { pointer, length in
            takeString(pc_list_requests(pointer, UInt(length), &errorOut))
        }
        guard let json else {
            throw CoreError(message: takeString(errorOut) ?? "could not list requests")
        }
        do {
            return try JSONDecoder().decode([ListedRequest].self, from: Data(json.utf8))
        } catch {
            throw CoreError(message: "malformed listing from core: \(error)")
        }
    }
}
