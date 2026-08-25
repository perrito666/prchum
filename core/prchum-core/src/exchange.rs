//! The review-exchange format, version 1 — full compatibility with
//! leanreview's `*.review.json` so the two clients interoperate on the
//! same files (the offline LLM review loop).
//!
//! Rules carried over:
//! * `leanreview_review` must be the first key (it doubles as the sniff
//!   marker); unknown versions are rejected, not guessed.
//! * `patch` marshals as an array of lines; a plain string is accepted.
//! * HTML escaping is off so `<`/`>`/`&` in code stay readable.
//! * Comments that don't resolve against the embedded patch become
//!   orphaned, never dropped. A `dismissed` state outranks anchoring.
//! * Unchanged documents round-trip byte-identically.

use serde::{Deserialize, Serialize};

use crate::diff::{FileDiff, Side};
use crate::location::{capture_anchor, find_file, ContextAnchor, Location};
use crate::review::{DraftComment, DraftState, ReviewReply};

pub const EXCHANGE_VERSION: u64 = 1;

/// Does this text look like an exchange document? Content-based, never
/// filename-based: a leading `{` and the marker key in the first 4 KiB.
pub fn is_exchange(text: &str) -> bool {
    let head = &text[..text.len().min(4096)];
    head.trim_start().starts_with('{') && head.contains("\"leanreview_review\"")
}

/// The document as serialized. Field order is the wire order.
#[derive(Serialize, Deserialize)]
pub struct ExchangeDoc {
    pub leanreview_review: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(with = "patch_lines")]
    pub patch: Vec<String>,
    #[serde(default)]
    pub comments: Vec<ExchangeComment>,
}

#[derive(Serialize, Deserialize)]
pub struct ExchangeComment {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    pub path: String,
    #[serde(default = "default_side")]
    pub side: String,
    pub start_line: u32,
    #[serde(default)]
    pub end_line: u32,
    pub body: String,
    #[serde(default = "default_state")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub snippet: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<ExchangeReply>,
}

#[derive(Serialize, Deserialize)]
pub struct ExchangeReply {
    #[serde(default)]
    pub author: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub at: String,
}

fn default_side() -> String {
    "RIGHT".to_string()
}

fn default_state() -> String {
    "active".to_string()
}

/// `patch` as an array of lines on the wire, accepting a plain string too.
mod patch_lines {
    use serde::de::Deserializer;
    use serde::ser::Serializer;
    use serde::Deserialize;

    pub fn serialize<S: Serializer>(lines: &[String], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(lines)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<String>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Lines(Vec<String>),
            Text(String),
        }
        match Wire::deserialize(deserializer)? {
            Wire::Lines(lines) => Ok(lines),
            Wire::Text(text) => Ok(text
                .trim_end_matches('\n')
                .split('\n')
                .map(str::to_string)
                .collect()),
        }
    }
}

/// Parses and validates a document.
pub fn parse(text: &str) -> Result<ExchangeDoc, String> {
    let doc: ExchangeDoc =
        serde_json::from_str(text).map_err(|e| format!("malformed exchange document: {e}"))?;
    if doc.leanreview_review != EXCHANGE_VERSION {
        return Err(format!(
            "unsupported exchange version {} (this build reads {EXCHANGE_VERSION})",
            doc.leanreview_review
        ));
    }
    if doc.patch.is_empty() {
        return Err("exchange document has no patch".to_string());
    }
    Ok(doc)
}

/// The embedded patch as unified-diff text (trailing newline implicit).
pub fn patch_text(doc: &ExchangeDoc) -> String {
    let mut text = doc.patch.join("\n");
    text.push('\n');
    text
}

