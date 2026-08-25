//! C ABI over `prchum-core`.
//!
//! This crate is the only place where the core meets a foreign language.
//! Its rules (inherited from textchum, unchanged):
//!
//! * Every exported type is opaque; callers hold pointers and pass them back.
//! * Strings cross the boundary as UTF-8 `(pointer, length)` pairs, except
//!   for returned strings which are nul-terminated and must be released with
//!   [`pc_string_free`].
//! * Fallible calls return `bool` or null; failure means the operation
//!   validated its inputs, rejected them, and changed nothing.
//! * Panics never unwind across the boundary: every entry point is wrapped
//!   in `catch_unwind` and reports failure instead.
//! * Calls into this API must come from a single thread. Events flow the
//!   other way on one core-owned dispatch thread (see [`pc_app_new`]).

use std::ffi::{c_char, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use prchum_core::diff::Side;
use prchum_core::review::ReviewEvent;
use prchum_core::source::GitSpec;
use prchum_core::{App, Config, Event, Session};
use prchum_forge::forgejo::ForgejoForge;
use prchum_forge::ghcli::{GhForge, ProcessRunner};
use prchum_forge::refs::{parse_ref, resolve_from_origin};
use prchum_forge::{kind_for_host, submit, Forge, ForgeKind, PullRequestRef};

/// Event kind: reply to `pc_app_ping`.
pub const PC_EVENT_PONG: u32 = 1;

/// An event delivered from the core to the shell.
///
/// The struct and every string it points to are only valid for the duration
/// of the callback invocation; copy anything you need out of it. Strings not
/// applicable to the event kind are null. Shells must tolerate unknown
/// kinds — new kinds are forward compatibility, not an error.
#[repr(C)]
pub struct PcEvent {
    /// One of the `PC_EVENT_*` constants.
    pub kind: u32,
    /// Sequence number (pong events).
    pub seq: u64,
    /// JSON payload, when the kind carries one.
    pub payload: *const c_char,
}

impl PcEvent {
    fn new(kind: u32) -> Self {
        Self {
            kind,
            seq: 0,
            payload: std::ptr::null(),
        }
    }
}

/// Shell-provided event sink. Invoked on the core's single dispatch thread;
/// implementations must hop to their UI thread themselves.
pub type PcEventCallback = Option<extern "C" fn(event: *const PcEvent, userdata: *mut c_void)>;

/// Root handle for a core instance. Create with [`pc_app_new`], release with
/// [`pc_app_free`].
pub struct PcApp {
    inner: App,
}

/// A review session over one diff source. Create with one of the
/// `pc_session_new_*` constructors, release with [`pc_session_free`].
pub struct PcSession {
    inner: Session,
    /// Set for PR-mode sessions; submission targets it.
    pr: Option<PrContext>,
}

/// What submission needs to reach the same forge the session came from.
struct PrContext {
    reference: PullRequestRef,
    kind: ForgeKind,
    /// Forgejo transport template (empty = the built-in default).
    forgejo_template: String,
}

impl PrContext {
    fn forge(&self) -> Box<dyn Forge> {
        match self.kind {
            ForgeKind::Forgejo => Box::new(ForgejoForge::with_runner(
                ProcessRunner,
                &self.forgejo_template,
            )),
            _ => Box::new(GhForge::new()),
        }
    }
}

/// The user configuration. Create with [`pc_config_new`], release with
/// [`pc_config_free`].
pub struct PcConfig {
    inner: Config,
}

/// Wrapper making the raw userdata pointer sendable to the dispatch thread.
/// The shell guarantees the pointee outlives the app handle.
struct UserData(*mut c_void);
unsafe impl Send for UserData {}
unsafe impl Sync for UserData {}

impl UserData {
    // Accessed via a method so closures capture the Send wrapper as a whole
    // rather than the raw pointer field (which is not Send).
    fn get(&self) -> *mut c_void {
        self.0
    }
}

/// Returns the core version as a static nul-terminated UTF-8 string.
/// The returned pointer is owned by the core; do not free it.
#[no_mangle]
pub extern "C" fn pc_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Creates a core instance whose events are delivered to `callback` with
/// `userdata`, one at a time, on a single core-owned thread.
#[no_mangle]
pub extern "C" fn pc_app_new(callback: PcEventCallback, userdata: *mut c_void) -> *mut PcApp {
    let userdata = UserData(userdata);
    let inner = App::new(move |event| {
        let Some(callback) = callback else { return };
        match event {
            Event::Pong { seq } => {
                let mut out = PcEvent::new(PC_EVENT_PONG);
                out.seq = seq;
                callback(&out, userdata.get());
            }
        }
    });
    Box::into_raw(Box::new(PcApp { inner }))
}

/// Releases an app handle. Joins the dispatch thread: after this returns,
/// the callback is guaranteed not to run again.
#[no_mangle]
pub unsafe extern "C" fn pc_app_free(app: *mut PcApp) {
    if !app.is_null() {
        drop(unsafe { Box::from_raw(app) });
    }
}

/// Asks the core to answer with a `PC_EVENT_PONG` carrying `seq`, from a
/// worker thread — the smallest possible proof of the async round trip.
#[no_mangle]
pub unsafe extern "C" fn pc_app_ping(app: *mut PcApp, seq: u64) {
    let Some(app) = (unsafe { app.as_ref() }) else {
        return;
    };
    let _ = catch_unwind(AssertUnwindSafe(|| app.inner.ping(seq)));
}

/// Opens a review session over a literal unified-diff text.
///
/// Returns null on failure; when `error_out` is non-null it receives a
/// human-readable message to release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_new_from_patch(
    title: *const c_char,
    title_len: usize,
    patch: *const c_char,
    patch_len: usize,
    error_out: *mut *mut c_char,
) -> *mut PcSession {
    let Some(title) = (unsafe { str_from_raw(title, title_len) }) else {
        unsafe { write_error(error_out, "title is not valid UTF-8") };
        return std::ptr::null_mut();
    };
    let Some(patch) = (unsafe { str_from_raw(patch, patch_len) }) else {
        unsafe { write_error(error_out, "patch is not valid UTF-8") };
        return std::ptr::null_mut();
    };
    let result = catch_unwind(AssertUnwindSafe(|| Session::from_patch(title, patch)));
    wrap_session(result, error_out)
}

