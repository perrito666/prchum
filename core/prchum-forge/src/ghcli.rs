//! GitHub adapter over the `gh` CLI.
//!
//! Every call is `gh api [--hostname H] <path> [flags]`; JSON bodies feed
//! stdin via `--method POST --input -`. The command runner is a trait so
//! tests script the CLI instead of the network.
//!
//! The canonical diff endpoint refuses requests past 300 files (or 20,000
//! lines). Those fall back to the paginated file list, which carries each
//! file's patch in the same form — except for files GitHub deems too
//! large or binary, and past the list's own 3,000-file cap. A local clone,
//! when one is known, fills those gaps with `git diff`; without one the
//! file is listed with no content rather than the review failing.

use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Stdio};

use prchum_core::source::git_in;
use serde_json::{json, Value};

use crate::{
    first_line, oldest_first, Comment, CommitInfo, CommitList, Forge, PullRequest,
    PullRequestRef, ReviewComment, ThreadInfo,
};

/// GitHub's pull-request commit listing ends here, however many commits
/// the request has.
pub const GITHUB_COMMIT_LIST_LIMIT: usize = 250;

const RESOLVED_THREADS_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!, $cursor: String) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { reviewThreads(first: 100, after: $cursor) { nodes { isResolved comments(first: 1) { nodes { databaseId } } } pageInfo { hasNextPage endCursor } } } } }";

const RESOLVED_THREADS_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!, $cursor: String) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { reviewThreads(first: 100, after: $cursor) { nodes { isResolved comments(first: 1) { nodes { databaseId } } } pageInfo { hasNextPage endCursor } } } } }";

/// Runs a CLI and returns stdout; nonzero exit is an error carrying stderr.
pub trait Runner: Send + Sync {
    fn run(&self, program: &str, args: &[String], stdin: Option<&[u8]>) -> Result<String, String>;
}

/// The real thing: a subprocess with the user's PATH and auth.
pub struct ProcessRunner;

