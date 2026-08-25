//! The reviewer's side of a session: draft comments and their persistence.
//!
//! Rules inherited from leanreview, load-bearing:
//! * Dismissed is not deleted — the verdict is information for the other
//!   side of a conversation, so it is kept and never submitted.
//! * Orphaned comments are kept and never submitted; a human repositions.
//! * Drafts persist per stable source key, atomically, on every change.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::location::Location;
use crate::util::{new_local_id, rfc3339_now};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftState {
    Active,
    Stale,
    Orphaned,
    Dismissed,
}

/// A reply travelling with a comment (the exchange conversation), as
/// opposed to a reply to a host thread (`reply_to`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewReply {
    pub author: String,
    pub body: String,
    pub at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DraftComment {
    pub local_id: String,
    pub location: Location,
    pub body: String,
    /// Verbatim code the range covered when the comment was made.
    pub snippet: String,
    pub state: DraftState,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub at: String,
    /// Host comment id this replies to (PR mode); posted individually.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<ReviewReply>,
}

/// A conversation-level (not line-anchored) draft, PR mode.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeneralDraft {
    pub local_id: String,
    pub body: String,
    pub at: String,
}

/// The submission event, GitHub vocabulary (GitLab maps onto it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewEvent {
    #[serde(rename = "COMMENT")]
    Comment,
    #[serde(rename = "APPROVE")]
    Approve,
    #[serde(rename = "REQUEST_CHANGES")]
    RequestChanges,
}

impl Default for ReviewEvent {
    fn default() -> Self {
        Self::Comment
    }
}

/// Everything the reviewer has drafted for one source.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DraftReview {
    #[serde(default)]
    pub source_key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub head_oid: String,
    #[serde(default)]
    pub comments: Vec<DraftComment>,
    #[serde(default)]
    pub general: Vec<GeneralDraft>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub event: ReviewEvent,
}

impl DraftReview {
    pub fn comment(&self, local_id: &str) -> Option<&DraftComment> {
        self.comments.iter().find(|c| c.local_id == local_id)
    }

    pub fn comment_mut(&mut self, local_id: &str) -> Option<&mut DraftComment> {
        self.comments.iter_mut().find(|c| c.local_id == local_id)
    }

    /// Creates and appends a comment; the caller has already built and
    /// validated the location.
    pub fn add_comment(
        &mut self,
        location: Location,
        snippet: String,
        body: String,
        author: &str,
    ) -> String {
        let local_id = new_local_id();
        self.comments.push(DraftComment {
            local_id: local_id.clone(),
            location,
            body,
            snippet,
            state: DraftState::Active,
            author: author.to_string(),
            at: rfc3339_now(),
            reply_to: None,
            replies: Vec::new(),
        });
        local_id
    }

    pub fn add_reply(&mut self, local_id: &str, body: String, author: &str) -> bool {
        let Some(comment) = self.comment_mut(local_id) else {
            return false;
        };
        comment.replies.push(ReviewReply {
            author: author.to_string(),
            body,
            at: rfc3339_now(),
        });
        true
    }

    /// Rewrites one reply's body (authors and timestamps stay — editing
    /// polishes, it doesn't re-attribute).
    pub fn update_reply(&mut self, local_id: &str, index: usize, body: String) -> bool {
        let Some(comment) = self.comment_mut(local_id) else {
            return false;
        };
        match comment.replies.get_mut(index) {
            Some(reply) => {
                reply.body = body;
                true
            }
            None => false,
        }
    }

    pub fn delete_reply(&mut self, local_id: &str, index: usize) -> bool {
        let Some(comment) = self.comment_mut(local_id) else {
            return false;
        };
        if index >= comment.replies.len() {
            return false;
        }
        comment.replies.remove(index);
        true
    }

    pub fn update_comment(&mut self, local_id: &str, body: String) -> bool {
        match self.comment_mut(local_id) {
            Some(comment) => {
                comment.body = body;
                true
            }
            None => false,
        }
    }

    pub fn delete_comment(&mut self, local_id: &str) -> bool {
        let before = self.comments.len();
        self.comments.retain(|c| c.local_id != local_id);
        self.comments.len() != before
    }

    /// Dismiss ↔ restore. Kept, never submitted while dismissed.
    pub fn toggle_dismiss(&mut self, local_id: &str) -> bool {
        match self.comment_mut(local_id) {
            Some(comment) => {
                comment.state = match comment.state {
                    DraftState::Dismissed => DraftState::Active,
                    _ => DraftState::Dismissed,
                };
                true
            }
            None => false,
        }
    }

