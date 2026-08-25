//! Syntax highlighting for diffs: tree-sitter over reconstructed sides.
//!
//! A diff shows two documents interleaved, so highlighting runs one pass
//! per side per hunk: the LEFT pass over context+deletions, the RIGHT pass
//! over context+additions — leanreview's per-hunk fidelity tier. That
//! keeps multi-line constructs (strings, comments) correct within a hunk
//! without needing the whole file's content.
//!
//! Styles follow textchum's design: capture names resolve by trimming
//! dotted segments (`function.method` falls back to `function`), and style
//! ids are indexes into the capture table, so they are stable across the
//! FFI. Each style carries a light and a dark color; the shell picks at
//! draw time.

use std::sync::OnceLock;

use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator};

use crate::diff::{FileDiff, LineKind};

pub const STYLE_BOLD: u32 = 1 << 0;
pub const STYLE_ITALIC: u32 = 1 << 1;

/// One entry of the style table. Colors are 0xRRGGBBAA.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Style {
    pub light: u32,
    pub dark: u32,
    pub flags: u32,
}

const fn style(light: u32, dark: u32, flags: u32) -> Style {
    Style { light, dark, flags }
}

/// The styled capture names. Order defines the style ids that cross the
/// FFI, so this list is append-only.
pub static CAPTURES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "escape",
    "function",
    "function.builtin",
    "keyword",
    "label",
    "module",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.special",
    "string",
    "string.special",
    "tag",
    "text.emphasis",
    "text.literal",
    "text.reference",
    "text.strong",
    "text.title",
    "text.uri",
    "type",
    "type.builtin",
    "variable.builtin",
    "variable.parameter",
];

/// The default palette, aligned with [`CAPTURES`] (textchum's).
pub static DEFAULT_STYLES: &[Style] = &[
    style(0x836C28FF, 0xBF8555FF, 0),            // attribute
    style(0x707F8CFF, 0x7F8C98FF, STYLE_ITALIC), // comment
    style(0x6F42C1FF, 0xB281EBFF, 0),            // constant
    style(0xAD3DA4FF, 0xFC5FA3FF, 0),            // constant.builtin
    style(0x326D74FF, 0x67B7A4FF, 0),            // constructor
    style(0x0F68A0FF, 0x67B7A4FF, 0),            // escape
    style(0x326D74FF, 0x67B7A4FF, 0),            // function
    style(0x326D74FF, 0x67B7A4FF, 0),            // function.builtin
    style(0xAD3DA4FF, 0xFC5FA3FF, 0),            // keyword
    style(0x836C28FF, 0xBF8555FF, 0),            // label
    style(0x3900A0FF, 0x5DD8FFFF, 0),            // module
    style(0x1C00CFFF, 0xD0BF69FF, 0),            // number
    style(0x52606DFF, 0xA0A7B0FF, 0),            // operator
    style(0x036A96FF, 0x75B492FF, 0),            // property
    style(0x52606DFF, 0x7F8C98FF, 0),            // punctuation
    style(0xAD3DA4FF, 0xFC5FA3FF, 0),            // punctuation.special
    style(0xC41A16FF, 0xFC6A5DFF, 0),            // string
    style(0x0F68A0FF, 0xFD8F3FFF, 0),            // string.special
    style(0xAD3DA4FF, 0xFC5FA3FF, 0),            // tag
    style(0x24292EFF, 0xDFDFE0FF, STYLE_ITALIC), // text.emphasis
    style(0xC41A16FF, 0xFC6A5DFF, 0),            // text.literal
    style(0x0F68A0FF, 0x6BDFFFFF, 0),            // text.reference
    style(0x24292EFF, 0xDFDFE0FF, STYLE_BOLD),   // text.strong
    style(0x0B60A0FF, 0x41A1C0FF, STYLE_BOLD),   // text.title
    style(0x0F68A0FF, 0x6BDFFFFF, 0),            // text.uri
    style(0x3900A0FF, 0x5DD8FFFF, 0),            // type
    style(0x3900A0FF, 0x5DD8FFFF, 0),            // type.builtin
    style(0xAD3DA4FF, 0xFC5FA3FF, STYLE_ITALIC), // variable.builtin
    style(0x24292EFF, 0xDFDFE0FF, STYLE_ITALIC), // variable.parameter
];