impl Runner for ProcessRunner {
    fn run(&self, program: &str, args: &[String], stdin: Option<&[u8]>) -> Result<String, String> {
        let mut command = Command::new(program);
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

/// The file list stops here however many pages are requested.
const FILE_LIST_CAP: usize = 3000;

pub struct GhForge<R: Runner> {
    runner: R,
    /// A local clone of the repository, for the parts of a large diff the
    /// API will not serve.
    clone: Option<String>,
}

impl GhForge<ProcessRunner> {
    pub fn new() -> Self {
        Self::with_runner(ProcessRunner)
    }
}

impl Default for GhForge<ProcessRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Runner> GhForge<R> {
    pub fn with_runner(runner: R) -> Self {
        Self {
            runner,
            clone: None,
        }
    }

    /// Names a local clone of the repository; an empty path is none.
    pub fn with_clone(mut self, clone: &str) -> Self {
        self.clone = (!clone.is_empty()).then(|| clone.to_string());
        self
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

    /// Root comment ids of the resolved review threads. REST does not
    /// carry resolution; GraphQL does, keyed by thread, and a thread's
    /// first comment's `databaseId` is the REST id of its root.
    fn resolved_roots(&self, pr: &PullRequestRef) -> Result<HashSet<i64>, String> {
        let owner = format!("owner={}", pr.owner);
        let name = format!("name={}", pr.repo);
        let number = format!("number={}", pr.number);
        let query = format!("query={RESOLVED_THREADS_QUERY}");
        let mut resolved = HashSet::new();
        let mut cursor: Option<String> = None;
        // Bounded so a host that keeps answering hasNextPage cannot hold
        // the review open; 100 pages is 10,000 threads.
        for _ in 0..100 {
            let after = cursor.as_ref().map(|c| format!("cursor={c}"));
            // -f sends strings as-is (a numeric repository name stays a
            // string); -F types the number.
            let mut args: Vec<&str> = vec![
                "graphql", "-f", &query, "-f", &owner, "-f", &name, "-F", &number,
            ];
            if let Some(after) = &after {
                args.extend(["-f", after.as_str()]);
            }
            let text = self.api(pr, &args, None)?;
            let value = parse_json(&text)?;
            if let Some(errors) = value["errors"].as_array().filter(|e| !e.is_empty()) {
                return Err(format!("graphql: {}", errors[0]["message"]));
            }
            let connection = &value["data"]["repository"]["pullRequest"]["reviewThreads"];
            for node in connection["nodes"].as_array().map(Vec::as_slice).unwrap_or_default() {
                if node["isResolved"].as_bool() != Some(true) {
                    continue;
                }
                if let Some(id) = node["comments"]["nodes"][0]["databaseId"].as_i64() {
                    resolved.insert(id);
                }
            }
            let info = &connection["pageInfo"];
            match (info["hasNextPage"].as_bool(), info["endCursor"].as_str()) {
                (Some(true), Some(end)) => cursor = Some(end.to_string()),
                _ => break,
            }
        }
        Ok(resolved)
    }

    fn repo_path(pr: &PullRequestRef, suffix: &str) -> String {
        format!("repos/{}/{}/{}", pr.owner, pr.repo, suffix)
    }

    /// The diff rebuilt from the paginated file list, for requests the
    /// diff endpoint refuses as too large.
    fn diff_from_files(&self, pr: &PullRequestRef) -> Result<String, String> {
        let path = Self::repo_path(pr, &format!("pulls/{}/files?per_page=100", pr.number));
        let files = parse_paginated(&self.api(pr, &[&path, "--paginate"], None)?)?;

        let needs_clone = files.len() >= FILE_LIST_CAP
            || files.iter().any(|file| file["patch"].as_str().is_none());
        let local = match (&self.clone, needs_clone) {
            (Some(clone), true) => Some(self.local_revisions(pr, clone)?),
            _ => None,
        };

        if files.len() >= FILE_LIST_CAP {
            // The list is truncated, and nothing says which files it left
            // out; only the clone knows the whole change.
            let Some((clone, base, head)) = &local else {
                return Err(format!(
                    "the pull request changes more than {FILE_LIST_CAP} files, more than \
                     GitHub will list — configure a local clone of {}/{} to review it",
                    pr.owner, pr.repo
                ));
            };
            return local_diff(clone, base, head, &[]);
        }

        let mut patch = String::new();
        for file in &files {
            let new_path = str_at(file, "filename");
            let old_path = file["previous_filename"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| new_path.clone());
            let header = format!("diff --git a/{old_path} b/{new_path}\n");
            let status = str_at(file, "status");

            if file["patch"].as_str().is_none() {
                if let Some((clone, base, head)) = &local {
                    let mut paths = vec![new_path.as_str()];
                    if old_path != new_path {
                        paths.push(old_path.as_str());
                    }
                    let text = local_diff(clone, base, head, &paths)?;
                    if !text.is_empty() {
                        patch.push_str(&text);
                        continue;
                    }
                }
            }

            patch.push_str(&header);
            match status.as_str() {
                "added" => {
                    patch.push_str("new file mode 100644\n--- /dev/null\n");
                    patch.push_str(&format!("+++ b/{new_path}\n"));
                }
                "removed" => {
                    patch.push_str("deleted file mode 100644\n");
                    patch.push_str(&format!("--- a/{old_path}\n+++ /dev/null\n"));
                }
                _ => {
                    if status == "renamed" {
                        patch.push_str(&format!("rename from {old_path}\nrename to {new_path}\n"));
                    } else if status == "copied" {
                        patch.push_str(&format!("copy from {old_path}\ncopy to {new_path}\n"));
                    }
                    patch.push_str(&format!("--- a/{old_path}\n+++ b/{new_path}\n"));
                }
            }
            if let Some(body) = file["patch"].as_str() {
                patch.push_str(body);
                if !patch.ends_with('\n') {
                    patch.push('\n');
                }
            }
        }
        if patch.is_empty() {
            return Err("the pull request has no changes".to_string());
        }
        Ok(patch)
    }

    /// The clone's root and the request's base and head commits, fetched
    /// into the clone when it does not have them. Fetching by commit
    /// writes no refs, so the clone's branches are left as they were.
    fn local_revisions(
        &self,
        pr: &PullRequestRef,
        clone: &str,
    ) -> Result<(String, String, String), String> {
        let root = git_in(clone, &["rev-parse", "--show-toplevel"])
            .map_err(|_| format!("{clone} is not a git repository"))?
            .trim()
            .to_string();
        let path = Self::repo_path(pr, &format!("pulls/{}", pr.number));
        let value = parse_json(&self.api(pr, &[&path], None)?)?;
        let base = value["base"]["sha"].as_str().unwrap_or_default().to_string();
        let head = value["head"]["sha"].as_str().unwrap_or_default().to_string();
        if base.is_empty() || head.is_empty() {
            return Err("the pull request names no base or head commit".to_string());
        }
        let has = |oid: &str| {
            git_in(&root, &["cat-file", "-e", &format!("{oid}^{{commit}}")]).is_ok()
        };
        if !has(&base) || !has(&head) {
            // The head may live on a fork; the pull ref reaches it anyway.
            let pull_ref = format!("refs/pull/{}/head", pr.number);
            git_in(&root, &["fetch", "--quiet", "--no-tags", "origin", &base, &pull_ref])
                .map_err(|error| format!("could not fetch the pull request into {root}: {error}"))?;
        }
        Ok((root, base, head))
    }
}

/// `git diff base...head` in the clone — the merge-base comparison GitHub
/// shows — limited to `paths` when any are given.
fn local_diff(clone: &str, base: &str, head: &str, paths: &[&str]) -> Result<String, String> {
    let range = format!("{base}...{head}");
    let literal: Vec<String> = paths.iter().map(|path| format!(":(literal){path}")).collect();
    let mut args = vec![
        "-c",
        "core.quotePath=false",
        "diff",
        "-M",
        "--no-color",
        "--no-ext-diff",
        &range,
    ];
    if !literal.is_empty() {
        args.push("--");
        args.extend(literal.iter().map(String::as_str));
    }
    git_in(clone, &args)
}

/// True when the diff endpoint refused the request for its size — the
/// case the file list exists for.
fn is_too_large(error: &str) -> bool {
    error.contains("exceeded the maximum")
        || error.contains("too_large")
        || error.contains("HTTP 406")
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
        match self.api(pr, &[&path, "-H", "Accept: application/vnd.github.v3.diff"], None) {
            Err(error) if is_too_large(&error) => self.diff_from_files(pr),
            other => other,
        }
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
                resolved: false,
                placement: Default::default(),
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
        if !threads.is_empty() {
            // Resolution is a display hint; a host that will not say
            // (an old enterprise server, a token without the scope)
            // leaves every thread shown in full rather than failing the
            // review.
            if let Ok(resolved) = self.resolved_roots(pr) {
                for thread in &mut threads {
                    thread.resolved = resolved.contains(&thread.id);
                }
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

    fn commits(&self, pr: &PullRequestRef) -> Result<CommitList, String> {
        let path = Self::repo_path(pr, &format!("pulls/{}/commits?per_page=100", pr.number));
        let text = self.api(pr, &[&path, "--paginate"], None)?;
        let commits: Vec<CommitInfo> = parse_paginated(&text)?
            .iter()
            .map(|item| CommitInfo {
                sha: str_at(item, "sha"),
                parents: item["parents"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .map(|parent| str_at(parent, "sha"))
                    .collect(),
                title: first_line(item["commit"]["message"].as_str().unwrap_or_default()),
                // The account when GitHub could match one, else the name
                // the commit itself carries.
                author: item["author"]["login"]
                    .as_str()
                    .or_else(|| item["commit"]["author"]["name"].as_str())
                    .unwrap_or_default()
                    .to_string(),
                date: item["commit"]["author"]["date"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            })
            .collect();

        // GitHub stops listing at 250 commits and does not say so in the
        // listing itself; the request's own count tells.
        let mut notice = String::new();
        if commits.len() >= GITHUB_COMMIT_LIST_LIMIT {
            let metadata = Self::repo_path(pr, &format!("pulls/{}", pr.number));
            let total = self
                .api(pr, &[&metadata], None)
                .ok()
                .and_then(|text| parse_json(&text).ok())
                .and_then(|value| value["commits"].as_u64());
            notice = match total {
                Some(total) if total as usize > commits.len() => format!(
                    "GitHub lists only the first {} of this pull request's {total} commits; \
                     the later ones can be reviewed only in All changes",
                    commits.len()
                ),
                Some(_) => String::new(),
                None => format!(
                    "GitHub lists at most {GITHUB_COMMIT_LIST_LIMIT} commits of a pull \
                     request; this list may be incomplete"
                ),
            };
        }
        Ok(CommitList {
            commits: oldest_first(commits),
            notice,
        })
    }

    fn commit_diff(&self, pr: &PullRequestRef, sha: &str) -> Result<String, String> {
        let path = Self::repo_path(pr, &format!("commits/{sha}"));
        self.api(pr, &[&path, "-H", "Accept: application/vnd.github.v3.diff"], None)
    }

    fn create_review(
        &self,
        pr: &PullRequestRef,
        event: &str,
        summary: &str,
        comments: &[ReviewComment],
        commit: Option<&CommitInfo>,
    ) -> Result<(), String> {
        let path = Self::repo_path(pr, &format!("pulls/{}/reviews", pr.number));
        let mut body = json!({
            "event": event,
            "body": summary,
            "comments": comments,
        });
        // Positions are then read in that commit's diff, as GitHub's own
        // single-commit view posts them; without it, the head's.
        if let Some(commit) = commit {
            body["commit_id"] = json!(commit.sha);
        }
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
        let forge = GhForge::with_runner(FakeRunner::new(vec![
            Ok(r#"[
            {"id": 1, "path": "a.rs", "side": "RIGHT", "line": 5,
             "body": "root", "user": {"login": "x"}, "created_at": "t1", "html_url": ""},
            {"id": 2, "in_reply_to_id": 1, "body": "reply",
             "user": {"login": "y"}, "created_at": "t2", "html_url": ""},
            {"id": 3, "path": "b.rs", "side": "LEFT", "line": null, "original_line": 9,
             "body": "old", "user": {"login": "z"}, "created_at": "t3", "html_url": ""}
        ]"#
        .into()),
            Err("graphql unavailable".into()),
        ]));
        let threads = forge.threads(&reference()).unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].comments.len(), 2);
        assert_eq!(threads[0].comments[1].body, "reply");
        assert!(threads[1].outdated);
        assert_eq!(threads[1].original_line, Some(9));
        // The GraphQL failure cost the resolution, not the threads.
        assert!(threads.iter().all(|t| !t.resolved));
    }

    fn one_thread(id: i64) -> String {
        format!(
            r#"[{{"id": {id}, "path": "a.rs", "side": "RIGHT", "line": 5, "body": "b",
                 "user": {{"login": "x"}}, "created_at": "t", "html_url": ""}}]"#
        )
    }

    fn page(resolved: &[(i64, bool)], next: Option<&str>) -> String {
        let nodes: Vec<Value> = resolved
            .iter()
            .map(|(id, is)| {
                json!({"isResolved": is, "comments": {"nodes": [{"databaseId": id}]}})
            })
            .collect();
        json!({"data": {"repository": {"pullRequest": {"reviewThreads": {
            "nodes": nodes,
            "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next},
        }}}}})
        .to_string()
    }

    #[test]
    fn resolution_comes_from_graphql_across_pages() {
        let rest = r#"[
            {"id": 1, "path": "a.rs", "side": "RIGHT", "line": 5, "body": "a",
             "user": {"login": "x"}, "created_at": "t", "html_url": ""},
            {"id": 2, "path": "a.rs", "side": "RIGHT", "line": 6, "body": "b",
             "user": {"login": "x"}, "created_at": "t", "html_url": ""},
            {"id": 3, "path": "a.rs", "side": "RIGHT", "line": 7, "body": "c",
             "user": {"login": "x"}, "created_at": "t", "html_url": ""}
        ]"#;
        let forge = GhForge::with_runner(FakeRunner::new(vec![
            Ok(rest.into()),
            Ok(page(&[(1, true), (2, false)], Some("CUR"))),
            Ok(page(&[(3, true)], None)),
        ]));
        let threads = forge.threads(&reference()).unwrap();
        let resolved: Vec<bool> = threads.iter().map(|t| t.resolved).collect();
        assert_eq!(resolved, vec![true, false, true]);

        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        let first = &calls[1].0;
        assert_eq!(first[..2], ["api", "graphql"]);
        assert!(first.contains(&"owner=o".to_string()));
        assert!(first.contains(&"name=r".to_string()));
        assert!(first.contains(&"number=7".to_string()));
        assert!(!first.iter().any(|a| a.starts_with("cursor=")));
        assert!(calls[2].0.contains(&"cursor=CUR".to_string()));
    }

