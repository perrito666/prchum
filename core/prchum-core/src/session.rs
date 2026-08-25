//! A review session: the parsed diff plus the review state over it.
//!
//! The diff under review is immutable; the mutable part of a session is
//! what the reviewer says about it. Every mutation persists the draft
//! (when a store is attached) — quitting never loses a note.

use crate::diff::{self, FileDiff, ParseError, Side, DEFAULT_TAB_WIDTH};
use crate::export;
use crate::location::{build_location, snippet};
use crate::review::{DraftReview, DraftStore};
use crate::util::fnv64_hex;

pub struct Session {
    title: String,
    files: Vec<FileDiff>,
    source_key: String,
    author: String,
    draft: DraftReview,
    store: Option<DraftStore>,
}

impl Session {
    /// Opens a session over a literal patch text. The source key hashes the
    /// content, so the same patch resumes the same draft.
    pub fn from_patch(title: &str, patch: &str) -> Result<Self, ParseError> {
        let key = format!("patch-{}", fnv64_hex(patch.as_bytes()));
        Self::from_patch_keyed(title, patch, key)
    }

    /// Opens a session over a patch file. The source key hashes the absolute
    /// path, so reviewing the file again — even from another directory —
    /// resumes the same draft.
    pub fn from_patch_file(path: &str) -> Result<Self, ParseError> {
        let text = std::fs::read_to_string(path).map_err(|error| ParseError {
            message: format!("could not read {path}: {error}"),
        })?;
        let absolute = std::fs::canonicalize(path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string());
        let title = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        let key = format!("patch-{}", fnv64_hex(absolute.as_bytes()));
        Self::from_patch_keyed(&title, &text, key)
    }

    pub fn from_patch_keyed(
        title: &str,
        patch: &str,
        source_key: String,
    ) -> Result<Self, ParseError> {
        let files = diff::parse(patch, DEFAULT_TAB_WIDTH)?;
        let draft = DraftReview {
            source_key: source_key.clone(),
            title: title.to_string(),
            ..Default::default()
        };
        Ok(Self {
            title: title.to_string(),
            files,
            source_key,
            author: String::new(),
            draft,
            store: None,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn files(&self) -> &[FileDiff] {
        &self.files
    }

    pub fn source_key(&self) -> &str {
        &self.source_key
    }

    pub fn set_author(&mut self, author: &str) {
        self.author = author.to_string();
    }

    pub fn draft(&self) -> &DraftReview {
        &self.draft
    }

    pub fn draft_mut(&mut self) -> &mut DraftReview {
        &mut self.draft
    }

    /// Attaches the persistence directory and loads any existing draft for
    /// this source. Returns a warning when the saved draft was unreadable
    /// (the file is left in place; the session starts fresh).
    pub fn attach_store(&mut self, dir: &str) -> Option<String> {
        let store = DraftStore::new(dir);
        let warning = match store.load(&self.source_key) {
            Ok(Some(saved)) => {
                self.draft = saved;
                // The diff may not be the one the draft was made against.
                self.draft.source_key = self.source_key.clone();
                self.draft.title = self.title.clone();
                None
            }
            Ok(None) => None,
            Err(message) => Some(message),
        };
        self.store = Some(store);
        warning
    }

    /// Adds a comment on `side` lines `start..=end` of the file at
    /// `file_index`. Validates host semantics, captures the anchor and
    /// snippet, persists, and returns the new comment's local id.
    pub fn add_comment(
        &mut self,
        file_index: usize,
        side: Side,
        start_line: u32,
        end_line: u32,
        body: String,
    ) -> Result<String, String> {
        let file = self
            .files
            .get(file_index)
            .ok_or_else(|| "no such file".to_string())?;
        let location =
            build_location(file, side, start_line, end_line).map_err(|e| e.to_string())?;
        let code = snippet(file, side, start_line, end_line);
        let author = self.author.clone();
        let id = self.draft.add_comment(location, code, body, &author);
        self.persist()?;
        Ok(id)
    }

    pub fn update_comment(&mut self, local_id: &str, body: String) -> Result<(), String> {
        if !self.draft.update_comment(local_id, body) {
            return Err("no such comment".to_string());
        }
        self.persist()
    }

    pub fn delete_comment(&mut self, local_id: &str) -> Result<(), String> {
        if !self.draft.delete_comment(local_id) {
            return Err("no such comment".to_string());
        }
        self.persist()
    }

    pub fn toggle_dismiss(&mut self, local_id: &str) -> Result<(), String> {
        if !self.draft.toggle_dismiss(local_id) {
            return Err("no such comment".to_string());
        }
        self.persist()
    }

    pub fn add_reply(&mut self, local_id: &str, body: String) -> Result<(), String> {
        let author = self.author.clone();
        if !self.draft.add_reply(local_id, body, &author) {
            return Err("no such comment".to_string());
        }
        self.persist()
    }

    pub fn set_summary(&mut self, summary: String) -> Result<(), String> {
        self.draft.summary = summary;
        self.persist()
    }

    /// The draft comments as JSON (array of comments with locations).
    pub fn comments_json(&self) -> String {
        serde_json::to_string(&self.draft.comments).unwrap_or_else(|_| "[]".to_string())
    }

    pub fn export_markdown(&self) -> String {
        export::markdown(&self.draft)
    }

    /// Persists the draft when a store is attached; a session without a
    /// store (tests, previews) simply keeps state in memory.
    pub fn persist(&self) -> Result<(), String> {
        match &self.store {
            Some(store) => store.save(&self.draft),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,2 @@\n context\n-a\n+b\n";

    #[test]
    fn session_over_patch() {
        let session = Session::from_patch("t", PATCH).unwrap();
        assert_eq!(session.files().len(), 1);
        assert_eq!(session.title(), "t");
        assert!(session.source_key().starts_with("patch-"));
    }

    #[test]
    fn bad_patch_is_an_error() {
        assert!(Session::from_patch("t", "nope").is_err());
    }

    #[test]
    fn comment_flow_with_persistence() {
        let dir = std::env::temp_dir().join(format!("prchum-session-{}", std::process::id()));
        let dir = dir.to_string_lossy().to_string();

        let mut session = Session::from_patch("t", PATCH).unwrap();
        assert!(session.attach_store(&dir).is_none());
        session.set_author("me");

        let id = session
            .add_comment(0, Side::Right, 2, 2, "note".into())
            .unwrap();
        assert!(session.comments_json().contains("note"));

        // Cross-side / missing lines are rejected before anything changes.
        assert!(session.add_comment(0, Side::Left, 9, 9, "x".into()).is_err());

        // A fresh session over the same content resumes the draft.
        let mut resumed = Session::from_patch("t", PATCH).unwrap();
        assert!(resumed.attach_store(&dir).is_none());
        assert_eq!(resumed.draft().comments.len(), 1);
        assert_eq!(resumed.draft().comments[0].local_id, id);
        assert_eq!(resumed.draft().comments[0].author, "me");

        resumed.delete_comment(&id).unwrap();
        let empty = Session::from_patch("t", PATCH)
            .map(|mut s| {
                s.attach_store(&dir);
                s
            })
            .unwrap();
        assert!(empty.draft().comments.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_includes_comment() {
        let mut session = Session::from_patch("t", PATCH).unwrap();
        session.add_comment(0, Side::Right, 2, 2, "note".into()).unwrap();
        let markdown = session.export_markdown();
        assert!(markdown.contains("## x.rs"));
        assert!(markdown.contains("> note"));
    }
}
