//! The review history: what has been reviewed, so the home screen can
//! offer it back. One JSON file, atomic writes, newest first.
//!
//! Entries leave in two ways: the user deletes one by hand, or a periodic
//! prune notices the pull request is merged, closed, or gone. Network
//! failures never prune — only a definite answer does.

use serde::{Deserialize, Serialize};

use crate::review::atomic_write;
use crate::util::rfc3339_now;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// The session's source key — the identity drafts key on too.
    pub key: String,
    /// `pr` | `patch` | `exchange` | `git`.
    pub kind: String,
    pub title: String,
    /// What to show under the title: a reference, a path, a comparison.
    pub display: String,
    /// How to reopen: a URL for PRs, an absolute path for files, a
    /// `repo\u{1F}kind\u{1F}arg1\u{1F}arg2` spec for git comparisons.
    pub reopen: String,
    /// RFC 3339; the list sorts by it, newest first.
    pub last_opened: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub submitted_at: String,
}

#[derive(Default, Serialize, Deserialize)]
struct HistoryFile {
    #[serde(default)]
    entries: Vec<HistoryEntry>,
}

fn history_path(dir: &str) -> std::path::PathBuf {
    std::path::Path::new(dir).join("history.json")
}

/// Loads the history, newest first. A missing or unreadable file is an
/// empty history — the home screen must never fail to open.
pub fn load(dir: &str) -> Vec<HistoryEntry> {
    let Ok(text) = std::fs::read_to_string(history_path(dir)) else {
        return Vec::new();
    };
    let mut file: HistoryFile = serde_json::from_str(&text).unwrap_or_default();
    file.entries
        .sort_by(|a, b| b.last_opened.cmp(&a.last_opened));
    file.entries
}

fn save(dir: &str, entries: Vec<HistoryEntry>) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("could not create {dir}: {error}"))?;
    let file = HistoryFile { entries };
    let json = serde_json::to_string_pretty(&file)
        .map_err(|error| format!("could not encode history: {error}"))?;
    atomic_write(&history_path(dir), json.as_bytes())
}

/// Records (or refreshes) an entry, keyed by `key`. `submitted` stamps
/// the submission time as well.
pub fn record(dir: &str, mut entry: HistoryEntry, submitted: bool) -> Result<(), String> {
    let mut entries = load(dir);
    entry.last_opened = rfc3339_now();
    if let Some(existing) = entries.iter_mut().find(|e| e.key == entry.key) {
        existing.title = entry.title;
        existing.display = entry.display;
        existing.reopen = entry.reopen;
        existing.last_opened = entry.last_opened;
        if submitted {
            existing.submitted_at = rfc3339_now();
        }
    } else {
        if submitted {
            entry.submitted_at = rfc3339_now();
        }
        entries.push(entry);
    }
    save(dir, entries)
}

/// Removes one entry by key (the user's hand deletion).
pub fn remove(dir: &str, key: &str) -> Result<(), String> {
    let mut entries = load(dir);
    entries.retain(|e| e.key != key);
    save(dir, entries)
}

/// Removes every entry `gone` says is finished (merged, closed, or
/// deleted). The caller supplies the judgement — it needs a forge.
pub fn prune(dir: &str, gone: impl Fn(&HistoryEntry) -> bool) -> Result<Vec<HistoryEntry>, String> {
    let mut entries = load(dir);
    entries.retain(|e| !gone(e));
    save(dir, entries.clone())?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> String {
        let dir = std::env::temp_dir().join(format!(
            "prchum-history-{}-{}",
            std::process::id(),
            crate::util::new_local_id()
        ));
        dir.to_string_lossy().to_string()
    }

    fn entry(key: &str) -> HistoryEntry {
        HistoryEntry {
            key: key.to_string(),
            kind: "pr".to_string(),
            title: format!("title {key}"),
            display: "o/r#1".to_string(),
            reopen: "https://github.com/o/r/pull/1".to_string(),
            last_opened: String::new(),
            submitted_at: String::new(),
        }
    }

    #[test]
    fn record_upserts_and_sorts() {
        let dir = scratch();
        record(&dir, entry("a"), false).unwrap();
        record(&dir, entry("b"), false).unwrap();
        // Re-recording refreshes, not duplicates, and floats to the top.
        record(&dir, entry("a"), true).unwrap();
        let entries = load(&dir);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "a");
        assert!(!entries[0].submitted_at.is_empty());
        assert!(entries[1].submitted_at.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_and_prune() {
        let dir = scratch();
        record(&dir, entry("a"), false).unwrap();
        record(&dir, entry("b"), false).unwrap();
        record(&dir, entry("c"), false).unwrap();
        remove(&dir, "b").unwrap();
        assert_eq!(load(&dir).len(), 2);
        let kept = prune(&dir, |e| e.key == "a").unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].key, "c");
        assert_eq!(load(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_empty() {
        assert!(load("/nonexistent/prchum-history").is_empty());
    }
}
