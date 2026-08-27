//! Pull/merge-request reference parsing and origin inference.

use prchum_core::source::git_in;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgeKind {
    GitHub,
    GitLab,
    Forgejo,
}

/// A fully or partially resolved request reference. Empty host/owner/repo
/// mean "infer from the repository's origin".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PullRequestRef {
    pub host: String,
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

impl PullRequestRef {
    pub fn is_resolved(&self) -> bool {
        !self.host.is_empty() && !self.owner.is_empty() && !self.repo.is_empty()
    }

    /// The browser URL for this request.
    pub fn web_url(&self, kind: ForgeKind) -> String {
        match kind {
            ForgeKind::GitHub => format!(
                "https://{}/{}/{}/pull/{}",
                self.host, self.owner, self.repo, self.number
            ),
            ForgeKind::GitLab => format!(
                "https://{}/{}/{}/-/merge_requests/{}",
                self.host, self.owner, self.repo, self.number
            ),
            ForgeKind::Forgejo => format!(
                "https://{}/{}/{}/pulls/{}",
                self.host, self.owner, self.repo, self.number
            ),
        }
    }

    /// A permalink to `path` as it stands at `oid`, anchored at `line`.
    ///
    /// Deliberately a blob permalink rather than a link into the diff
    /// view: the anchors a forge uses on a pull request's Files tab are
    /// derived from the path in ways that differ per host and change
    /// when the request is updated, whereas a blob at an explicit commit
    /// resolves for whoever you send it to, forever.
    pub fn blob_url(
        &self,
        kind: ForgeKind,
        oid: &str,
        path: &str,
        line: Option<u32>,
    ) -> String {
        let base = match kind {
            ForgeKind::GitHub => format!(
                "https://{}/{}/{}/blob/{}/{}",
                self.host, self.owner, self.repo, oid, path
            ),
            ForgeKind::GitLab => format!(
                "https://{}/{}/{}/-/blob/{}/{}",
                self.host, self.owner, self.repo, oid, path
            ),
            ForgeKind::Forgejo => format!(
                "https://{}/{}/{}/src/commit/{}/{}",
                self.host, self.owner, self.repo, oid, path
            ),
        };
        match line {
            // Every one of the three spells the anchor the same way,
            // which is the one mercy in this.
            Some(line) => format!("{base}#L{line}"),
            None => base,
        }
    }

    /// The request's own Files tab, for sharing the review rather than a
    /// particular line.
    pub fn files_url(&self, kind: ForgeKind) -> String {
        match kind {
            ForgeKind::GitHub => format!("{}/files", self.web_url(kind)),
            ForgeKind::GitLab => format!("{}/diffs", self.web_url(kind)),
            ForgeKind::Forgejo => format!("{}/files", self.web_url(kind)),
        }
    }
}

/// Parses every accepted spelling:
/// URLs (`https://host/o/r/pull/N`, `https://host/g/sub/r/-/merge_requests/N`),
/// `owner/repo#N`, `group/sub/repo!N`, and bare `N` / `#N` / `!N`.
pub fn parse_ref(text: &str) -> Option<PullRequestRef> {
    let text = text.trim();

    if let Some(rest) = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
    {
        return parse_url(rest);
    }

    // Bare numbers, with or without the #/! marker.
    let bare = text.strip_prefix('#').or_else(|| text.strip_prefix('!')).unwrap_or(text);
    if let Ok(number) = bare.parse::<u64>() {
        if number > 0 {
            return Some(PullRequestRef {
                number,
                ..Default::default()
            });
        }
    }

    // owner/repo#N or group/sub/repo!N.
    for marker in ['#', '!'] {
        if let Some((path, number)) = text.rsplit_once(marker) {
            let number: u64 = number.parse().ok()?;
            let (owner, repo) = path.rsplit_once('/')?;
            if owner.is_empty() || repo.is_empty() {
                return None;
            }
            return Some(PullRequestRef {
                host: String::new(),
                owner: owner.to_string(),
                repo: repo.to_string(),
                number,
            });
        }
    }
    None
}

fn parse_url(rest: &str) -> Option<PullRequestRef> {
    let (host, path) = rest.split_once('/')?;
    let path = path.trim_end_matches('/');

    // GitLab: g/sub/r/-/merge_requests/N
    if let Some((prefix, number)) = path.split_once("/-/merge_requests/") {
        let number: u64 = number.split('/').next()?.parse().ok()?;
        let (owner, repo) = prefix.rsplit_once('/')?;
        return Some(PullRequestRef {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        });
    }

    // GitHub: o/r/pull/N
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() >= 4 && (segments[2] == "pull" || segments[2] == "pulls") {
        let number: u64 = segments[3].parse().ok()?;
        return Some(PullRequestRef {
            host: host.to_string(),
            owner: segments[0].to_string(),
            repo: segments[1].to_string(),
            number,
        });
    }
    None
}

/// Fills host/owner/repo from the repository's `origin` remote.
pub fn resolve_from_origin(reference: &mut PullRequestRef, repo_path: &str) -> Result<(), String> {
    if reference.is_resolved() {
        return Ok(());
    }
    let origin = git_in(repo_path, &["config", "--get", "remote.origin.url"])
        .map_err(|_| "no origin remote to infer the repository from".to_string())?;
    let (host, owner, repo) = parse_remote(origin.trim())
        .ok_or_else(|| format!("could not parse origin remote {}", origin.trim()))?;
    if reference.host.is_empty() {
        reference.host = host;
    }
    if reference.owner.is_empty() {
        reference.owner = owner;
        reference.repo = repo;
    }
    Ok(())
}

