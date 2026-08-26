import CPrchum
import Foundation

/// One entry of the core's syntax style table.
public struct SyntaxStyle: Sendable {
    /// 0xRRGGBBAA for the light appearance.
    public let light: UInt32
    /// 0xRRGGBBAA for the dark appearance.
    public let dark: UInt32
    public let bold: Bool
    public let italic: Bool
}

/// One styled span within a line: byte offsets into the line's display
/// text plus an index into the style table.
public struct HighlightSpan: Sendable {
    public let startByte: Int
    public let endByte: Int
    public let styleIndex: Int
}

public enum CoreSyntax {
    /// The style table; ids in highlight spans index it. Cached — call
    /// [`reloadStyles`] after the theme changes.
    public private(set) static var styles: [SyntaxStyle] = loadStyles()

    /// Re-reads the style table from the core (after a theme switch).
    public static func reloadStyles() {
        styles = loadStyles()
    }

    private static func loadStyles() -> [SyntaxStyle] {
        guard let json = takeString(pc_style_table_json()),
            let data = json.data(using: .utf8),
            let raw = try? JSONDecoder().decode([RawStyle].self, from: data)
        else { return [] }
        return raw.map {
            SyntaxStyle(
                light: $0.light, dark: $0.dark,
                bold: $0.flags & 1 != 0, italic: $0.flags & 2 != 0)
        }
    }

    /// Built-in theme names, in presentation order.
    public static var builtinThemeNames: [String] {
        guard let joined = takeString(pc_theme_builtin_names()) else { return ["default"] }
        return joined.split(separator: "\n").map(String.init)
    }

    /// Applies the theme config.json names (built-in or themes/<name>.json
    /// next to the config); a warning means the default stayed.
    @discardableResult
    public static func applyTheme(configPath: String = CoreConfig.defaultPath) -> String? {
        let warning = withUTF8Pointer(configPath) { pointer, length in
            takeString(pc_theme_apply(pointer, UInt(length)))
        }
        reloadStyles()
        return warning
    }

    private struct RawStyle: Decodable {
        let light: UInt32
        let dark: UInt32
        let flags: UInt32
    }
}

extension CoreSession {
    /// Syntax highlights for the context projection at `index` — indexed
    /// by the projection's hunks, so gap regions color too. Nil when the
    /// language is unknown or the projection is unavailable.
    public func contextHighlights(at index: Int) -> [[[HighlightSpan]]]? {
        guard
            let json = takeString(
                pc_session_context_highlights_json(handle, UInt(index)))
        else { return nil }
        return Self.decodeSpans(json)
    }

    /// Syntax highlights for the file at `index`: `[hunk][line]` → spans.
    /// Nil when the file's language is unknown.
    public func fileHighlights(at index: Int) -> [[[HighlightSpan]]]? {
        guard let json = takeString(pc_session_file_highlights_json(handle, UInt(index))),
            let spans = Self.decodeSpans(json)
        else { return nil }
        return spans
    }

    private static func decodeSpans(_ json: String) -> [[[HighlightSpan]]]? {
        guard let data = json.data(using: .utf8),
            let raw = try? JSONDecoder().decode([[[[UInt32]]]].self, from: data)
        else { return nil }
        return raw.map { hunk in
            hunk.map { line in
                line.compactMap { triple in
                    guard triple.count == 3 else { return nil }
                    return HighlightSpan(
                        startByte: Int(triple[0]),
                        endByte: Int(triple[1]),
                        styleIndex: Int(triple[2]))
                }
            }
        }
    }
}
