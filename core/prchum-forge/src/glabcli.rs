//! GitLab adapter over the `glab` CLI.
//!
//! `glab api` is a deliberate port of `gh api` (same `--method`,
//! `--input`, `--hostname`, `--paginate`), so the transport matches. The
//! model does not:
//!
//! * There is **no canonical diff endpoint** — the diff is reconstructed
//!   from the `changes` payload, one git-style header per file.
//! * There is **no atomic review** — each line comment becomes a
//!   positioned diff discussion, the summary a note, `APPROVE` an
//!   approval, `REQUEST_CHANGES` a "Changes requested" note. Comments
//!   post in order; a failure reports how many were already published
//!   (a retry may repeat those — GitLab has nothing atomic to lean on).
//! * GitLab positions a discussion on a single line; multi-line
//!   selections anchor on their end line, and GitHub-style
//!   ```suggestion fences are rewritten into GitLab's ranged
//!   ```suggestion:-N+0 form so the whole selection is replaced.

use serde_json::{json, Value};

use crate::ghcli::Runner;
use crate::{Comment, Forge, PullRequest, PullRequestRef, ReviewComment, ThreadInfo};

pub struct GlabForge<R: Runner> {
    runner: R,
}

impl GlabForge<crate::ghcli::ProcessRunner> {
    pub fn new() -> Self {
        Self {
            runner: crate::ghcli::ProcessRunner,
        }
    }
}

impl Default for GlabForge<crate::ghcli::ProcessRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Runner> GlabForge<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }

    fn api(
        &self,
        pr: &PullRequestRef,
        extra: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<String, String> {
        let mut args = vec!["api".to_string()];
        if !pr.host.is_empty() && pr.host != "gitlab.com" {
            args.push("--hostname".to_string());
            args.push(pr.host.clone());
        }
        args.extend(extra.iter().map(|s| s.to_string()));
        self.runner.run("glab", &args, stdin)
    }

    /// The URL-encoded project id: slashes become %2F, which also covers
    /// nested subgroups.
    fn project(pr: &PullRequestRef) -> String {
        format!("{}/{}", pr.owner, pr.repo).replace('/', "%2F")
    }

    fn mr_path(pr: &PullRequestRef, suffix: &str) -> String {
        format!(
            "projects/{}/merge_requests/{}{}",
            Self::project(pr),
            pr.number,
            suffix
        )
    }

    fn diff_refs(&self, pr: &PullRequestRef) -> Result<(String, String, String), String> {
        let text = self.api(pr, &[&Self::mr_path(pr, "")], None)?;
        let value: Value = parse_json(&text)?;
        let refs = &value["diff_refs"];
        Ok((
            str_at(refs, "base_sha"),
            str_at(refs, "start_sha"),
            str_at(refs, "head_sha"),
        ))
    }
}

impl<R: Runner> Forge for GlabForge<R> {
    fn pull_request(&self, pr: &PullRequestRef) -> Result<PullRequest, String> {
        let text = self.api(pr, &[&Self::mr_path(pr, "")], None)?;
        let value: Value = parse_json(&text)?;
        let state = str_at(&value, "state"); // opened | closed | merged
        Ok(PullRequest {
            number: pr.number,
            merged: state == "merged",
            state: if state == "opened" { "open".to_string() } else { "closed".to_string() },
            title: str_at(&value, "title"),
            body: str_at(&value, "description"),
            author: value["author"]["username"].as_str().unwrap_or_default().to_string(),
            url: str_at(&value, "web_url"),
            head_oid: str_at(&value, "sha"),
            base_ref: str_at(&value, "target_branch"),
            head_ref: str_at(&value, "source_branch"),
        })
    }

    fn diff(&self, pr: &PullRequestRef) -> Result<String, String> {
        let text = self.api(pr, &[&Self::mr_path(pr, "/changes")], None)?;
        let value: Value = parse_json(&text)?;
        let mut patch = String::new();
        for change in value["changes"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let old_path = str_at(change, "old_path");
            let new_path = str_at(change, "new_path");
            patch.push_str(&format!("diff --git a/{old_path} b/{new_path}\n"));
            if change["new_file"].as_bool() == Some(true) {
                patch.push_str("new file mode 100644\n");
                patch.push_str("--- /dev/null\n");
                patch.push_str(&format!("+++ b/{new_path}\n"));
            } else if change["deleted_file"].as_bool() == Some(true) {
                patch.push_str("deleted file mode 100644\n");
                patch.push_str(&format!("--- a/{old_path}\n"));
                patch.push_str("+++ /dev/null\n");
            } else {
                if change["renamed_file"].as_bool() == Some(true) {
                    patch.push_str(&format!("rename from {old_path}\nrename to {new_path}\n"));
                }
                patch.push_str(&format!("--- a/{old_path}\n+++ b/{new_path}\n"));
            }
            patch.push_str(&str_at(change, "diff"));
            if !patch.ends_with('\n') {
                patch.push('\n');
            }
        }
        if patch.is_empty() {
            return Err("the merge request has no changes".to_string());
        }
        Ok(patch)
    }

