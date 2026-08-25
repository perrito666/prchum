//! Semantic locations and their context anchors.
//!
//! The application's load-bearing rule: rendered rows are never canonical
//! comment locations. A comment anchors to `(path, side, line range)` plus
//! the text around it, so it survives layout changes and — via
//! [`relocate`] — head-commit moves.

use serde::{Deserialize, Serialize};

use crate::diff::{FileDiff, LineKind, Side};
use crate::util::fnv64_hex;

/// How many surrounding lines an anchor captures on each side.
const ANCHOR_CONTEXT: usize = 3;

/// The text around an anchored line, captured when the comment is made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextAnchor {
    pub hunk_header: String,
    /// Up to [`ANCHOR_CONTEXT`] lines before the anchor, on the same side.
    pub before: Vec<String>,
    /// The anchored line's display text.
    pub anchor_text: String,
    /// Up to [`ANCHOR_CONTEXT`] lines after the anchor, on the same side.
    pub after: Vec<String>,
    /// Stable hash of before + anchor + after, joined by newlines.
    pub content_hash: String,
}

/// A semantic diff location: the canonical form every comment anchors to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub path: String,
    pub side: Side,
    /// 1-based, inclusive, on `side`.
    pub start_line: u32,
    pub end_line: u32,
    pub anchor: ContextAnchor,
}

/// Why a location could not be built.
#[derive(Debug, PartialEq)]
pub enum LocationError {
    /// A line in the range does not exist on that side of the diff.
    MissingLine(u32),
    /// The range spans more than one hunk (hosts reject that).
    CrossesHunks,
    /// start > end or a zero line number.
    InvalidRange,
    /// No such file in the diff.
    UnknownFile,
}

impl std::fmt::Display for LocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLine(line) => {
                write!(f, "line {line} is not part of the diff on that side")
            }
            Self::CrossesHunks => write!(f, "a comment cannot span two hunks"),
            Self::InvalidRange => write!(f, "invalid line range"),
            Self::UnknownFile => write!(f, "no such file in the diff"),
        }
    }
}

impl std::error::Error for LocationError {}

/// Is this diff line visible on `side`? Context lines are on both sides.
fn on_side(kind: LineKind, side: Side) -> bool {
    match kind {
        LineKind::Context => true,
        LineKind::Addition => side == Side::Right,
        LineKind::Deletion => side == Side::Left,
        LineKind::Meta => false,
    }
}

fn side_number(line: &crate::diff::DiffLine, side: Side) -> Option<u32> {
    match side {
        Side::Left => line.old_line,
        Side::Right => line.new_line,
    }
}

/// Finds `(hunk index, line index)` of `line` on `side` within `file`.
pub fn find_line(file: &FileDiff, side: Side, line: u32) -> Option<(usize, usize)> {
    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        for (line_index, candidate) in hunk.lines.iter().enumerate() {
            if on_side(candidate.kind, side) && side_number(candidate, side) == Some(line) {
                return Some((hunk_index, line_index));
            }
        }
    }
    None
}

/// Builds the canonical location for a range on one side of one file,
/// validating host semantics (every line present, one hunk) and capturing
/// the context anchor.
pub fn build_location(
    file: &FileDiff,
    side: Side,
    start_line: u32,
    end_line: u32,
) -> Result<Location, LocationError> {
    if start_line == 0 || end_line < start_line {
        return Err(LocationError::InvalidRange);
    }
    let mut hunk_of_range: Option<usize> = None;
    for line in start_line..=end_line {
        let Some((hunk_index, _)) = find_line(file, side, line) else {
            return Err(LocationError::MissingLine(line));
        };
        match hunk_of_range {
            None => hunk_of_range = Some(hunk_index),
            Some(existing) if existing != hunk_index => {
                return Err(LocationError::CrossesHunks);
            }
            Some(_) => {}
        }
    }
    let anchor = capture_anchor(file, side, start_line).ok_or(LocationError::InvalidRange)?;
    Ok(Location {
        path: file.display_path().to_string(),
        side,
        start_line,
        end_line,
        anchor,
    })
}

