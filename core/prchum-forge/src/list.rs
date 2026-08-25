//! Discovery: the open requests waiting for the user's review.

use serde::Serialize;
use serde_json::Value;

use crate::ghcli::Runner;

/// One request in the review queue.
#[derive(Clone, Debug, Serialize)]
pub struct ListedRequest {
    pub host: String,
    pub owner: String,
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub author: String,
    pub updated_at: String,
    pub url: String,
}

/// GitHub's default queue: PRs waiting on the user.
pub const GITHUB_DEFAULT_FILTER: &str = "is:open review-requested:@me";

/// Lists via `gh search prs`. `filter` is a GitHub search query; empty
/// uses the default. Splits on whitespace but keeps double-quoted spans
/// intact so `label:"needs review"` stays one qualifier.
pub fn list_github(runner: &dyn Runner, filter: &str) -> Result<Vec<ListedRequest>, String> {
    let filter = if filter.is_empty() {
        GITHUB_DEFAULT_FILTER
    } else {
        filter
    };
    let mut args = vec!["search".to_string(), "prs".to_string()];
    args.extend(split_query(filter));
    args.extend(
        [
            "--json",
            "number,title,author,repository,updatedAt,url",
            "--limit",
            "30",
        ]
        .map(str::to_string),
    );
    let text = runner.run("gh", &args, None)?;
    let items: Value =
        serde_json::from_str(&text).map_err(|e| format!("unexpected gh output: {e}"))?;
    let mut requests = Vec::new();
    for item in items.as_array().map(Vec::as_slice).unwrap_or_default() {
        let full_name = item["repository"]["nameWithOwner"]
            .as_str()
            .unwrap_or_default();
        let (owner, repo) = full_name.rsplit_once('/').unwrap_or(("", full_name));
        let url = item["url"].as_str().unwrap_or_default().to_string();
        let host = url
            .strip_prefix("https://")
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("github.com")
            .to_string();
        requests.push(ListedRequest {
            host,
            owner: owner.to_string(),
            repo: repo.to_string(),
            number: item["number"].as_u64().unwrap_or(0),
            title: item["title"].as_str().unwrap_or_default().to_string(),
            author: item["author"]["login"].as_str().unwrap_or_default().to_string(),
            updated_at: item["updatedAt"].as_str().unwrap_or_default().to_string(),
            url,
        });
    }
    Ok(requests)
}

/// Lists via Forgejo's issue search (`type=pulls`, review requested).
/// `filter` is extra query-string qualifiers joined with `&`.
pub fn list_forgejo(
    forge: &crate::forgejo::ForgejoForge<impl Runner>,
    host: &str,
    filter: &str,
) -> Result<Vec<ListedRequest>, String> {
    let mut query = "type=pulls&state=open&review_requested=true".to_string();
    if !filter.is_empty() {
        query.push('&');
        query.push_str(filter);
    }
    let text = forge.raw_request(host, "GET", &format!("/repos/issues/search?{query}"))?;
    let items: Value =
        serde_json::from_str(&text).map_err(|e| format!("unexpected forgejo output: {e}"))?;
    let mut requests = Vec::new();
    for item in items.as_array().map(Vec::as_slice).unwrap_or_default() {
        // repository.full_name is "owner/repo".
        let full_name = item["repository"]["full_name"].as_str().unwrap_or_default();
        let (owner, repo) = full_name.rsplit_once('/').unwrap_or(("", full_name));
        requests.push(ListedRequest {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            number: item["number"].as_u64().unwrap_or(0),
            title: item["title"].as_str().unwrap_or_default().to_string(),
            author: item["user"]["login"].as_str().unwrap_or_default().to_string(),
            updated_at: item["updated_at"].as_str().unwrap_or_default().to_string(),
            url: item["html_url"].as_str().unwrap_or_default().to_string(),
        });
    }
    Ok(requests)
}

/// Whitespace split that keeps double-quoted spans intact.
fn split_query(query: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in query.chars() {
        match character {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ghcli::Runner;
    use std::sync::Mutex;

    struct FakeRunner {
        calls: Mutex<Vec<Vec<String>>>,
        response: String,
    }

    impl Runner for FakeRunner {
        fn run(
            &self,
            _program: &str,
            args: &[String],
            _stdin: Option<&[u8]>,
        ) -> Result<String, String> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(self.response.clone())
        }
    }

    #[test]
    fn github_listing_parses_and_quotes_survive() {
        let runner = FakeRunner {
            calls: Mutex::new(Vec::new()),
            response: r#"[{
                "number": 7, "title": "T", "url": "https://github.com/o/r/pull/7",
                "updatedAt": "2026-01-01T00:00:00Z",
                "author": {"login": "alice"},
                "repository": {"nameWithOwner": "o/r"}
            }]"#
            .to_string(),
        };
        let requests =
            list_github(&runner, r#"is:open label:"needs review""#).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].owner, "o");
        assert_eq!(requests[0].number, 7);
        assert_eq!(requests[0].host, "github.com");
        let calls = runner.calls.lock().unwrap();
        assert!(calls[0].contains(&"label:needs review".to_string()), "{:?}", calls[0]);
    }

    #[test]
    fn default_filter_applies() {
        let runner = FakeRunner {
            calls: Mutex::new(Vec::new()),
            response: "[]".to_string(),
        };
        list_github(&runner, "").unwrap();
        let calls = runner.calls.lock().unwrap();
        assert!(calls[0].contains(&"review-requested:@me".to_string()));
    }
}