/// Maximum-legibility palette: near-black saturated colors on light,
/// bright saturated colors on dark (textchum's high-contrast set).
pub static HIGH_CONTRAST_STYLES: &[Style] = &[
    style(0x664400FF, 0xFFCC66FF, 0),            // attribute
    style(0x3D4C59FF, 0xA8B5C2FF, STYLE_ITALIC), // comment
    style(0x4B0082FF, 0xCC99FFFF, 0),            // constant
    style(0x8B008BFF, 0xFF66CCFF, 0),            // constant.builtin
    style(0x004D40FF, 0x66FFCCFF, 0),            // constructor
    style(0x003D66FF, 0x66E0FFFF, 0),            // escape
    style(0x004D40FF, 0x66FFCCFF, 0),            // function
    style(0x004D40FF, 0x66FFCCFF, 0),            // function.builtin
    style(0x8B008BFF, 0xFF66CCFF, STYLE_BOLD),   // keyword
    style(0x664400FF, 0xFFCC66FF, 0),            // label
    style(0x1A0099FF, 0x80DFFFFF, 0),            // module
    style(0x0000CCFF, 0xFFE066FF, 0),            // number
    style(0x1A2633FF, 0xD0D8E0FF, 0),            // operator
    style(0x00456AFF, 0x99E0BBFF, 0),            // property
    style(0x1A2633FF, 0xA8B5C2FF, 0),            // punctuation
    style(0x8B008BFF, 0xFF66CCFF, 0),            // punctuation.special
    style(0x990000FF, 0xFF8073FF, 0),            // string
    style(0x003D66FF, 0xFFB066FF, 0),            // string.special
    style(0x8B008BFF, 0xFF66CCFF, 0),            // tag
    style(0x000000FF, 0xFFFFFFFF, STYLE_ITALIC), // text.emphasis
    style(0x990000FF, 0xFF8073FF, 0),            // text.literal
    style(0x003D66FF, 0x80EFFFFF, 0),            // text.reference
    style(0x000000FF, 0xFFFFFFFF, STYLE_BOLD),   // text.strong
    style(0x003366FF, 0x66C2FFFF, STYLE_BOLD),   // text.title
    style(0x003D66FF, 0x80EFFFFF, 0),            // text.uri
    style(0x1A0099FF, 0x80DFFFFF, 0),            // type
    style(0x1A0099FF, 0x80DFFFFF, 0),            // type.builtin
    style(0x8B008BFF, 0xFF66CCFF, STYLE_ITALIC), // variable.builtin
    style(0x000000FF, 0xFFFFFFFF, STYLE_ITALIC), // variable.parameter
];

/// Built-in theme names, selectable by `theme` in config.json.
pub static BUILTIN_THEMES: &[&str] = &["default", "high-contrast"];

static CURRENT_STYLES: std::sync::RwLock<Option<Vec<Style>>> = std::sync::RwLock::new(None);

/// The active style table.
pub fn styles() -> Vec<Style> {
    CURRENT_STYLES
        .read()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_else(|| DEFAULT_STYLES.to_vec())
}

/// Switches to a built-in theme. `false` for an unknown name.
pub fn set_builtin(name: &str) -> bool {
    let table: &[Style] = match name {
        "" | "default" => DEFAULT_STYLES,
        "high-contrast" => HIGH_CONTRAST_STYLES,
        _ => return false,
    };
    if let Ok(mut guard) = CURRENT_STYLES.write() {
        *guard = Some(table.to_vec());
    }
    true
}

