//! The rows a reviewer actually sees, decided once for every shell.
//!
//! A diff is hunks of lines; a *rendering* is the flat sequence of rows a
//! window scrolls through, each knowing which side and line it stands
//! for. That mapping is the thing comments anchor through, so it must
//! not be decided twice — a shell that computes it itself will disagree
//! with the other one eventually, and the disagreement will show up as a
//! comment landing on the wrong line.
//!
//! So the core decides the rows, their markers, their line numbers and
//! their styled spans. A shell only paints them.

use crate::diff::{DiffLine, FileDiff, LineKind};
use crate::syntax::LineSpan;

/// What a row stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum RowKind {
    /// The `@@ -a,b +c,d @@` line that opens a hunk.
    HunkHeader,
    Context,
    Addition,
    Deletion,
    /// `\ No newline at end of file` and friends.
    Meta,
}

impl RowKind {
    /// The character in the marker column, as a diff writes it.
    pub fn marker(self) -> char {
        match self {
            RowKind::Addition => '+',
            RowKind::Deletion => '-',
            RowKind::HunkHeader | RowKind::Context | RowKind::Meta => ' ',
        }
    }
}

/// A run of text sharing one style id, as byte offsets into `Row::text`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StyledSpan {
    pub start: usize,
    pub end: usize,
    /// Index into [`crate::syntax::styles`].
    pub style: u32,
}

/// One row of a rendered file.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Row {
    pub kind: RowKind,
    /// 1-based, when this row exists on that side.
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    /// The text without the marker column: what the file actually says.
    pub text: String,
    pub spans: Vec<StyledSpan>,
    /// Which hunk this row belongs to, for folding and navigation.
    pub hunk: usize,
}

impl Row {
    /// True when this row is part of the change rather than its
    /// surroundings — what "next change" steps between.
    pub fn is_change(&self) -> bool {
        matches!(self.kind, RowKind::Addition | RowKind::Deletion)
    }
}

/// A file's rows, plus where each hunk starts, so a shell can fold and
/// jump without re-deriving any of it.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct RenderedFile {
    pub rows: Vec<Row>,
    /// Index into `rows` of each hunk's header.
    pub hunk_starts: Vec<usize>,
}

impl RenderedFile {
    /// The row holding `line` on `side`, if the rendering has one.
    ///
    /// This is the lookup a caret uses to name where it is, and the one
    /// a comment uses to find its way back after the file re-renders.
    pub fn row_for(&self, side_is_new: bool, line: u32) -> Option<usize> {
        self.rows.iter().position(|row| {
            if side_is_new {
                row.new_line == Some(line)
            } else {
                row.old_line == Some(line)
            }
        })
    }
}

fn kind_of(line: &DiffLine) -> RowKind {
    match line.kind {
        LineKind::Context => RowKind::Context,
        LineKind::Addition => RowKind::Addition,
        LineKind::Deletion => RowKind::Deletion,
        LineKind::Meta => RowKind::Meta,
    }
}

/// Turns a file's hunks into rows, carrying syntax spans across when
/// they were computed.
///
/// `highlights` is indexed `[hunk][line]`, the shape
/// [`crate::syntax::highlight_file`] returns. Rows for lines it has no
/// entry for simply carry no spans, so an unknown language renders as
/// plain text rather than not at all.
pub fn render_file(
    file: &FileDiff,
    highlights: Option<&[Vec<Vec<LineSpan>>]>,
) -> RenderedFile {
    let mut rendered = RenderedFile::default();

    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        rendered.hunk_starts.push(rendered.rows.len());
        rendered.rows.push(Row {
            kind: RowKind::HunkHeader,
            old_line: None,
            new_line: None,
            text: hunk.header.clone(),
            spans: Vec::new(),
            hunk: hunk_index,
        });

        for (line_index, line) in hunk.lines.iter().enumerate() {
            let spans = highlights
                .and_then(|table| table.get(hunk_index))
                .and_then(|hunk_spans| hunk_spans.get(line_index))
                .map(|spans| {
                    spans
                        .iter()
                        .map(|(start, end, style)| StyledSpan {
                            start: *start as usize,
                            end: *end as usize,
                            style: *style,
                        })
                        .collect()
                })
                .unwrap_or_default();

            rendered.rows.push(Row {
                kind: kind_of(line),
                old_line: line.old_line,
                new_line: line.new_line,
                text: line.text.clone(),
                spans,
                hunk: hunk_index,
            });
        }
    }

    rendered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse;

    const PATCH: &str = "\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 fn one() {}
