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
use prchum_forge::open::{open_session, PrContext};
use prchum_forge::forgejo::ForgejoForge;
use prchum_forge::ghcli::{GhForge, ProcessRunner};
use prchum_forge::glabcli::GlabForge;
use prchum_forge::refs::parse_ref;
use prchum_forge::{kind_for_host, submit, Forge, ForgeKind};

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
///
/// The session is internally synchronized: calls may arrive from more
/// than one thread (the shell runs slow operations — submission, the
/// first context fetch — off its UI thread) and serialize on a mutex.
pub struct PcSession {
    inner: std::sync::Mutex<Session>,
    /// Set for PR-mode sessions; submission targets it.
    pr: Option<PrContext>,
}

impl PcSession {
    fn lock(&self) -> std::sync::MutexGuard<'_, Session> {
        // A poisoned lock means a panic mid-operation; the data is still
        // consistent enough to read, and refusing forever helps nobody.
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
        Ok(Ok(inner)) => Box::into_raw(Box::new(PcSession {
            inner: std::sync::Mutex::new(inner),
            pr: None,
        })),
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

    // The config is loaded here rather than inside the opener: the shells
    // that call it directly already have one.
    let config = if config_path.is_empty() {
        Config::default()
    } else {
        Config::load(std::path::Path::new(config_path))
    };
    let built = catch_unwind(AssertUnwindSafe(|| {
        open_session(reference, repo_hint, &config)
    }));
    match built {
        Ok(Ok((inner, pr))) => Box::into_raw(Box::new(PcSession {
            inner: std::sync::Mutex::new(inner),
            pr: Some(pr),
        })),
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

/// The whole-file projection of one file — the context view: content
/// fetched through the source (blocking for PR sessions; run off the UI
/// thread on first use), verified against the diff, hunks overlaid. Same
/// JSON shape as [`pc_session_file_json`]. Null with `error_out` set when
/// the source has no content (plain patches), the file is deleted, or the
/// content does not match the diff.
#[no_mangle]
pub unsafe extern "C" fn pc_session_context_file_json(
    session: *mut PcSession,
    index: usize,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return std::ptr::null_mut();
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut inner = session.lock();
        inner
            .context_file(index)
            .and_then(|file| serde_json::to_string(file).map_err(|e| e.to_string()))
    }));
    match result {
        Ok(Ok(json)) => owned_c_string(json),
        Ok(Err(message)) => {
            unsafe { write_error(error_out, &message) };
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe { write_error(error_out, "internal error building the context view") };
            std::ptr::null_mut()
        }
    }
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
    owned_c_string(session.lock().title().to_string())
}

/// Number of changed files in the session.
#[no_mangle]
pub unsafe extern "C" fn pc_session_file_count(session: *const PcSession) -> usize {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return 0;
    };
    session.lock().files().len()
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
    let inner = session.lock();
    let Some(file) = inner.files().get(index) else {
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
    let result = catch_unwind(AssertUnwindSafe(|| session.lock().attach_store(dir)));
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
    session.lock().set_author(author);
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
        let id = session.lock().add_thread_reply(
            file_index,
            side_from(side),
            start_line,
            end_line,
            body.to_string(),
            reply_to,
        )?;
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
        session.lock().update_comment(id, body.to_string()).is_ok()
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
        session.lock().delete_comment(id).is_ok()
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
        session.lock().toggle_dismiss(id).is_ok()
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
        session.lock().add_reply(id, body.to_string()).is_ok()
    })
}

/// Rewrites one reply of a draft's conversation (authors stay).
#[no_mangle]
pub unsafe extern "C" fn pc_session_update_reply(
    session: *mut PcSession,
    local_id: *const c_char,
    local_id_len: usize,
    index: usize,
    body: *const c_char,
    body_len: usize,
) -> bool {
    with_comment_id(session, local_id, local_id_len, |session, id| {
        let Some(body) = (unsafe { str_from_raw(body, body_len) }) else {
            return false;
        };
        session.lock().update_reply(id, index, body.to_string()).is_ok()
    })
}

