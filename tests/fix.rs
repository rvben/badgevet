//! Tests for the in-place `fix` mutation, exercising `apply_fixes` directly with
//! a hand-built report so no network is involved.

use badgevet::{BadgeResult, Report, State, apply_fixes};

const OLD: &str = "https://img.shields.io/visual-studio-marketplace/v/rvben.rumdl";
const NEW: &str = "https://vsmarketplacebadges.dev/version/rvben.rumdl.svg";

fn broken(file: &str, url: &str, suggestion: Option<&str>) -> BadgeResult {
    BadgeResult {
        file: file.to_string(),
        line: 1,
        label: String::new(),
        url: url.to_string(),
        provider: "shields.io".to_string(),
        state: State::Broken,
        rendered: Some("retired badge".to_string()),
        suggestion: suggestion.map(str::to_string),
    }
}

/// Write a README containing `body` in a fresh temp dir; return (dir, path-string).
fn write_readme(body: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("README.md");
    std::fs::write(&path, body).unwrap();
    (dir, path.display().to_string())
}

#[test]
fn rewrites_broken_badge_with_suggestion_and_is_idempotent() {
    let (dir, file) = write_readme(&format!("[![v]({OLD})](m)\n"));

    let report = Report {
        results: vec![broken(&file, OLD, Some(NEW))],
    };
    let result = apply_fixes(&report).unwrap();

    assert_eq!(result.fixed.len(), 1);
    assert_eq!(result.unfixable, 0);
    assert_eq!(result.exit_code(), 0);
    assert_eq!(result.fixed[0].old, OLD);
    assert_eq!(result.fixed[0].new, NEW);

    let content = std::fs::read_to_string(dir.path().join("README.md")).unwrap();
    assert!(content.contains(NEW));
    assert!(!content.contains(OLD));

    // Running the same report again is a no-op: the old URL is already gone.
    let again = apply_fixes(&report).unwrap();
    assert!(again.fixed.is_empty());
}

#[test]
fn leaves_the_link_target_and_only_swaps_the_badge_url() {
    // The badge image URL and the surrounding link target differ, so only the
    // image URL is rewritten.
    let (dir, file) = write_readme(&format!(
        "[![v]({OLD})](https://marketplace.visualstudio.com/items?itemName=rvben.rumdl)\n"
    ));
    let report = Report {
        results: vec![broken(&file, OLD, Some(NEW))],
    };
    apply_fixes(&report).unwrap();

    let content = std::fs::read_to_string(dir.path().join("README.md")).unwrap();
    assert!(content.contains(NEW));
    assert!(content.contains("marketplace.visualstudio.com/items?itemName=rvben.rumdl"));
}

#[test]
fn broken_without_suggestion_is_unfixable_and_file_untouched() {
    let url = "https://img.shields.io/github/stars/x/y";
    let (dir, file) = write_readme(&format!("[![s]({url})](m)\n"));

    let report = Report {
        results: vec![broken(&file, url, None)],
    };
    let result = apply_fixes(&report).unwrap();

    assert!(result.fixed.is_empty());
    assert_eq!(result.unfixable, 1);
    assert_eq!(result.exit_code(), 1);
    assert!(
        std::fs::read_to_string(dir.path().join("README.md"))
            .unwrap()
            .contains(url)
    );
}

#[test]
fn does_not_touch_the_url_in_prose_or_code() {
    // The same URL appears as a badge and as a bare mention in prose; only the
    // badge destination is rewritten.
    let (dir, file) = write_readme(&format!("[![v]({OLD})](m)\n\nWe used `{OLD}` once.\n"));
    let report = Report {
        results: vec![broken(&file, OLD, Some(NEW))],
    };
    let result = apply_fixes(&report).unwrap();

    assert_eq!(result.fixed.len(), 1);
    let content = std::fs::read_to_string(dir.path().join("README.md")).unwrap();
    assert!(
        content.contains(&format!("]({NEW})")),
        "badge destination rewritten"
    );
    assert!(
        content.contains(&format!("`{OLD}`")),
        "prose mention preserved"
    );
}
