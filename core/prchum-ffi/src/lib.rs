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

use prchum_core::{App, Event, Session};

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

/// A review session over one diff source. Create with
/// [`pc_session_new_from_patch`], release with [`pc_session_free`].
pub struct PcSession {
    inner: Session,
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
    match result {
        Ok(Ok(inner)) => Box::into_raw(Box::new(PcSession { inner })),
        Ok(Err(error)) => {
            unsafe { write_error(error_out, &error.to_string()) };
            std::ptr::null_mut()
        }
        Err(_) => {
            unsafe { write_error(error_out, "internal error while parsing") };
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
