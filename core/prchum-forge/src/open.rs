//! Opening a pull request as a review session.
//!
//! Both shells need this and neither should own it: the reference
//! parsing, the origin inference, the fetch of metadata, diff, threads
//! and conversation, and the content provider the context view reads
//! through are all one decision, and a second copy would drift.
//!
//! A request can also be opened one commit at a time. That session is a
//! separate review with a source key of its own, so its drafts are kept
//! apart from the whole request's and come back when the same commit is
//! picked again.

use prchum_core::review::DraftReview;
use prchum_core::session::Session;
use prchum_core::Config;

use crate::refs::{parse_ref, resolve_from_origin, PullRequestRef};
use crate::forgejo::ForgejoForge;
use crate::ghcli::{GhForge, ProcessRunner};
use crate::glabcli::GlabForge;
use crate::{kind_for_host, submit, CommitInfo, Forge, ForgeKind};

/// What submission needs to reach the same forge the session came from.
#[derive(Clone)]
pub struct PrContext {
    pub reference: PullRequestRef,
    pub kind: ForgeKind,
    /// Forgejo transport template (empty = the built-in default).
    pub forgejo_template: String,
    /// Set when the session reviews one commit of the request; its line
    /// comments are positioned in that commit's diff and must be
    /// submitted pinned to it.
    pub commit: Option<CommitInfo>,
}

impl PrContext {
    /// The submission plan for `draft`, pinned to the session's commit
    /// when it has one. Submission goes through here so that a commit
    /// review cannot be posted against the whole request's diff.
    pub fn plan(&self, draft: &DraftReview) -> submit::SubmissionPlan {
        let mut plan = submit::plan(draft);
        plan.commit = self.commit.clone();
        plan
    }

    pub fn forge(&self) -> Box<dyn Forge> {
        match self.kind {
            ForgeKind::Forgejo => Box::new(ForgejoForge::with_runner(
                ProcessRunner,
                &self.forgejo_template,
            )),
            ForgeKind::GitLab => Box::new(GlabForge::new()),
            ForgeKind::GitHub => Box::new(GhForge::new()),
        }
    }

    /// The whole request's source key, whichever commit this context is
    /// on. History entries and worktrees are the request's, not a
    /// commit's.
    pub fn request_key(&self) -> String {
        let prefix = match self.kind {
            ForgeKind::Forgejo => "fj",
            ForgeKind::GitLab => "gl",
            ForgeKind::GitHub => "gh",
        };
        let reference = &self.reference;
        format!(
            "{prefix}-{}-{}-{}-pr{}",
            reference.host,
            reference.owner.replace('/', "-"),
            reference.repo,
            reference.number
        )
    }

    /// The source key a session of this context keeps its drafts under.
    pub fn session_key(&self) -> String {
        match &self.commit {
            Some(commit) => commit_key(&self.request_key(), &commit.sha),
            None => self.request_key(),
        }
    }

    /// The same request, whole.
    pub fn whole_request(&self) -> PrContext {
        PrContext {
            commit: None,
            ..self.clone()
        }
    }
}

/// The source key for one commit of the request keyed `request_key`. The
/// full sha: a short one could come to name two commits.
pub fn commit_key(request_key: &str, sha: &str) -> String {
    format!("{request_key}-commit-{sha}")
}

/// The title a whole-request session carries.
pub fn request_title(reference: &PullRequestRef, pr_title: &str) -> String {
    format!(
        "{}/{}#{}: {pr_title}",
        reference.owner, reference.repo, reference.number
    )
}

/// Opens `reference` as a session, with the context the shell needs to
/// submit back to the same forge it came from.
pub fn open_session(
    reference: &str,
    repo_hint: &str,
    config: &Config,
) -> Result<(Session, PrContext), String> {
    let mut pr_ref =
        parse_ref(reference).ok_or_else(|| format!("not a pull-request reference: {reference}"))?;
    // Origin inference is only for bare numbers; an explicit owner/repo#N
    // without a host defaults to github.com rather than requiring the
    // current directory to be a checkout of anything.
    if pr_ref.owner.is_empty() || pr_ref.repo.is_empty() {
        let hint = if repo_hint.is_empty() { "." } else { repo_hint };
        resolve_from_origin(&mut pr_ref, hint).map_err(|error| {
            format!(
                "{error} — a bare number needs to run from inside the repository's \
                 checkout; otherwise use owner/repo#N or the full URL"
            )
        })?;
    }
    if pr_ref.host.is_empty() {
        pr_ref.host = "github.com".to_string();
    }

    let kind = kind_for_host(&pr_ref.host, config.forge_for_host(&pr_ref.host));
    let context = PrContext {
        reference: pr_ref,
        kind,
        forgejo_template: config.forgejo_api_command().to_string(),
        commit: None,
    };
    open_with_context(context)
}