/// Boxes a session-creation result, reporting failures through `error_out`.
fn wrap_session(
    result: std::thread::Result<Result<Session, prchum_core::diff::ParseError>>,
    error_out: *mut *mut c_char,
) -> *mut PcSession {
    match result {
        Ok(Ok(inner)) => Box::into_raw(Box::new(PcSession { inner, pr: None })),
        Ok(Err(error)) => {
            unsafe { write_error(error_out, &error.to_string()) };
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe { write_error(error_out, "internal error while opening the session") };
            std::ptr::null_mut()
        }
    }
}

/// Opens a session over a file on disk: a unified diff, or a
/// review-exchange document (detected by content, never filename — an
/// exchange session rewrites its file on every save).
#[no_mangle]
pub unsafe extern "C" fn pc_session_new_from_file(
    path: *const c_char,
    path_len: usize,
    error_out: *mut *mut c_char,
) -> *mut PcSession {
    let Some(path) = (unsafe { str_from_raw(path, path_len) }) else {
        unsafe { write_error(error_out, "path is not valid UTF-8") };
        return std::ptr::null_mut();
    };
    let result = catch_unwind(AssertUnwindSafe(|| Session::from_patch_file(path)));
    wrap_session(result, error_out)
}

/// Git comparison kinds for [`pc_session_new_from_git`].
pub const PC_GIT_WORKTREE: u32 = 0;
/// Index vs HEAD.
pub const PC_GIT_STAGED: u32 = 1;
/// `arg1...HEAD` (merge base).
pub const PC_GIT_BASE: u32 = 2;
/// Explicit range `arg1..arg2`.
pub const PC_GIT_RANGE: u32 = 3;