/// Captures the anchor around `line` on `side`: the same-side lines before
/// and after it within its hunk.
pub fn capture_anchor(file: &FileDiff, side: Side, line: u32) -> Option<ContextAnchor> {
    let (hunk_index, line_index) = find_line(file, side, line)?;
    let hunk = &file.hunks[hunk_index];
    let visible: Vec<(usize, &str)> = hunk
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| on_side(l.kind, side))
        .map(|(i, l)| (i, l.text.as_str()))
        .collect();
    let position = visible.iter().position(|(i, _)| *i == line_index)?;
    let before: Vec<String> = visible[position.saturating_sub(ANCHOR_CONTEXT)..position]
        .iter()
        .map(|(_, text)| text.to_string())
        .collect();
    let after: Vec<String> = visible[position + 1..]
        .iter()
        .take(ANCHOR_CONTEXT)
        .map(|(_, text)| text.to_string())
        .collect();
    let anchor_text = visible[position].1.to_string();
    let content_hash = anchor_hash(&before, &anchor_text, &after);
    Some(ContextAnchor {
        hunk_header: hunk.header.clone(),
        before,
        anchor_text,
        after,
        content_hash,
    })
}

fn anchor_hash(before: &[String], anchor: &str, after: &[String]) -> String {
    let mut joined = before.join("\n");
    joined.push('\n');
    joined.push_str(anchor);
    joined.push('\n');
    joined.push_str(&after.join("\n"));
    fnv64_hex(joined.as_bytes())
}

/// The verbatim text of a location's lines, for snippets and suggestions.
pub fn snippet(file: &FileDiff, side: Side, start_line: u32, end_line: u32) -> String {
    let mut lines = Vec::new();
    for line in start_line..=end_line {
        if let Some((hunk_index, line_index)) = find_line(file, side, line) {
            lines.push(file.hunks[hunk_index].lines[line_index].raw_text().to_string());
        }
    }
    lines.join("\n")
}

/// The outcome of re-anchoring a location against a (possibly new) diff.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RelocateResult {
    /// The location still holds: same line, same context.
    Exact,
    /// The anchor text was found at exactly one other line.
    Moved,
    /// No line — or more than one — matches. Guessing would put the note
    /// on the wrong line, so the location is left as it was.
    Orphaned,
}

/// Finds the file a location refers to, following renames.
pub fn find_file<'a>(files: &'a [FileDiff], path: &str) -> Option<&'a FileDiff> {
    files
        .iter()
        .find(|f| f.display_path() == path)
        .or_else(|| files.iter().find(|f| f.old_path == path || f.new_path == path))
}

