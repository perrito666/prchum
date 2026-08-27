//! Per-request worktrees: a local checkout of the branch under review.
//!
//! Reviewing a pull request often ends in "let me just fix this" — which
//! needs the branch on disk. Prchum keeps a worktree per request beside
//! its other state, created from a clone the user configured, so the
//! working checkout is never disturbed.
//!
//! Ownership is the load-bearing rule: a worktree that already existed
//! (the branch is checked out in the clone, or in a worktree the user
//! made) is **used, never managed** — prchum records only what it created
//! itself, and only those are ever removed when the request is finished.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::review::atomic_write;
use crate::source::git_in;

/// A worktree the review can edit in.
#[derive(Clone, Debug, Serialize)]
pub struct WorktreeInfo {
    pub path: String,
    /// The branch checked out there, or empty when detached.
    pub branch: String,
    /// True when prchum created it — the only ones it may remove.
    pub created: bool,
}

/// One managed worktree, keyed by the session's source key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManagedWorktree {
    pub key: String,
    pub clone: String,
    pub path: String,
    pub branch: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Registry {
    #[serde(default)]
    entries: Vec<ManagedWorktree>,
}

fn registry_path(dir: &str) -> PathBuf {
    Path::new(dir).join("worktrees.json")
}

fn load_registry(dir: &str) -> Registry {
    std::fs::read_to_string(registry_path(dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_registry(dir: &str, registry: &Registry) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("could not create {dir}: {error}"))?;
    let json = serde_json::to_string_pretty(registry)
        .map_err(|error| format!("could not encode the worktree registry: {error}"))?;
    atomic_write(&registry_path(dir), json.as_bytes())
}

/// Every worktree prchum manages.
pub fn managed(dir: &str) -> Vec<ManagedWorktree> {
    load_registry(dir).entries
}

/// The worktrees a clone already has: `(path, branch)`, branch empty when
/// detached. The main checkout is included — a branch checked out there
/// is checked out, and that is what matters.
fn existing_worktrees(clone: &str) -> Result<Vec<(String, String)>, String> {
    let text = git_in(clone, &["worktree", "list", "--porcelain"])?;
    let mut found = Vec::new();
    let mut path = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            path = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("branch ") {
            let branch = rest.strip_prefix("refs/heads/").unwrap_or(rest).to_string();
            found.push((std::mem::take(&mut path), branch));
        } else if line.is_empty() && !path.is_empty() {
            // A detached worktree: recorded with no branch.
            found.push((std::mem::take(&mut path), String::new()));
        }
    }
    if !path.is_empty() {
        found.push((path, String::new()));
    }
    Ok(found)
}

/// Finds or creates a worktree of `branch` for the request keyed by `key`.
///
/// * A worktree prchum made earlier for this key is reused, and stays
///   ours.
/// * A worktree that already has the branch checked out (the clone's own
///   checkout included) is reused and never managed.
/// * Otherwise one is created under `dir/worktrees/<key>`: on the local
///   branch when it exists, else on a `prchum/pr-<n>` branch fetched from
///   `fetch_ref` (the forge's request head ref), else detached at
///   `head_oid` — whatever the repository can actually offer.
pub fn ensure(
    dir: &str,
    key: &str,
    clone: &str,
    branch: &str,
    number: u64,
    fetch_ref: &str,
    head_oid: &str,
) -> Result<WorktreeInfo, String> {
    let root = git_in(clone, &["rev-parse", "--show-toplevel"])
        .map_err(|_| format!("{clone} is not a git repository"))?
        .trim()
        .to_string();

    // One we made earlier for this request? Ours takes precedence over
    // the checkout scan below, which would otherwise find this very
    // worktree and report it as somebody else's.
    let registry = load_registry(dir);
    if let Some(entry) = registry.entries.iter().find(|entry| entry.key == key) {
        if Path::new(&entry.path).exists() {
            return Ok(WorktreeInfo {
                path: entry.path.clone(),
                branch: entry.branch.clone(),
                created: true,
            });
        }
    }

    // Already checked out somewhere else? Use it, and leave it alone.
    if !branch.is_empty() {
        for (path, checked_out) in existing_worktrees(&root)? {
            if checked_out == branch && Path::new(&path).exists() {
                return Ok(WorktreeInfo {
                    path,
                    branch: branch.to_string(),
                    created: false,
                });
            }
        }
    }

    let target = Path::new(dir).join("worktrees").join(key);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    // A stale directory from an interrupted run would block `worktree add`.
    if target.exists() {
        let _ = git_in(&root, &["worktree", "remove", "--force", &target.to_string_lossy()]);
        let _ = std::fs::remove_dir_all(&target);
    }
    let target = target.to_string_lossy().to_string();

    let local_branch_exists = !branch.is_empty()
        && git_in(&root, &["rev-parse", "--verify", "--quiet", branch]).is_ok();

    let mut checked_out_branch = String::new();
    if local_branch_exists {
        git_in(&root, &["worktree", "add", &target, branch])?;
        checked_out_branch = branch.to_string();
    } else if !fetch_ref.is_empty() {
        // The request's head, fetched into a branch that is plainly ours.
        let local = format!("prchum/pr-{number}");
        git_in(
            &root,
            &["fetch", "origin", &format!("+{fetch_ref}:refs/heads/{local}")],
        )
        .map_err(|error| format!("could not fetch {fetch_ref} from origin: {error}"))?;
        git_in(&root, &["worktree", "add", &target, &local])?;
        checked_out_branch = local;
    } else if !head_oid.is_empty() {
        git_in(&root, &["worktree", "add", "--detach", &target, head_oid])?;
    } else {
        return Err("nothing to check out: no branch, fetch ref, or commit".to_string());
    }

    let mut registry = load_registry(dir);
    registry.entries.retain(|entry| entry.key != key);
    registry.entries.push(ManagedWorktree {
        key: key.to_string(),
        clone: root,
        path: target.clone(),
        branch: checked_out_branch.clone(),
    });
    save_registry(dir, &registry)?;

    Ok(WorktreeInfo {
        path: target,
        branch: checked_out_branch,
        created: true,
    })
}