/// Opens a session over a local git comparison rooted at `repo`.
#[no_mangle]
pub unsafe extern "C" fn pc_session_new_from_git(
    repo: *const c_char,
    repo_len: usize,
    kind: u32,
    arg1: *const c_char,
    arg1_len: usize,
    arg2: *const c_char,
    arg2_len: usize,
    context: u32,
    error_out: *mut *mut c_char,
) -> *mut PcSession {
    let Some(repo) = (unsafe { str_from_raw(repo, repo_len) }) else {
        unsafe { write_error(error_out, "repo path is not valid UTF-8") };
        return std::ptr::null_mut();
    };
    let arg1 = unsafe { str_from_raw(arg1, arg1_len) }.unwrap_or_default();
    let arg2 = unsafe { str_from_raw(arg2, arg2_len) }.unwrap_or_default();
    let spec = match kind {
        PC_GIT_STAGED => GitSpec::Staged,
        PC_GIT_BASE => GitSpec::Base(arg1.to_string()),
        PC_GIT_RANGE => GitSpec::Range(arg1.to_string(), arg2.to_string()),
        _ => GitSpec::WorkingTree,
    };
    let result = catch_unwind(AssertUnwindSafe(|| Session::from_git(repo, &spec, context)));
    wrap_session(result, error_out)
}

/// Opens a session over a pull request. `reference` accepts every spelling
/// (URL, `owner/repo#N`, bare number); `repo_hint` is a local checkout used
/// to infer host/owner/repo for underspecified references (may be empty).
/// `config_path` locates config.json for forge-kind overrides and the
/// Forgejo transport template (may be empty).
///
/// Fetches the host's canonical diff, metadata, and review threads through
/// the forge CLI — a blocking call; run it off the UI thread and hand the
/// session over once built.
#[no_mangle]
pub unsafe extern "C" fn pc_session_new_from_pr(
    reference: *const c_char,
    reference_len: usize,
    repo_hint: *const c_char,
    repo_hint_len: usize,
    config_path: *const c_char,
    config_path_len: usize,
    error_out: *mut *mut c_char,
) -> *mut PcSession {
    let Some(reference) = (unsafe { str_from_raw(reference, reference_len) }) else {
        unsafe { write_error(error_out, "reference is not valid UTF-8") };
        return std::ptr::null_mut();
    };
    let repo_hint = unsafe { str_from_raw(repo_hint, repo_hint_len) }.unwrap_or_default();
    let config_path = unsafe { str_from_raw(config_path, config_path_len) }.unwrap_or_default();

    let built = catch_unwind(AssertUnwindSafe(|| {
        build_pr_session(reference, repo_hint, config_path)
    }));
    match built {
        Ok(Ok((inner, pr))) => Box::into_raw(Box::new(PcSession { inner, pr: Some(pr) })),
        Ok(Err(message)) => {
            unsafe { write_error(error_out, &message) };
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe { write_error(error_out, "internal error while opening the pull request") };
            std::ptr::null_mut()
        }
    }
}

fn build_pr_session(
    reference: &str,
    repo_hint: &str,
    config_path: &str,
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

    let config = if config_path.is_empty() {
        Config::default()
    } else {
        Config::load(std::path::Path::new(config_path))
    };
    let kind = kind_for_host(&pr_ref.host, config.forge_for_host(&pr_ref.host));
    if kind == ForgeKind::GitLab {
        return Err("GitLab merge requests are not supported yet".to_string());
    }
    let context = PrContext {
        reference: pr_ref.clone(),
        kind,
        forgejo_template: config.forgejo_api_command().to_string(),
    };

    let forge = context.forge();
    let metadata = forge.pull_request(&pr_ref)?;
    let diff = forge.diff(&pr_ref)?;
    let threads = forge.threads(&pr_ref)?;

    let prefix = if kind == ForgeKind::Forgejo { "fj" } else { "gh" };
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
    Ok((session, context))
}

/// Releases a session handle.
#[no_mangle]
pub unsafe extern "C" fn pc_session_free(session: *mut PcSession) {
    if !session.is_null() {
        drop(unsafe { Box::from_raw(session) });
    }
}

/// The session title. Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_title(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(session.inner.title().to_string())
}

/// Number of changed files in the session.
#[no_mangle]
pub unsafe extern "C" fn pc_session_file_count(session: *const PcSession) -> usize {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return 0;
    };
    session.inner.files().len()
}

/// One changed file, hunks and lines included, as JSON:
/// `{old_path, new_path, status, is_binary, hunks: [{header, lines: [{kind,
/// text, raw?, old_line?, new_line?, patch_position?}]}]}`.
/// Returns null for an out-of-range index. Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_file_json(
    session: *const PcSession,
    index: usize,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let Some(file) = session.inner.files().get(index) else {
        return std::ptr::null_mut();
    };
    let result = catch_unwind(AssertUnwindSafe(|| serde_json::to_string(file)));
    match result {
        Ok(Ok(json)) => owned_c_string(json),
        _ => std::ptr::null_mut(),
    }
}