/// Re-anchors `location` against `files`. Conservative by design: a
/// non-unique match orphans rather than guesses.
pub fn relocate(files: &[FileDiff], location: &Location) -> (Location, RelocateResult) {
    let Some(file) = find_file(files, &location.path) else {
        return (location.clone(), RelocateResult::Orphaned);
    };

    // Exact: the recorded line still carries the same anchor and context.
    if let Some(anchor) = capture_anchor(file, location.side, location.start_line) {
        if anchor.content_hash == location.anchor.content_hash {
            return (location.clone(), RelocateResult::Exact);
        }
    }

    // Moved: a unique line with the same anchor text.
    let mut matches: Vec<u32> = Vec::new();
    for hunk in &file.hunks {
        for line in &hunk.lines {
            if on_side(line.kind, location.side)
                && line.text == location.anchor.anchor_text
            {
                if let Some(number) = side_number(line, location.side) {
                    matches.push(number);
                }
            }
        }
    }
    if matches.len() != 1 {
        return (location.clone(), RelocateResult::Orphaned);
    }
    let span = location.end_line - location.start_line;
    let start = matches[0];
    match build_location(file, location.side, start, start + span) {
        Ok(moved) => (moved, RelocateResult::Moved),
        Err(_) => (location.clone(), RelocateResult::Orphaned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse;

    fn sample() -> Vec<FileDiff> {
        parse(
            "\
diff --git a/main.rs b/main.rs
--- a/main.rs
+++ b/main.rs
@@ -1,4 +1,4 @@
 fn main() {
-    old();
+    new();
     tail();
 }
@@ -10,3 +10,3 @@
 fn other() {
-    a();
+    b();
 }
",
            4,
        )
        .unwrap()
    }

    #[test]
    fn build_and_snippet() {
        let files = sample();
        let location = build_location(&files[0], Side::Right, 2, 3).unwrap();
        assert_eq!(location.path, "main.rs");
        assert_eq!(location.anchor.anchor_text, "    new();");
        assert_eq!(location.anchor.before, vec!["fn main() {"]);
        assert_eq!(location.anchor.after, vec!["    tail();", "}"]);
        assert_eq!(snippet(&files[0], Side::Right, 2, 3), "    new();\n    tail();");
    }

    #[test]
    fn left_side_sees_deletions() {
        let files = sample();
        let location = build_location(&files[0], Side::Left, 2, 2).unwrap();
        assert_eq!(location.anchor.anchor_text, "    old();");
        // The addition is invisible on the left.
        assert!(build_location(&files[0], Side::Left, 5, 5).is_err());
    }

    #[test]
    fn validation_rejects_bad_ranges() {
        let files = sample();
        assert_eq!(
            build_location(&files[0], Side::Right, 3, 2).unwrap_err(),
            LocationError::InvalidRange
        );
        assert_eq!(
            build_location(&files[0], Side::Right, 7, 7).unwrap_err(),
            LocationError::MissingLine(7)
        );
        // A line in the gap between hunks is simply missing on that side.
        assert_eq!(
            build_location(&files[0], Side::Right, 4, 10).unwrap_err(),
            LocationError::MissingLine(5)
        );
        // Adjacent hunks make a numerically continuous range that still
        // crosses a hunk boundary — hosts reject that.
        let adjacent = parse(
            "--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n one\n two\n@@ -3,2 +3,2 @@\n three\n four\n",
            4,
        )
        .unwrap();
        assert_eq!(
            build_location(&adjacent[0], Side::Right, 2, 3).unwrap_err(),
            LocationError::CrossesHunks
        );
    }

    #[test]
    fn relocate_exact_moved_orphaned() {
        let files = sample();
        let location = build_location(&files[0], Side::Right, 2, 2).unwrap();
        assert_eq!(relocate(&files, &location).1, RelocateResult::Exact);

        // The same change shifted down two lines in a new diff.
        let shifted = parse(
            "\
--- a/main.rs
+++ b/main.rs
@@ -1,6 +1,6 @@
 // new
 // header
 fn main() {
-    old();
+    new();
     tail();
 }
",
            4,
        )
        .unwrap();
        let (moved, result) = relocate(&shifted, &location);
        assert_eq!(result, RelocateResult::Moved);
        assert_eq!(moved.start_line, 4);

        // Ambiguous: the anchor text appears twice.
        let ambiguous = parse(
            "\
--- a/main.rs
+++ b/main.rs
@@ -1,4 +1,6 @@
 fn main() {
+    new();
+    new();
     tail();
 }
",
            4,
        )
        .unwrap();
        assert_eq!(relocate(&ambiguous, &location).1, RelocateResult::Orphaned);

        // Gone entirely.
        let gone = parse("--- a/main.rs\n+++ b/main.rs\n@@ -1 +1 @@\n-x\n+y\n", 4).unwrap();
        assert_eq!(relocate(&gone, &location).1, RelocateResult::Orphaned);
    }

    #[test]
    fn relocate_follows_renames() {
        let files = sample();
        let location = build_location(&files[0], Side::Right, 2, 2).unwrap();
        let renamed = parse(
            "\
diff --git a/main.rs b/renamed.rs
rename from main.rs
rename to renamed.rs
--- a/main.rs
+++ b/renamed.rs
@@ -1,4 +1,4 @@
 fn main() {
-    old();
+    new();
     tail();
 }
",
            4,
        )
        .unwrap();
        let (moved, result) = relocate(&renamed, &location);
        // Exact still applies (same line, same context) even under rename.
        assert_eq!(result, RelocateResult::Exact);
        assert_eq!(moved.start_line, 2);
    }
}
