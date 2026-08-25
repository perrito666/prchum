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