/// Attaches the persistence directory: loads any saved draft for this
/// source (re-anchoring it if the head moved) and persists every later
/// change. Returns a warning string when the saved draft was unreadable
/// (released with [`pc_string_free`]), null otherwise.
#[no_mangle]
pub unsafe extern "C" fn pc_session_attach_store(
    session: *mut PcSession,
    dir: *const c_char,
    dir_len: usize,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return std::ptr::null_mut();
    };
    let Some(dir) = (unsafe { str_from_raw(dir, dir_len) }) else {
        return owned_c_string("drafts directory is not valid UTF-8".to_string());
    };
    let result = catch_unwind(AssertUnwindSafe(|| session.inner.attach_store(dir)));
    match result {
        Ok(Some(warning)) => owned_c_string(warning),
        _ => std::ptr::null_mut(),
    }
}

/// Sets the author attributed to new comments and replies.
#[no_mangle]
pub unsafe extern "C" fn pc_session_set_author(
    session: *mut PcSession,
    author: *const c_char,
    author_len: usize,
) {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return;
    };
    let Some(author) = (unsafe { str_from_raw(author, author_len) }) else {
        return;
    };
    session.inner.set_author(author);
}

/// Side values crossing the boundary.
pub const PC_SIDE_LEFT: u32 = 0;
pub const PC_SIDE_RIGHT: u32 = 1;

fn side_from(value: u32) -> Side {
    if value == PC_SIDE_LEFT {
        Side::Left
    } else {
        Side::Right
    }
}

/// Adds a draft comment on one side of one file's lines. `reply_to` is a
/// host comment id (0 = a plain comment). Validates host semantics,
/// captures anchor and snippet, persists, and returns the new comment's
/// local id — or null with `error_out` set.
#[no_mangle]
pub unsafe extern "C" fn pc_session_add_comment(
    session: *mut PcSession,
    file_index: usize,
    side: u32,
    start_line: u32,
    end_line: u32,
    body: *const c_char,
    body_len: usize,
    reply_to: i64,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return std::ptr::null_mut();
    };
    let Some(body) = (unsafe { str_from_raw(body, body_len) }) else {
        unsafe { write_error(error_out, "comment body is not valid UTF-8") };
        return std::ptr::null_mut();
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let id = session.inner.add_comment(
            file_index,
            side_from(side),
            start_line,
            end_line,
            body.to_string(),
        )?;
        if reply_to != 0 {
            if let Some(comment) = session.inner.draft_mut().comment_mut(&id) {
                comment.reply_to = Some(reply_to);
            }
            session.inner.persist()?;
        }
        Ok::<String, String>(id)
    }));
    match result {
        Ok(Ok(id)) => owned_c_string(id),
        Ok(Err(message)) => {
            unsafe { write_error(error_out, &message) };
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe { write_error(error_out, "internal error while adding the comment") };
            std::ptr::null_mut()
        }
    }
}

/// Rewrites a draft comment's body. `false` if the id is unknown.
#[no_mangle]
pub unsafe extern "C" fn pc_session_update_comment(
    session: *mut PcSession,
    local_id: *const c_char,
    local_id_len: usize,
    body: *const c_char,
    body_len: usize,
) -> bool {
    with_comment_id(session, local_id, local_id_len, |session, id| {
        let Some(body) = (unsafe { str_from_raw(body, body_len) }) else {
            return false;
        };
        session.inner.update_comment(id, body.to_string()).is_ok()
    })
}

/// Deletes a draft comment. `false` if the id is unknown.
#[no_mangle]
pub unsafe extern "C" fn pc_session_delete_comment(
    session: *mut PcSession,
    local_id: *const c_char,
    local_id_len: usize,
) -> bool {
    with_comment_id(session, local_id, local_id_len, |session, id| {
        session.inner.delete_comment(id).is_ok()
    })
}

/// Dismiss ↔ restore a draft comment (kept, never submitted while
/// dismissed).
#[no_mangle]
pub unsafe extern "C" fn pc_session_toggle_dismiss(
    session: *mut PcSession,
    local_id: *const c_char,
    local_id_len: usize,
) -> bool {
    with_comment_id(session, local_id, local_id_len, |session, id| {
        session.inner.toggle_dismiss(id).is_ok()
    })
}

