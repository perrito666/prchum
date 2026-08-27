//! GitHub adapter over the `gh` CLI.
//!
//! Every call is `gh api [--hostname H] <path> [flags]`; JSON bodies feed
//! stdin via `--method POST --input -`. The command runner is a trait so
//! tests script the CLI instead of the network.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use crate::{Comment, Forge, PullRequest, PullRequestRef, ReviewComment, ThreadInfo};

/// Runs a CLI and returns stdout; nonzero exit is an error carrying stderr.
pub trait Runner: Send + Sync {
    fn run(&self, program: &str, args: &[String], stdin: Option<&[u8]>) -> Result<String, String>;
}

/// The real thing: a subprocess with the user's PATH and auth.
pub struct ProcessRunner;

impl Runner for ProcessRunner {
    fn run(&self, program: &str, args: &[String], stdin: Option<&[u8]>) -> Result<String, String> {
        let mut command = prchum_core::host::command(program);
        command.args(args);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        let mut child = command.spawn().map_err(|error| {
            format!("could not run {program}: {error} (is it installed and on PATH?)")
        })?;
        if let Some(data) = stdin {
            if let Some(mut pipe) = child.stdin.take() {
                let _ = pipe.write_all(data);
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|error| format!("{program} failed: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("{program} {}: {}", args.join(" "), stderr.trim()));
        }
        String::from_utf8(output.stdout).map_err(|_| format!("{program} output was not UTF-8"))
    }
}

pub struct GhForge<R: Runner> {
    runner: R,
}

impl GhForge<ProcessRunner> {
    pub fn new() -> Self {
        Self {
            runner: ProcessRunner,
        }
    }
}

impl Default for GhForge<ProcessRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Runner> GhForge<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }

    fn api(&self, pr: &PullRequestRef, extra: &[&str], stdin: Option<&[u8]>) -> Result<String, String> {
        let mut args = vec!["api".to_string()];
        // github.com is gh's default; only enterprise hosts need the flag.
        if !pr.host.is_empty() && pr.host != "github.com" {
            args.push("--hostname".to_string());
            args.push(pr.host.clone());
        }
        args.extend(extra.iter().map(|s| s.to_string()));
        self.runner.run("gh", &args, stdin)
    }

    fn repo_path(pr: &PullRequestRef, suffix: &str) -> String {
        format!("repos/{}/{}/{}", pr.owner, pr.repo, suffix)
    }
}