/// Opens the commit `sha` of the request `context` belongs to, or — for
/// an empty `sha` — the whole request again. `context` may itself be on
/// a commit; only its request matters.
///
/// The commit is looked up in the request's own list, so a sha that is
/// not part of the request is refused rather than reviewed out of
/// context, and its parent comes from the forge, not the caller.
pub fn open_commit_session(context: &PrContext, sha: &str) -> Result<(Session, PrContext), String> {
    let mut context = context.whole_request();
    if sha.is_empty() {
        return open_with_context(context);
    }
    let list = context.forge().commits(&context.reference)?;
    let commit = list
        .commits
        .into_iter()
        .find(|commit| commit.sha == sha)
        .ok_or_else(|| {
            format!(
                "commit {} is not among the pull request's commits",
                sha.get(..7).unwrap_or(sha)
            )
        })?;
    context.commit = Some(commit);
    open_with_context(context)
}

fn open_with_context(context: PrContext) -> Result<(Session, PrContext), String> {
    let forge = context.forge();
    let mut session = build_session(forge.as_ref(), &context)?;

    // The context view fetches new-side content at the reviewed revision:
    // the head, or the commit.
    let provider_context = context.clone();
    let head = session.head_oid().to_string();
    session.set_content_provider(Box::new(move |path| {
        provider_context
            .forge()
            .file_content(&provider_context.reference, path, &head)
    }));
    Ok((session, context))
}