/// Applies a user theme JSON: `{"name": …, "styles": {capture: {"light":
/// "#RRGGBB", "dark": "#RRGGBB", "bold": …, "italic": …}}}`. Anything
/// missing keeps the default palette's value. Errors change nothing.
pub fn set_theme_json(text: &str) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("theme is not valid JSON: {e}"))?;
    let Some(styles_map) = value.get("styles").and_then(|v| v.as_object()) else {
        return Err("theme has no styles object".to_string());
    };
    let mut table: Vec<Style> = DEFAULT_STYLES.to_vec();
    for (capture, entry) in styles_map {
        let Some(id) = resolve(capture) else {
            return Err(format!("unknown style role {capture}"));
        };
        let slot = &mut table[id as usize];
        if let Some(color) = entry.get("light").and_then(|v| v.as_str()) {
            slot.light = parse_color(color).ok_or_else(|| format!("bad color {color}"))?;
        }
        if let Some(color) = entry.get("dark").and_then(|v| v.as_str()) {
            slot.dark = parse_color(color).ok_or_else(|| format!("bad color {color}"))?;
        }
        if let Some(bold) = entry.get("bold").and_then(|v| v.as_bool()) {
            if bold { slot.flags |= STYLE_BOLD } else { slot.flags &= !STYLE_BOLD }
        }
        if let Some(italic) = entry.get("italic").and_then(|v| v.as_bool()) {
            if italic { slot.flags |= STYLE_ITALIC } else { slot.flags &= !STYLE_ITALIC }
        }
    }
    if let Ok(mut guard) = CURRENT_STYLES.write() {
        *guard = Some(table);
    }
    Ok(())
}

/// `#RRGGBB` or `#RRGGBBAA` → 0xRRGGBBAA.
fn parse_color(text: &str) -> Option<u32> {
    let hex = text.strip_prefix('#')?;
    match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok().map(|rgb| (rgb << 8) | 0xFF),
        8 => u32::from_str_radix(hex, 16).ok(),
        _ => None,
    }
}

/// Resolves a capture name to a style id by trimming dotted segments —
/// the tree-sitter convention. None = unstyled (plain text).
pub fn resolve(capture: &str) -> Option<u32> {
    let mut name = capture;
    loop {
        if let Some(index) = CAPTURES.iter().position(|entry| *entry == name) {
            return Some(index as u32);
        }
        match name.rfind('.') {
            Some(dot) => name = &name[..dot],
            None => return None,
        }
    }
}

struct LanguageSpec {
    name: &'static str,
    extensions: &'static [&'static str],
    filenames: &'static [&'static str],
    language: fn() -> Language,
    highlights: &'static str,
}

struct CompiledLanguage {
    language: Language,
    highlights: Query,
    /// Style id for each capture index (None = unstyled).
    capture_styles: Vec<Option<u32>>,
}

struct RegisteredLanguage {
    spec: LanguageSpec,
    compiled: OnceLock<Option<CompiledLanguage>>,
}

impl RegisteredLanguage {
    fn compiled(&self) -> Option<&CompiledLanguage> {
        self.compiled
            .get_or_init(|| {
                let language = (self.spec.language)();
                let highlights = Query::new(&language, self.spec.highlights).ok()?;
                let capture_styles = highlights
                    .capture_names()
                    .iter()
                    .map(|name| resolve(name))
                    .collect();
                Some(CompiledLanguage {
                    language,
                    highlights,
                    capture_styles,
                })
            })
            .as_ref()
    }
}

macro_rules! lang {
    ($name:literal, $exts:expr, $files:expr, $lang:expr, $hl:expr) => {
        RegisteredLanguage {
            spec: LanguageSpec {
                name: $name,
                extensions: $exts,
                filenames: $files,
                language: || $lang.into(),
                highlights: $hl,
            },
            compiled: OnceLock::new(),
        }
    };
}

