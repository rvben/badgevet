//! Full-pipeline tests that exercise the exact production `run` entry point with
//! a fake `Http`, so file discovery, badge extraction, classification, and
//! result mapping are all covered offline (no network, no real sleeps).

use std::collections::HashMap;
use std::path::PathBuf;

use badgevet::{BadgeResult, Http, OutputFormat, Report, Request, RetryPolicy, State, run};

/// A fake HTTP client returning canned SVG bodies keyed by URL.
struct FakeHttp {
    responses: HashMap<String, String>,
}

impl Http for FakeHttp {
    fn get(&self, url: &str) -> Result<String, String> {
        self.responses
            .get(url)
            .cloned()
            .ok_or_else(|| "unmapped url".to_string())
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

fn scan(path: PathBuf, http: &FakeHttp) -> Report {
    let request = Request {
        paths: vec![path],
        format: OutputFormat::Json,
        strict: false,
        only_broken: false,
        // No retries and zero backoff keep the ambiguous path instant.
        retry: RetryPolicy {
            retries: 0,
            backoff: std::time::Duration::ZERO,
        },
    };
    run(http, &request).expect("scan succeeds")
}

fn find<'a>(report: &'a Report, needle: &str) -> &'a BadgeResult {
    report
        .results
        .iter()
        .find(|r| r.url.contains(needle))
        .expect("badge present")
}

#[test]
fn mixed_readme_classifies_each_badge_and_ignores_non_badges() {
    let (_dir, path) = readme(concat!(
        "# Project\n\n",
        "[![Version](https://img.shields.io/visual-studio-marketplace/v/rvben.rumdl)](m)\n",
        "[![Crates](https://img.shields.io/crates/v/rumdl)](c)\n",
        "[![Stars](https://img.shields.io/github/stars/rvben/rumdl)](s)\n",
        "![logo](https://example.com/logo.png)\n",
    ));
    let http = FakeHttp {
        responses: HashMap::from([
            (
                "https://img.shields.io/visual-studio-marketplace/v/rvben.rumdl".to_string(),
                svg("visual-studio-marketplace: retired badge"),
            ),
            (
                "https://img.shields.io/crates/v/rumdl".to_string(),
                svg("crates.io: v0.2.27"),
            ),
            (
                "https://img.shields.io/github/stars/rvben/rumdl".to_string(),
                svg("stars: invalid"),
            ),
        ]),
    };

    let report = scan(path, &http);

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
    let (_dir, path) = readme("[![Crates](https://img.shields.io/crates/v/rumdl)](c)\n");
    let http = FakeHttp {
        responses: HashMap::from([(
            "https://img.shields.io/crates/v/rumdl".to_string(),
            svg("crates.io: v0.2.27"),
        )]),
    };

    let report = scan(path, &http);
    assert_eq!(report.results.len(), 1);
    assert_eq!(report.exit_code(false), 0);
}

#[test]
fn strict_treats_unconfirmed_as_failure() {
    let (_dir, path) = readme("[![Stars](https://img.shields.io/github/stars/rvben/rumdl)](s)\n");
    let http = FakeHttp {
        responses: HashMap::from([(
            "https://img.shields.io/github/stars/rvben/rumdl".to_string(),
            svg("stars: invalid"),
        )]),
    };

    let report = scan(path, &http);
    assert_eq!(report.unconfirmed(), 1);
    assert_eq!(report.broken(), 0);
    // Unconfirmed is not a failure by default, but is under --strict.
    assert_eq!(report.exit_code(false), 0);
    assert_eq!(report.exit_code(true), 1);
}