/// Appends a reply to a draft comment's travelling conversation.
#[no_mangle]
pub unsafe extern "C" fn pc_session_add_reply(
    session: *mut PcSession,
    local_id: *const c_char,
    local_id_len: usize,
    body: *const c_char,
    body_len: usize,
) -> bool {
    with_comment_id(session, local_id, local_id_len, |session, id| {
        let Some(body) = (unsafe { str_from_raw(body, body_len) }) else {
            return false;
        };
        session.inner.add_reply(id, body.to_string()).is_ok()
    })
}

fn with_comment_id(
    session: *mut PcSession,
    local_id: *const c_char,
    local_id_len: usize,
    operation: impl FnOnce(&mut PcSession, &str) -> bool,
) -> bool {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return false;
    };
    let Some(id) = (unsafe { str_from_raw(local_id, local_id_len) }) else {
        return false;
    };
    catch_unwind(AssertUnwindSafe(|| operation(session, id))).unwrap_or(false)
}

/// Every draft comment (with its location and state) as a JSON array.
/// Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_comments_json(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(session.inner.comments_json())
}

/// Existing host review threads as a JSON array (empty string outside PR
/// mode). Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_threads_json(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(session.inner.threads_json().to_string())
}

/// Pull-request metadata as JSON (empty string outside PR mode). Release
/// with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_pr_json(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(session.inner.pr_json().to_string())
}

/// The review summary.
#[no_mangle]
pub unsafe extern "C" fn pc_session_summary(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(session.inner.draft().summary.clone())
}

/// Sets the review summary (persisted immediately).
#[no_mangle]
pub unsafe extern "C" fn pc_session_set_summary(
    session: *mut PcSession,
    summary: *const c_char,
    summary_len: usize,
) -> bool {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return false;
    };
    let Some(summary) = (unsafe { str_from_raw(summary, summary_len) }) else {
        return false;
    };
    catch_unwind(AssertUnwindSafe(|| {
        session.inner.set_summary(summary.to_string()).is_ok()
    }))
    .unwrap_or(false)
}

/// Review events crossing the boundary.
pub const PC_EVENT_REVIEW_COMMENT: u32 = 0;
pub const PC_EVENT_REVIEW_APPROVE: u32 = 1;
pub const PC_EVENT_REVIEW_REQUEST_CHANGES: u32 = 2;

/// Sets the submission event.
#[no_mangle]
pub unsafe extern "C" fn pc_session_set_event(session: *mut PcSession, event: u32) {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return;
    };
    session.inner.draft_mut().event = match event {
        PC_EVENT_REVIEW_APPROVE => ReviewEvent::Approve,
        PC_EVENT_REVIEW_REQUEST_CHANGES => ReviewEvent::RequestChanges,
        _ => ReviewEvent::Comment,
    };
    let _ = session.inner.persist();
}

/// Exports the review to `path`: `.json` writes a review-exchange
/// document, anything else Markdown. `false` with `error_out` on failure.
#[no_mangle]
pub unsafe extern "C" fn pc_session_export_to_file(
    session: *const PcSession,
    path: *const c_char,
    path_len: usize,
    error_out: *mut *mut c_char,
) -> bool {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return false;
    };
    let Some(path) = (unsafe { str_from_raw(path, path_len) }) else {
        unsafe { write_error(error_out, "path is not valid UTF-8") };
        return false;
    };
    match catch_unwind(AssertUnwindSafe(|| session.inner.export_to_file(path))) {
        Ok(Ok(())) => true,
        Ok(Err(message)) => {
            unsafe { write_error(error_out, &message) };
            false
        }
        Err(_) => {
            unsafe { write_error(error_out, "internal error while exporting") };
            false
        }
    }
}