fn registry() -> &'static [RegisteredLanguage] {
    static REGISTRY: OnceLock<Vec<RegisteredLanguage>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        vec![
            lang!("rust", &["rs"], &[], tree_sitter_rust::LANGUAGE, tree_sitter_rust::HIGHLIGHTS_QUERY),
            lang!("go", &["go"], &[], tree_sitter_go::LANGUAGE, tree_sitter_go::HIGHLIGHTS_QUERY),
            lang!("python", &["py", "pyi"], &[], tree_sitter_python::LANGUAGE, tree_sitter_python::HIGHLIGHTS_QUERY),
            lang!("c", &["c", "h"], &[], tree_sitter_c::LANGUAGE, tree_sitter_c::HIGHLIGHT_QUERY),
            lang!(
                "javascript",
                &["js", "jsx", "mjs", "cjs", "ts"],
                &[],
                tree_sitter_javascript::LANGUAGE,
                tree_sitter_javascript::HIGHLIGHT_QUERY
            ),
            lang!("json", &["json", "jsonc"], &[], tree_sitter_json::LANGUAGE, tree_sitter_json::HIGHLIGHTS_QUERY),
            lang!(
                "bash",
                &["sh", "bash", "zsh"],
                &[],
                tree_sitter_bash::LANGUAGE,
                tree_sitter_bash::HIGHLIGHT_QUERY
            ),
            lang!("html", &["html", "htm"], &[], tree_sitter_html::LANGUAGE, tree_sitter_html::HIGHLIGHTS_QUERY),
            lang!("css", &["css"], &[], tree_sitter_css::LANGUAGE, tree_sitter_css::HIGHLIGHTS_QUERY),
            lang!("toml", &["toml"], &[], tree_sitter_toml_ng::LANGUAGE, tree_sitter_toml_ng::HIGHLIGHTS_QUERY),
            lang!("yaml", &["yaml", "yml"], &[], tree_sitter_yaml::LANGUAGE, tree_sitter_yaml::HIGHLIGHTS_QUERY),
            lang!("swift", &["swift"], &[], tree_sitter_swift::LANGUAGE, tree_sitter_swift::HIGHLIGHTS_QUERY),
            lang!(
                "make",
                &["mk", "mak"],
                &["Makefile", "makefile", "GNUmakefile"],
                tree_sitter_make::LANGUAGE,
                tree_sitter_make::HIGHLIGHTS_QUERY
            ),
            // Block grammar only: headings, lists, fences — inline
            // emphasis and fence injections join with the injection
            // machinery later.
            lang!(
                "markdown",
                &["md", "markdown"],
                &[],
                tree_sitter_md::LANGUAGE,
                tree_sitter_md::HIGHLIGHT_QUERY_BLOCK
            ),
        ]
    })
}

fn language_for_path(path: &str) -> Option<&'static RegisteredLanguage> {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    if let Some(entry) = registry()
        .iter()
        .find(|entry| entry.spec.filenames.iter().any(|name| *name == file_name))
    {
        return Some(entry);
    }
    let extension = file_name.rsplit('.').next()?.to_ascii_lowercase();
    if extension == file_name.to_ascii_lowercase() {
        return None; // no dot at all
    }
    registry()
        .iter()
        .find(|entry| entry.spec.extensions.iter().any(|ext| *ext == extension))
}

/// The language name prchum would highlight `path` with, if any.
pub fn language_name(path: &str) -> Option<&'static str> {
    language_for_path(path).map(|entry| entry.spec.name)
}

/// One styled span within one line: byte offsets into the line's display
/// text, plus a style id.
pub type LineSpan = (u32, u32, u32);

/// Highlights one file's hunks. The result is `[hunk][line index in hunk]`
/// → spans in application order (later wins where they overlap). `None`
/// when the file's language is unknown.
pub fn highlight_file(file: &FileDiff) -> Option<Vec<Vec<Vec<LineSpan>>>> {
    let language = language_for_path(file.display_path())?;
    let compiled = language.compiled()?;
    let mut parser = Parser::new();
    parser.set_language(&compiled.language).ok()?;

    let mut result: Vec<Vec<Vec<LineSpan>>> = Vec::with_capacity(file.hunks.len());
    for hunk in &file.hunks {
        let mut lines: Vec<Vec<LineSpan>> = vec![Vec::new(); hunk.lines.len()];
        // RIGHT pass covers context + additions; LEFT covers deletions
        // (context takes its colors from the RIGHT pass).
        run_side_pass(
            &mut parser,
            compiled,
            hunk,
            |kind| kind != LineKind::Deletion && kind != LineKind::Meta,
            &mut lines,
        );
        run_side_pass(
            &mut parser,
            compiled,
            hunk,
            |kind| kind == LineKind::Deletion,
            &mut lines,
        );
        result.push(lines);
    }
    Some(result)
}

