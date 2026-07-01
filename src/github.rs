//! Fetching READMEs across an owner's GitHub repositories.
//!
//! This checks the *canonical, published* README each repo shows on GitHub, so
//! badge rot is caught owner-wide without cloning anything. Repo listing and
//! README fetching go through the [`Http`](crate::Http) seam, so the whole path
//! is tested offline.

use crate::Document;
use crate::error::Error;
use crate::fetch::Http;
use base64::Engine as _;
use serde::Deserialize;

const API: &str = "https://api.github.com";
const PER_PAGE: usize = 100;
/// README fetches run concurrently, up to this many at once.
const MAX_WORKERS: usize = 8;

/// Which of an owner's repositories to scan.
#[derive(Debug, Clone)]
pub struct GitHubScope {
    pub owner: String,
    pub include_forks: bool,
    pub include_archived: bool,
    pub include_private: bool,
}

#[derive(Debug, Deserialize)]
struct Repo {
    name: String,
    full_name: String,
    #[serde(default)]
    fork: bool,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    private: bool,
}

#[derive(Debug, Deserialize)]
struct Readme {
    content: String,
    encoding: String,
}

/// Fetch the README of every in-scope repo owned by `scope.owner`.
pub fn fetch_readmes(http: &dyn Http, scope: &GitHubScope) -> Result<Vec<Document>, Error> {
    let repos = list_repos(http, scope)?;
    let outcomes = crate::parallel_map(&repos, MAX_WORKERS, |_, repo| {
        fetch_readme(http, scope, repo)
    });
    // A single failed README fetch aborts the scan rather than silently
    // dropping the repo: a partial owner-wide result must not look complete.
    let mut documents = Vec::new();
    for outcome in outcomes {
        if let Some(document) = outcome? {
            documents.push(document);
        }
    }
    Ok(documents)
}

/// List and filter the owner's repositories, following pagination.
fn list_repos(http: &dyn Http, scope: &GitHubScope) -> Result<Vec<Repo>, Error> {
    let mut repos = Vec::new();
    let mut page = 1;
    loop {
        // Private repos require the authenticated-user endpoint; public scoping
        // uses the owner endpoint, which needs no token.
        let url = if scope.include_private {
            format!("{API}/user/repos?per_page={PER_PAGE}&page={page}&affiliation=owner")
        } else {
            format!(
                "{API}/users/{}/repos?per_page={PER_PAGE}&page={page}&type=owner",
                scope.owner
            )
        };
        let body = http
            .get_github(&url)
            .map_err(|e| Error::GitHub {
                message: format!("listing repositories: {e}"),
            })?
            .ok_or_else(|| Error::GitHub {
                message: format!("owner {:?} not found", scope.owner),
            })?;
        let page_repos: Vec<Repo> = serde_json::from_str(&body).map_err(|e| Error::GitHub {
            message: format!("parsing repository list: {e}"),
        })?;
        let count = page_repos.len();
        repos.extend(page_repos);
        if count < PER_PAGE {
            break;
        }
        page += 1;
    }

    let owner = &scope.owner;
    repos.retain(|r| {
        (scope.include_forks || !r.fork)
            && (scope.include_archived || !r.archived)
            && (scope.include_private || !r.private)
            && owned_by(&r.full_name, owner)
    });
    Ok(repos)
}

/// Fetch and decode one repo's README.
///
/// `Ok(None)` when the repo has no README (404) or its content cannot be decoded;
/// `Err` when the request itself failed (e.g. rate limiting), so a partial
/// owner-wide scan is surfaced rather than mistaken for a complete one.
fn fetch_readme(
    http: &dyn Http,
    scope: &GitHubScope,
    repo: &Repo,
) -> Result<Option<Document>, Error> {
    let url = format!("{API}/repos/{}/{}/readme", scope.owner, repo.name);
    let body = match http.get_github(&url) {
        Ok(Some(body)) => body,
        Ok(None) => return Ok(None),
        Err(e) => {
            return Err(Error::GitHub {
                message: format!("fetching README of {}: {e}", repo.full_name),
            });
        }
    };
    // A README we cannot decode is skipped, not fatal.
    let Ok(readme) = serde_json::from_str::<Readme>(&body) else {
        return Ok(None);
    };
    if readme.encoding != "base64" {
        return Ok(None);
    }
    // GitHub wraps the base64 payload at 60 columns; strip whitespace first.
    let payload: String = readme.content.split_whitespace().collect();
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(payload) else {
        return Ok(None);
    };
    let Ok(content) = String::from_utf8(bytes) else {
        return Ok(None);
    };
    Ok(Some(Document {
        name: format!("{}/README.md", repo.full_name),
        content,
    }))
}