    fn threads(&self, pr: &PullRequestRef) -> Result<Vec<ThreadInfo>, String> {
        let path = Self::mr_path(pr, "/discussions");
        let text = self.api(pr, &[&path, "--paginate"], None)?;
        let discussions = parse_paginated(&text)?;
        let mut threads = Vec::new();
        for discussion in &discussions {
            let notes = discussion["notes"].as_array().map(Vec::as_slice).unwrap_or_default();
            let Some(root) = notes.first() else { continue };
            let position = &root["position"];
            if position["position_type"].as_str() != Some("text") {
                continue;
            }
            let new_line = position["new_line"].as_u64().map(|n| n as u32);
            let old_line = position["old_line"].as_u64().map(|n| n as u32);
            threads.push(ThreadInfo {
                id: root["id"].as_i64().unwrap_or(0),
                path: {
                    let new_path = position["new_path"].as_str().unwrap_or_default();
                    if new_path.is_empty() {
                        position["old_path"].as_str().unwrap_or_default().to_string()
                    } else {
                        new_path.to_string()
                    }
                },
                side: if new_line.is_some() { "RIGHT" } else { "LEFT" }.to_string(),
                line: new_line.or(old_line),
                start_line: None,
                original_line: None,
                outdated: false,
                comments: notes.iter().map(note_comment).collect(),
            });
        }
        Ok(threads)
    }

    fn general_comments(&self, pr: &PullRequestRef) -> Result<Vec<Comment>, String> {
        let path = Self::mr_path(pr, "/notes");
        let text = self.api(pr, &[&path, "--paginate"], None)?;
        Ok(parse_paginated(&text)?
            .iter()
            .filter(|note| {
                note["system"].as_bool() != Some(true) && note["position"].is_null()
            })
            .map(note_comment)
            .collect())
    }

    fn create_review(
        &self,
        pr: &PullRequestRef,
        event: &str,
        summary: &str,
        comments: &[ReviewComment],
    ) -> Result<(), String> {
        let (base_sha, start_sha, head_sha) = if comments.is_empty() {
            (String::new(), String::new(), String::new())
        } else {
            self.diff_refs(pr)?
        };

        // Each comment is its own positioned discussion; posting is
        // ordered so a failure can say how far it got.
        for (index, comment) in comments.iter().enumerate() {
            let mut position = json!({
                "position_type": "text",
                "base_sha": base_sha,
                "start_sha": start_sha,
                "head_sha": head_sha,
                "old_path": comment.path,
                "new_path": comment.path,
            });
            if comment.side == "LEFT" {
                position["old_line"] = json!(comment.line);
            } else {
                position["new_line"] = json!(comment.line);
            }
            let body = json!({
                "body": adapt_suggestion(&comment.body, comment.start_line, comment.line),
                "position": position,
            });
            let path = Self::mr_path(pr, "/discussions");
            self.api(
                pr,
                &[&path, "--method", "POST", "--input", "-"],
                Some(body.to_string().as_bytes()),
            )
            .map_err(|error| {
                format!("posted {index} of {} comments, then: {error}", comments.len())
            })?;
        }

        match event {
            "APPROVE" => {
                let path = Self::mr_path(pr, "/approve");
                self.api(pr, &[&path, "--method", "POST"], None)?;
                if !summary.is_empty() {
                    self.add_general_comment(pr, summary)?;
                }
            }
            "REQUEST_CHANGES" => {
                let note = if summary.is_empty() {
                    "**Changes requested.**".to_string()
                } else {
                    format!("**Changes requested.**\n\n{summary}")
                };
                self.add_general_comment(pr, &note)?;
            }
            _ => {
                if !summary.is_empty() {
                    self.add_general_comment(pr, summary)?;
                }
            }
        }
        Ok(())
    }