/// Converts wire comments to draft comments against the parsed patch.
/// Unresolvable anchors orphan the comment; a dismissed verdict survives
/// regardless.
pub fn to_drafts(doc: &ExchangeDoc, files: &[FileDiff]) -> Vec<DraftComment> {
    doc.comments
        .iter()
        .map(|comment| {
            let side = if comment.side.eq_ignore_ascii_case("left") {
                Side::Left
            } else {
                Side::Right
            };
            let end_line = comment.end_line.max(comment.start_line);
            let resolved = find_file(files, &comment.path)
                .and_then(|file| capture_anchor(file, side, comment.start_line));
            let (anchor, resolvable) = match resolved {
                Some(anchor) => (anchor, true),
                None => (
                    ContextAnchor {
                        hunk_header: String::new(),
                        before: Vec::new(),
                        anchor_text: comment
                            .snippet
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .to_string(),
                        after: Vec::new(),
                        content_hash: String::new(),
                    },
                    false,
                ),
            };
            let state = match comment.state.as_str() {
                "dismissed" => DraftState::Dismissed,
                _ if !resolvable => DraftState::Orphaned,
                "orphaned" => DraftState::Orphaned,
                "stale" => DraftState::Stale,
                _ => DraftState::Active,
            };
            DraftComment {
                local_id: if comment.id.is_empty() {
                    crate::util::new_local_id()
                } else {
                    comment.id.clone()
                },
                location: Location {
                    path: comment.path.clone(),
                    side,
                    start_line: comment.start_line,
                    end_line,
                    anchor,
                },
                body: comment.body.clone(),
                snippet: comment.snippet.clone(),
                state,
                author: comment.author.clone(),
                at: comment.at.clone(),
                reply_to: None,
                replies: comment
                    .replies
                    .iter()
                    .map(|r| ReviewReply {
                        author: r.author.clone(),
                        body: r.body.clone(),
                        at: r.at.clone(),
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Converts draft comments back to the wire form.
pub fn from_drafts(
    title: &str,
    summary: &str,
    patch: Vec<String>,
    comments: &[DraftComment],
) -> ExchangeDoc {
    ExchangeDoc {
        leanreview_review: EXCHANGE_VERSION,
        title: title.to_string(),
        summary: summary.to_string(),
        patch,
        comments: comments
            .iter()
            .map(|draft| ExchangeComment {
                id: draft.local_id.clone(),
                author: draft.author.clone(),
                path: draft.location.path.clone(),
                side: match draft.location.side {
                    Side::Left => "LEFT".to_string(),
                    Side::Right => "RIGHT".to_string(),
                },
                start_line: draft.location.start_line,
                end_line: draft.location.end_line,
                body: draft.body.clone(),
                state: match draft.state {
                    DraftState::Active => "active",
                    DraftState::Stale => "stale",
                    DraftState::Orphaned => "orphaned",
                    DraftState::Dismissed => "dismissed",
                }
                .to_string(),
                snippet: draft.snippet.clone(),
                at: draft.at.clone(),
                replies: draft
                    .replies
                    .iter()
                    .map(|r| ExchangeReply {
                        author: r.author.clone(),
                        body: r.body.clone(),
                        at: r.at.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// Serializes with the wire conventions: 2-space indent, no HTML escaping
/// (serde_json's default), stable field order, trailing newline.
pub fn render(doc: &ExchangeDoc) -> String {
    let mut out = serde_json::to_string_pretty(doc).unwrap_or_default();
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff;

    fn sample_doc() -> String {
        r#"{
  "leanreview_review": 1,
  "title": "auth refactor",
  "summary": "One issue.",
  "patch": [
    "--- a/handler.go",
    "+++ b/handler.go",
    "@@ -70,3 +70,3 @@",
    " func Handle() {",
    "-    result := calc()",
    "+    result, err := calc()"
  ],
  "comments": [
    {
      "id": "c1",
      "author": "assistant",
      "path": "handler.go",
      "side": "RIGHT",
      "start_line": 71,
      "end_line": 71,
      "body": "`err` is discarded.",
      "state": "active",
      "snippet": "result, err := calc()",
      "at": "2026-01-02T03:04:05Z"
    }
  ]
}
"#
        .to_string()
    }

    #[test]
    fn sniffing() {
        assert!(is_exchange(&sample_doc()));
        assert!(!is_exchange("--- a\n+++ b\n"));
        assert!(!is_exchange("{\"other\": 1}"));
    }

    #[test]
    fn parse_resolve_round_trip() {
        let doc = parse(&sample_doc()).unwrap();
        assert_eq!(doc.title, "auth refactor");
        let files = diff::parse(&patch_text(&doc), 4).unwrap();
        let drafts = to_drafts(&doc, &files);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].state, DraftState::Active);
        assert_eq!(drafts[0].location.start_line, 71);
        assert!(!drafts[0].location.anchor.content_hash.is_empty());

        // Byte-identical round trip for an unchanged document.
        let back = from_drafts(&doc.title, &doc.summary, doc.patch.clone(), &drafts);
        assert_eq!(render(&back), sample_doc());
    }

    #[test]
    fn unresolvable_comment_is_orphaned_not_dropped() {
        let mut text = sample_doc();
        text = text.replace("\"start_line\": 71", "\"start_line\": 999");
        let doc = parse(&text).unwrap();
        let files = diff::parse(&patch_text(&doc), 4).unwrap();
        let drafts = to_drafts(&doc, &files);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].state, DraftState::Orphaned);
    }

    #[test]
    fn dismissed_outranks_anchoring_failure() {
        let mut text = sample_doc();
        text = text.replace("\"start_line\": 71", "\"start_line\": 999");
        text = text.replace("\"state\": \"active\"", "\"state\": \"dismissed\"");
        let doc = parse(&text).unwrap();
        let files = diff::parse(&patch_text(&doc), 4).unwrap();
        assert_eq!(to_drafts(&doc, &files)[0].state, DraftState::Dismissed);
    }

    #[test]
    fn version_and_patch_are_enforced() {
        assert!(parse(&sample_doc().replace(": 1", ": 2")).is_err());
        assert!(parse(r#"{"leanreview_review": 1, "patch": []}"#).is_err());
    }

    #[test]
    fn patch_as_string_is_accepted() {
        let doc = parse(
            r#"{"leanreview_review": 1, "patch": "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n"}"#,
        )
        .unwrap();
        assert_eq!(doc.patch.len(), 5);
        assert!(diff::parse(&patch_text(&doc), 4).is_ok());
    }
}