/// Parses the concatenation of the hunk lines selected by `included` and
/// scatters the resulting spans back onto those lines.
fn run_side_pass(
    parser: &mut Parser,
    compiled: &CompiledLanguage,
    hunk: &crate::diff::Hunk,
    included: impl Fn(LineKind) -> bool,
    out: &mut [Vec<LineSpan>],
) {
    let mut text = String::new();
    // (line index in hunk, byte offset of its start in `text`).
    let mut mapping: Vec<(usize, usize)> = Vec::new();
    for (index, line) in hunk.lines.iter().enumerate() {
        if !included(line.kind) {
            continue;
        }
        mapping.push((index, text.len()));
        text.push_str(&line.text);
        text.push('\n');
    }
    if mapping.is_empty() {
        return;
    }
    let Some(tree) = parser.parse(&text, None) else {
        return;
    };

    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(&compiled.highlights, tree.root_node(), text.as_bytes());
    while let Some((matched, capture_index)) = captures.next() {
        let capture = matched.captures[*capture_index];
        let Some(style) = compiled.capture_styles[capture.index as usize] else {
            continue;
        };
        let start = capture.node.start_byte();
        let end = capture.node.end_byte().min(text.len());
        if start >= end {
            continue;
        }
        // Scatter the span over every line it touches.
        for (line_index, line_start) in &mapping {
            let line_len = hunk.lines[*line_index].text.len();
            let line_end = line_start + line_len;
            let span_start = start.max(*line_start);
            let span_end = end.min(line_end);
            if span_start < span_end {
                out[*line_index].push((
                    (span_start - line_start) as u32,
                    (span_end - line_start) as u32,
                    style,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse;

    #[test]
    fn resolves_by_trimming_segments() {
        assert_eq!(resolve("keyword"), resolve("keyword.control.flow"));
        assert!(resolve("keyword").is_some());
        assert_eq!(resolve("variable"), None, "plain text stays unstyled");
        assert_eq!(CAPTURES.len(), DEFAULT_STYLES.len());
        assert_eq!(CAPTURES.len(), HIGH_CONTRAST_STYLES.len());
    }

    #[test]
    fn language_detection() {
        assert_eq!(language_name("src/main.rs"), Some("rust"));
        assert_eq!(language_name("a/b/handler.go"), Some("go"));
        assert_eq!(language_name("Makefile"), Some("make"));
        assert_eq!(language_name("README"), None);
        assert_eq!(language_name("photo.jpg"), None);
    }

    #[test]
    fn highlights_both_sides_of_a_rust_hunk() {
        let files = parse(
            "--- a/x.rs\n+++ b/x.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    let old = \"gone\";\n+    let fresh = \"here\";\n }\n",
            4,
        )
        .unwrap();
        let highlights = highlight_file(&files[0]).expect("rust is supported");
        assert_eq!(highlights.len(), 1);
        let hunk = &highlights[0];
        assert_eq!(hunk.len(), 4);
        // The context `fn` keyword is styled.
        let keyword = resolve("keyword").unwrap();
        assert!(hunk[0].iter().any(|s| s.2 == keyword), "{:?}", hunk[0]);
        // The deletion (LEFT pass) and the addition (RIGHT pass) both get
        // string spans.
        let string = resolve("string").unwrap();
        assert!(hunk[1].iter().any(|s| s.2 == string), "{:?}", hunk[1]);
        assert!(hunk[2].iter().any(|s| s.2 == string), "{:?}", hunk[2]);
        // Span offsets stay within their line.
        for line in hunk {
            for (start, end, _) in line {
                assert!(start < end);
            }
        }
    }

    #[test]
    fn unknown_language_is_none() {
        let files = parse("--- a/data.bin\n+++ b/data.bin\n@@ -1 +1 @@\n-a\n+b\n", 4).unwrap();
        assert!(highlight_file(&files[0]).is_none());
    }
}