/// Everything but the content provider, which needs a forge it can own;
/// kept apart so it can be tested against a scripted forge.
fn build_session(forge: &dyn Forge, context: &PrContext) -> Result<Session, String> {
    let pr_ref = &context.reference;
    let metadata = forge.pull_request(pr_ref)?;
    let (diff, threads) = match &context.commit {
        Some(commit) => {
            // Host threads are positioned in the whole request's diff and
            // would land on the wrong rows of a commit's. They are left
            // out rather than guessed at; the conversation still shows.
            (forge.commit_diff(pr_ref, &commit.sha)?, Vec::new())
        }
        None => (forge.diff(pr_ref)?, forge.threads(pr_ref)?),
    };
    // Conversation comments are display data; failure to fetch them must
    // not block the review.
    let generals = forge.general_comments(pr_ref).unwrap_or_default();

    let title = match &context.commit {
        Some(commit) => format!(
            "{}/{}#{} @ {}: {}",
            pr_ref.owner,
            pr_ref.repo,
            pr_ref.number,
            commit.short_sha(),
            commit.title
        ),
        None => request_title(pr_ref, &metadata.title),
    };
    let what = if context.commit.is_some() {
        "commit's"
    } else {
        "pull request's"
    };
    let mut session = Session::from_patch_keyed(&title, &diff, context.session_key())
        .map_err(|error| format!("could not parse the {what} diff: {error}"))?;
    // A commit is its own head: permalinks point into it, and it never
    // moves, so its drafts never need relocating.
    let head = match &context.commit {
        Some(commit) => commit.sha.as_str(),
        None => metadata.head_oid.as_str(),
    };
    session.set_head_oid(head);
    session.set_pr_json(serde_json::to_string(&metadata).unwrap_or_default());
    session.set_threads_json(serde_json::to_string(&threads).unwrap_or_default());
    session.set_general_json(serde_json::to_string(&generals).unwrap_or_default());
    let reopen = if metadata.url.is_empty() {
        pr_ref.web_url(context.kind)
    } else {
        metadata.url.clone()
    };
    session.set_reopen_hint(&reopen);
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Comment, CommitList, PullRequest, ReviewComment, ThreadInfo};

    fn context(commit: Option<CommitInfo>) -> PrContext {
        PrContext {
            reference: PullRequestRef {
                host: "github.com".into(),
                owner: "o".into(),
                repo: "r".into(),
                number: 7,
            },
            kind: ForgeKind::GitHub,
            forgejo_template: String::new(),
            commit,
        }
    }

    fn commit() -> CommitInfo {
        CommitInfo {
            sha: "c0ffee1234567890".into(),
            parents: vec!["beef".into()],
            title: "Make it so".into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_plan_carries_the_sessions_commit() {
        let mut draft = DraftReview::default();
        draft.add_general("hello".into());
        assert!(context(None).plan(&draft).commit.is_none());
        let plan = context(Some(commit())).plan(&draft);
        assert_eq!(plan.commit, Some(commit()));
        assert_eq!(plan.generals.len(), 1);
    }

    #[test]
    fn a_commit_has_its_own_key_beside_the_request() {
        let whole = context(None);
        let one = context(Some(commit()));
        assert_eq!(whole.session_key(), "gh-github.com-o-r-pr7");
        assert_eq!(one.request_key(), whole.session_key());
        assert_eq!(one.session_key(), "gh-github.com-o-r-pr7-commit-c0ffee1234567890");
        assert_eq!(one.whole_request().session_key(), whole.session_key());
    }

    /// Answers from fixed data; records which diff was asked for.
    struct ScriptedForge {
        asked: std::sync::Mutex<Vec<String>>,
    }

    impl Forge for ScriptedForge {
        fn pull_request(&self, _: &PullRequestRef) -> Result<PullRequest, String> {
            Ok(PullRequest {
                number: 7,
                state: "open".into(),
                merged: false,
                title: "The request".into(),
                body: String::new(),
                author: "al".into(),
                url: "https://github.com/o/r/pull/7".into(),
                head_oid: "head".into(),
                base_ref: "main".into(),
                head_ref: "feat".into(),
            })
        }
        fn diff(&self, _: &PullRequestRef) -> Result<String, String> {
            self.asked.lock().unwrap().push("request".into());
            Ok("--- a/x\n+++ b/x\n@@ -1 +1,2 @@\n a\n+b\n".into())
        }
        fn commits(&self, _: &PullRequestRef) -> Result<CommitList, String> {
            Ok(CommitList {
                commits: vec![commit()],
                notice: String::new(),
            })
        }
        fn commit_diff(&self, _: &PullRequestRef, sha: &str) -> Result<String, String> {
            self.asked.lock().unwrap().push(sha.to_string());
            Ok("--- a/y\n+++ b/y\n@@ -1 +1 @@\n-a\n+c\n".into())
        }
        fn threads(&self, _: &PullRequestRef) -> Result<Vec<ThreadInfo>, String> {
            Ok(vec![ThreadInfo {
                id: 1,
                path: "x".into(),
                side: "RIGHT".into(),
                line: Some(2),
                ..Default::default()
            }])
        }
        fn general_comments(&self, _: &PullRequestRef) -> Result<Vec<Comment>, String> {
            Ok(vec![Comment {
                body: "overall".into(),
                ..Default::default()
            }])
        }
        fn create_review(
            &self,
            _: &PullRequestRef,
            _: &str,
            _: &str,
            _: &[ReviewComment],
            _: Option<&CommitInfo>,
        ) -> Result<(), String> {
            unreachable!()
        }
        fn reply(&self, _: &PullRequestRef, _: i64, _: &str) -> Result<(), String> {
            unreachable!()
        }
        fn file_content(&self, _: &PullRequestRef, _: &str, _: &str) -> Result<String, String> {
            unreachable!()
        }
        fn add_general_comment(&self, _: &PullRequestRef, _: &str) -> Result<(), String> {
            unreachable!()
        }
    }

    #[test]
    fn a_commit_session_shows_the_commit_without_the_requests_threads() {
        let forge = ScriptedForge {
            asked: Default::default(),
        };
        let whole = build_session(&forge, &context(None)).unwrap();
        assert_eq!(whole.title(), "o/r#7: The request");
        assert_eq!(whole.head_oid(), "head");
        assert!(whole.threads_json().contains("\"id\":1"));

        let one = build_session(&forge, &context(Some(commit()))).unwrap();
        assert_eq!(one.title(), "o/r#7 @ c0ffee1: Make it so");
        assert_eq!(one.source_key(), "gh-github.com-o-r-pr7-commit-c0ffee1234567890");
        assert_eq!(one.head_oid(), "c0ffee1234567890");
        assert_eq!(one.files()[0].new_path, "y");
        assert_eq!(one.threads_json(), "[]");
        // The same request behind it: links, metadata, conversation.
        assert!(one.pr_json().contains("The request"));
        assert!(one.general_json().contains("overall"));
        assert_eq!(one.reopen_hint(), "https://github.com/o/r/pull/7");

        assert_eq!(
            *forge.asked.lock().unwrap(),
            vec!["request".to_string(), "c0ffee1234567890".to_string()]
        );
    }
}