/// Parses `git@host:owner/repo.git`, `ssh://git@host/owner/repo`, and
/// `https://host/owner/repo(.git)` — nested GitLab groups keep their path.
fn parse_remote(url: &str) -> Option<(String, String, String)> {
    let stripped = url.strip_suffix(".git").unwrap_or(url);
    if let Some(rest) = stripped
        .strip_prefix("https://")
        .or_else(|| stripped.strip_prefix("http://"))
        .or_else(|| stripped.strip_prefix("ssh://git@"))
        .or_else(|| stripped.strip_prefix("ssh://"))
    {
        let (host, path) = rest.split_once('/')?;
        let (owner, repo) = path.rsplit_once('/')?;
        return Some((host.to_string(), owner.to_string(), repo.to_string()));
    }
    if let Some(rest) = stripped.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        let (owner, repo) = path.rsplit_once('/')?;
        return Some((host.to_string(), owner.to_string(), repo.to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_permalink_names_a_commit_not_a_branch() {
        let reference = PullRequestRef {
            host: "github.com".to_string(),
            owner: "ada".to_string(),
            repo: "gambit".to_string(),
            number: 7,
        };
        assert_eq!(
            reference.blob_url(ForgeKind::GitHub, "abc123", "src/board.rs", Some(18)),
            "https://github.com/ada/gambit/blob/abc123/src/board.rs#L18"
        );
        // Without a line it is still a link to the file.
        assert_eq!(
            reference.blob_url(ForgeKind::GitHub, "abc123", "src/board.rs", None),
            "https://github.com/ada/gambit/blob/abc123/src/board.rs"
        );
    }

    #[test]
    fn each_forge_spells_the_path_its_own_way() {
        let reference = PullRequestRef {
            host: "example.org".to_string(),
            owner: "team".to_string(),
            repo: "thing".to_string(),
            number: 3,
        };
        assert!(reference
            .blob_url(ForgeKind::GitLab, "deadbeef", "a.rs", Some(2))
            .contains("/-/blob/deadbeef/a.rs#L2"));
        assert!(reference
            .blob_url(ForgeKind::Forgejo, "deadbeef", "a.rs", Some(2))
            .contains("/src/commit/deadbeef/a.rs#L2"));

        assert!(reference.files_url(ForgeKind::GitHub).ends_with("/pull/3/files"));
        assert!(reference.files_url(ForgeKind::GitLab).ends_with("/merge_requests/3/diffs"));
        assert!(reference.files_url(ForgeKind::Forgejo).ends_with("/pulls/3/files"));
    }

    use super::*;

    #[test]
    fn parses_all_spellings() {
        assert_eq!(
            parse_ref("https://github.com/owner/repo/pull/418").unwrap(),
            PullRequestRef {
                host: "github.com".into(),
                owner: "owner".into(),
                repo: "repo".into(),
                number: 418
            }
        );
        assert_eq!(
            parse_ref("https://gitlab.com/group/sub/repo/-/merge_requests/42").unwrap(),
            PullRequestRef {
                host: "gitlab.com".into(),
                owner: "group/sub".into(),
                repo: "repo".into(),
                number: 42
            }
        );
        assert_eq!(
            parse_ref("owner/repo#7").unwrap(),
            PullRequestRef {
                host: String::new(),
                owner: "owner".into(),
                repo: "repo".into(),
                number: 7
            }
        );
        assert_eq!(
            parse_ref("group/sub/repo!9").unwrap().owner,
            "group/sub".to_string()
        );
        assert_eq!(parse_ref("418").unwrap().number, 418);
        assert_eq!(parse_ref("#418").unwrap().number, 418);
        assert_eq!(parse_ref("!42").unwrap().number, 42);
        assert!(parse_ref("not a ref").is_none());
        assert!(parse_ref("0").is_none());
    }

    #[test]
    fn remote_forms() {
        assert_eq!(
            parse_remote("git@github.com:owner/repo.git").unwrap(),
            ("github.com".into(), "owner".into(), "repo".into())
        );
        assert_eq!(
            parse_remote("https://gitlab.com/group/sub/repo.git").unwrap(),
            ("gitlab.com".into(), "group/sub".into(), "repo".into())
        );
        assert_eq!(
            parse_remote("ssh://git@github.example.com/owner/repo").unwrap(),
            ("github.example.com".into(), "owner".into(), "repo".into())
        );
        assert!(parse_remote("/local/path").is_none());
    }

    #[test]
    fn host_dispatch() {
        use crate::kind_for_host;
        assert_eq!(kind_for_host("github.com", None), ForgeKind::GitHub);
        assert_eq!(kind_for_host("github.corp.example", None), ForgeKind::GitHub);
        assert_eq!(kind_for_host("gitlab.com", None), ForgeKind::GitLab);
        assert_eq!(kind_for_host("gitlab.corp.example", None), ForgeKind::GitLab);
        assert_eq!(kind_for_host("codeberg.org", None), ForgeKind::Forgejo);
        assert_eq!(kind_for_host("forgejo.corp.example", None), ForgeKind::Forgejo);
        // Self-hosted instances with opaque names need the config override.
        assert_eq!(kind_for_host("git.corp.example", None), ForgeKind::GitHub);
        assert_eq!(
            kind_for_host("git.corp.example", Some("forgejo")),
            ForgeKind::Forgejo
        );
        assert_eq!(
            kind_for_host("git.corp.example", Some("gitlab")),
            ForgeKind::GitLab
        );
    }
}
