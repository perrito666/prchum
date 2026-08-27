//! Opening a pull request as a review session.
//!
//! Both shells need this and neither should own it: the reference
//! parsing, the origin inference, the fetch of metadata, diff, threads
//! and conversation, and the content provider the context view reads
//! through are all one decision, and a second copy would drift.

use prchum_core::session::Session;
use prchum_core::Config;

use crate::refs::{parse_ref, resolve_from_origin, PullRequestRef};
use crate::forgejo::ForgejoForge;
use crate::ghcli::{GhForge, ProcessRunner};
use crate::glabcli::GlabForge;
use crate::{kind_for_host, Forge, ForgeKind};

/// What submission needs to reach the same forge the session came from.
#[derive(Clone)]
pub struct PrContext {
    pub reference: PullRequestRef,
    pub kind: ForgeKind,
    /// Forgejo transport template (empty = the built-in default).
    pub forgejo_template: String,
}

impl PrContext {
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
        reference: pr_ref.clone(),
        kind,
        forgejo_template: config.forgejo_api_command().to_string(),
    };

    let forge = context.forge();
    let metadata = forge.pull_request(&pr_ref)?;
    let diff = forge.diff(&pr_ref)?;
    let threads = forge.threads(&pr_ref)?;
    // Conversation comments are display data; failure to fetch them must
    // not block the review.
    let generals = forge.general_comments(&pr_ref).unwrap_or_default();

    let prefix = match kind {
        ForgeKind::Forgejo => "fj",
        ForgeKind::GitLab => "gl",
        ForgeKind::GitHub => "gh",
    };
    let key = format!(
        "{prefix}-{}-{}-{}-pr{}",
        pr_ref.host,
        pr_ref.owner.replace('/', "-"),
        pr_ref.repo,
        pr_ref.number
    );
    let title = format!("{}/{}#{}: {}", pr_ref.owner, pr_ref.repo, pr_ref.number, metadata.title);
    let mut session = Session::from_patch_keyed(&title, &diff, key)
        .map_err(|error| format!("could not parse the pull request's diff: {error}"))?;
    session.set_head_oid(&metadata.head_oid);
    session.set_pr_json(serde_json::to_string(&metadata).unwrap_or_default());
    session.set_threads_json(serde_json::to_string(&threads).unwrap_or_default());
    session.set_general_json(serde_json::to_string(&generals).unwrap_or_default());
    let reopen = if metadata.url.is_empty() {
        pr_ref.web_url(kind)
    } else {
        metadata.url.clone()
    };
    session.set_reopen_hint(&reopen);

    // The context view fetches new-side content at the head revision.
    let provider_ref = pr_ref.clone();
    let provider_kind = kind;
    let provider_template = config.forgejo_api_command().to_string();
    let head = metadata.head_oid.clone();
    session.set_content_provider(Box::new(move |path| {
        let forge: Box<dyn Forge> = match provider_kind {
            ForgeKind::Forgejo => Box::new(ForgejoForge::with_runner(
                ProcessRunner,
                &provider_template,
            )),
            ForgeKind::GitLab => Box::new(GlabForge::new()),
            ForgeKind::GitHub => Box::new(GhForge::new()),
        };
        forge.file_content(&provider_ref, path, &head)
    }));
    Ok((session, context))
}