    #[test]
    fn graphql_errors_and_enterprise_hosts() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![
            Ok(one_thread(1)),
            Ok(r#"{"errors": [{"message": "no scope"}]}"#.into()),
        ]));
        let mut pr = reference();
        pr.host = "github.corp.example".into();
        let threads = forge.threads(&pr).unwrap();
        assert!(!threads[0].resolved);
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(calls[1].0[..4], ["api", "--hostname", "github.corp.example", "graphql"]);
    }

    #[test]
    fn no_threads_asks_no_graphql() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![Ok("[]".into())]));
        assert!(forge.threads(&reference()).unwrap().is_empty());
        assert_eq!(forge.runner.calls.lock().unwrap().len(), 1);
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
            .create_review(&reference(), "APPROVE", "lgtm", &comments, None)
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
        assert!(body.get("commit_id").is_none());
    }

    #[test]
    fn a_commit_review_pins_the_commit() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![Ok("{}".into())]));
        let commit = CommitInfo {
            sha: "c0ffee".into(),
            parents: vec!["beef".into()],
            ..Default::default()
        };
        forge
            .create_review(&reference(), "COMMENT", "", &[], Some(&commit))
            .unwrap();
        let calls = forge.runner.calls.lock().unwrap();
        let body: Value = serde_json::from_str(calls[0].1.as_ref().unwrap()).unwrap();
        assert_eq!(body["commit_id"], "c0ffee");
    }

    #[test]
    fn commits_parse_and_come_oldest_first() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![Ok(r#"[
            {"sha": "a1", "parents": [{"sha": "base"}],
             "commit": {"message": "First\n\nbody", "author": {"name": "Al", "date": "d1"}},
             "author": {"login": "al"}},
            {"sha": "b2", "parents": [{"sha": "a1"}],
             "commit": {"message": "Second", "author": {"name": "Bo", "date": "d2"}},
             "author": null}
        ]"#
        .into())]));
        let list = forge.commits(&reference()).unwrap();
        assert_eq!(list.commits.len(), 2);
        assert_eq!(list.commits[0].sha, "a1");
        assert_eq!(list.commits[0].title, "First");
        assert_eq!(list.commits[0].author, "al");
        assert_eq!(list.commits[1].author, "Bo");
        assert_eq!(list.commits[1].parents, vec!["a1"]);
        assert!(list.notice.is_empty());
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(
            calls[0].0,
            vec!["api", "repos/o/r/pulls/7/commits?per_page=100", "--paginate"]
        );
    }

    #[test]
    fn a_full_commit_list_says_it_was_cut_short() {
        let page: Vec<Value> = (0..GITHUB_COMMIT_LIST_LIMIT)
            .map(|n| json!({"sha": format!("s{n}"), "parents": [], "commit": {"message": "m"}}))
            .collect();
        let forge = GhForge::with_runner(FakeRunner::new(vec![
            Ok(Value::Array(page.clone()).to_string()),
            Ok(r#"{"commits": 300}"#.into()),
        ]));
        let list = forge.commits(&reference()).unwrap();
        assert_eq!(list.commits.len(), GITHUB_COMMIT_LIST_LIMIT);
        assert!(list.notice.contains("first 250 of this pull request's 300"), "{}", list.notice);

        // Exactly 250 commits is complete, and says nothing.
        let forge = GhForge::with_runner(FakeRunner::new(vec![
            Ok(Value::Array(page.clone()).to_string()),
            Ok(r#"{"commits": 250}"#.into()),
        ]));
        assert!(forge.commits(&reference()).unwrap().notice.is_empty());

        // An unanswered count still warns rather than truncating silently.
        let forge = GhForge::with_runner(FakeRunner::new(vec![
            Ok(Value::Array(page).to_string()),
            Err("offline".into()),
        ]));
        assert!(forge.commits(&reference()).unwrap().notice.contains("may be incomplete"));
    }

    #[test]
    fn commit_diff_asks_for_the_diff_media_type() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![Ok("diff --git a/x b/x\n".into())]));
        forge.commit_diff(&reference(), "c0ffee").unwrap();
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(
            calls[0].0,
            vec!["api", "repos/o/r/commits/c0ffee", "-H", "Accept: application/vnd.github.v3.diff"]
        );
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

    const TOO_LARGE: &str = "gh api repos/o/r/pulls/7: Sorry, the diff exceeded the maximum \
        number of files (300). Consider using 'List pull requests files' API or locally \
        cloning the repository instead. (HTTP 406)";

    #[test]
    fn a_diff_too_large_for_the_endpoint_is_rebuilt_from_the_file_list() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![
            Err(TOO_LARGE.into()),
            Ok(r#"[
                {"filename": "a.rs", "status": "modified",
                 "patch": "@@ -1 +1 @@\n-x\n+y"},
                {"filename": "new.rs", "previous_filename": "old.rs", "status": "renamed",
                 "patch": "@@ -1 +1 @@\n-a\n+b"}
            ][
                {"filename": "born.rs", "status": "added", "patch": "@@ -0,0 +1 @@\n+hi"},
                {"filename": "big.bin", "status": "modified"}
            ]"#
            .into()),
        ]));
        let patch = forge.diff(&reference()).unwrap();
        let files = prchum_core::diff::parse(&patch, 4).unwrap();
        assert_eq!(files.len(), 4);
        assert_eq!(files[1].status, prchum_core::diff::FileStatus::Renamed);
        assert_eq!(files[1].old_path, "old.rs");
        assert_eq!(files[2].status, prchum_core::diff::FileStatus::Added);
        // No patch and no clone: listed, with nothing to show.
        assert_eq!(files[3].new_path, "big.bin");
        assert!(files[3].hunks.is_empty());
        let calls = forge.runner.calls.lock().unwrap();
        assert_eq!(calls[1].0, vec!["api", "repos/o/r/pulls/7/files?per_page=100", "--paginate"]);
    }

    #[test]
    fn other_diff_failures_are_not_retried() {
        let forge = GhForge::with_runner(FakeRunner::new(vec![Err("HTTP 404".into())]));
        assert_eq!(forge.diff(&reference()).unwrap_err(), "HTTP 404");
        assert_eq!(forge.runner.calls.lock().unwrap().len(), 1);
    }

    /// A repository with a base commit and a head commit that changes a
    /// text file and adds a binary one: `(dir, base, head)`.
    fn scratch_clone() -> (std::path::PathBuf, String, String) {
        let dir = std::env::temp_dir().join(format!(
            "prchum-gh-{}-{}",
            std::process::id(),
            prchum_core::util::new_local_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let repo = dir.to_string_lossy().to_string();
        let run = |args: &[&str]| git_in(&repo, args).unwrap();
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.name", "Test"]);
        run(&["config", "user.email", "test@example.com"]);
        std::fs::write(dir.join("huge.txt"), "one\ntwo\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "base"]);
        let base = run(&["rev-parse", "HEAD"]).trim().to_string();
        std::fs::write(dir.join("huge.txt"), "one\nchanged\n").unwrap();
        std::fs::write(dir.join("logo.png"), [0u8, 159, 146, 150]).unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "head"]);
        let head = run(&["rev-parse", "HEAD"]).trim().to_string();
        (dir, base, head)
    }

    #[test]
    fn a_local_clone_fills_the_patches_the_list_leaves_out() {
        let (dir, base, head) = scratch_clone();
        let forge = GhForge::with_runner(FakeRunner::new(vec![
            Err(TOO_LARGE.into()),
            Ok(r#"[
                {"filename": "huge.txt", "status": "modified"},
                {"filename": "logo.png", "status": "added"}
            ]"#
            .into()),
            Ok(format!(r#"{{"base": {{"sha": "{base}"}}, "head": {{"sha": "{head}"}}}}"#)),
        ]))
        .with_clone(&dir.to_string_lossy());
        let patch = forge.diff(&reference()).unwrap();
        let files = prchum_core::diff::parse(&patch, 4).unwrap();
        assert_eq!(files.len(), 2);
        assert!(patch.contains("+changed"));
        assert!(files[1].is_binary);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_truncated_file_list_needs_the_clone() {
        let entry = r#"{"filename": "f", "status": "modified", "patch": "@@ -1 +1 @@\n-a\n+b"}"#;
        let list = format!("[{}]", vec![entry; FILE_LIST_CAP].join(","));

        let forge = GhForge::with_runner(FakeRunner::new(vec![
            Err(TOO_LARGE.into()),
            Ok(list.clone()),
        ]));
        assert!(forge.diff(&reference()).unwrap_err().contains("local clone"));

        let (dir, base, head) = scratch_clone();
        let forge = GhForge::with_runner(FakeRunner::new(vec![
            Err(TOO_LARGE.into()),
            Ok(list),
            Ok(format!(r#"{{"base": {{"sha": "{base}"}}, "head": {{"sha": "{head}"}}}}"#)),
        ]))
        .with_clone(&dir.to_string_lossy());
        let files = prchum_core::diff::parse(&forge.diff(&reference()).unwrap(), 4).unwrap();
        assert_eq!(files.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paginated_concatenated_arrays() {
        let items = parse_paginated(r#"[{"id": 1}][{"id": 2}]"#).unwrap();
        assert_eq!(items.len(), 2);
    }
}
