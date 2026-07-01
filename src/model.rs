//! Core data types: a badge occurrence, its checked result, and the report.

use serde::Serialize;

/// Health of a single badge.
///
/// Only `Broken` is a deterministic, permanent failure (a shields.io "retired"
/// or "deprecated" state). `Unconfirmed` covers anything that might be a
/// transient provider hiccup or upstream rate limit, so it never fails CI unless
/// the caller opts into `--strict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Ok,
    Broken,
    Unconfirmed,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Ok => "ok",
            State::Broken => "broken",
            State::Unconfirmed => "unconfirmed",
        }
    }
}

/// A badge occurrence found in a Markdown file, before it is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    /// The file the badge was found in, as given on the command line.
    pub file: String,
    /// 1-based line number of the badge image.
    pub line: usize,
    /// Alt text of the image (may be empty).
    pub label: String,
    /// The badge image URL.
    pub url: String,
}

/// The result of checking one badge occurrence.
#[derive(Debug, Clone, Serialize)]
pub struct BadgeResult {
    pub file: String,
    pub line: usize,
    pub label: String,
    pub url: String,
    /// Display name of the badge provider (e.g. "shields.io").
    pub provider: String,
    pub state: State,
    /// The text rendered inside the badge SVG's `<title>`, when observed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered: Option<String>,
    /// A modern replacement URL for a known-dead badge, when one is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// The outcome of a full scan.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub results: Vec<BadgeResult>,
}

impl Report {
    pub fn count(&self, state: State) -> usize {
        self.results.iter().filter(|r| r.state == state).count()
    }

    pub fn broken(&self) -> usize {
        self.count(State::Broken)
    }

    pub fn unconfirmed(&self) -> usize {
        self.count(State::Unconfirmed)
    }

    /// Exit code for the scan: `1` when any badge is permanently broken (or,
    /// with `strict`, unconfirmed), otherwise `0`.
    pub fn exit_code(&self, strict: bool) -> i32 {
        if self.broken() > 0 || (strict && self.unconfirmed() > 0) {
            1
        } else {
            0
        }
    }
}

/// A single badge URL rewritten in place by `fix`.
#[derive(Debug, Clone, Serialize)]
pub struct AppliedFix {
    pub file: String,
    pub old: String,
    pub new: String,
}

/// The outcome of a `fix` run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FixResult {
    pub fixed: Vec<AppliedFix>,
    /// Broken badges left untouched because no replacement is known.
    pub unfixable: usize,
}

impl FixResult {
    /// Exit code: `1` when broken badges remain that could not be fixed.
    pub fn exit_code(&self) -> i32 {
        if self.unfixable > 0 { 1 } else { 0 }
    }
}
