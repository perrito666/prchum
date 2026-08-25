import CPrchum
import Foundation

/// The user configuration, owned by the core.
///
/// Loading never fails: a missing file is the defaults, and a broken file
/// is the defaults plus `loadWarning` — the file on disk is never touched.
public final class CoreConfig {
    private let handle: OpaquePointer

    /// The default configuration file location.
    public static var defaultPath: String {
        let base = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("Prchum/config.json").path
    }

    public init(path: String) {
        let handle = withUTF8Pointer(path) { pointer, length in
            pc_config_new(pointer, UInt(length))
        }
        // The only failure mode is a non-UTF-8 path, which Swift strings
        // cannot produce.
        self.handle = handle!
    }

    public convenience init() {
        self.init(path: Self.defaultPath)
    }

    deinit {
        pc_config_free(handle)
    }

    /// The problem found while loading, if any. Defaults are in effect
    /// when this is set.
    public var loadWarning: String? {
        takeString(pc_config_load_warning(handle))
    }

    /// Key-binding overrides: action name → key spec. An empty spec means
    /// the action's default binding is removed.
    public var keyOverrides: [String: String] {
        guard let json = takeString(pc_config_keys_json(handle)),
            let data = json.data(using: .utf8),
            let decoded = try? JSONDecoder().decode([String: String].self, from: data)
        else {
            return [:]
        }
        return decoded
    }
}
