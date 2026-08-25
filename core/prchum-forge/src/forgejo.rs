//! Forgejo adapter, over the Gitea-compatible v1 REST API.
//!
//! The Forgejo CLI ecosystem is younger and more fragmented than gh/glab,
//! so the transport is a **command template** with `{host}`, `{method}`,
//! and `{path}` placeholders and the JSON body on stdin. The default
//! targets the `fj` CLI (forgejo-contrib), which owns auth the way gh
//! does; a config override (`forgejo_api_command`) adapts to whatever the
//! instance's tooling is without touching code — prchum still never
//! manages a token.
//!
//! Mapping notes (Forgejo has no atomic-review parity with GitHub):
//! * Reviews are created with `APPROVED`/`REQUEST_CHANGES`/`COMMENT`
//!   events; line comments ride the review as `new_position` (RIGHT) or
//!   `old_position` (LEFT) line numbers. Multi-line ranges anchor on the
//!   end line.
//! * There is no per-comment reply endpoint: a reply becomes a positioned
//!   comment in a fresh `COMMENT` review at the thread's location.
//! * Thread positions read back as line numbers without an explicit side;
//!   `position == 0` marks an outdated thread.

use serde_json::{json, Value};

use crate::ghcli::Runner;
use crate::{Comment, Forge, PullRequest, PullRequestRef, ReviewComment, ThreadInfo};

/// The default transport: the `fj` CLI's host-scoped api passthrough.
/// `{path}` is relative to `/api/v1`, with a leading slash.
pub const DEFAULT_API_COMMAND: &str = "fj -H {host} api {method} {path}";

pub struct ForgejoForge<R: Runner> {
    runner: R,
    /// Command template with `{host}`, `{method}`, `{path}` placeholders.
    template: String,
}

impl<R: Runner> ForgejoForge<R> {
    pub fn with_runner(runner: R, template: &str) -> Self {
        Self {
            runner,
            template: if template.is_empty() {
                DEFAULT_API_COMMAND.to_string()
            } else {
                template.to_string()
            },
        }
    }

    fn request(
        &self,
        pr: &PullRequestRef,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<String, String> {
        let mut words: Vec<String> = Vec::new();
        for word in self.template.split_whitespace() {
            words.push(
                word.replace("{host}", &pr.host)
                    .replace("{method}", method)
                    .replace("{path}", path),
            );
        }
        let Some((program, args)) = words.split_first() else {
            return Err("forgejo_api_command is empty".to_string());
        };
        let payload = body.map(|value| value.to_string());
        self.runner
            .run(program, args, payload.as_ref().map(|p| p.as_bytes()))
    }

    fn pulls_path(pr: &PullRequestRef, suffix: &str) -> String {
        format!("/repos/{}/{}/pulls/{}{}", pr.owner, pr.repo, pr.number, suffix)
    }
}

impl<R: Runner> Forge for ForgejoForge<R> {
    fn pull_request(&self, pr: &PullRequestRef) -> Result<PullRequest, String> {
        let text = self.request(pr, "GET", &Self::pulls_path(pr, ""), None)?;
        let value: Value = parse_json(&text)?;
        Ok(PullRequest {
            number: pr.number,
            title: str_at(&value, "title"),
            body: str_at(&value, "body"),
            author: value["user"]["login"].as_str().unwrap_or_default().to_string(),
            url: str_at(&value, "html_url"),
            head_oid: value["head"]["sha"].as_str().unwrap_or_default().to_string(),
            base_ref: value["base"]["ref"].as_str().unwrap_or_default().to_string(),
            head_ref: value["head"]["ref"].as_str().unwrap_or_default().to_string(),
        })
    }

    fn diff(&self, pr: &PullRequestRef) -> Result<String, String> {
        // The `.diff` endpoint serves the canonical patch as plain text.
        self.request(pr, "GET", &Self::pulls_path(pr, ".diff"), None)
    }