impl<R: Runner> Forge for GhForge<R> {
    fn pull_request(&self, pr: &PullRequestRef) -> Result<PullRequest, String> {
        let path = Self::repo_path(pr, &format!("pulls/{}", pr.number));
        let text = self.api(pr, &[&path], None)?;
        let value: Value = parse_json(&text)?;
        Ok(PullRequest {
            number: pr.number,
            state: str_at(&value, "state"),
            merged: value["merged"].as_bool().unwrap_or(false),
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
        let path = Self::repo_path(pr, &format!("pulls/{}", pr.number));
        self.api(pr, &[&path, "-H", "Accept: application/vnd.github.v3.diff"], None)
    }

    fn threads(&self, pr: &PullRequestRef) -> Result<Vec<ThreadInfo>, String> {
        let path = Self::repo_path(pr, &format!("pulls/{}/comments", pr.number));
        // full+json adds body_html, whose img tags carry the signed
        // variants of session-gated attachment URLs.
        let text = self.api(
            pr,
            &[&path, "--paginate", "-H", "Accept: application/vnd.github.full+json"],
            None,
        )?;
        let items = parse_paginated(&text)?;

        // Roots first (no in_reply_to_id), replies attach to their root.
        let mut threads: Vec<ThreadInfo> = Vec::new();
        for item in &items {
            if item.get("in_reply_to_id").and_then(Value::as_i64).is_some() {
                continue;
            }
            let line = item["line"].as_u64().map(|n| n as u32);
            let original_line = item["original_line"].as_u64().map(|n| n as u32);
            threads.push(ThreadInfo {
                id: item["id"].as_i64().unwrap_or(0),
                path: str_at(item, "path"),
                side: {
                    let side = str_at(item, "side");
                    if side.is_empty() { "RIGHT".to_string() } else { side }
                },
                line,
                start_line: item["start_line"].as_u64().map(|n| n as u32),
                original_line,
                outdated: line.is_none() && original_line.is_some(),
                comments: vec![comment_from(item)],
            });
        }
        for item in &items {
            let Some(parent) = item.get("in_reply_to_id").and_then(Value::as_i64) else {
                continue;
            };
            if let Some(thread) = threads.iter_mut().find(|t| t.id == parent) {
                thread.comments.push(comment_from(item));
            }
        }
        Ok(threads)
    }

    fn general_comments(&self, pr: &PullRequestRef) -> Result<Vec<Comment>, String> {
        // A pull request is an issue with code attached.
        let path = Self::repo_path(pr, &format!("issues/{}/comments", pr.number));
        let text = self.api(
            pr,
            &[&path, "--paginate", "-H", "Accept: application/vnd.github.full+json"],
            None,
        )?;
        Ok(parse_paginated(&text)?.iter().map(comment_from).collect())
    }

    fn create_review(
        &self,
        pr: &PullRequestRef,
        event: &str,
        summary: &str,
        comments: &[ReviewComment],
    ) -> Result<(), String> {
        let path = Self::repo_path(pr, &format!("pulls/{}/reviews", pr.number));
        let body = json!({
            "event": event,
            "body": summary,
            "comments": comments,
        });
        self.api(
            pr,
            &[&path, "--method", "POST", "--input", "-"],
            Some(body.to_string().as_bytes()),
        )?;
        Ok(())
    }

    fn reply(&self, pr: &PullRequestRef, comment_id: i64, body: &str) -> Result<(), String> {
        let path = Self::repo_path(
            pr,
            &format!("pulls/{}/comments/{comment_id}/replies", pr.number),
        );
        let payload = json!({ "body": body });
        self.api(
            pr,
            &[&path, "--method", "POST", "--input", "-"],
            Some(payload.to_string().as_bytes()),
        )?;
        Ok(())
    }

    fn file_content(&self, pr: &PullRequestRef, path: &str, rev: &str) -> Result<String, String> {
        let escaped: String = path
            .split('/')
            .map(|segment| segment.replace('%', "%25").replace('#', "%23").replace('?', "%3F"))
            .collect::<Vec<_>>()
            .join("/");
        let api_path = Self::repo_path(pr, &format!("contents/{escaped}?ref={rev}"));
        self.api(pr, &[&api_path, "-H", "Accept: application/vnd.github.raw"], None)
    }

    fn add_general_comment(&self, pr: &PullRequestRef, body: &str) -> Result<(), String> {
        let path = Self::repo_path(pr, &format!("issues/{}/comments", pr.number));
        let payload = json!({ "body": body });
        self.api(
            pr,
            &[&path, "--method", "POST", "--input", "-"],
            Some(payload.to_string().as_bytes()),
        )?;
        Ok(())
    }
}

fn parse_json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("unexpected gh output: {error}"))
}

/// `--paginate` concatenates JSON arrays; accept one array or several.
fn parse_paginated(text: &str) -> Result<Vec<Value>, String> {
    let mut items = Vec::new();
    let mut deserializer = serde_json::Deserializer::from_str(text).into_iter::<Value>();
    for value in &mut deserializer {
        let value = value.map_err(|error| format!("unexpected gh output: {error}"))?;
        match value {
            Value::Array(chunk) => items.extend(chunk),
            other => items.push(other),
        }
    }
    Ok(items)
}

fn comment_from(item: &Value) -> Comment {
    let body = str_at(item, "body");
    Comment {
        id: item["id"].as_i64().unwrap_or(0),
        author: item["user"]["login"].as_str().unwrap_or_default().to_string(),
        image_map: attachment_map(&body, item["body_html"].as_str().unwrap_or_default()),
        body,
        created_at: str_at(item, "created_at"),
        url: str_at(item, "html_url"),
    }
}

/// Maps session-gated `github.com/user-attachments/assets/<id>` URLs in
/// the body to the signed `private-user-images` variants the rendered
/// HTML carries — matched by the asset id embedded in the signed URL.
/// Deliberately not fetched through gh: the raw asset URLs answer API
/// credentials with a viewer page, not the asset.
fn attachment_map(
    body: &str,
    body_html: &str,
) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    if body_html.is_empty() {
        return map;
    }
    // Signed URLs out of the HTML's src attributes.
    let mut signed: Vec<String> = Vec::new();
    for chunk in body_html.split("src=\"").skip(1) {
        if let Some(url) = chunk.split('"').next() {
            if url.contains("private-user-images.githubusercontent.com") {
                signed.push(url.to_string());
            }
        }
    }
    // Gated URLs out of the plain body, matched by asset id.
    for chunk in body.split("github.com/user-attachments/assets/").skip(1) {
        let id: String = chunk
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if id.is_empty() {
            continue;
        }
        let original = format!("https://github.com/user-attachments/assets/{id}");
        if let Some(url) = signed.iter().find(|s| s.contains(&id)) {
            map.insert(original, url.clone());
        }
    }
    map
}

