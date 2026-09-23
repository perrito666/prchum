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
    /// Root first, replies after.
    pub comments: Vec<Comment>,
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

/// One commit of a pull request, as the request's commit list shows it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommitInfo {
    pub sha: String,
    /// First parent first; a merge commit has more than one.
    pub parents: Vec<String>,
    /// The first line of the message.
    pub title: String,
    pub author: String,
    pub date: String,
}

impl CommitInfo {
    /// The abbreviation the forges' own UIs show.
    pub fn short_sha(&self) -> &str {
        self.sha.get(..7).unwrap_or(&self.sha)
    }

    /// What the commit's changes are measured against.
    pub fn first_parent(&self) -> Option<&str> {
        self.parents.first().map(String::as_str)
    }
}

/// A request's commits, oldest first, and anything the forge withheld.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CommitList {
    pub commits: Vec<CommitInfo>,
    /// Non-empty when the list is known or suspected to be incomplete
    /// (GitHub stops at 250 commits); the shell shows it.
    pub notice: String,
}

/// Puts commits in history order, parents before children, whatever
/// order the forge answered in (GitHub lists oldest first, GitLab newest
/// first). Parents outside the list are ignored; ties keep input order.
pub fn oldest_first(commits: Vec<CommitInfo>) -> Vec<CommitInfo> {
    use std::collections::{HashMap, HashSet};
    let index: HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(position, commit)| (commit.sha.as_str(), position))
        .collect();
    let mut order = Vec::with_capacity(commits.len());
    let mut done: HashSet<usize> = HashSet::new();
    // Iterative depth-first walk emitting parents before the child: a
    // long history must not exhaust the stack.
    for start in 0..commits.len() {
        let mut stack = vec![(start, false)];
        while let Some((position, expanded)) = stack.pop() {
            if done.contains(&position) {
                continue;
            }
            if expanded {
                done.insert(position);
                order.push(position);
                continue;
            }
            stack.push((position, true));
            for parent in commits[position].parents.iter().rev() {
                if let Some(&parent_position) = index.get(parent.as_str()) {
                    if !done.contains(&parent_position) {
                        stack.push((parent_position, false));
                    }
                }
            }
        }
    }
    let mut slots: Vec<Option<CommitInfo>> = commits.into_iter().map(Some).collect();
    order
        .into_iter()
        .filter_map(|position| slots[position].take())
        .collect()
}

/// The first line of a commit message.
pub(crate) fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or_default().trim().to_string()
}

/// Host-agnostic operations the UI depends on. Everything returns a plain
/// error string; the shell shows it and the draft survives.
pub trait Forge {
    fn pull_request(&self, pr: &PullRequestRef) -> Result<PullRequest, String>;
    /// The host's canonical diff, so comment positions always match.
    fn diff(&self, pr: &PullRequestRef) -> Result<String, String>;
    fn threads(&self, pr: &PullRequestRef) -> Result<Vec<ThreadInfo>, String>;
    fn general_comments(&self, pr: &PullRequestRef) -> Result<Vec<Comment>, String>;
    /// The request's commits, oldest first.
    fn commits(&self, pr: &PullRequestRef) -> Result<CommitList, String>;
    /// One commit's changes against its first parent, as a unified diff.
    fn commit_diff(&self, pr: &PullRequestRef, sha: &str) -> Result<String, String>;
    /// One atomic review: the event, the summary, and every line comment.
    /// With `commit`, the line comments are positioned in that commit's
    /// diff rather than the whole request's — the forge's own
    /// single-commit view.
    fn create_review(
        &self,
        pr: &PullRequestRef,
        event: &str,
        summary: &str,
        comments: &[ReviewComment],
        commit: Option<&CommitInfo>,
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

    fn commit(sha: &str, parents: &[&str]) -> CommitInfo {
        CommitInfo {
            sha: sha.to_string(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            ..Default::default()
        }
    }

    fn shas(commits: &[CommitInfo]) -> Vec<&str> {
        commits.iter().map(|c| c.sha.as_str()).collect()
    }

    #[test]
    fn history_order_regardless_of_the_forge_order() {
        let oldest = vec![commit("a", &["base"]), commit("b", &["a"]), commit("c", &["b"])];
        assert_eq!(shas(&oldest_first(oldest.clone())), ["a", "b", "c"]);
        let newest: Vec<_> = oldest.into_iter().rev().collect();
        assert_eq!(shas(&oldest_first(newest)), ["a", "b", "c"]);
    }

    #[test]
    fn merges_follow_both_parents() {
        let commits = vec![
            commit("m", &["a", "side"]),
            commit("side", &["base"]),
            commit("a", &["base"]),
        ];
        assert_eq!(shas(&oldest_first(commits)), ["a", "side", "m"]);
    }

    #[test]
    fn short_sha_and_first_line() {
        assert_eq!(commit("0123456789", &[]).short_sha(), "0123456");
        assert_eq!(commit("abc", &[]).short_sha(), "abc");
        assert_eq!(first_line("Fix it\n\nBecause."), "Fix it");
        assert_eq!(first_line(""), "");
    }
}