    fn reply(&self, pr: &PullRequestRef, comment_id: i64, body: &str) -> Result<(), String> {
        // Replies target a discussion id (a hex string), not a note id;
        // find the discussion whose root note this is.
        let path = Self::mr_path(pr, "/discussions");
        let text = self.api(pr, &[&path, "--paginate"], None)?;
        let discussions = parse_paginated(&text)?;
        let discussion_id = discussions
            .iter()
            .find(|d| {
                d["notes"]
                    .as_array()
                    .and_then(|notes| notes.first())
                    .and_then(|note| note["id"].as_i64())
                    == Some(comment_id)
            })
            .and_then(|d| d["id"].as_str())
            .ok_or_else(|| format!("comment {comment_id} not found to reply to"))?
            .to_string();
        let path = Self::mr_path(pr, &format!("/discussions/{discussion_id}/notes"));
        let payload = json!({ "body": body });
        self.api(
            pr,
            &[&path, "--method", "POST", "--input", "-"],
            Some(payload.to_string().as_bytes()),
        )?;
        Ok(())
    }

    fn file_content(&self, pr: &PullRequestRef, path: &str, rev: &str) -> Result<String, String> {
        let encoded = path.replace('%', "%25").replace('/', "%2F").replace('.', "%2E");
        let api_path = format!(
            "projects/{}/repository/files/{encoded}/raw?ref={rev}",
            Self::project(pr)
        );
        self.api(pr, &[&api_path], None)
    }

    fn add_general_comment(&self, pr: &PullRequestRef, body: &str) -> Result<(), String> {
        let path = Self::mr_path(pr, "/notes");
        let payload = json!({ "body": body });
        self.api(
            pr,
            &[&path, "--method", "POST", "--input", "-"],
            Some(payload.to_string().as_bytes()),
        )?;
        Ok(())
    }
}

/// Rewrites GitHub's ```suggestion into GitLab's ranged
/// ```suggestion:-N+0: GitLab's fence is relative to the positioned line
/// (the range's end), so without the `-N` only the last line would be
/// replaced.
fn adapt_suggestion(body: &str, start_line: Option<u32>, end_line: u32) -> String {
    let span = start_line
        .map(|start| end_line.saturating_sub(start))
        .unwrap_or(0);
    body.replace("```suggestion\n", &format!("```suggestion:-{span}+0\n"))
}

fn parse_json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("unexpected glab output: {error}"))
}

fn parse_paginated(text: &str) -> Result<Vec<Value>, String> {
    let mut items = Vec::new();
    let mut deserializer = serde_json::Deserializer::from_str(text).into_iter::<Value>();
    for value in &mut deserializer {
        let value = value.map_err(|error| format!("unexpected glab output: {error}"))?;
        match value {
            Value::Array(chunk) => items.extend(chunk),
            other => items.push(other),
        }
    }
    Ok(items)
}

fn note_comment(note: &Value) -> Comment {
    Comment {
        id: note["id"].as_i64().unwrap_or(0),
        author: note["author"]["username"].as_str().unwrap_or_default().to_string(),
        body: str_at(note, "body"),
        created_at: str_at(note, "created_at"),
        url: String::new(),
        image_map: Default::default(),
    }
}

