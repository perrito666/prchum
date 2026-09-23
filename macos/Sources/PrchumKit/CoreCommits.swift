import CPrchum
import Foundation

/// One commit of a pull request.
public struct CommitInfo: Codable, Sendable, Equatable {
    public let sha: String
    /// First parent first.
    public let parents: [String]
    /// The first line of the message.
    public let title: String
    public let author: String
    public let date: String
    /// Drafts waiting in this commit's own review (only in a listing).
    public let drafts: Int?

    /// The abbreviation the forges show.
    public var shortSHA: String {
        String(sha.prefix(7))
    }
}

/// A pull request's commits, oldest first.
public struct CommitListing: Codable, Sendable {
    public let commits: [CommitInfo]
    /// Non-empty when the forge withheld commits (GitHub lists at most
    /// 250); worth showing, not an error.
    public let notice: String
    /// Drafts waiting in the whole request's review.
    public let drafts: Int
    /// The commit the asking session reviews; empty for the whole request.
    public let current: String
}

extension CoreSession {
    /// The pull request's commits, with the drafts waiting on each. A
    /// blocking network call — run it off the main thread.
    public func commits() throws -> CommitListing {
        var errorOut: UnsafeMutablePointer<CChar>?
        guard let json = takeString(pc_session_commits_json(handle, &errorOut)) else {
            throw CoreError(message: takeString(errorOut) ?? "could not list the commits")
        }
        do {
            return try JSONDecoder().decode(CommitListing.self, from: Data(json.utf8))
        } catch {
            throw CoreError(message: "malformed commit listing from core: \(error)")
        }
    }

    /// The commit this session reviews; nil for a whole request, and for
    /// anything that is not a pull request.
    public var reviewedCommit: CommitInfo? {
        guard let json = takeString(pc_session_commit_json(handle)), !json.isEmpty else {
            return nil
        }
        return try? JSONDecoder().decode(CommitInfo.self, from: Data(json.utf8))
    }
}
