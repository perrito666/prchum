//! The canonical diff model and a unified-diff parser.
//!
//! The load-bearing rule of the whole application lives here: rendered rows
//! are never canonical locations. Everything downstream — comments, threads,
//! relocation — anchors to `(path, side, line)` in this model, and layouts
//! are projections over it.

use serde::{Deserialize, Serialize};

/// Which side of the change a line (or a comment) belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    /// The old file: deletions and their context.
    Left,
    /// The new file: additions and context. The default side.
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Context,
    Addition,
    Deletion,
    /// `\ No newline at end of file` and friends.
    Meta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Binary,
}

/// One line of a hunk.
///
/// `text` is the display form with tabs expanded; `raw` keeps the verbatim
/// original when expansion changed it. Anything that leaves the program as
/// code (patches, suggestion fences) must use [`DiffLine::raw_text`].
#[derive(Clone, Debug, Serialize)]
pub struct DiffLine {
    pub kind: LineKind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    /// 1-based line number in the old file; `None` for additions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_line: Option<u32>,
    /// 1-based line number in the new file; `None` for deletions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_line: Option<u32>,
    /// GitHub-style position: 1-based offset from the file's first `@@`
    /// header, where subsequent `@@` headers themselves consume a position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_position: Option<u32>,
}

impl DiffLine {
    /// The verbatim form for anything leaving the program as code.
    pub fn raw_text(&self) -> &str {
        self.raw.as_deref().unwrap_or(&self.text)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Hunk {
    /// The full `@@ -a,b +c,d @@ context` header line.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub status: FileStatus,
    pub hunks: Vec<Hunk>,
    pub is_binary: bool,
}

impl FileDiff {
    /// The path a reviewer refers to this file by: the new path, or the old
    /// one for deletions.
    pub fn display_path(&self) -> &str {
        if self.status == FileStatus::Deleted {
            &self.old_path
        } else {
            &self.new_path
        }
    }

    /// (additions, deletions) across all hunks.
    pub fn change_counts(&self) -> (u32, u32) {
        let mut added = 0;
        let mut deleted = 0;
        for hunk in &self.hunks {
            for line in &hunk.lines {
                match line.kind {
                    LineKind::Addition => added += 1,
                    LineKind::Deletion => deleted += 1,
                    _ => {}
                }
            }
        }
        (added, deleted)
    }
}

/// Columns a tab expands to in display text.
pub const DEFAULT_TAB_WIDTH: usize = 4;

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parses a unified diff (git or plain) into the canonical model.
pub fn parse(patch: &str, tab_width: usize) -> Result<Vec<FileDiff>, ParseError> {
    Parser::new(tab_width).parse(patch)
}

struct Parser {
    tab_width: usize,
    files: Vec<FileDiff>,
    /// Position counter for the current file, `None` before its first `@@`.
    position: Option<u32>,
    old_line: u32,
    new_line: u32,
    in_hunk: bool,
}

impl Parser {
    fn new(tab_width: usize) -> Self {
        Self {
            tab_width,
            files: Vec::new(),
            position: None,
            old_line: 0,
            new_line: 0,
            in_hunk: false,
        }
    }

    fn parse(mut self, patch: &str) -> Result<Vec<FileDiff>, ParseError> {
        for (index, line) in patch.lines().enumerate() {
            self.line(index + 1, line)?;
        }
        let files = self.files;
        if files.is_empty() || files.iter().all(|f| f.hunks.is_empty() && !f.is_binary) {
            return Err(ParseError {
                message: "no changes found in input".to_string(),
            });
        }
        Ok(files)
    }

    fn line(&mut self, number: usize, line: &str) -> Result<(), ParseError> {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            self.start_file(rest);
            return Ok(());
        }
        if let Some(header) = line.strip_prefix("@@ ") {
            return self.start_hunk(number, header, line);
        }
        if self.in_hunk {
            return self.hunk_line(number, line);
        }
        self.file_header(line);
        Ok(())
    }

    fn start_file(&mut self, names: &str) {
        self.position = None;
        self.in_hunk = false;
        // "a/old b/new"; paths with spaces are refined by ---/+++ lines.
        let (old_path, new_path) = split_git_names(names);
        self.files.push(FileDiff {
            old_path,
            new_path,
            status: FileStatus::Modified,
            hunks: Vec::new(),
            is_binary: false,
        });
    }

    /// A file-header line between `diff --git` and the first hunk — or, for
    /// plain (non-git) diffs, the `---`/`+++` pair that starts a file.
    fn file_header(&mut self, line: &str) {
        if let Some(path) = line.strip_prefix("--- ") {
            if self.current().is_none() || !self.current().unwrap().hunks.is_empty() {
                // Plain unified diff without a `diff --git` opener.
                self.position = None;
                self.files.push(FileDiff {
                    old_path: String::new(),
                    new_path: String::new(),
                    status: FileStatus::Modified,
                    hunks: Vec::new(),
                    is_binary: false,
                });
            }
            let path = strip_prefix_marker(path);
            let file = self.current().unwrap();
            if !path.is_empty() {
                file.old_path = path;
            }
            if file.old_path == "/dev/null" {
                file.status = FileStatus::Added;
            }
            return;
        }
        if let Some(path) = line.strip_prefix("+++ ") {
            if let Some(file) = self.current() {
                let path = strip_prefix_marker(path);
                if !path.is_empty() {
                    file.new_path = path;
                }
                if file.new_path == "/dev/null" {
                    file.status = FileStatus::Deleted;
                }
            }
            return;
        }
        let Some(file) = self.files.last_mut() else {
            return;
        };
        if line.starts_with("new file mode") {
            file.status = FileStatus::Added;
        } else if line.starts_with("deleted file mode") {
            file.status = FileStatus::Deleted;
        } else if let Some(from) = line.strip_prefix("rename from ") {
            file.status = FileStatus::Renamed;
            file.old_path = from.to_string();
        } else if let Some(to) = line.strip_prefix("rename to ") {
            file.status = FileStatus::Renamed;
            file.new_path = to.to_string();
        } else if let Some(from) = line.strip_prefix("copy from ") {
            file.status = FileStatus::Copied;
            file.old_path = from.to_string();
        } else if let Some(to) = line.strip_prefix("copy to ") {
            file.status = FileStatus::Copied;
            file.new_path = to.to_string();
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            file.is_binary = true;
            file.status = FileStatus::Binary;
        }
        // index/mode/similarity lines carry nothing the model needs.
    }

    fn start_hunk(&mut self, number: usize, header: &str, full: &str) -> Result<(), ParseError> {
        let Some(file) = self.files.last_mut() else {
            return Err(ParseError {
                message: format!("line {number}: hunk header before any file header"),
            });
        };
        let Some((old_start, new_start)) = parse_hunk_ranges(header) else {
            return Err(ParseError {
                message: format!("line {number}: malformed hunk header {full:?}"),
            });
        };
        // The first @@ of a file starts the count; later ones consume one.
        self.position = Some(match self.position {
            None => 0,
            Some(p) => p + 1,
        });
        self.old_line = old_start;
        self.new_line = new_start;
        self.in_hunk = true;
        file.hunks.push(Hunk {
            header: full.to_string(),
            lines: Vec::new(),
        });
        Ok(())
    }

    fn hunk_line(&mut self, number: usize, line: &str) -> Result<(), ParseError> {
        let (kind, body) = match line.chars().next() {
            Some('+') => (LineKind::Addition, &line[1..]),
            Some('-') => (LineKind::Deletion, &line[1..]),
            Some(' ') => (LineKind::Context, &line[1..]),
            Some('\\') => (LineKind::Meta, line),
            // An empty line inside a hunk is a context line whose leading
            // space some tools trim in transit.
            None => (LineKind::Context, ""),
            _ => {
                // The hunk ended without a new file/hunk header (e.g. a
                // trailing commit-message trailer). Fall back to headers.
                self.in_hunk = false;
                self.file_header(line);
                return Ok(());
            }
        };
        let position = self.position.as_mut().map(|p| {
            *p += 1;
            *p
        });
        let (old_line, new_line) = match kind {
            LineKind::Context => {
                let pair = (Some(self.old_line), Some(self.new_line));
                self.old_line += 1;
                self.new_line += 1;
                pair
            }
            LineKind::Deletion => {
                let value = Some(self.old_line);
                self.old_line += 1;
                (value, None)
            }
            LineKind::Addition => {
                let value = Some(self.new_line);
                self.new_line += 1;
                (None, value)
            }
            LineKind::Meta => (None, None),
        };
        let expanded = expand_tabs(body, self.tab_width);
        let raw = if expanded == body {
            None
        } else {
            Some(body.to_string())
        };
        let Some(file) = self.files.last_mut() else {
            return Err(ParseError {
                message: format!("line {number}: content outside a file"),
            });
        };
        let Some(hunk) = file.hunks.last_mut() else {
            return Err(ParseError {
                message: format!("line {number}: content outside a hunk"),
            });
        };
        hunk.lines.push(DiffLine {
            kind,
            text: expanded,
            raw,
            old_line,
            new_line,
            patch_position: position,
        });
        Ok(())
    }

    fn current(&mut self) -> Option<&mut FileDiff> {
        self.files.last_mut()
    }
}

/// Splits `a/old b/new` from a `diff --git` line. Paths containing spaces
/// are ambiguous here; the `---`/`+++` lines refine them.
fn split_git_names(names: &str) -> (String, String) {
    let names = names.trim();
    if let Some((old, new)) = names.split_once(' ') {
        (
            strip_prefix_marker(old),
            strip_prefix_marker(new),
        )
    } else {
        (names.to_string(), names.to_string())
    }
}

/// Drops the `a/`/`b/` prefix and any trailing tab-metadata git appends.
fn strip_prefix_marker(path: &str) -> String {
    let path = path.split('\t').next().unwrap_or(path).trim();
    if path == "/dev/null" {
        return path.to_string();
    }
    for prefix in ["a/", "b/", "\"a/", "\"b/"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            return rest.trim_end_matches('"').to_string();
        }
    }
    path.trim_matches('"').to_string()
}