/// Deletes one reply of a draft's conversation.
#[no_mangle]
pub unsafe extern "C" fn pc_session_delete_reply(
    session: *mut PcSession,
    local_id: *const c_char,
    local_id_len: usize,
    index: usize,
) -> bool {
    with_comment_id(session, local_id, local_id_len, |session, id| {
        session.lock().delete_reply(id, index).is_ok()
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
    owned_c_string(session.lock().comments_json())
}

/// Existing host review threads as a JSON array (empty string outside PR
/// mode). Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_threads_json(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(session.lock().threads_json().to_string())
}

/// Pull-request metadata as JSON (empty string outside PR mode). Release
/// with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_pr_json(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(session.lock().pr_json().to_string())
}

/// The review summary.
#[no_mangle]
pub unsafe extern "C" fn pc_session_summary(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(session.lock().draft().summary.clone())
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
        session.lock().set_summary(summary.to_string()).is_ok()
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
    session.lock().draft_mut().event = match event {
        PC_EVENT_REVIEW_APPROVE => ReviewEvent::Approve,
        PC_EVENT_REVIEW_REQUEST_CHANGES => ReviewEvent::RequestChanges,
        _ => ReviewEvent::Comment,
    };
    let _ = session.lock().persist();
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
    match catch_unwind(AssertUnwindSafe(|| session.lock().export_to_file(path))) {
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
        // One guard across the whole submission: the session is
        // consistent for its duration, and any concurrent access simply
        // waits (the shell keeps its UI thread away meanwhile).
        let mut inner = session.lock();
        let plan = submit::plan(inner.draft());
        let outcome = submit::execute(forge.as_ref(), &pr, inner.draft(), &plan);

        let posted = outcome.accepted.len();
        let remaining = inner
            .apply_accepted(&outcome.accepted, outcome.error.is_none())
            .unwrap_or(0);

        serde_json::json!({
            "posted": posted,
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

/// The selected named keymap, or an empty string. `exists_out` (when
/// non-null) reports whether `keymaps` defines it — the shell warns on a
/// typo. Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_config_keymap(
    config: *const PcConfig,
    exists_out: *mut bool,
) -> *mut c_char {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let (name, exists) = config.inner.keymap_status();
    if !exists_out.is_null() {
        unsafe { *exists_out = exists };
    }
    owned_c_string(name)
}

/// The named discovery filters as a JSON object (`{name: filter}`).
/// Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_config_list_filters_json(config: *const PcConfig) -> *mut c_char {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(config.inner.list_filters_json())
}

/// The configured clones as a JSON object (`{"owner/repo": path}`).
/// Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_config_clones_json(config: *const PcConfig) -> *mut c_char {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(config.inner.clones_json())
}

/// Who drafts are attributed to (empty = the account name). Release
/// with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_config_author(config: *const PcConfig) -> *mut c_char {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(config.inner.author().to_string())
}

/// The editor template (empty = the built-in textchum URL). Release with
/// [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_config_editor_command(config: *const PcConfig) -> *mut c_char {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(config.inner.editor_command().to_string())
}

/// The fallback discovery filter (empty = the engine's default).
/// Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_config_list_filter(config: *const PcConfig) -> *mut c_char {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(config.inner.list_filter().to_string())
}

/// Writes one entry of a top-level map setting (`list_filters`, `keys`,
/// `forges`…) into config.json, preserving everything else. An empty
/// value removes the entry; a broken file is left untouched.
#[no_mangle]
pub unsafe extern "C" fn pc_config_set_map_entry(
    config_path: *const c_char,
    config_path_len: usize,
    map_key: *const c_char,
    map_key_len: usize,
    entry_key: *const c_char,
    entry_key_len: usize,
    value: *const c_char,
    value_len: usize,
) -> bool {
    let Some(config_path) = (unsafe { str_from_raw(config_path, config_path_len) }) else {
        return false;
    };
    let Some(map_key) = (unsafe { str_from_raw(map_key, map_key_len) }) else {
        return false;
    };
    let Some(entry_key) = (unsafe { str_from_raw(entry_key, entry_key_len) }) else {
        return false;
    };
    let Some(value) = (unsafe { str_from_raw(value, value_len) }) else {
        return false;
    };
    catch_unwind(AssertUnwindSafe(|| {
        prchum_core::config::set_map_entry(
            std::path::Path::new(config_path),
            map_key,
            entry_key,
            value,
        )
        .is_ok()
    }))
    .unwrap_or(false)
}

/// The configured appearance: 0 = system, 1 = light, 2 = dark.
#[no_mangle]
pub unsafe extern "C" fn pc_config_appearance(config: *const PcConfig) -> u32 {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return 0;
    };
    match config.inner.appearance() {
        "light" => 1,
        "dark" => 2,
        _ => 0,
    }
}

