//! Local git sources: the working tree, the index, a base ref, a range.
//!
//! Prchum is a review client, not a git client — the installed `git` binary
//! answers every repository question; this module only shapes the questions
//! and keys the answers so drafts resume per comparison.

use std::process::Command;

use crate::util::fnv64_hex;

/// What to compare, leanreview's matrix.
#[derive(Clone, Debug, PartialEq)]
pub enum GitSpec {
    /// Working tree vs HEAD.
    WorkingTree,
    /// Index vs HEAD (`--staged`).
    Staged,
    /// `<base>...HEAD` (merge-base comparison).
    Base(String),
    /// Explicit revision range.
    Range(String, String),
}

impl GitSpec {
    /// The human title for the comparison (also part of the source key, so
    /// `--staged` and `--base main` keep separate drafts).
    pub fn title(&self) -> String {
        match self {
            Self::WorkingTree => "working tree".to_string(),
            Self::Staged => "staged".to_string(),
            Self::Base(base) => format!("{base}...HEAD"),
            Self::Range(a, b) => format!("{a}..{b}"),
        }
    }

    fn diff_args(&self, context: u32) -> Vec<String> {
        let mut args = vec!["diff".to_string(), format!("-U{context}")];
        match self {
            Self::WorkingTree => {}
            Self::Staged => args.push("--cached".to_string()),
            Self::Base(base) => args.push(format!("{base}...HEAD")),
            Self::Range(a, b) => {
                args.push(a.clone());
                args.push(b.clone());
            }
        }
        args
    }
}

/// The material a git comparison yields, ready for a session.
pub struct GitDiff {
    pub title: String,
    pub patch: String,
    pub source_key: String,
    pub head_oid: String,
    pub repo_root: String,
}

/// Runs the comparison in `repo` (any path inside the repository).
pub fn git_diff(repo: &str, spec: &GitSpec, context: u32) -> Result<GitDiff, String> {
    let repo_root = git_in(repo, &["rev-parse", "--show-toplevel"])?
        .trim()
        .to_string();
    if repo_root.is_empty() {
        return Err(format!("{repo} is not inside a git repository"));
    }
    let head_oid = git_in(repo, &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let args = spec.diff_args(context);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let patch = git_in(repo, &arg_refs)?;
    if patch.trim().is_empty() {
        return Err(format!("no changes for {}", spec.title()));
    }

    let root_name = std::path::Path::new(&repo_root)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| repo_root.clone());
    let spec_title = spec.title();
    Ok(GitDiff {
        title: format!("{root_name}: {spec_title}"),
        patch,
        source_key: format!(
            "git-{}-{}",
            fnv64_hex(repo_root.as_bytes()),
            fnv64_hex(spec_title.as_bytes())
        ),
        head_oid,
        repo_root,
    })
}

/// Runs git in `repo`, returning stdout; a nonzero exit is an error carrying
/// stderr.
pub fn git_in(repo: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| format!("could not run git: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {} failed: {}", args.join(" "), stderr.trim()));
    }
    String::from_utf8(output.stdout).map_err(|_| "git output was not UTF-8".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a throwaway repository with one committed file and a change.
    fn scratch_repo() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "prchum-git-{}-{}",
            std::process::id(),
            crate::util::new_local_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let repo = dir.to_string_lossy().to_string();
        let run = |args: &[&str]| git_in(&repo, args).unwrap();
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.name", "Test"]);
        run(&["config", "user.email", "test@example.com"]);
        std::fs::write(dir.join("file.txt"), "one\ntwo\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
        std::fs::write(dir.join("file.txt"), "one\nchanged\n").unwrap();
        dir
    }

    #[test]
    fn worktree_and_staged_diffs() {
        let dir = scratch_repo();
        let repo = dir.to_string_lossy().to_string();

        let worktree = git_diff(&repo, &GitSpec::WorkingTree, 3).unwrap();
        assert!(worktree.patch.contains("+changed"));
        assert!(worktree.source_key.starts_with("git-"));
        assert!(!worktree.head_oid.is_empty());

        // Nothing staged yet.
        assert!(git_diff(&repo, &GitSpec::Staged, 3).is_err());
        git_in(&repo, &["add", "."]).unwrap();
        let staged = git_diff(&repo, &GitSpec::Staged, 3).unwrap();
        assert!(staged.patch.contains("+changed"));
        // Different comparisons keep different drafts.
        assert_ne!(worktree.source_key, staged.source_key);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn base_comparison() {
        let dir = scratch_repo();
        let repo = dir.to_string_lossy().to_string();
        git_in(&repo, &["checkout", "-q", "-b", "feature"]).unwrap();
        git_in(&repo, &["add", "."]).unwrap();
        git_in(&repo, &["commit", "-q", "-m", "change"]).unwrap();

        let diff = git_diff(&repo, &GitSpec::Base("main".into()), 3).unwrap();
        assert!(diff.patch.contains("+changed"));
        assert!(diff.title.contains("main...HEAD"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn not_a_repo_is_an_error() {
        assert!(git_diff("/", &GitSpec::WorkingTree, 3).is_err());
    }
}