    fn threads(&self, pr: &PullRequestRef) -> Result<Vec<ThreadInfo>, String> {
        let text = self.request(pr, "GET", &Self::pulls_path(pr, "/reviews"), None)?;
        let reviews: Value = parse_json(&text)?;
        let mut threads: Vec<ThreadInfo> = Vec::new();
        for review in reviews.as_array().map(Vec::as_slice).unwrap_or_default() {
            let Some(review_id) = review["id"].as_i64() else {
                continue;
            };
            if review["comments_count"].as_i64() == Some(0) {
                continue;
            }
            let path = Self::pulls_path(pr, &format!("/reviews/{review_id}/comments"));
            let text = self.request(pr, "GET", &path, None)?;
            let comments: Value = parse_json(&text)?;
            for item in comments.as_array().map(Vec::as_slice).unwrap_or_default() {
                let position = item["position"].as_u64().unwrap_or(0) as u32;
                let original = item["original_position"].as_u64().unwrap_or(0) as u32;
                threads.push(ThreadInfo {
                    id: item["id"].as_i64().unwrap_or(0),
                    path: str_at(item, "path"),
                    // Forgejo reports a line number without an explicit
                    // side; treat it as the new file's.
                    side: "RIGHT".to_string(),
                    line: (position > 0).then_some(position),
                    start_line: None,
                    original_line: (original > 0).then_some(original),
                    outdated: position == 0 && original > 0,
                    comments: vec![comment_from(item)],
                });
            }
        }
        Ok(threads)
    }

    fn general_comments(&self, pr: &PullRequestRef) -> Result<Vec<Comment>, String> {
        let path = format!("/repos/{}/{}/issues/{}/comments", pr.owner, pr.repo, pr.number);
        let text = self.request(pr, "GET", &path, None)?;
        let items: Value = parse_json(&text)?;
        Ok(items
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(comment_from)
            .collect())
    }

    fn create_review(
        &self,
        pr: &PullRequestRef,
        event: &str,
        summary: &str,
        comments: &[ReviewComment],
    ) -> Result<(), String> {
        // Forgejo's approve event is APPROVED, not GitHub's APPROVE.
        let event = if event == "APPROVE" { "APPROVED" } else { event };
        let wire: Vec<Value> = comments
            .iter()
            .map(|comment| {
                let mut entry = json!({
                    "path": comment.path,
                    "body": comment.body,
                });
                if comment.side == "LEFT" {
                    entry["old_position"] = json!(comment.line);
                } else {
                    entry["new_position"] = json!(comment.line);
                }
                entry
            })
            .collect();
        let body = json!({ "event": event, "body": summary, "comments": wire });
        self.request(pr, "POST", &Self::pulls_path(pr, "/reviews"), Some(&body))?;
        Ok(())
    }

    fn reply(&self, pr: &PullRequestRef, comment_id: i64, body: &str) -> Result<(), String> {
        // No reply endpoint: find the thread's position, then post a
        // positioned comment in a fresh COMMENT review at the same spot.
        let target = self
            .threads(pr)?
            .into_iter()
            .find(|thread| thread.id == comment_id)
            .ok_or_else(|| format!("comment {comment_id} not found to reply to"))?;
        let comment = ReviewComment {
            path: target.path,
            body: body.to_string(),
            line: target.line.or(target.original_line).unwrap_or(1),
            side: target.side,
            start_line: None,
            start_side: None,
        };
        self.create_review(pr, "COMMENT", "", &[comment])
    }

    fn add_general_comment(&self, pr: &PullRequestRef, body: &str) -> Result<(), String> {
        let path = format!("/repos/{}/{}/issues/{}/comments", pr.owner, pr.repo, pr.number);
        self.request(pr, "POST", &path, Some(&json!({ "body": body })))?;
        Ok(())
    }
}

fn parse_json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("unexpected forgejo output: {error}"))
}