/// The configured theme name (empty = default). Release with
/// [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_config_theme(config: *const PcConfig) -> *mut c_char {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(config.inner.theme().to_string())
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
/// with `list_host`). `filter` overrides the config's `list_filter`
/// (empty = use it). Blocking — run off the UI thread. Returns a JSON
/// array of `{host, owner, repo, number, title, author, updated_at, url}`
/// or null with `error_out` set. Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_list_requests(
    config_path: *const c_char,
    config_path_len: usize,
    filter: *const c_char,
    filter_len: usize,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    let config_path = unsafe { str_from_raw(config_path, config_path_len) }.unwrap_or_default();
    let filter = unsafe { str_from_raw(filter, filter_len) }.unwrap_or_default();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let config = if config_path.is_empty() {
            Config::default()
        } else {
            Config::load(std::path::Path::new(config_path))
        };
        let effective = if filter.is_empty() {
            config.list_filter()
        } else {
            filter
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
                prchum_forge::list::list_forgejo(&forge, host, effective)
            }
            _ => prchum_forge::list::list_github(&ProcessRunner, effective),
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

/// Host conversation-level comments (PR mode) as a JSON array (empty
/// string otherwise). Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_general_json(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(session.lock().general_json().to_string())
}

/// The staged conversation comments as a JSON array. Release with
/// [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_general_drafts_json(
    session: *const PcSession,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    owned_c_string(session.lock().general_drafts_json())
}

/// Stages a conversation-level comment (posts on submit). Returns its
/// local id, or null. Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_add_general(
    session: *mut PcSession,
    body: *const c_char,
    body_len: usize,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return std::ptr::null_mut();
    };
    let Some(body) = (unsafe { str_from_raw(body, body_len) }) else {
        return std::ptr::null_mut();
    };
    match catch_unwind(AssertUnwindSafe(|| session.lock().add_general(body.to_string()))) {
        Ok(Ok(id)) => owned_c_string(id),
        _ => std::ptr::null_mut(),
    }
}

/// Deletes a staged conversation comment by local id.
#[no_mangle]
pub unsafe extern "C" fn pc_session_delete_general(
    session: *mut PcSession,
    local_id: *const c_char,
    local_id_len: usize,
) -> bool {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return false;
    };
    let Some(id) = (unsafe { str_from_raw(local_id, local_id_len) }) else {
        return false;
    };
    catch_unwind(AssertUnwindSafe(|| session.lock().delete_general(id).is_ok()))
        .unwrap_or(false)
}

/// Records (or refreshes) this session in the review history at `dir`.
/// `submitted` also stamps the submission time. `false` on failure.
#[no_mangle]
pub unsafe extern "C" fn pc_session_record_history(
    session: *const PcSession,
    dir: *const c_char,
    dir_len: usize,
    submitted: bool,
) -> bool {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return false;
    };
    let Some(dir) = (unsafe { str_from_raw(dir, dir_len) }) else {
        return false;
    };
    catch_unwind(AssertUnwindSafe(|| {
        let inner = session.lock();
        let display = match inner.kind() {
            "pr" => session
                .pr
                .as_ref()
                .map(|c| {
                    format!(
                        "{}/{}#{}",
                        c.reference.owner, c.reference.repo, c.reference.number
                    )
                })
                .unwrap_or_default(),
            "git" => inner
                .reopen_hint()
                .split('\u{1F}')
                .next()
                .unwrap_or_default()
                .to_string(),
            _ => inner.reopen_hint().to_string(),
        };
        let entry = prchum_core::history::HistoryEntry {
            key: inner.source_key().to_string(),
            kind: inner.kind().to_string(),
            title: inner.title().to_string(),
            display,
            reopen: inner.reopen_hint().to_string(),
            last_opened: String::new(),
            submitted_at: String::new(),
        };
        prchum_core::history::record(dir, entry, submitted).is_ok()
    }))
    .unwrap_or(false)
}

/// The review history at `dir` as a JSON array, newest first. Release
/// with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_history_list_json(dir: *const c_char, dir_len: usize) -> *mut c_char {
    let Some(dir) = (unsafe { str_from_raw(dir, dir_len) }) else {
        return std::ptr::null_mut();
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        serde_json::to_string(&prchum_core::history::load(dir)).unwrap_or_default()
    }));
    owned_c_string(result.unwrap_or_default())
}