-fn two() {}
+fn two() -> u8 { 2 }
+fn three() {}
 fn four() {}
";

    fn rendered() -> RenderedFile {
        let files = parse(PATCH, 4).unwrap();
        render_file(&files[0], None)
    }

    #[test]
    fn rows_carry_their_sides_and_markers() {
        let file = rendered();
        assert_eq!(file.hunk_starts, vec![0]);
        assert_eq!(file.rows[0].kind, RowKind::HunkHeader);
        assert!(file.rows[0].text.starts_with("@@"));

        let kinds: Vec<RowKind> = file.rows[1..].iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            vec![
                RowKind::Context,
                RowKind::Deletion,
                RowKind::Addition,
                RowKind::Addition,
                RowKind::Context
            ]
        );

        // A deletion is on the left only, an addition on the right only.
        let deletion = &file.rows[2];
        assert!(deletion.old_line.is_some() && deletion.new_line.is_none());
        assert_eq!(deletion.kind.marker(), '-');
        let addition = &file.rows[3];
        assert!(addition.new_line.is_some() && addition.old_line.is_none());
        assert_eq!(addition.kind.marker(), '+');
    }

    #[test]
    fn a_row_can_be_found_by_side_and_line() {
        let file = rendered();
        // The first addition is new-side line 2.
        let row = file.row_for(true, 2).expect("a row on the new side");
        assert_eq!(file.rows[row].kind, RowKind::Addition);
        assert_eq!(file.rows[row].text, "fn two() -> u8 { 2 }");

        // Old-side line 2 is the deletion, a different row.
        let old = file.row_for(false, 2).expect("a row on the old side");
        assert_ne!(old, row);
        assert_eq!(file.rows[old].kind, RowKind::Deletion);

        assert!(file.row_for(true, 999).is_none());
    }

    #[test]
    fn changes_are_distinguishable_from_their_surroundings() {
        let file = rendered();
        let changes = file.rows.iter().filter(|r| r.is_change()).count();
        assert_eq!(changes, 3, "one deletion and two additions");
    }

    #[test]
    fn missing_highlights_render_as_plain_rows() {
        let files = parse(PATCH, 4).unwrap();
        // A table too short for the file: rows still render, without spans.
        let empty: Vec<Vec<Vec<LineSpan>>> = Vec::new();
        let file = render_file(&files[0], Some(&empty));
        assert!(!file.rows.is_empty());
        assert!(file.rows.iter().all(|row| row.spans.is_empty()));
    }

    #[test]
    fn highlights_land_on_the_right_rows() {
        let files = parse(PATCH, 4).unwrap();
        // One hunk; give the second line (the deletion) a single span.
        let mut table: Vec<Vec<Vec<LineSpan>>> = vec![vec![Vec::new(); files[0].hunks[0].lines.len()]];
        table[0][1] = vec![(0, 2, 7)];
        let file = render_file(&files[0], Some(&table));

        // Row 0 is the header, so hunk line 1 is row 2.
        assert_eq!(file.rows[2].kind, RowKind::Deletion);
        assert_eq!(
            file.rows[2].spans,
            vec![StyledSpan { start: 0, end: 2, style: 7 }]
        );
        assert!(file.rows[1].spans.is_empty());
    }
}