fn str_at(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Scripted gh: records calls, answers from a queue.
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
            assert_eq!(program, "gh");
            self.calls.lock().unwrap().push((
                args.to_vec(),
                stdin.map(|b| String::from_utf8_lossy(b).to_string()),
            ));
            self.responses.lock().unwrap().remove(0)
        }
    }

    fn reference() -> PullRequestRef {
        PullRequestRef {
            host: "github.com".into(),
            owner: "o".into(),
            repo: "r".into(),
            number: 7,
        }
    }

    #[test]
    fn pull_request_parses_and_omits_default_hostname() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![Ok(r#"{
            "title": "T", "body": "B", "html_url": "u",
            "user": {"login": "alice"},
            "head": {"sha": "abc", "ref": "feat"}, "base": {"ref": "main"}
        }"#
        .into())]));
        let pr = forge.pull_request(&reference()).unwrap();
        assert_eq!(pr.title, "T");
        assert_eq!(pr.author, "alice");
        assert_eq!(pr.head_oid, "abc");
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(calls[0].0, vec!["api", "repos/o/r/pulls/7"]);
    }

    #[test]
    fn enterprise_hosts_get_the_hostname_flag() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![Ok("{}".into())]));
        let mut pr = reference();
        pr.host = "github.corp.example".into();
        let _ = forge.pull_request(&pr);
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(calls[0].0[..3], ["api", "--hostname", "github.corp.example"]);
    }

    #[test]
    fn threads_group_roots_and_replies() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![Ok(r#"[
            {"id": 1, "path": "a.rs", "side": "RIGHT", "line": 5,
             "body": "root", "user": {"login": "x"}, "created_at": "t1", "html_url": ""},
            {"id": 2, "in_reply_to_id": 1, "body": "reply",
             "user": {"login": "y"}, "created_at": "t2", "html_url": ""},
            {"id": 3, "path": "b.rs", "side": "LEFT", "line": null, "original_line": 9,
             "body": "old", "user": {"login": "z"}, "created_at": "t3", "html_url": ""}
        ]"#
        .into())]));
        let threads = forge.threads(&reference()).unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].comments.len(), 2);
        assert_eq!(threads[0].comments[1].body, "reply");
        assert!(threads[1].outdated);
        assert_eq!(threads[1].original_line, Some(9));
    }

    #[test]
    fn create_review_posts_one_atomic_body() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![Ok("{}".into())]));
        let comments = vec![ReviewComment {
            path: "a.rs".into(),
            body: "note".into(),
            line: 5,
            side: "RIGHT".into(),
            start_line: Some(3),
            start_side: Some("RIGHT".into()),
        }];
        forge
            .create_review(&reference(), "APPROVE", "lgtm", &comments)
            .unwrap();
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(
            calls[0].0,
            vec!["api", "repos/o/r/pulls/7/reviews", "--method", "POST", "--input", "-"]
        );
        let body: Value = serde_json::from_str(calls[0].1.as_ref().unwrap()).unwrap();
        assert_eq!(body["event"], "APPROVE");
        assert_eq!(body["comments"][0]["start_line"], 3);
        assert_eq!(body["comments"][0]["line"], 5);
    }

    #[test]
    fn reply_targets_the_root_comment() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![Ok("{}".into())]));
        forge.reply(&reference(), 99, "hello").unwrap();
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(calls[0].0[1], "repos/o/r/pulls/7/comments/99/replies");
        assert!(calls[0].1.as_ref().unwrap().contains("hello"));
    }

    #[test]
    fn attachment_map_matches_by_asset_id() {
        let body = "look:\nhttps://github.com/user-attachments/assets/abc-123\nand text";
        let html = r#"<p>look:</p><img src="https://private-user-images.githubusercontent.com/1/abc-123.png?jwt=tok"><p>and text</p>"#;
        let map = attachment_map(body, html);
        assert_eq!(map.len(), 1);
        assert!(map["https://github.com/user-attachments/assets/abc-123"].contains("jwt=tok"));
        assert!(attachment_map(body, "").is_empty());
        assert!(attachment_map("no attachments", html).is_empty());
    }

    #[test]
    fn paginated_concatenated_arrays() {
        let items = parse_paginated(r#"[{"id": 1}][{"id": 2}]"#).unwrap();
        assert_eq!(items.len(), 2);
    }
}