/// Submits the draft to the pull request: one atomic review, then staged
/// replies, then conversation comments. Blocking — run off the UI thread.
///
/// Returns JSON `{"posted": n, "remaining": n, "skipped_dismissed": n,
/// "skipped_orphaned": n, "error": "…"|null}`. Accepted drafts are removed
/// and the draft persisted **before** any error is reported, so a retry
/// sends only what is still pending — never a duplicate. Null only for a
/// session that is not in PR mode.
#[no_mangle]
pub unsafe extern "C" fn pc_session_submit(session: *mut PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return std::ptr::null_mut();
    };
    let Some(context) = session.pr.as_ref() else {
        return std::ptr::null_mut();
    };
    let forge = context.forge();
    let pr = context.reference.clone();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let plan = submit::plan(session.inner.draft());
        let outcome = submit::execute(forge.as_ref(), &pr, session.inner.draft(), &plan);

        // Retry safety: whatever the host accepted leaves the draft now,
        // even when a later step failed.
        let accepted = &outcome.accepted;
        let draft = session.inner.draft_mut();
        draft.comments.retain(|c| !accepted.contains(&c.local_id));
        draft.general.retain(|g| !accepted.contains(&g.local_id));
        if outcome.error.is_none() {
            draft.summary.clear();
            draft.event = ReviewEvent::Comment;
        }
        let remaining = draft.comments.len() + draft.general.len();
        let _ = session.inner.persist();

        serde_json::json!({
            "posted": accepted.len(),
            "remaining": remaining,
            "skipped_dismissed": plan.skipped_dismissed,
            "skipped_orphaned": plan.skipped_orphaned,
            "error": outcome.error,
        })
        .to_string()
    }));
    match result {
        Ok(json) => owned_c_string(json),
        Err(_) => owned_c_string(
            r#"{"posted":0,"remaining":0,"skipped_dismissed":0,"skipped_orphaned":0,"error":"internal error during submission"}"#.to_string(),
        ),
    }
}

/// Loads the configuration file at `path`. Never fails: a missing file is
/// the defaults, a broken one is defaults plus a load warning (the file is
/// left untouched). Returns null only for invalid UTF-8 in `path`.
#[no_mangle]
pub unsafe extern "C" fn pc_config_new(path: *const c_char, path_len: usize) -> *mut PcConfig {
    let Some(path) = (unsafe { str_from_raw(path, path_len) }) else {
        return std::ptr::null_mut();
    };
    let result = catch_unwind(AssertUnwindSafe(|| Config::load(std::path::Path::new(path))));
    match result {
        Ok(inner) => Box::into_raw(Box::new(PcConfig { inner })),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Releases a configuration handle.
#[no_mangle]
pub unsafe extern "C" fn pc_config_free(config: *mut PcConfig) {
    if !config.is_null() {
        drop(unsafe { Box::from_raw(config) });
    }
}

/// The problem found while loading, or null when the file loaded cleanly
/// (or did not exist). Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_config_load_warning(config: *const PcConfig) -> *mut c_char {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    match config.inner.load_warning() {
        Some(warning) => owned_c_string(warning.to_string()),
        None => std::ptr::null_mut(),
    }
}

/// Key-binding overrides as a JSON object string (`{"action": "key spec"}`;
/// an empty spec unbinds the default). Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_config_keys_json(config: *const PcConfig) -> *mut c_char {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(config.inner.keys_json())
}

/// Lists the open requests waiting for the user's review, through the
/// engine the config selects (`list_engine`: `gh` default, or `forgejo`
/// with `list_host`). Blocking — run off the UI thread. Returns a JSON
/// array of `{host, owner, repo, number, title, author, updated_at, url}`
/// or null with `error_out` set. Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_list_requests(
    config_path: *const c_char,
    config_path_len: usize,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    let config_path = unsafe { str_from_raw(config_path, config_path_len) }.unwrap_or_default();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let config = if config_path.is_empty() {
            Config::default()
        } else {
            Config::load(std::path::Path::new(config_path))
        };
        match config.list_engine() {
            "forgejo" => {
                let host = config.list_host();
                if host.is_empty() {
                    return Err(
                        "the forgejo list engine needs list_host in config.json".to_string()
                    );
                }
                let forge =
                    ForgejoForge::with_runner(ProcessRunner, config.forgejo_api_command());
                prchum_forge::list::list_forgejo(&forge, host, config.list_filter())
            }
            _ => prchum_forge::list::list_github(&ProcessRunner, config.list_filter()),
        }
    }));
    match result {
        Ok(Ok(requests)) => owned_c_string(serde_json::to_string(&requests).unwrap_or_default()),
        Ok(Err(message)) => {
            unsafe { write_error(error_out, &message) };
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe { write_error(error_out, "internal error while listing requests") };
            std::ptr::null_mut()
        }
    }
}