/// Removes one history entry by source key (the user's hand deletion).
#[no_mangle]
pub unsafe extern "C" fn pc_history_remove(
    dir: *const c_char,
    dir_len: usize,
    key: *const c_char,
    key_len: usize,
) -> bool {
    let Some(dir) = (unsafe { str_from_raw(dir, dir_len) }) else {
        return false;
    };
    let Some(key) = (unsafe { str_from_raw(key, key_len) }) else {
        return false;
    };
    catch_unwind(AssertUnwindSafe(|| {
        prchum_core::history::remove(dir, key).is_ok()
    }))
    .unwrap_or(false)
}

/// Prunes history entries whose pull request is merged, closed, or gone,
/// asking each entry's forge. Blocking (one network call per PR entry) —
/// run off the UI thread. Network failures never prune; only a definite
/// answer does. Returns the surviving entries as JSON, newest first.
#[no_mangle]
pub unsafe extern "C" fn pc_history_prune_json(
    dir: *const c_char,
    dir_len: usize,
    config_path: *const c_char,
    config_path_len: usize,
) -> *mut c_char {
    let Some(dir) = (unsafe { str_from_raw(dir, dir_len) }) else {
        return std::ptr::null_mut();
    };
    let config_path = unsafe { str_from_raw(config_path, config_path_len) }.unwrap_or_default();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let config = if config_path.is_empty() {
            Config::default()
        } else {
            Config::load(std::path::Path::new(config_path))
        };
        let kept = prchum_core::history::prune(dir, |entry| {
            // A request that is finished with takes its worktree along —
            // but only one prchum created (see worktree::remove_for_key).
            if entry.kind != "pr" {
                // Files and comparisons only leave by hand.
                return false;
            }
            let Some(pr_ref) = parse_ref(&entry.reopen) else {
                return false;
            };
            let kind = kind_for_host(&pr_ref.host, config.forge_for_host(&pr_ref.host));
            let forge: Box<dyn Forge> = match kind {
                ForgeKind::Forgejo => Box::new(ForgejoForge::with_runner(
                    ProcessRunner,
                    config.forgejo_api_command(),
                )),
                ForgeKind::GitLab => Box::new(GlabForge::new()),
                ForgeKind::GitHub => Box::new(GhForge::new()),
            };
            let finished = match forge.pull_request(&pr_ref) {
                Ok(pr) => pr.merged || pr.state == "closed",
                // 404 means gone; anything else (auth, network) keeps it.
                Err(error) => error.contains("404") || error.contains("Not Found"),
            };
            if finished {
                let _ = prchum_core::worktree::remove_for_key(dir, &entry.key);
            }
            finished
        });
        kept.map(|entries| serde_json::to_string(&entries).unwrap_or_default())
    }));
    match result {
        Ok(Ok(json)) => owned_c_string(json),
        _ => std::ptr::null_mut(),
    }
}

/// The repository this session belongs to as `owner/repo`, or an empty
/// string when it has none (patches, exchange documents). Release with
/// [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_repo_slug(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let slug = session
        .pr
        .as_ref()
        .map(|context| {
            format!("{}/{}", context.reference.owner, context.reference.repo)
        })
        .unwrap_or_default();
    owned_c_string(slug)
}

/// A shareable permalink to `path` at the request's head revision,
/// anchored at `line` when it is non-zero.
///
/// Empty for a session with no forge behind it: a patch or a local git
/// comparison has nowhere to point at. Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_line_url(
    session: *const PcSession,
    path: *const c_char,
    path_len: usize,
    line: u32,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let Some(path) = (unsafe { str_from_raw(path, path_len) }) else {
        return std::ptr::null_mut();
    };
    let Some(context) = session.pr.as_ref() else {
        return owned_c_string(String::new());
    };
    let head = session.lock().head_oid().to_string();
    if head.is_empty() {
        return owned_c_string(String::new());
    }
    let url = context.reference.blob_url(
        context.kind,
        &head,
        path,
        if line == 0 { None } else { Some(line) },
    );
    owned_c_string(url)
}

/// The request's own Files tab, for sharing the review rather than a
/// line of it. Empty when the session has no forge. Release with
/// [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_files_url(session: *const PcSession) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let url = session
        .pr
        .as_ref()
        .map(|context| context.reference.files_url(context.kind))
        .unwrap_or_default();
    owned_c_string(url)
}

