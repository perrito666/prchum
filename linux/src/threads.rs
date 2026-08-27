//! Threads already on the pull request.
//!
//! The session carries them as JSON because the FFI has to; here they
//! are decoded back into the very types the forge produced, so a
//! reviewer sees the conversation and their own drafts in one column
//! and nothing describes the shape twice.

use prchum_forge::ThreadInfo;

/// Decodes what the session carries, tolerating a shape it does not
/// recognise: a conversation that cannot be read must not stop the
/// review, it simply is not shown.
pub fn decode(json: &str) -> Vec<ThreadInfo> {
    if json.trim().is_empty() {
        return Vec::new();
    }
    serde_json::from_str(json).unwrap_or_default()
}

/// True when the thread hangs on the new side of the diff.
pub fn is_new_side(thread: &ThreadInfo) -> bool {
    !thread.side.eq_ignore_ascii_case("LEFT")
}
