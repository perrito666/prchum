//! Markdown export of a draft review — the shape leanreview produces, so
//! notes can be pasted into a prompt or shared as-is.

use std::collections::BTreeMap;

use crate::diff::Side;
use crate::review::{DraftReview, DraftState};

/// Renders the review as Markdown grouped by file, files in first-seen
/// order, comments by start line within a file.
pub fn markdown(draft: &DraftReview) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Review: {}\n", draft.title));
    if !draft.summary.is_empty() {
        out.push('\n');
        out.push_str(&draft.summary);
        out.push('\n');
    }

    // First-seen file order, then start line.
    let mut order: Vec<&str> = Vec::new();
    let mut by_file: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, comment) in draft.comments.iter().enumerate() {
        let path = comment.location.path.as_str();
        if !order.contains(&path) {
            order.push(path);
        }
        by_file.entry(path).or_default().push(index);
    }

    for path in order {
        out.push_str(&format!("\n## {path}\n"));
        let mut indices = by_file[path].clone();
        indices.sort_by_key(|&i| draft.comments[i].location.start_line);
        for index in indices {
            let comment = &draft.comments[index];
            let location = &comment.location;
            let side = match location.side {
                Side::Left => "LEFT",
                Side::Right => "RIGHT",
            };
            let lines = if location.start_line == location.end_line {
                format!("L{}", location.start_line)
            } else {
                format!("L{}-{}", location.start_line, location.end_line)
            };
            let state = match comment.state {
                DraftState::Active => String::new(),
                DraftState::Stale => " — stale".to_string(),
                DraftState::Orphaned => " — orphaned".to_string(),
                DraftState::Dismissed => " — dismissed".to_string(),
            };
            out.push_str(&format!("\n### {lines} ({side}){state}\n"));
            if !comment.snippet.is_empty() {
                let language = language_for(path);
                out.push_str(&format!("```{language}\n{}\n```\n", comment.snippet));
            }
            for line in comment.body.lines() {
                if line.is_empty() {
                    out.push_str(">\n");
                } else {
                    out.push_str(&format!("> {line}\n"));
                }
            }
            for reply in &comment.replies {
                out.push_str(">\n");
                let mut lines = reply.body.lines();
                if let Some(first) = lines.next() {
                    out.push_str(&format!("> ↳ @{}: {first}\n", reply.author));
                }
                for line in lines {
                    if line.is_empty() {
                        out.push_str(">\n");
                    } else {
                        out.push_str(&format!("> {line}\n"));
                    }
                }
            }
        }
    }
    out
}

/// A fence language guessed from the file extension; empty when unknown.
fn language_for(path: &str) -> &'static str {
    let extension = path.rsplit('.').next().unwrap_or("");
    match extension {
        "rs" => "rust",
        "go" => "go",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "swift" => "swift",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" => "cpp",
        "java" => "java",
        "rb" => "ruby",
        "sh" | "bash" => "bash",
        "json" => "json",
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "md" => "markdown",
        "html" => "html",
        "css" => "css",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse;
    use crate::location::{build_location, snippet};
    use crate::review::DraftReview;

    #[test]
    fn export_shape() {
        let files = parse(
            "--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,3 @@\n context\n-a\n+b\n+c\n",
            4,
        )
        .unwrap();
        let mut draft = DraftReview {
            title: "demo".into(),
            summary: "Overall fine.".into(),
            ..Default::default()
        };
        let location = build_location(&files[0], Side::Right, 2, 3).unwrap();
        let code = snippet(&files[0], Side::Right, 2, 3);
        let id = draft.add_comment(location, code, "First line.\n\nSecond.".into(), "me");
        draft.add_reply(&id, "agreed".into(), "other");

        let out = markdown(&draft);
        assert!(out.starts_with("# Review: demo\n"), "{out}");
        assert!(out.contains("Overall fine."), "{out}");
        assert!(out.contains("## x.rs"), "{out}");
        assert!(out.contains("### L2-3 (RIGHT)\n"), "{out}");
        assert!(out.contains("```rust\nb\nc\n```"), "{out}");
        assert!(out.contains("> First line.\n>\n> Second.\n"), "{out}");
        assert!(out.contains("> ↳ @other: agreed\n"), "{out}");
    }
}