/// Finds or creates the local worktree to edit this session's files in,
/// and returns `{path, branch, created}` as JSON.
///
/// * A pull request checks its branch out of `clone` — reusing a
///   worktree that already has it (including the clone's own checkout,
///   which is then left alone), or creating one under `dir/worktrees/`
///   that prchum owns and later cleans up.
/// * A git comparison already *is* a checkout: its repository root comes
///   back unmanaged, whatever `clone` says.
/// * Patches and exchange documents have no repository — an error.
///
/// Blocking (fetches when the branch is unknown locally); run off the UI
/// thread. Null with `error_out` set on failure.
#[no_mangle]
pub unsafe extern "C" fn pc_session_worktree_json(
    session: *const PcSession,
    dir: *const c_char,
    dir_len: usize,
    clone: *const c_char,
    clone_len: usize,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let Some(dir) = (unsafe { str_from_raw(dir, dir_len) }) else {
        unsafe { write_error(error_out, "state directory is not valid UTF-8") };
        return std::ptr::null_mut();
    };
    let clone = unsafe { str_from_raw(clone, clone_len) }.unwrap_or_default();

    let result = catch_unwind(AssertUnwindSafe(|| {
        let inner = session.lock();
        match inner.kind() {
            // The comparison's own checkout is already the right tree.
            "git" => {
                let root = inner
                    .reopen_hint()
                    .split('\u{1F}')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                if root.is_empty() {
                    return Err("this comparison has no repository on disk".to_string());
                }
                Ok(prchum_core::worktree::WorktreeInfo {
                    path: root,
                    branch: String::new(),
                    created: false,
                })
            }
            "pr" => {
                if clone.is_empty() {
                    return Err(
                        "no local clone is configured for this repository — add one in Settings"
                            .to_string(),
                    );
                }
                let Some(context) = session.pr.as_ref() else {
                    return Err("this session has no pull request".to_string());
                };
                let metadata: serde_json::Value =
                    serde_json::from_str(inner.pr_json()).unwrap_or_default();
                let branch = metadata["head_ref"].as_str().unwrap_or_default();
                let number = context.reference.number;
                // The forge's ref for a request's head — the way to reach
                // a branch that lives on a fork.
                let fetch_ref = match context.kind {
                    ForgeKind::GitLab => format!("refs/merge-requests/{number}/head"),
                    _ => format!("refs/pull/{number}/head"),
                };
                prchum_core::worktree::ensure(
                    dir,
                    inner.source_key(),
                    clone,
                    branch,
                    number,
                    &fetch_ref,
                    inner.head_oid(),
                )
            }
            _ => Err("this source has no repository to edit in".to_string()),
        }
    }));
    match result {
        Ok(Ok(info)) => owned_c_string(serde_json::to_string(&info).unwrap_or_default()),
        Ok(Err(message)) => {
            unsafe { write_error(error_out, &message) };
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe { write_error(error_out, "internal error preparing the worktree") };
            std::ptr::null_mut()
        }
    }
}

/// Removes the worktree prchum created for `key`, if any. Worktrees it
/// did not create are never touched. `true` when one went away.
#[no_mangle]
pub unsafe extern "C" fn pc_worktree_remove(
    dir: *const c_char,
    dir_len: usize,
    key: *const c_char,
    key_len: usize,
) -> bool {
    let Some(dir) = (unsafe { str_from_raw(dir, dir_len) }) else {
        return false;
    };
    let Some(key) = (unsafe { str_from_raw(key, key_len) }) else {
        return false;
    };
    catch_unwind(AssertUnwindSafe(|| {
        prchum_core::worktree::remove_for_key(dir, key).unwrap_or(false)
    }))
    .unwrap_or(false)
}

/// The editor invocation for opening `path` at `line`: JSON
/// `{"kind": "url", "url": …}` or `{"kind": "command", "program": …,
/// "args": [...]}`. An empty template means the built-in textchum URL.
/// Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_editor_invocation_json(
    template: *const c_char,
    template_len: usize,
    path: *const c_char,
    path_len: usize,
    line: u32,
    dir: *const c_char,
    dir_len: usize,
) -> *mut c_char {
    let template = unsafe { str_from_raw(template, template_len) }.unwrap_or_default();
    let Some(path) = (unsafe { str_from_raw(path, path_len) }) else {
        return std::ptr::null_mut();
    };
    let dir = unsafe { str_from_raw(dir, dir_len) }.unwrap_or_default();
    let json = match prchum_core::editor::invocation(template, path, line, dir) {
        prchum_core::editor::Invocation::Url(url) => {
            serde_json::json!({ "kind": "url", "url": url })
        }
        prchum_core::editor::Invocation::Command { program, args } => {
            serde_json::json!({ "kind": "command", "program": program, "args": args })
        }
    };
    owned_c_string(json.to_string())
}

