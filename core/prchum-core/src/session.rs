//! A review session: the parsed diff plus the review state over it.
//!
//! The diff under review is immutable; the mutable part of a session is
//! what the reviewer says about it. Every mutation persists the draft
//! (when a store is attached) — quitting never loses a note.

use crate::diff::{self, FileDiff, ParseError, Side, DEFAULT_TAB_WIDTH};
use crate::exchange;
use crate::export;
use crate::location::{build_location, relocate, snippet, RelocateResult};
use crate::review::{atomic_write, DraftReview, DraftState, DraftStore};
use crate::source::{git_diff, GitSpec};
use crate::util::fnv64_hex;

pub struct Session {
    title: String,
    files: Vec<FileDiff>,
    source_key: String,
    head_oid: String,
    author: String,
    draft: DraftReview,
    store: Option<DraftStore>,
    /// The verbatim patch text the session was parsed from (exchange export
    /// embeds it).
    raw_patch: String,
    /// When set, every save also rewrites this exchange document in place.
    exchange_path: Option<String>,
    /// The exchange document's patch lines, kept verbatim for writeback.
    exchange_patch: Vec<String>,
    /// Existing host threads (PR mode), opaque JSON for the shell.
    threads_json: String,
    /// Pull-request metadata (PR mode), opaque JSON for the shell.
    pr_json: String,
    /// Host conversation-level comments (PR mode), opaque JSON.
    general_json: String,
    /// How the home screen reopens this session: a URL, a path, or a git
    /// spec. Empty for stdin-style sources that cannot reopen.
    reopen_hint: String,
    /// Fetches a file's new-side content by path, for the context view.
    content_provider: Option<Box<dyn Fn(&str) -> Result<String, String> + Send>>,
    /// Context projections, built once per file.
    context_cache: std::collections::HashMap<usize, FileDiff>,
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
    /// resumes the same draft. Exchange documents are detected by content,
    /// never by filename.
    pub fn from_patch_file(path: &str) -> Result<Self, ParseError> {
        let text = std::fs::read_to_string(path).map_err(|error| ParseError {
            message: format!("could not read {path}: {error}"),
        })?;
        if exchange::is_exchange(&text) {
            return Self::from_exchange_file(path);
        }
        let absolute = std::fs::canonicalize(path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string());
        let title = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        let key = format!("patch-{}", fnv64_hex(absolute.as_bytes()));
        let mut session = Self::from_patch_keyed(&title, &text, key)?;
        session.reopen_hint = absolute;
        Ok(session)
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
            head_oid: String::new(),
            author: String::new(),
            draft,
            store: None,
            raw_patch: patch.to_string(),
            exchange_path: None,
            exchange_patch: Vec::new(),
            threads_json: String::new(),
            pr_json: String::new(),
            general_json: String::new(),
            reopen_hint: String::new(),
            content_provider: None,
            context_cache: std::collections::HashMap::new(),
        })
    }

    /// Opens a session over a local git comparison.
    pub fn from_git(repo: &str, spec: &GitSpec, context: u32) -> Result<Self, ParseError> {
        let diff = git_diff(repo, spec, context).map_err(|message| ParseError { message })?;
        let mut session = Self::from_patch_keyed(&diff.title, &diff.patch, diff.source_key)?;
        session.head_oid = diff.head_oid;
        let (kind_tag, arg1, arg2) = match spec {
            GitSpec::WorkingTree => ("worktree", String::new(), String::new()),
            GitSpec::Staged => ("staged", String::new(), String::new()),
            GitSpec::Base(base) => ("base", base.clone(), String::new()),
            GitSpec::Range(a, b) => ("range", a.clone(), b.clone()),
        };
        let sep = '\u{1F}';
        session.reopen_hint =
            format!("{}{sep}{kind_tag}{sep}{arg1}{sep}{arg2}", diff.repo_root);
        let root = diff.repo_root;
        let new_rev = diff.new_rev;
        session.content_provider = Some(Box::new(move |path| {
            crate::source::git_file_content(&root, &new_rev, path)
        }));
        Ok(session)
    }

    /// Opens a session over a review-exchange document. Every save rewrites
    /// the file in place, so quitting leaves the conversation current.
    pub fn from_exchange_file(path: &str) -> Result<Self, ParseError> {
        let text = std::fs::read_to_string(path).map_err(|error| ParseError {
            message: format!("could not read {path}: {error}"),
        })?;
        let doc = exchange::parse(&text).map_err(|message| ParseError { message })?;
        let absolute = std::fs::canonicalize(path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string());
        let title = if doc.title.is_empty() {
            std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string())
        } else {
            doc.title.clone()
        };
        let key = format!("exchange-{}", fnv64_hex(absolute.as_bytes()));
        let mut session = Self::from_patch_keyed(&title, &exchange::patch_text(&doc), key)?;
        session.draft.comments = exchange::to_drafts(&doc, &session.files);
        session.draft.summary = doc.summary.clone();
        session.exchange_patch = doc.patch;
        session.reopen_hint = absolute.clone();
        session.exchange_path = Some(absolute);
        Ok(session)
    }

    /// Is this text an exchange document rather than a plain patch?
    pub fn sniff_exchange(text: &str) -> bool {
        exchange::is_exchange(text)
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
    /// (the file is left in place; the session starts fresh). An exchange
    /// session skips the load — the exchange document is the truth.
    pub fn attach_store(&mut self, dir: &str) -> Option<String> {
        let store = DraftStore::new(dir);
        let warning = if self.exchange_path.is_some() {
            None
        } else {
            match store.load(&self.source_key) {
                Ok(Some(saved)) => {
                    self.draft = saved;
                    // The diff may not be the one the draft was made against.
                    self.draft.source_key = self.source_key.clone();
                    self.draft.title = self.title.clone();
                    None
                }
                Ok(None) => None,
                Err(message) => Some(message),
            }
        };
        self.store = Some(store);

        // The head moved since the draft was saved: re-anchor before the
        // reviewer sees stale positions.
        if !self.head_oid.is_empty()
            && !self.draft.head_oid.is_empty()
            && self.draft.head_oid != self.head_oid
            && !self.draft.comments.is_empty()
        {
            self.relocate_drafts();
        }
        if !self.head_oid.is_empty() {
            self.draft.head_oid = self.head_oid.clone();
            // Don't litter the store with empty drafts for merely-opened
            // sessions; the first real change persists everything.
            if !self.draft.comments.is_empty() || !self.draft.general.is_empty() {
                let _ = self.persist();
            }
        }
        warning
    }

    pub fn head_oid(&self) -> &str {
        &self.head_oid
    }

    pub fn set_head_oid(&mut self, oid: &str) {
        self.head_oid = oid.to_string();
    }

    pub fn threads_json(&self) -> &str {
        &self.threads_json
    }

    pub fn set_threads_json(&mut self, json: String) {
        self.threads_json = json;
    }

    pub fn pr_json(&self) -> &str {
        &self.pr_json
    }

    pub fn set_pr_json(&mut self, json: String) {
        self.pr_json = json;
    }

    pub fn general_json(&self) -> &str {
        &self.general_json
    }

    pub fn set_general_json(&mut self, json: String) {
        self.general_json = json;
    }

    /// Stages a conversation-level comment (posts on submit); persists.
    pub fn add_general(&mut self, body: String) -> Result<String, String> {
        let id = self.draft.add_general(body);
        self.persist()?;
        Ok(id)
    }

    pub fn delete_general(&mut self, local_id: &str) -> Result<(), String> {
        if !self.draft.delete_general(local_id) {
            return Err("no such staged comment".to_string());
        }
        self.persist()
    }

    /// The staged conversation comments as JSON.
    pub fn general_drafts_json(&self) -> String {
        serde_json::to_string(&self.draft.general).unwrap_or_else(|_| "[]".to_string())
    }

    pub fn raw_patch(&self) -> &str {
        &self.raw_patch
    }

    /// The source kind, from the key's prefix.
    pub fn kind(&self) -> &'static str {
        for prefix in ["gh", "fj"] {
            if self.source_key.starts_with(prefix) {
                return "pr";
            }
        }
        if self.source_key.starts_with("git-") {
            "git"
        } else if self.source_key.starts_with("exchange-") {
            "exchange"
        } else {
            "patch"
        }
    }

    pub fn reopen_hint(&self) -> &str {
        &self.reopen_hint
    }

    pub fn set_reopen_hint(&mut self, hint: &str) {
        self.reopen_hint = hint.to_string();
    }

    /// Installs the function the context view fetches new-side file
    /// content with (PR sessions get a forge-backed one from the shell
    /// boundary; git sessions install their own).
    pub fn set_content_provider(
        &mut self,
        provider: Box<dyn Fn(&str) -> Result<String, String> + Send>,
    ) {
        self.content_provider = Some(provider);
    }

    /// The whole-file projection of one file: content fetched through the
    /// provider (once — cached), verified against the diff, hunks
    /// overlaid. See [`crate::context`].
    pub fn context_file(&mut self, file_index: usize) -> Result<&FileDiff, String> {
        if !self.context_cache.contains_key(&file_index) {
            let file = self
                .files
                .get(file_index)
                .ok_or_else(|| "no such file".to_string())?;
            if file.status == crate::diff::FileStatus::Deleted {
                return Err("a deleted file has no new side to show".to_string());
            }
            let provider = self.content_provider.as_ref().ok_or_else(|| {
                "this source has no file content to fetch (patch files carry only the diff)"
                    .to_string()
            })?;
            let content = provider(file.display_path())?;
            let projection =
                crate::context::context_file(file, &content, crate::diff::DEFAULT_TAB_WIDTH)?;
            self.context_cache.insert(file_index, projection);
        }
        Ok(&self.context_cache[&file_index])
    }

    /// Syntax highlights for the context projection — computed over the
    /// projection's own hunks, so gap regions color too. `None` when the
    /// language is unknown.
    pub fn context_highlights(
        &mut self,
        file_index: usize,
    ) -> Result<Option<Vec<Vec<Vec<crate::syntax::LineSpan>>>>, String> {
        let file = self.context_file(file_index)?;
        Ok(crate::syntax::highlight_file(file))
    }

    /// Exports to `path`: a `.json` extension writes a review-exchange
    /// document (embedding the session's patch), anything else Markdown.
    pub fn export_to_file(&self, path: &str) -> Result<(), String> {
        let content = if path.ends_with(".json") {
            let lines: Vec<String> = self
                .raw_patch
                .trim_end_matches('\n')
                .split('\n')
                .map(str::to_string)
                .collect();
            let doc = exchange::from_drafts(
                &self.title,
                &self.draft.summary,
                lines,
                &self.draft.comments,
            );
            exchange::render(&doc)
        } else {
            self.export_markdown()
        };
        atomic_write(std::path::Path::new(path), content.as_bytes())
    }

    /// Re-anchors saved drafts after the head moved: exact matches keep
    /// their place, unique moves follow, everything else is orphaned (kept,
    /// never submitted). Replies to host threads are keyed by comment
    /// identity, not by line, and are skipped. Returns (moved, orphaned).
    pub fn relocate_drafts(&mut self) -> (usize, usize) {
        let mut moved = 0;
        let mut orphaned = 0;
        let files = std::mem::take(&mut self.files);
        for comment in &mut self.draft.comments {
            if comment.reply_to.is_some() || comment.state == DraftState::Dismissed {
                continue;
            }
            let (location, result) = relocate(&files, &comment.location);
            match result {
                RelocateResult::Exact => {
                    if comment.state == DraftState::Orphaned {
                        comment.state = DraftState::Active;
                    }
                }
                RelocateResult::Moved => {
                    comment.location = location;
                    comment.state = DraftState::Active;
                    moved += 1;
                }
                RelocateResult::Orphaned => {
                    comment.state = DraftState::Orphaned;
                    orphaned += 1;
                }
            }
        }
        self.files = files;
        (moved, orphaned)
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

    pub fn update_reply(
        &mut self,
        local_id: &str,
        index: usize,
        body: String,
    ) -> Result<(), String> {
        if !self.draft.update_reply(local_id, index, body) {
            return Err("no such reply".to_string());
        }
        self.persist()
    }

    pub fn delete_reply(&mut self, local_id: &str, index: usize) -> Result<(), String> {
        if !self.draft.delete_reply(local_id, index) {
            return Err("no such reply".to_string());
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
    /// store (tests, previews) simply keeps state in memory. An exchange
    /// session also rewrites its document in place on every save.
    pub fn persist(&self) -> Result<(), String> {
        if let Some(store) = &self.store {
            store.save(&self.draft)?;
        }
        if let Some(path) = &self.exchange_path {
            let doc = exchange::from_drafts(
                &self.title,
                &self.draft.summary,
                self.exchange_patch.clone(),
                &self.draft.comments,
            );
            atomic_write(std::path::Path::new(path), exchange::render(&doc).as_bytes())?;
        }
        Ok(())
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
    fn exchange_session_writes_back_in_place() {
        let dir = std::env::temp_dir().join(format!("prchum-exch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("loop.review.json");
        std::fs::write(
            &path,
            r#"{"leanreview_review": 1, "title": "loop", "patch": ["--- a/x.rs", "+++ b/x.rs", "@@ -1,2 +1,2 @@", " context", "-a", "+b"], "comments": [{"id": "c1", "author": "assistant", "path": "x.rs", "side": "RIGHT", "start_line": 2, "end_line": 2, "body": "why b?", "state": "active"}]}"#,
        )
        .unwrap();

        // Detection is by content: opened as a plain file, it still becomes
        // an exchange session.
        let mut session = Session::from_patch_file(&path.to_string_lossy()).unwrap();
        assert_eq!(session.title(), "loop");
        assert_eq!(session.draft().comments.len(), 1);

        // Triaging rewrites the document in place.
        session.set_author("me");
        session.add_reply("c1", "because a was wrong".into()).unwrap();
        let rewritten = std::fs::read_to_string(&path).unwrap();
        assert!(rewritten.contains("because a was wrong"), "{rewritten}");
        assert!(rewritten.starts_with("{\n  \"leanreview_review\": 1"), "{rewritten}");
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