fn comment_from(item: &Value) -> Comment {
    Comment {
        id: item["id"].as_i64().unwrap_or(0),
        author: item["user"]["login"].as_str().unwrap_or_default().to_string(),
        body: str_at(item, "body"),
        created_at: str_at(item, "created_at"),
        url: str_at(item, "html_url"),
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
        calls: Mutex<Vec<(String, Vec<String>, Option<String>)>>,
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
            self.calls.lock().unwrap().push((
                program.to_string(),
                args.to_vec(),
                stdin.map(|b| String::from_utf8_lossy(b).to_string()),
            ));
            self.responses.lock().unwrap().remove(0)
        }
    }

    fn reference() -> PullRequestRef {
        PullRequestRef {
            host: "codeberg.org".into(),
            owner: "o".into(),
            repo: "r".into(),
            number: 7,
        }
    }

    #[test]
    fn default_template_targets_fj() {
        let forge = ForgejoForge::with_runner(FakeRunner::new(vec![Ok("{}".into())]), "");
        let _ = forge.pull_request(&reference());
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(calls[0].0, "fj");
        assert_eq!(
            calls[0].1,
            vec!["-H", "codeberg.org", "api", "GET", "/repos/o/r/pulls/7"]
        );
    }

    #[test]
    fn custom_template_substitutes_placeholders() {
        let forge = ForgejoForge::with_runner(
            FakeRunner::new(vec![Ok("{}".into())]),
            "curl -sf -X {method} https://{host}/api/v1{path}",
        );
        let _ = forge.pull_request(&reference());
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(calls[0].0, "curl");
        assert_eq!(
            calls[0].1.last().unwrap(),
            "https://codeberg.org/api/v1/repos/o/r/pulls/7"
        );
    }

    #[test]
    fn diff_uses_the_diff_endpoint() {
        let forge = ForgejoForge::with_runner(
            FakeRunner::new(vec![Ok("--- a/x\n+++ b/x\n".into())]),
            "",
        );
        let diff = forge.diff(&reference()).unwrap();
        assert!(diff.starts_with("--- a/x"));
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(calls[0].1.last().unwrap(), "/repos/o/r/pulls/7.diff");
    }

    #[test]
    fn create_review_maps_events_and_sides() {
        let forge = ForgejoForge::with_runner(FakeRunner::new(vec![Ok("{}".into())]), "");
        let comments = vec![
            ReviewComment {
                path: "a.rs".into(),
                body: "right side".into(),
                line: 5,
                side: "RIGHT".into(),
                start_line: Some(3),
                start_side: Some("RIGHT".into()),
            },
            ReviewComment {
                path: "b.rs".into(),
                body: "left side".into(),
                line: 9,
                side: "LEFT".into(),
                start_line: None,
                start_side: None,
            },
        ];
        forge
            .create_review(&reference(), "APPROVE", "lgtm", &comments)
            .unwrap();
        let calls = forge.runner.calls.lock().unwrap();
        let body: Value = serde_json::from_str(calls[0].2.as_ref().unwrap()).unwrap();
        assert_eq!(body["event"], "APPROVED");
        assert_eq!(body["comments"][0]["new_position"], 5);
        assert!(body["comments"][0].get("old_position").is_none());
        assert_eq!(body["comments"][1]["old_position"], 9);
    }

    #[test]
    fn threads_flatten_review_comments() {
        let forge = ForgejoForge::with_runner(
            FakeRunner::new(vec![
                Ok(r#"[{"id": 1, "comments_count": 1}, {"id": 2, "comments_count": 0}]"#.into()),
                Ok(r#"[{"id": 11, "path": "a.rs", "position": 5, "original_position": 5,
                        "body": "hm", "user": {"login": "x"}, "created_at": "t", "html_url": ""}]"#
                    .into()),
            ]),
            "",
        );
        let threads = forge.threads(&reference()).unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].line, Some(5));
        assert!(!threads[0].outdated);
        // The empty review was skipped without a comments fetch.
        assert_eq!(forge.runner.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn reply_posts_a_positioned_comment_review() {
        let forge = ForgejoForge::with_runner(
            FakeRunner::new(vec![
                Ok(r#"[{"id": 1, "comments_count": 1}]"#.into()),
                Ok(r#"[{"id": 11, "path": "a.rs", "position": 5, "original_position": 5,
                        "body": "hm", "user": {"login": "x"}, "created_at": "t", "html_url": ""}]"#
                    .into()),
                Ok("{}".into()),
            ]),
            "",
        );
        forge.reply(&reference(), 11, "answer").unwrap();
        let calls = forge.runner.calls.lock().unwrap();
        let body: Value = serde_json::from_str(calls[2].2.as_ref().unwrap()).unwrap();
        assert_eq!(body["event"], "COMMENT");
        assert_eq!(body["comments"][0]["new_position"], 5);
        assert_eq!(body["comments"][0]["body"], "answer");
    }
}