/// Removes the worktree prchum made for `key`, if any — the request is
/// finished with. Worktrees prchum did not create are never touched
/// (they are not in the registry). `Ok(false)` means there was none.
pub fn remove_for_key(dir: &str, key: &str) -> Result<bool, String> {
    let mut registry = load_registry(dir);
    let Some(position) = registry.entries.iter().position(|entry| entry.key == key) else {
        return Ok(false);
    };
    let entry = registry.entries.remove(position);
    // Git first, so its bookkeeping goes with the directory; a worktree
    // the user deleted by hand still leaves the registry clean.
    let _ = git_in(&entry.clone, &["worktree", "remove", "--force", &entry.path]);
    if Path::new(&entry.path).exists() {
        let _ = std::fs::remove_dir_all(&entry.path);
    }
    let _ = git_in(&entry.clone, &["worktree", "prune"]);
    save_registry(dir, &registry)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_repo() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "prchum-wt-{}-{}",
            std::process::id(),
            crate::util::new_local_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let repo = dir.to_string_lossy().to_string();
        let run = |args: &[&str]| git_in(&repo, args).unwrap();
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.name", "Test"]);
        run(&["config", "user.email", "test@example.com"]);
        std::fs::write(dir.join("file.txt"), "one\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn creates_reuses_and_removes() {
        let repo = scratch_repo();
        let clone = repo.to_string_lossy().to_string();
        git_in(&clone, &["branch", "feature"]).unwrap();
        let state = repo.join("state").to_string_lossy().to_string();

        // Created on the local branch, recorded as ours.
        let first = ensure(&state, "gh-pr1", &clone, "feature", 1, "", "").unwrap();
        assert!(first.created);
        assert_eq!(first.branch, "feature");
        assert!(Path::new(&first.path).join("file.txt").exists());
        assert_eq!(managed(&state).len(), 1);

        // Asking again reuses the same one.
        let second = ensure(&state, "gh-pr1", &clone, "feature", 1, "", "").unwrap();
        assert_eq!(
            std::fs::canonicalize(&second.path).unwrap(),
            std::fs::canonicalize(&first.path).unwrap()
        );
        assert!(second.created, "our own worktree stays ours on reuse");
        assert_eq!(managed(&state).len(), 1);

        // Removal takes the directory and the registry entry.
        assert!(remove_for_key(&state, "gh-pr1").unwrap());
        assert!(!Path::new(&first.path).exists());
        assert!(managed(&state).is_empty());
        assert!(!remove_for_key(&state, "gh-pr1").unwrap());

        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn an_existing_checkout_is_used_never_managed() {
        let repo = scratch_repo();
        let clone = repo.to_string_lossy().to_string();
        let state = repo.join("state").to_string_lossy().to_string();

        // `main` is checked out in the clone itself.
        let info = ensure(&state, "gh-pr2", &clone, "main", 2, "", "").unwrap();
        assert!(!info.created, "an existing checkout must not be managed");
        assert_eq!(
            std::fs::canonicalize(&info.path).unwrap(),
            std::fs::canonicalize(&repo).unwrap()
        );
        assert!(managed(&state).is_empty(), "nothing to clean up later");

        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn detached_fallback_when_only_a_commit_is_known() {
        let repo = scratch_repo();
        let clone = repo.to_string_lossy().to_string();
        let state = repo.join("state").to_string_lossy().to_string();
        let head = git_in(&clone, &["rev-parse", "HEAD"]).unwrap().trim().to_string();

        let info = ensure(&state, "gh-pr3", &clone, "", 3, "", &head).unwrap();
        assert!(info.created);
        assert!(info.branch.is_empty());
        assert!(Path::new(&info.path).join("file.txt").exists());

        let _ = remove_for_key(&state, "gh-pr3");
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn a_bad_clone_is_an_error() {
        assert!(ensure("/tmp/prchum-none", "k", "/", "b", 1, "", "").is_err());
    }
}