/// Built-in theme names, newline-joined, in presentation order.
/// Release with [`pc_string_free`].
#[no_mangle]
pub extern "C" fn pc_theme_builtin_names() -> *mut c_char {
    owned_c_string(prchum_core::syntax::BUILTIN_THEMES.join("\n"))
}

/// Syntax highlights for the context projection of one file: same JSON
/// shape as [`pc_session_file_highlights_json`], but indexed by the
/// projection's hunks, so gap regions color too. Null when the language
/// is unknown or the projection is unavailable. Cached content — cheap
/// after the first [`pc_session_context_file_json`]. Release with
/// [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_session_context_highlights_json(
    session: *mut PcSession,
    index: usize,
) -> *mut c_char {
    let Some(session) = (unsafe { session.as_mut() }) else {
        return std::ptr::null_mut();
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut inner = session.lock();
        match inner.context_highlights(index) {
            Ok(Some(spans)) => serde_json::to_string(&spans).ok(),
            _ => None,
        }
    }));
    match result {
        Ok(Some(json)) => owned_c_string(json),
        _ => std::ptr::null_mut(),
    }
}

/// The syntax style table as a JSON array of `{light, dark, flags}`
/// (colors 0xRRGGBBAA as numbers; flags bit 0 = bold, bit 1 = italic).
/// Style ids in highlight spans index this table. Release with
/// [`pc_string_free`].
#[no_mangle]
pub extern "C" fn pc_style_table_json() -> *mut c_char {
    let json = serde_json::to_string(&prchum_core::syntax::styles()).unwrap_or_default();
    owned_c_string(json)
}

/// Applies the theme config.json names: a built-in (`default`,
/// `high-contrast`) or a `themes/<name>.json` next to the config file.
/// Returns a warning string when the theme could not apply (the default
/// stays), null on success. Release with [`pc_string_free`].
#[no_mangle]
pub unsafe extern "C" fn pc_theme_apply(
    config_path: *const c_char,
    config_path_len: usize,
) -> *mut c_char {
    let Some(config_path) = (unsafe { str_from_raw(config_path, config_path_len) }) else {
        return owned_c_string("config path is not valid UTF-8".to_string());
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let config = Config::load(std::path::Path::new(config_path));
        let name = config.theme();
        if prchum_core::syntax::set_builtin(name) {
            return None;
        }
        let themes_dir = std::path::Path::new(config_path)
            .parent()
            .map(|dir| dir.join("themes"))
            .unwrap_or_default();
        let file = themes_dir.join(format!("{name}.json"));
        match std::fs::read_to_string(&file) {
            Ok(text) => prchum_core::syntax::set_theme_json(&text)
                .err()
                .map(|error| format!("theme {name}: {error} — the default stays")),
            Err(_) => Some(format!(
                "theme {name} is neither built in nor at {} — the default stays",
                file.display()
            )),
        }
    }));
    match result {
        Ok(None) => std::ptr::null_mut(),
        Ok(Some(warning)) => owned_c_string(warning),
        Err(_) => owned_c_string("internal error applying the theme".to_string()),
    }
}

/// Writes one string setting into config.json, preserving everything
/// else in the file (unknown keys included). `false` with the file left
/// untouched on any problem.
#[no_mangle]
pub unsafe extern "C" fn pc_config_set_string(
    config_path: *const c_char,
    config_path_len: usize,
    key: *const c_char,
    key_len: usize,
    value: *const c_char,
    value_len: usize,
) -> bool {
    let Some(config_path) = (unsafe { str_from_raw(config_path, config_path_len) }) else {
        return false;
    };
    let Some(key) = (unsafe { str_from_raw(key, key_len) }) else {
        return false;
    };
    let Some(value) = (unsafe { str_from_raw(value, value_len) }) else {
        return false;
    };
    catch_unwind(AssertUnwindSafe(|| {
        prchum_core::config::set_string(std::path::Path::new(config_path), key, value).is_ok()
    }))
    .unwrap_or(false)
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
    let inner = session.lock();
    let Some(file) = inner.files().get(index) else {
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
