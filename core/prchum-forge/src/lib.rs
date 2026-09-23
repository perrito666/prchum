//! The forge seam: host-agnostic review operations over `gh`/`glab`.
//!
//! Adapters shell out to the forge CLIs — no API SDK — so authentication,
//! token storage, and enterprise hosts stay the CLI's already-configured
//! problem. Prchum never manages a forge credential.

pub mod forgejo;
pub mod ghcli;
pub mod glabcli;
pub mod list;
pub mod open;
pub mod refs;
pub mod submit;

use serde::{Deserialize, Serialize};

pub use refs::{ForgeKind, PullRequestRef};

/// One comment as the host stores it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Comment {
    pub id: i64,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub url: String,
    /// Session-gated attachment URLs in `body` → the signed, anonymously
    /// fetchable variants the host's rendered HTML carries (GitHub's
    /// user-attachments). Empty for hosts without the distinction.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub image_map: std::collections::BTreeMap<String, String>,
}

/// A review thread anchored to a diff position.
///
/// Deserialize as well as Serialize: the session carries these as JSON
/// for the FFI's sake, and a Rust shell reads them back rather than
/// describing the same shape a second time.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ThreadInfo {
    /// The root comment's host id (the reply target).
    pub id: i64,
    pub path: String,
    /// `LEFT` or `RIGHT`.
    pub side: String,
    /// Current line on that side; `None` when the thread is outdated.
    pub line: Option<u32>,
    pub start_line: Option<u32>,
    /// The historical position, for listing outdated threads.
    pub original_line: Option<u32>,
    pub outdated: bool,
    /// Marked resolved on the host. False when the host cannot say, so
    /// a thread whose state is unknown is shown in full.
    pub resolved: bool,
    /// Where a shell shows the thread; see [`ThreadInfo::decide_placement`].
    /// Set once, when the review opens, so every shell reads the same
    /// answer instead of restating the rule.
    pub placement: Placement,
    /// Root first, replies after.
    pub comments: Vec<Comment>,
}

/// How a shell presents a host thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// In full, under its current line.
    #[default]
    Inline,
    /// Under its current line as a one-line summary the reviewer can
    /// expand.
    Collapsed,
    /// Nowhere in today's diff: the line it was written against is gone,
    /// so it is listed and read on its own. Drawing it at its original
    /// line number would put it beside code it was never about.
    ListOnly,
}

impl ThreadInfo {
    /// Outdated wins over resolved: a resolved thread whose line is gone
    /// has nowhere to collapse into.
    pub fn decide_placement(&self) -> Placement {
        if self.outdated || self.line.is_none() {
            Placement::ListOnly
        } else if self.resolved {
            Placement::Collapsed
        } else {
            Placement::Inline
        }
    }
}

/// Settles every thread's placement; called once on the way into the
/// session.
pub fn place_threads(threads: &mut [ThreadInfo]) {
    for thread in threads {
        thread.placement = thread.decide_placement();
    }
}

/// Pull-request metadata.
#[derive(Clone, Debug, Serialize)]
pub struct PullRequest {
    pub number: u64,
    /// `open` or `closed` (a merged request reads closed + merged).
    pub state: String,
    pub merged: bool,
    pub title: String,
    pub body: String,
    pub author: String,
    pub url: String,
    pub head_oid: String,
    pub base_ref: String,
    pub head_ref: String,
}

/// One line comment in a submission, host vocabulary.
#[derive(Clone, Debug, Serialize)]
pub struct ReviewComment {
    pub path: String,
    pub body: String,
    /// The anchor line (the range's end, per GitHub semantics).
    pub line: u32,
    pub side: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_side: Option<String>,
}

/// Host-agnostic operations the UI depends on. Everything returns a plain
/// error string; the shell shows it and the draft survives.
pub trait Forge {
    fn pull_request(&self, pr: &PullRequestRef) -> Result<PullRequest, String>;
    /// The host's canonical diff, so comment positions always match.
    fn diff(&self, pr: &PullRequestRef) -> Result<String, String>;
    fn threads(&self, pr: &PullRequestRef) -> Result<Vec<ThreadInfo>, String>;
    fn general_comments(&self, pr: &PullRequestRef) -> Result<Vec<Comment>, String>;
    /// One atomic review: the event, the summary, and every line comment.
    fn create_review(
        &self,
        pr: &PullRequestRef,
        event: &str,
        summary: &str,
        comments: &[ReviewComment],
    ) -> Result<(), String>;
    /// A reply into an existing thread, by root comment id.
    fn reply(&self, pr: &PullRequestRef, comment_id: i64, body: &str) -> Result<(), String>;
    /// A file's raw content at a revision (the context view uses the head).
    fn file_content(&self, pr: &PullRequestRef, path: &str, rev: &str) -> Result<String, String>;
    fn add_general_comment(&self, pr: &PullRequestRef, body: &str) -> Result<(), String>;
}

/// Picks the adapter for a host. A configured override wins (self-hosted
/// instances rarely say what they are in their hostname); then heuristics:
/// codeberg/forgejo/gitea → Forgejo, gitlab → GitLab, everything else
/// (github.com and GHE) → GitHub.
pub fn kind_for_host(host: &str, configured: Option<&str>) -> ForgeKind {
    match configured {
        Some("forgejo") | Some("gitea") => return ForgeKind::Forgejo,
        Some("gitlab") => return ForgeKind::GitLab,
        Some("github") => return ForgeKind::GitHub,
        _ => {}
    }
    if host == "codeberg.org" || host.contains("forgejo") || host.contains("gitea") {
        ForgeKind::Forgejo
    } else if host.contains("gitlab") {
        ForgeKind::GitLab
    } else {
        ForgeKind::GitHub
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(line: Option<u32>, outdated: bool, resolved: bool) -> ThreadInfo {
        ThreadInfo {
            line,
            original_line: Some(4),
            outdated,
            resolved,
            ..Default::default()
        }
    }

    #[test]
    fn placement_follows_line_then_resolution() {
        assert_eq!(thread(Some(4), false, false).decide_placement(), Placement::Inline);
        assert_eq!(thread(Some(4), false, true).decide_placement(), Placement::Collapsed);
        assert_eq!(thread(None, true, false).decide_placement(), Placement::ListOnly);
        assert_eq!(thread(None, true, true).decide_placement(), Placement::ListOnly);
        // A host that keeps a line but calls the thread outdated is
        // believed: the line no longer means what it did.
        assert_eq!(thread(Some(4), true, false).decide_placement(), Placement::ListOnly);
    }

    #[test]
    fn placement_travels_as_snake_case_and_defaults_when_absent() {
        let mut threads = vec![thread(None, true, true)];
        place_threads(&mut threads);
        let json = serde_json::to_string(&threads).unwrap();
        assert!(json.contains(r#""placement":"list_only""#), "{json}");
        assert!(json.contains(r#""resolved":true"#), "{json}");
        let old: ThreadInfo = serde_json::from_str(r#"{"id": 1, "line": 3}"#).unwrap();
        assert!(!old.resolved);
        assert_eq!(old.placement, Placement::Inline);
    }
}