fn str_at(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeRunner {
        calls: Mutex<Vec<(Vec<String>, Option<String>)>>,
        responses: Mutex<Vec<Result<String, String>>>,
    }

    impl FakeRunner {
        fn new(responses: Vec<Result<String, String>>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                responses: Mutex::new(responses),
            }
        }
    }

    impl Runner for FakeRunner {
        fn run(
            &self,
            program: &str,
            args: &[String],
            stdin: Option<&[u8]>,
        ) -> Result<String, String> {
            assert_eq!(program, "glab");
            self.calls.lock().unwrap().push((
                args.to_vec(),
                stdin.map(|b| String::from_utf8_lossy(b).to_string()),
            ));
            self.responses.lock().unwrap().remove(0)
        }
    }

    fn reference() -> PullRequestRef {
        PullRequestRef {
            host: "gitlab.com".into(),
            owner: "group/sub".into(),
            repo: "repo".into(),
            number: 42,
        }
    }

    #[test]
    fn nested_groups_encode_and_states_map() {
        let forge = GlabForge::with_runner(FakeRunner::new(vec![Ok(r#"{
            "title": "T", "description": "D", "state": "merged", "web_url": "u",
            "author": {"username": "alice"}, "sha": "abc",
            "source_branch": "feat", "target_branch": "main"
        }"#
        .into())]));
        let pr = forge.pull_request(&reference()).unwrap();
        assert!(pr.merged);
        assert_eq!(pr.state, "closed");
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(calls[0].0[1], "projects/group%2Fsub%2Frepo/merge_requests/42");
    }

    #[test]
    fn diff_reconstruction() {
        let forge = GlabForge::with_runner(FakeRunner::new(vec![Ok(r#"{
            "changes": [
                {"old_path": "a.rs", "new_path": "a.rs",
                 "diff": "@@ -1 +1 @@\n-x\n+y\n"},
                {"old_path": "old.rs", "new_path": "new.rs", "renamed_file": true,
                 "diff": "@@ -1 +1 @@\n-a\n+b\n"},
                {"old_path": "born.rs", "new_path": "born.rs", "new_file": true,
                 "diff": "@@ -0,0 +1 @@\n+hi\n"}
            ]
        }"#
        .into())]));
        let patch = forge.diff(&reference()).unwrap();
        let files = prchum_core::diff::parse(&patch, 4).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[1].status, prchum_core::diff::FileStatus::Renamed);
        assert_eq!(files[2].status, prchum_core::diff::FileStatus::Added);
    }

    #[test]
    fn review_posts_positioned_discussions_and_counts_failures() {
        let forge = GlabForge::with_runner(FakeRunner::new(vec![
            Ok(r#"{"diff_refs": {"base_sha": "b", "start_sha": "s", "head_sha": "h"}}"#.into()),
            Ok("{}".into()),
            Err("boom".into()),
        ]));
        let comments = vec![
            ReviewComment {
                path: "a.rs".into(), body: "one".into(), line: 5,
                side: "RIGHT".into(), start_line: None, start_side: None,
            },
            ReviewComment {
                path: "b.rs".into(), body: "two".into(), line: 9,
                side: "LEFT".into(), start_line: None, start_side: None,
            },
        ];
        let error = forge
            .create_review(&reference(), "COMMENT", "", &comments)
            .unwrap_err();
        assert!(error.contains("posted 1 of 2"), "{error}");
        let calls = forge.runner.calls.lock().unwrap();
        let body: Value = serde_json::from_str(calls[1].1.as_ref().unwrap()).unwrap();
        assert_eq!(body["position"]["new_line"], 5);
        assert_eq!(body["position"]["base_sha"], "b");
    }

    #[test]
    fn approve_and_request_changes_map() {
        let forge = GlabForge::with_runner(FakeRunner::new(vec![
            Ok("{}".into()), // approve
            Ok("{}".into()), // summary note
        ]));
        forge.create_review(&reference(), "APPROVE", "ship it", &[]).unwrap();
        let calls = forge.runner.calls.lock().unwrap();
        assert!(calls[0].0[1].ends_with("/approve"));
        assert!(calls[1].1.as_ref().unwrap().contains("ship it"));
        drop(calls);

        let forge = GlabForge::with_runner(FakeRunner::new(vec![Ok("{}".into())]));
        forge
            .create_review(&reference(), "REQUEST_CHANGES", "needs work", &[])
            .unwrap();
        let calls = forge.runner.calls.lock().unwrap();
        assert!(calls[0].1.as_ref().unwrap().contains("Changes requested"));
    }

    #[test]
    fn suggestion_fences_gain_the_range() {
        assert_eq!(
            adapt_suggestion("```suggestion\nnew code\n```", Some(3), 5),
            "```suggestion:-2+0\nnew code\n```"
        );
        assert_eq!(
            adapt_suggestion("```suggestion\nx\n```", None, 5),
            "```suggestion:-0+0\nx\n```"
        );
        assert_eq!(adapt_suggestion("plain body", Some(1), 2), "plain body");
    }

    #[test]
    fn reply_finds_the_discussion_by_root_note() {
        let forge = GlabForge::with_runner(FakeRunner::new(vec![
            Ok(r#"[{"id": "deadbeef", "notes": [{"id": 77, "body": "root",
                "author": {"username": "x"}, "created_at": "t",
                "position": {"position_type": "text", "new_line": 5, "new_path": "a.rs"}}]}]"#
                .into()),
            Ok("{}".into()),
        ]));
        forge.reply(&reference(), 77, "answer").unwrap();
        let calls = forge.runner.calls.lock().unwrap();
        assert!(calls[1].0[1].contains("/discussions/deadbeef/notes"));
    }
}
