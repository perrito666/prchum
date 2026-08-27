import CPrchum
import Foundation

/// The user configuration, owned by the core.
///
/// Loading never fails: a missing file is the defaults, and a broken file
/// is the defaults plus `loadWarning` — the file on disk is never touched.
public final class CoreConfig {
    let handle: OpaquePointer

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

    /// Named discovery filters: name → filter.
    public var listFilters: [String: String] {
        guard let json = takeString(pc_config_list_filters_json(handle)),
            let data = json.data(using: .utf8),
            let decoded = try? JSONDecoder().decode([String: String].self, from: data)
        else { return [:] }
        return decoded
    }

    /// The fallback discovery filter (empty = the engine's default).
    public var listFilter: String {
        takeString(pc_config_list_filter(handle)) ?? ""
    }

    /// Writes one entry of a top-level map setting (`list_filters`,
    /// `keys`, `forges`…); an empty value removes the entry.
    @discardableResult
    public static func setMapEntry(
        _ mapKey: String, _ entryKey: String, _ value: String,
        path: String = defaultPath
    ) -> Bool {
        withUTF8Pointer(path) { pathPtr, pathLen in
            withUTF8Pointer(mapKey) { mapPtr, mapLen in
                withUTF8Pointer(entryKey) { keyPtr, keyLen in
                    withUTF8Pointer(value) { valuePtr, valueLen in
                        pc_config_set_map_entry(
                            pathPtr, UInt(pathLen), mapPtr, UInt(mapLen),
                            keyPtr, UInt(keyLen), valuePtr, UInt(valueLen))
                    }
                }
            }
        }
    }

    /// The selected named keymap and whether `keymaps` defines it.
    public var keymapSelection: (name: String, exists: Bool) {
        var exists = false
        let name = takeString(pc_config_keymap(handle, &exists)) ?? ""
        return (name, exists)
    }

    /// `system` | `light` | `dark`.
    public enum Appearance: UInt32 {
        case system = 0
        case light = 1
        case dark = 2
    }

    public var appearance: Appearance {
        Appearance(rawValue: pc_config_appearance(handle)) ?? .system
    }

    /// The configured theme name (empty = default).
    public var theme: String {
        takeString(pc_config_theme(handle)) ?? ""
    }

    /// Writes one string setting into config.json, preserving everything
    /// else in the file. A broken file is left untouched.
    @discardableResult
    public static func setString(
        _ key: String, _ value: String, path: String = defaultPath
    ) -> Bool {
        withUTF8Pointer(path) { pathPtr, pathLen in
            withUTF8Pointer(key) { keyPtr, keyLen in
                withUTF8Pointer(value) { valuePtr, valueLen in
                    pc_config_set_string(
                        pathPtr, UInt(pathLen), keyPtr, UInt(keyLen),
                        valuePtr, UInt(valueLen))
                }
            }
        }
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
