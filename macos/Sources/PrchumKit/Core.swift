import CPrchum
import Foundation

/// Version and shared error types for the core wrapper.
public enum Core {
    /// The core library's version string.
    public static var version: String {
        guard let cString = pc_version() else { return "unknown" }
        return String(cString: cString)
    }
}

/// The core validated an operation's inputs, rejected them, and changed
/// nothing.
public struct CoreRejectedOperation: Error, CustomStringConvertible {
    public let operation: String

    public init(operation: String) {
        self.operation = operation
    }

    public var description: String { "core rejected: \(operation)" }
}

/// A failure the core described with a message (parse errors, I/O).
public struct CoreError: Error, CustomStringConvertible {
    public let message: String

    public init(message: String) {
        self.message = message
    }

    public var description: String { message }
}

/// Runs `body` with a `(pointer, length)` view of the string's UTF-8.
func withUTF8Pointer<R>(
    _ text: String,
    _ body: (UnsafePointer<CChar>?, Int) -> R
) -> R {
    var text = text
    return text.withUTF8 { bytes in
        let pointer = bytes.baseAddress.map {
            UnsafeRawPointer($0).assumingMemoryBound(to: CChar.self)
        }
        return body(pointer, bytes.count)
    }
}

/// Consumes a core-owned C string: copies it into a `String` and frees it.
func takeString(_ cString: UnsafeMutablePointer<CChar>?) -> String? {
    guard let cString else { return nil }
    defer { pc_string_free(cString) }
    return String(cString: cString)
}

/// What a command-line argument asked for, as the core reads it.
///
/// Decided there rather than here so `prchum main` means the same thing
/// on both platforms.
public enum CoreTarget: Sendable {
    case home
    case file(String)
    case git(repo: String, spec: GitComparison)
    case request(String)

    public static func parse(
        argument: String, cwd: String, staged: Bool
    ) -> CoreTarget {
        let json = argument.withCString { argumentPointer in
            cwd.withCString { cwdPointer in
                takeString(
                    pc_target_parse_json(
                        argumentPointer, UInt(strlen(argumentPointer)),
                        cwdPointer, UInt(strlen(cwdPointer)),
                        staged))
            }
        }
        guard
            let data = json?.data(using: .utf8),
            let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let kind = object["kind"] as? String
        else { return .home }

        switch kind {
        case "file":
            return .file(object["path"] as? String ?? "")
        case "request":
            return .request(object["0"] as? String ?? firstString(object) ?? "")
        case "git":
            let repo = object["repo"] as? String ?? ""
            switch object["spec"] as? String {
            case "staged": return .git(repo: repo, spec: .staged)
            case "base": return .git(repo: repo, spec: .base(object["base"] as? String ?? ""))
            case "range":
                return .git(
                    repo: repo,
                    spec: .range(
                        object["from"] as? String ?? "", object["to"] as? String ?? ""))
            default: return .git(repo: repo, spec: .workingTree)
            }
        default:
            return .home
        }
    }
}

/// Newtype enums serialize as a single unnamed field; its key depends on
/// serde's spelling, so the one string in the object is taken.
private func firstString(_ object: [String: Any]) -> String? {
    for (key, value) in object where key != "kind" {
        if let text = value as? String { return text }
    }
    return nil
}
