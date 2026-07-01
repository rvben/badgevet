//! Full-pipeline tests that exercise the exact production `run` entry point with
//! a fake `Http`, so file discovery, GitHub fetching, badge extraction,
//! classification, and result mapping are all covered offline (no network, no
//! real sleeps).

use std::collections::HashMap;
use std::path::PathBuf;

use badgevet::{
    BadgeResult, GitHubScope, Http, OutputFormat, Report, Request, RetryPolicy, Source, State, run,
};

/// A fake HTTP client: `responses` serves badge SVGs by exact URL; `github`
/// serves GitHub API bodies matched by URL substring.
struct FakeHttp {
    responses: HashMap<String, String>,
    github: HashMap<String, String>,
}

impl FakeHttp {
    fn badges(responses: HashMap<String, String>) -> Self {
        Self {
            responses,
            github: HashMap::new(),
        }
    }
}

impl Http for FakeHttp {
    fn get(&self, url: &str) -> Result<String, String> {
        self.responses
            .get(url)
            .cloned()
            .ok_or_else(|| "unmapped url".to_string())
    }

    fn get_github(&self, url: &str) -> Result<Option<String>, String> {
        for (needle, body) in &self.github {
            if url.contains(needle.as_str()) {
                return Ok(Some(body.clone()));
            }
        }
        Ok(None)
    }
}

fn svg(title: &str) -> String {
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\"><title>{title}</title></svg>")
}

/// Write `content` to a README.md in a fresh temp dir and return (dir, path).
/// The dir is returned so it outlives the scan.
fn readme(content: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("README.md");
    std::fs::write(&path, content).unwrap();
    (dir, path)
}

fn request(source: Source) -> Request {
    Request {
        source,
        format: OutputFormat::Json,
        strict: false,
        only_broken: false,
        // No retries and zero backoff keep the ambiguous path instant.
        retry: RetryPolicy {
            retries: 0,
            backoff: std::time::Duration::ZERO,
        },
    }
}

fn scan_paths(path: PathBuf, http: &FakeHttp) -> Report {
    run(http, &request(Source::Paths(vec![path]))).expect("scan succeeds")
}

fn find<'a>(report: &'a Report, needle: &str) -> &'a BadgeResult {
    report
        .results
        .iter()
        .find(|r| r.url.contains(needle))
        .expect("badge present")
}

const RETIRED_URL: &str = "https://img.shields.io/visual-studio-marketplace/v/rvben.rumdl";
const CRATES_URL: &str = "https://img.shields.io/crates/v/rumdl";
const STARS_URL: &str = "https://img.shields.io/github/stars/rvben/rumdl";

fn badge_svgs() -> HashMap<String, String> {
    HashMap::from([
        (
            RETIRED_URL.to_string(),
            svg("visual-studio-marketplace: retired badge"),
        ),
        (CRATES_URL.to_string(), svg("crates.io: v0.2.27")),
        (STARS_URL.to_string(), svg("stars: invalid")),
    ])
}

#[test]
fn mixed_readme_classifies_each_badge_and_ignores_non_badges() {
    let (_dir, path) = readme(&format!(
        "# Project\n\n[![Version]({RETIRED_URL})](m)\n[![Crates]({CRATES_URL})](c)\n[![Stars]({STARS_URL})](s)\n![logo](https://example.com/logo.png)\n"
    ));
    let http = FakeHttp::badges(badge_svgs());

    let report = scan_paths(path, &http);

    // The non-badge example.com image is excluded.
    assert_eq!(report.results.len(), 3);

    let retired = find(&report, "visual-studio-marketplace");
    assert_eq!(retired.state, State::Broken);
    assert_eq!(
        retired.suggestion.as_deref(),
        Some("https://vsmarketplacebadges.dev/version/rvben.rumdl.svg")
    );

    assert_eq!(find(&report, "crates/v").state, State::Ok);
    assert_eq!(find(&report, "github/stars").state, State::Unconfirmed);

    // A broken badge is present, so the scan exits 1 regardless of strictness.
    assert_eq!(report.exit_code(false), 1);
    assert_eq!(report.exit_code(true), 1);
}

#[test]
fn all_healthy_readme_exits_zero() {
    let (_dir, path) = readme(&format!("[![Crates]({CRATES_URL})](c)\n"));
    let http = FakeHttp::badges(HashMap::from([(
        CRATES_URL.to_string(),
        svg("crates.io: v0.2.27"),
    )]));

    let report = scan_paths(path, &http);
    assert_eq!(report.results.len(), 1);
    assert_eq!(report.exit_code(false), 0);
}

#[test]
fn strict_treats_unconfirmed_as_failure() {
    let (_dir, path) = readme(&format!("[![Stars]({STARS_URL})](s)\n"));
    let http = FakeHttp::badges(HashMap::from([(
        STARS_URL.to_string(),
        svg("stars: invalid"),
    )]));

    let report = scan_paths(path, &http);
    assert_eq!(report.unconfirmed(), 1);
    assert_eq!(report.broken(), 0);
    // Unconfirmed is not a failure by default, but is under --strict.
    assert_eq!(report.exit_code(false), 0);
    assert_eq!(report.exit_code(true), 1);
}

#[test]
fn github_source_scans_repo_readmes() {
    use base64::Engine as _;
    let readme_md = format!("# rumdl\n[![v]({RETIRED_URL})](m) [![c]({CRATES_URL})](c)\n");
    let encoded = base64::engine::general_purpose::STANDARD.encode(&readme_md);
    let repos = r#"[{"name":"rumdl","full_name":"rvben/rumdl","fork":false,"archived":false,"private":false}]"#;

    let http = FakeHttp {
        responses: badge_svgs(),
        github: HashMap::from([
            ("/users/rvben/repos".to_string(), repos.to_string()),
            (
                "/repos/rvben/rumdl/readme".to_string(),
                format!("{{\"content\":\"{encoded}\",\"encoding\":\"base64\"}}"),
            ),
        ]),
    };

    let scope = GitHubScope {
        owner: "rvben".into(),
        include_forks: false,
        include_archived: false,
        include_private: false,
    };
    let report = run(&http, &request(Source::GitHub(scope))).expect("scan succeeds");

    assert_eq!(report.results.len(), 2);
    assert_eq!(
        find(&report, "visual-studio-marketplace").state,
        State::Broken
    );
    assert_eq!(find(&report, "crates/v").state, State::Ok);
    // Results are attributed to the repo, not a local path.
    assert_eq!(report.results[0].file, "rvben/rumdl/README.md");
    assert_eq!(report.exit_code(false), 1);
}
