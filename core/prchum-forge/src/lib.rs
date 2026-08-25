//! The forge seam: host-agnostic review operations over `gh`/`glab`.
//!
//! Adapters shell out to the forge CLIs — no API SDK — so authentication,
//! token storage, and enterprise hosts stay the CLI's already-configured
//! problem. Prchum never manages a forge credential.

pub mod forgejo;
pub mod ghcli;
pub mod refs;
pub mod submit;

use serde::Serialize;

pub use refs::{ForgeKind, PullRequestRef};

/// One comment as the host stores it.
#[derive(Clone, Debug, Serialize)]
pub struct Comment {
    pub id: i64,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub url: String,
}

/// A review thread anchored to a diff position.
#[derive(Clone, Debug, Serialize)]
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
    /// Root first, replies after.
    pub comments: Vec<Comment>,
}

/// Pull-request metadata.
#[derive(Clone, Debug, Serialize)]
pub struct PullRequest {
    pub number: u64,
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
