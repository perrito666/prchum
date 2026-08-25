//! A review session: the parsed diff plus (eventually) the review state.
//!
//! Unlike an editor buffer, the diff under review is immutable; the mutable
//! part of a session is what the reviewer says about it. Phase 0 carries
//! only the diff — drafts, threads, and relocation join in Phase 1+.

use crate::diff::{self, FileDiff, ParseError, DEFAULT_TAB_WIDTH};

pub struct Session {
    title: String,
    files: Vec<FileDiff>,
}

impl Session {
    /// Opens a session over a literal patch text.
    pub fn from_patch(title: &str, patch: &str) -> Result<Self, ParseError> {
        let files = diff::parse(patch, DEFAULT_TAB_WIDTH)?;
        Ok(Self {
            title: title.to_string(),
            files,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn files(&self) -> &[FileDiff] {
        &self.files
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_over_patch() {
        let session =
            Session::from_patch("t", "--- a\n+++ b\n@@ -1 +1 @@\n-x\n+y\n").unwrap();
        assert_eq!(session.files().len(), 1);
        assert_eq!(session.title(), "t");
    }

    #[test]
    fn bad_patch_is_an_error() {
        assert!(Session::from_patch("t", "nope").is_err());
    }
}