/// Parses `-a,b +c,d` (counts optional) into the two start lines.
fn parse_hunk_ranges(header: &str) -> Option<(u32, u32)> {
    let mut parts = header.split(" @@").next()?.split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let start = |range: &str| -> Option<u32> {
        let text = range.split(',').next()?;
        text.parse().ok()
    };
    Some((start(old)?, start(new)?))
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

    const SAMPLE: &str = "\
diff --git a/src/main.rs b/src/main.rs
index 123..456 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"hi\");
+    println!(\"hello\");
+    run();
 }
@@ -10,2 +11,2 @@ mod tail
 // one
-\told
+\tnew
";

    #[test]
    fn parses_git_diff() {
        let files = parse(SAMPLE, 4).unwrap();
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.new_path, "src/main.rs");
        assert_eq!(file.status, FileStatus::Modified);
        assert_eq!(file.hunks.len(), 2);
        assert_eq!(file.change_counts(), (3, 2));

        let first = &file.hunks[0].lines;
        assert_eq!(first[0].kind, LineKind::Context);
        assert_eq!(first[0].old_line, Some(1));
        assert_eq!(first[0].new_line, Some(1));
        assert_eq!(first[1].kind, LineKind::Deletion);
        assert_eq!(first[1].old_line, Some(2));
        assert_eq!(first[1].new_line, None);
        assert_eq!(first[2].kind, LineKind::Addition);
        assert_eq!(first[2].new_line, Some(2));
        assert_eq!(first[3].new_line, Some(3));
        assert_eq!(first[4].old_line, Some(3));
        assert_eq!(first[4].new_line, Some(4));
    }

    #[test]
    fn github_positions_count_across_hunks() {
        let files = parse(SAMPLE, 4).unwrap();
        let file = &files[0];
        // First hunk: positions 1..=5.
        assert_eq!(file.hunks[0].lines[0].patch_position, Some(1));
        assert_eq!(file.hunks[0].lines[4].patch_position, Some(5));
        // The second @@ consumes position 6; its first line is 7.
        assert_eq!(file.hunks[1].lines[0].patch_position, Some(7));
    }

    #[test]
    fn tabs_expand_but_raw_survives() {
        let files = parse(SAMPLE, 4).unwrap();
        let line = &files[0].hunks[1].lines[2];
        assert_eq!(line.text, "    new");
        assert_eq!(line.raw_text(), "\tnew");
    }

    #[test]
    fn new_and_deleted_files() {
        let patch = "\
diff --git a/added.txt b/added.txt
new file mode 100644
--- /dev/null
+++ b/added.txt
@@ -0,0 +1 @@
+hello
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
";
        let files = parse(patch, 4).unwrap();
        assert_eq!(files[0].status, FileStatus::Added);
        assert_eq!(files[0].display_path(), "added.txt");
        assert_eq!(files[1].status, FileStatus::Deleted);
        assert_eq!(files[1].display_path(), "gone.txt");
    }

    #[test]
    fn renames_and_binaries() {
        let patch = "\
diff --git a/old_name.rs b/new_name.rs
similarity index 90%
rename from old_name.rs
rename to new_name.rs
--- a/old_name.rs
+++ b/new_name.rs
@@ -1 +1 @@
-x
+y
diff --git a/logo.png b/logo.png
Binary files a/logo.png and b/logo.png differ
";
        let files = parse(patch, 4).unwrap();
        assert_eq!(files[0].status, FileStatus::Renamed);
        assert_eq!(files[0].old_path, "old_name.rs");
        assert_eq!(files[0].new_path, "new_name.rs");
        assert_eq!(files[1].status, FileStatus::Binary);
        assert!(files[1].is_binary);
    }

    #[test]
    fn plain_unified_diff_without_git_header() {
        let patch = "\
--- before.txt
+++ after.txt
@@ -1,2 +1,2 @@
 same
-old
+new
";
        let files = parse(patch, 4).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].old_path, "before.txt");
        assert_eq!(files[0].new_path, "after.txt");
        assert_eq!(files[0].change_counts(), (1, 1));
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(parse("not a diff at all\n", 4).is_err());
        assert!(parse("", 4).is_err());
    }

    #[test]
    fn malformed_hunk_header_is_reported() {
        let err = parse("--- a\n+++ b\n@@ nonsense @@\n x\n", 4).unwrap_err();
        assert!(err.message.contains("hunk header"), "{}", err.message);
    }
}