/// The syntax style table as a JSON array of `{light, dark, flags}`
/// (colors 0xRRGGBBAA as numbers; flags bit 0 = bold, bit 1 = italic).
/// Style ids in highlight spans index this table. Release with
/// [`pc_string_free`].
#[no_mangle]
pub extern "C" fn pc_style_table_json() -> *mut c_char {
    let json = serde_json::to_string(prchum_core::syntax::STYLES).unwrap_or_default();
    owned_c_string(json)
}

/// Syntax highlights for one file: JSON `[hunk][line][ [start, end,
/// style], … ]` with byte offsets into each line's display text. Null when
/// the file's language is unknown. Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_file_highlights_json(
    session: *const PcSession,
    index: usize,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let Some(file) = session.inner.files().get(index) else {
        return std::ptr::null_mut();
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        prchum_core::syntax::highlight_file(file)
            .and_then(|spans| serde_json::to_string(&spans).ok())
    }));
    match result {
        Ok(Some(json)) => owned_c_string(json),
        _ => std::ptr::null_mut(),
    }
}

/// Releases a string returned by this API.
#[no_mangle]
pub unsafe extern "C" fn pc_string_free(text: *mut c_char) {
    if !text.is_null() {
        drop(unsafe { CString::from_raw(text) });
    }
}

/// Writes `message` into the optional error out-parameter.
unsafe fn write_error(error_out: *mut *mut c_char, message: &str) {
    if error_out.is_null() {
        return;
    }
    unsafe { *error_out = owned_c_string(message.to_string()) };
}

/// An owned, nul-terminated C string; interior NULs become U+FFFD.
fn owned_c_string(text: String) -> *mut c_char {
    let sanitized = text.replace('\0', "\u{FFFD}");
    CString::new(sanitized)
        .expect("nul bytes replaced")
        .into_raw()
}

/// Borrows a `(pointer, length)` pair as `&str`; None on null or bad UTF-8.
unsafe fn str_from_raw<'a>(text: *const c_char, len: usize) -> Option<&'a str> {
    if text.is_null() {
        return if len == 0 { Some("") } else { None };
    }
    let bytes = unsafe { std::slice::from_raw_parts(text as *const u8, len) };
    std::str::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    const PATCH: &str = "--- a\n+++ b\n@@ -1 +1 @@\n-x\n+y\n";

    #[test]
    fn session_round_trip_through_ffi() {
        let mut error: *mut c_char = std::ptr::null_mut();
        let session = unsafe {
            pc_session_new_from_patch(
                "demo\0".as_ptr() as *const c_char,
                4,
                PATCH.as_ptr() as *const c_char,
                PATCH.len(),
                &mut error,
            )
        };
        assert!(!session.is_null());
        assert!(error.is_null());
        assert_eq!(unsafe { pc_session_file_count(session) }, 1);

        let json = unsafe { pc_session_file_json(session, 0) };
        assert!(!json.is_null());
        let text = unsafe { std::ffi::CStr::from_ptr(json) }
            .to_str()
            .unwrap()
            .to_string();
        assert!(text.contains("\"new_path\":\"b\""), "{text}");
        unsafe { pc_string_free(json) };

        assert!(unsafe { pc_session_file_json(session, 9) }.is_null());
        unsafe { pc_session_free(session) };
    }

    #[test]
    fn parse_failure_reports_error() {
        let mut error: *mut c_char = std::ptr::null_mut();
        let bad = "junk";
        let session = unsafe {
            pc_session_new_from_patch(
                std::ptr::null(),
                0,
                bad.as_ptr() as *const c_char,
                bad.len(),
                &mut error,
            )
        };
        assert!(session.is_null());
        assert!(!error.is_null());
        unsafe { pc_string_free(error) };
    }

    #[test]
    fn app_pong_round_trip() {
        extern "C" fn on_event(event: *const PcEvent, userdata: *mut c_void) {
            let event = unsafe { &*event };
            if event.kind == PC_EVENT_PONG {
                let tx = unsafe { &*(userdata as *const mpsc::Sender<u64>) };
                let _ = tx.send(event.seq);
            }
        }
        let (tx, rx) = mpsc::channel::<u64>();
        let app = pc_app_new(Some(on_event), &tx as *const _ as *mut c_void);
        unsafe { pc_app_ping(app, 42) };
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), 42);
        unsafe { pc_app_free(app) };
    }
}
