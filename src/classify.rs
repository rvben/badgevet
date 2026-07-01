//! Classify a badge SVG's `<title>` text into a health verdict.
//!
//! This is the deterministic core. Only shields.io's explicit deprecation
//! states ("retired", "deprecated") are treated as permanently broken. Anything
//! that could be a transient provider hiccup ("invalid", "inaccessible", a rate
//! limit, an empty or absent title) is [`Verdict::Ambiguous`], so the caller
//! retries and, if it never resolves, reports it as `unconfirmed` rather than
//! failing CI on a badge that renders fine for real users.

/// The verdict for a single observed badge title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A real value is rendered (e.g. "crates.io: v1.2.3", "build: passing").
    Healthy(String),
    /// A permanent, deterministic dead state (e.g. "retired badge").
    Broken(String),
    /// Could not be confirmed either way (transient, rate-limited, empty).
    Ambiguous(String),
}

/// Substrings that mark a deterministically dead badge. Case-insensitive.
///
/// These are the states a provider renders when it has permanently removed
/// support for a badge route, not when a lookup transiently fails.
const DEAD_MARKERS: &[&str] = &[
    "retired",
    "deprecated",
    "no longer available",
    "has been removed",
];

/// Substrings that mark an inconclusive result. A shields.io badge shows
/// "invalid" / "inaccessible" both for a genuinely bad request and when its
/// upstream API is rate-limiting, so these are never treated as hard failures.
const AMBIGUOUS_MARKERS: &[&str] = &[
    "invalid",
    "inaccessible",
    "not found",
    "error",
    "rate limit",
    "unavailable",
    "unknown",
];

/// Classify the text found inside a badge SVG's `<title>` element.
pub fn classify_title(title: &str) -> Verdict {
    let trimmed = title.trim();
    let lower = trimmed.to_lowercase();

    if DEAD_MARKERS.iter().any(|m| lower.contains(m)) {
        return Verdict::Broken(trimmed.to_string());
    }
    if trimmed.is_empty() || AMBIGUOUS_MARKERS.iter().any(|m| lower.contains(m)) {
        return Verdict::Ambiguous(trimmed.to_string());
    }
    Verdict::Healthy(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_marketplace_badge_is_broken() {
        // The exact title behind rumdl-vscode issue #147.
        assert_eq!(
            classify_title("visual-studio-marketplace: retired badge"),
            Verdict::Broken("visual-studio-marketplace: retired badge".into())
        );
    }

    #[test]
    fn deprecated_is_broken() {
        assert!(matches!(
            classify_title("service: deprecated"),
            Verdict::Broken(_)
        ));
    }

    #[test]
    fn real_values_are_healthy() {
        for title in [
            "crates.io: v0.2.27",
            "build: passing",
            "downloads: 46.63K",
            "rating: average: 4.67/5 (3 ratings)",
            "open vsx: v0.0.276",
            "license: MIT",
        ] {
            assert!(
                matches!(classify_title(title), Verdict::Healthy(_)),
                "expected healthy for {title:?}"
            );
        }
    }

    #[test]
    fn invalid_is_ambiguous_not_broken() {
        // shields.io renders "invalid" when its GitHub token pool is rate-limited;
        // this must not be treated as a permanent failure.
        assert_eq!(
            classify_title("stars: invalid"),
            Verdict::Ambiguous("stars: invalid".into())
        );
        assert!(matches!(
            classify_title("release: invalid"),
            Verdict::Ambiguous(_)
        ));
    }

    #[test]
    fn empty_title_is_ambiguous() {
        assert_eq!(classify_title(""), Verdict::Ambiguous(String::new()));
        assert_eq!(classify_title("   "), Verdict::Ambiguous(String::new()));
    }

    #[test]
    fn not_found_is_ambiguous() {
        // A deleted repo and a rate-limited lookup are indistinguishable here,
        // so we stay conservative.
        assert!(matches!(
            classify_title("repo: not found"),
            Verdict::Ambiguous(_)
        ));
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert!(matches!(
            classify_title("RETIRED BADGE"),
            Verdict::Broken(_)
        ));
        assert!(matches!(classify_title("Invalid"), Verdict::Ambiguous(_)));
    }
}