    pub fn add_general(&mut self, body: String) -> String {
        let local_id = new_local_id();
        self.general.push(GeneralDraft {
            local_id: local_id.clone(),
            body,
            at: rfc3339_now(),
        });
        local_id
    }

    pub fn delete_general(&mut self, local_id: &str) -> bool {
        let before = self.general.len();
        self.general.retain(|g| g.local_id != local_id);
        self.general.len() != before
    }
}

/// One JSON file per source key, written atomically.
pub struct DraftStore {
    dir: PathBuf,
}

impl DraftStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path_for(&self, source_key: &str) -> PathBuf {
        self.dir.join(format!("{source_key}.json"))
    }

    /// Missing file is `Ok(None)`; a malformed file is an error (the file
    /// is left in place for inspection, never clobbered by load).
    pub fn load(&self, source_key: &str) -> Result<Option<DraftReview>, String> {
        let path = self.path_for(source_key);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("could not read {}: {error}", path.display())),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|error| format!("malformed draft {}: {error}", path.display()))
    }

    pub fn save(&self, draft: &DraftReview) -> Result<(), String> {
        if draft.source_key.is_empty() {
            return Err("draft has no source key".to_string());
        }
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| format!("could not create {}: {error}", self.dir.display()))?;
        let path = self.path_for(&draft.source_key);
        let json = serde_json::to_string_pretty(draft)
            .map_err(|error| format!("could not encode draft: {error}"))?;
        atomic_write(&path, json.as_bytes())
    }

    pub fn discard(&self, source_key: &str) -> Result<(), String> {
        let path = self.path_for(source_key);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("could not remove {}: {error}", path.display())),
        }
    }
}

/// Write-temp-then-rename so a crash never leaves a half-written file.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, data)
        .map_err(|error| format!("could not write {}: {error}", temp.display()))?;
    std::fs::rename(&temp, path)
        .map_err(|error| format!("could not rename into {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{parse, Side};
    use crate::location::build_location;

    fn location() -> Location {
        let files = parse("--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,2 @@\n context\n-a\n+b\n", 4)
            .unwrap();
        build_location(&files[0], Side::Right, 2, 2).unwrap()
    }

    #[test]
    fn comment_lifecycle() {
        let mut draft = DraftReview::default();
        let id = draft.add_comment(location(), "b".into(), "note".into(), "me");
        assert_eq!(draft.comments.len(), 1);
        assert!(!draft.comment(&id).unwrap().at.is_empty());

        assert!(draft.update_comment(&id, "edited".into()));
        assert_eq!(draft.comment(&id).unwrap().body, "edited");

        assert!(draft.toggle_dismiss(&id));
        assert_eq!(draft.comment(&id).unwrap().state, DraftState::Dismissed);
        assert!(draft.toggle_dismiss(&id));
        assert_eq!(draft.comment(&id).unwrap().state, DraftState::Active);

        assert!(draft.add_reply(&id, "reply".into(), "other"));
        assert_eq!(draft.comment(&id).unwrap().replies.len(), 1);

        assert!(draft.delete_comment(&id));
        assert!(draft.comments.is_empty());
        assert!(!draft.delete_comment(&id));
    }

    #[test]
    fn store_round_trip() {
        let dir = std::env::temp_dir().join(format!("prchum-store-{}", std::process::id()));
        let store = DraftStore::new(&dir);
        let mut draft = DraftReview {
            source_key: "test-key".into(),
            title: "t".into(),
            ..Default::default()
        };
        draft.add_comment(location(), "b".into(), "note".into(), "me");
        store.save(&draft).unwrap();

        let loaded = store.load("test-key").unwrap().unwrap();
        assert_eq!(loaded.comments.len(), 1);
        assert_eq!(loaded.comments[0].body, "note");
        assert_eq!(loaded.comments[0].location.start_line, 2);

        assert!(store.load("missing").unwrap().is_none());
        store.discard("test-key").unwrap();
        assert!(store.load("test-key").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_draft_is_an_error_not_a_clobber() {
        let dir = std::env::temp_dir().join(format!("prchum-store-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.json");
        std::fs::write(&path, "{ broken").unwrap();
        let store = DraftStore::new(&dir);
        assert!(store.load("bad").is_err());
        // The broken file is still there.
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