/// Whether `full_name` ("owner/repo") belongs to `owner`, case-insensitively.
fn owned_by(full_name: &str, owner: &str) -> bool {
    full_name
        .split('/')
        .next()
        .is_some_and(|o| o.eq_ignore_ascii_case(owner))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A fake GitHub that matches request URLs by substring. URLs matching any
    /// `errors` substring return `Err`, simulating rate limiting or a 5xx.
    struct FakeGitHub {
        routes: HashMap<String, String>,
        errors: Vec<String>,
    }

    impl FakeGitHub {
        fn new(routes: HashMap<String, String>) -> Self {
            Self {
                routes,
                errors: Vec::new(),
            }
        }

        fn failing(mut self, needle: &str) -> Self {
            self.errors.push(needle.to_string());
            self
        }
    }

    impl Http for FakeGitHub {
        fn get(&self, _url: &str) -> Result<String, String> {
            Err("badge fetch not used in this test".into())
        }
        fn get_github(&self, url: &str) -> Result<Option<String>, String> {
            if self.errors.iter().any(|n| url.contains(n.as_str())) {
                return Err("HTTP 429 (rate limited)".into());
            }
            for (needle, body) in &self.routes {
                if url.contains(needle.as_str()) {
                    return Ok(Some(body.clone()));
                }
            }
            Ok(None)
        }
    }

    fn readme_json(markdown: &str) -> String {
        let encoded = base64::engine::general_purpose::STANDARD.encode(markdown);
        format!("{{\"content\":\"{encoded}\",\"encoding\":\"base64\"}}")
    }

    fn scope() -> GitHubScope {
        GitHubScope {
            owner: "acme".into(),
            include_forks: false,
            include_archived: false,
            include_private: false,
        }
    }

    #[test]
    fn fetches_only_in_scope_repos_and_decodes_readmes() {
        let repos = r#"[
            {"name":"good","full_name":"acme/good","fork":false,"archived":false,"private":false},
            {"name":"forked","full_name":"acme/forked","fork":true,"archived":false,"private":false},
            {"name":"old","full_name":"acme/old","fork":false,"archived":true,"private":false}
        ]"#;
        let http = FakeGitHub::new(HashMap::from([
            ("/users/acme/repos".to_string(), repos.to_string()),
            (
                "/repos/acme/good/readme".to_string(),
                readme_json("# Good\n![b](https://img.shields.io/x)\n"),
            ),
        ]));

        let docs = fetch_readmes(&http, &scope()).unwrap();
        assert_eq!(docs.len(), 1, "fork and archived repos are skipped");
        assert_eq!(docs[0].name, "acme/good/README.md");
        assert!(docs[0].content.contains("shields.io/x"));
    }

    #[test]
    fn repo_without_readme_is_skipped() {
        let repos = r#"[{"name":"empty","full_name":"acme/empty","fork":false,"archived":false,"private":false}]"#;
        // No readme route mapped, so get_github returns Ok(None) (a 404).
        let http = FakeGitHub::new(HashMap::from([(
            "/users/acme/repos".to_string(),
            repos.to_string(),
        )]));
        assert!(fetch_readmes(&http, &scope()).unwrap().is_empty());
    }

    #[test]
    fn unknown_owner_is_an_error() {
        let http = FakeGitHub::new(HashMap::new());
        let err = fetch_readmes(&http, &scope()).unwrap_err();
        assert_eq!(err.kind(), "github");
    }

    #[test]
    fn readme_fetch_failure_is_surfaced_not_skipped() {
        let repos = r#"[{"name":"good","full_name":"acme/good","fork":false,"archived":false,"private":false}]"#;
        // The repo list succeeds, but its README fetch is rate-limited.
        let http = FakeGitHub::new(HashMap::from([(
            "/users/acme/repos".to_string(),
            repos.to_string(),
        )]))
        .failing("/repos/acme/good/readme");
        let err = fetch_readmes(&http, &scope()).unwrap_err();
        assert_eq!(err.kind(), "github");
    }
}
