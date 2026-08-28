//! What a command-line argument asks for.
//!
//! `prchum` is meant to be reached for the way `git diff` is, so the
//! arguments read like git's: nothing means the working tree, a revision
//! means "since that", `a..b` means a range. It also still opens a patch
//! file or a pull request, because those are the other two things a
//! reviewer arrives with.
//!
//! Both shells parse the same way, from here, so `prchum main` cannot
//! come to mean one thing on a Mac and another on Linux.

use crate::source::GitSpec;

/// What the argument named.
///
/// Serialized for the FFI so the Swift shell reads the same decision
/// rather than making its own; `kind` says which arm, and the fields it
/// carries follow.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    /// The home screen: no argument at all.
    Home,
    /// A patch, or an exchange document, at this path.
    File(String),
    /// A comparison in the repository at this path.
    Git { repo: String, spec: GitSpec },
    /// A pull request, in any spelling `parse_ref` accepts.
    Request(String),
}

/// Whether a string looks like a git revision range rather than a single
/// revision: `a..b`, or `a...b` for the merge-base form git itself uses.
fn split_range(text: &str) -> Option<(String, String)> {
    // Three dots first, or "a...b" would split as ("a", ".b").
    for separator in ["...", ".."] {
        if let Some((left, right)) = text.split_once(separator) {
            if !left.is_empty() && !right.is_empty() && !right.starts_with('.') {
                return Some((left.to_string(), right.to_string()));
            }
        }
    }
    None
}

/// Resolves `argument` against `cwd`, which is where the shell was when
/// the command ran and what a relative path is relative to.
///
/// `exists` answers whether a path is there; injected so the decision is
/// testable without touching a filesystem, and so a shell can answer for
/// a sandbox or a remote if it ever has to.
pub fn parse_with(
    argument: Option<&str>,
    cwd: &str,
    staged: bool,
    exists: &dyn Fn(&str) -> bool,
    is_repo: &dyn Fn(&str) -> bool,
) -> Target {
    let argument = argument.map(str::trim).filter(|text| !text.is_empty());

    // No argument in a repository is the commonest case by far: what
    // `git diff` shows, which is what someone reaching for a diff viewer
    // almost always wants.
    let Some(argument) = argument else {
        if is_repo(cwd) {
            return Target::Git {
                repo: cwd.to_string(),
                spec: if staged { GitSpec::Staged } else { GitSpec::WorkingTree },
            };
        }
        return Target::Home;
    };

    // A path wins over every reading of the same text: a file called
    // `main` in front of you is the file you meant.
    let absolute = if argument.starts_with('/') {
        argument.to_string()
    } else {
        format!("{}/{}", cwd.trim_end_matches('/'), argument)
    };
    for candidate in [argument.to_string(), absolute] {
        if exists(&candidate) {
            if is_repo(&candidate) {
                return Target::Git {
                    repo: candidate,
                    spec: if staged { GitSpec::Staged } else { GitSpec::WorkingTree },
                };
            }
            return Target::File(candidate);
        }
    }

    // A request reference: anything carrying the marks of one, so that a
    // branch named `fix/thing` is not mistaken for `owner/repo`.
    let looks_like_request = argument.contains('#')
        || argument.contains('!')
        || argument.starts_with("http://")
        || argument.starts_with("https://")
        || argument.chars().all(|character| character.is_ascii_digit());
    if looks_like_request {
        return Target::Request(argument.to_string());
    }

    // Otherwise it is a revision, which only means anything in a
    // repository — outside one there is nothing to compare against.
    if is_repo(cwd) {
        let spec = match split_range(argument) {
            Some((from, to)) => GitSpec::Range(from, to),
            None => GitSpec::Base(argument.to_string()),
        };
        return Target::Git { repo: cwd.to_string(), spec };
    }

    Target::Request(argument.to_string())
}

/// True when `path` is inside a git repository, asked of git rather
/// than guessed from a `.git` directory — worktrees and submodules keep
/// theirs somewhere else entirely.
fn is_git_repo(path: &str) -> bool {
    std::process::Command::new("git")
        .args(["-C", path, "rev-parse", "--git-dir"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// [`parse_with`], against the filesystem and git as they actually are.
pub fn parse(argument: Option<&str>, cwd: &str, staged: bool) -> Target {
    parse_with(
        argument,
        cwd,
        staged,
        &|path| std::path::Path::new(path).exists(),
        &is_git_repo,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nothing(_: &str) -> bool {
        false
    }

    fn in_repo(argument: Option<&str>) -> Target {
        parse_with(argument, "/work", false, &nothing, &|path| path == "/work")
    }

    #[test]
    fn nothing_in_a_repository_is_what_git_diff_shows() {
        assert_eq!(
            in_repo(None),
            Target::Git { repo: "/work".to_string(), spec: GitSpec::WorkingTree }
        );
    }

    #[test]
    fn staged_asks_for_the_index() {
        let target = parse_with(None, "/work", true, &nothing, &|path| path == "/work");
        assert_eq!(
            target,
            Target::Git { repo: "/work".to_string(), spec: GitSpec::Staged }
        );
    }

    #[test]
    fn nothing_outside_a_repository_is_the_home_screen() {
        assert_eq!(parse_with(None, "/tmp", false, &nothing, &nothing), Target::Home);
    }

    #[test]
    fn a_revision_compares_against_it() {
        assert_eq!(
            in_repo(Some("main")),
            Target::Git { repo: "/work".to_string(), spec: GitSpec::Base("main".to_string()) }
        );
    }

    #[test]
    fn ranges_are_spelled_the_way_git_spells_them() {
        assert_eq!(
            in_repo(Some("v1..v2")),
            Target::Git {
                repo: "/work".to_string(),
                spec: GitSpec::Range("v1".to_string(), "v2".to_string()),
            }
        );
        // Three dots must not split as ("v1", ".v2").
        assert_eq!(
            in_repo(Some("v1...v2")),
            Target::Git {
                repo: "/work".to_string(),
                spec: GitSpec::Range("v1".to_string(), "v2".to_string()),
            }
        );
    }

    #[test]
    fn a_file_that_exists_beats_every_other_reading() {
        // A branch called `main` and a file called `main` can both exist;
        // the one you can see in front of you is the one you meant.
        let target = parse_with(
            Some("main"),
            "/work",
            false,
            &|path| path == "/work/main",
            &|path| path == "/work",
        );
        assert_eq!(target, Target::File("/work/main".to_string()));
    }

    #[test]
    fn requests_are_recognised_by_their_marks() {
        for reference in ["owner/repo#7", "418", "https://github.com/o/r/pull/7", "g/r!3"] {
            assert_eq!(
                in_repo(Some(reference)),
                Target::Request(reference.to_string()),
                "{reference} should read as a request"
            );
        }
    }

    #[test]
    fn a_branch_with_a_slash_is_not_a_repository_name() {
        // `fix/thing` has a slash like `owner/repo`, but no number and no
        // marker, so it is a revision — which is what it almost always is.
        assert_eq!(
            in_repo(Some("fix/thing")),
            Target::Git {
                repo: "/work".to_string(),
                spec: GitSpec::Base("fix/thing".to_string()),
            }
        );
    }
}
