//! The context view: the whole new file with the diff's hunks overlaid —
//! an unbounded `-U`, as a synthetic [`FileDiff`] so every downstream
//! consumer (rendering, navigation, commenting) works unchanged.
//!
//! Gap lines — file lines outside every hunk — carry both line numbers
//! but are not part of the diff, so commenting on one fails validation
//! against the real diff, exactly as it should: hosts anchor to diff
//! positions.
//!
//! Every context and addition line is **verified against the file
//! content**; a mismatch aborts with an error rather than rendering a lie.

use crate::diff::{DiffLine, FileDiff, Hunk, LineKind};

/// Builds the whole-file projection from the new side's content.
pub fn context_file(file: &FileDiff, new_content: &str, tab_width: usize) -> Result<FileDiff, String> {
    if file.is_binary {
        return Err("binary files have no context view".to_string());
    }
    let content_lines: Vec<String> = new_content
        .trim_end_matches('\n')
        .split('\n')
        .map(|line| expand_tabs(line, tab_width))
        .collect();

    let mut hunks: Vec<Hunk> = Vec::new();
    let mut gap = Hunk {
        header: String::new(),
        lines: Vec::new(),
    };
    // The old-file line a gap row maps to: new line + running offset.
    let mut offset: i64 = 0;
    let mut next_new_line: usize = 1;

    for hunk in &file.hunks {
        let hunk_start = hunk
            .lines
            .iter()
            .find_map(|line| line.new_line)
            .unwrap_or(next_new_line as u32) as usize;

        // Gap rows up to the hunk.
        while next_new_line < hunk_start {
            let Some(text) = content_lines.get(next_new_line - 1) else {
                return Err(format!(
                    "the file ends at line {} but the diff expects line {hunk_start}",
                    content_lines.len()
                ));
            };
            gap.lines.push(DiffLine {
                kind: LineKind::Context,
                text: text.clone(),
                raw: None,
                old_line: Some((next_new_line as i64 + offset) as u32),
                new_line: Some(next_new_line as u32),
                patch_position: None,
            });
            next_new_line += 1;
        }
        if !gap.lines.is_empty() {
            hunks.push(std::mem::replace(
                &mut gap,
                Hunk {
                    header: String::new(),
                    lines: Vec::new(),
                },
            ));
        }

        // The hunk itself, verified line by line against the content.
        let mut old_count: i64 = 0;
        let mut new_count: i64 = 0;
        for line in &hunk.lines {
            match line.kind {
                LineKind::Context | LineKind::Addition => {
                    let Some(new_line) = line.new_line else { continue };
                    let file_text = content_lines.get(new_line as usize - 1);
                    if file_text.map(String::as_str) != Some(line.text.as_str()) {
                        return Err(format!(
                            "line {new_line} of the fetched file does not match the diff — \
                             the content is from a different version"
                        ));
                    }
                    new_count += 1;
                    if line.kind == LineKind::Context {
                        old_count += 1;
                    }
                    next_new_line = new_line as usize + 1;
                }
                LineKind::Deletion => old_count += 1,
                LineKind::Meta => {}
            }
        }
        offset += old_count - new_count;
        hunks.push(hunk.clone());
    }

    // The tail after the last hunk.
    while next_new_line <= content_lines.len() {
        gap.lines.push(DiffLine {
            kind: LineKind::Context,
            text: content_lines[next_new_line - 1].clone(),
            raw: None,
            old_line: Some((next_new_line as i64 + offset) as u32),
            new_line: Some(next_new_line as u32),
            patch_position: None,
        });
        next_new_line += 1;
    }
    if !gap.lines.is_empty() {
        hunks.push(gap);
    }

    Ok(FileDiff {
        old_path: file.old_path.clone(),
        new_path: file.new_path.clone(),
        status: file.status,
        hunks,
        is_binary: false,
    })
}

fn expand_tabs(text: &str, tab_width: usize) -> String {
    if !text.contains('\t') {
        return text.to_string();
    }
    let width = tab_width.max(1);
    let mut out = String::with_capacity(text.len());
    let mut column = 0usize;
    for ch in text.chars() {
        if ch == '\t' {
            let pad = width - (column % width);
            out.extend(std::iter::repeat(' ').take(pad));
            column += pad;
        } else {
            out.push(ch);
            column += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse;

    const PATCH: &str = "\
--- a/x.txt
+++ b/x.txt
@@ -3,3 +3,3 @@
 three
-old four
+four
 five
";
    const CONTENT: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";

    #[test]
    fn gaps_hunks_and_tail() {
        let files = parse(PATCH, 4).unwrap();
        let context = context_file(&files[0], CONTENT, 4).unwrap();
        // Leading gap, the hunk, trailing gap.
        assert_eq!(context.hunks.len(), 3);
        assert!(context.hunks[0].header.is_empty());
        assert_eq!(context.hunks[0].lines.len(), 2); // one, two
        assert_eq!(context.hunks[0].lines[0].new_line, Some(1));
        assert_eq!(context.hunks[0].lines[0].old_line, Some(1));
        assert_eq!(context.hunks[1].header, "@@ -3,3 +3,3 @@");
        // Tail: six, seven — same old/new numbers (offset zero here).
        assert_eq!(context.hunks[2].lines.len(), 2);
        assert_eq!(context.hunks[2].lines[0].new_line, Some(6));
        assert_eq!(context.hunks[2].lines[0].old_line, Some(6));
    }

    #[test]
    fn gap_old_numbers_follow_the_offset() {
        // The hunk adds a line, so tail old numbers lag by one.
        let patch = "--- a/x\n+++ b/x\n@@ -1,2 +1,3 @@\n one\n+extra\n two\n";
        let files = parse(patch, 4).unwrap();
        let context = context_file(&files[0], "one\nextra\ntwo\nthree\n", 4).unwrap();
        let tail = context.hunks.last().unwrap();
        assert_eq!(tail.lines[0].new_line, Some(4));
        assert_eq!(tail.lines[0].old_line, Some(3));
    }

    #[test]
    fn mismatched_content_is_refused() {
        let files = parse(PATCH, 4).unwrap();
        let err = context_file(&files[0], "totally\ndifferent\nfile\n", 4).unwrap_err();
        assert!(err.contains("does not match"), "{err}");
    }

    #[test]
    fn short_content_is_refused() {
        let files = parse(PATCH, 4).unwrap();
        assert!(context_file(&files[0], "one\n", 4).is_err());
    }
}
