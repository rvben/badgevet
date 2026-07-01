//! Error type, the stable error `kind` set, and the exit-code contract.
//!
//! Errors are reported as a clispec structured envelope on the last line of
//! stderr: `{"error":{"kind":...,"message":...,"exit_code":...,"hint":...}}`.
//! Note that finding broken badges is *not* an error: it is the exit-1 `outcome`
//! declared in `schema.rs`, and writes a normal report to stdout.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// Invalid command-line arguments (also wraps clap errors) or no scan target.
    #[error("{message}")]
    Usage { message: String },

    /// A path could not be read.
    #[error("{path}: {message}")]
    Io { path: String, message: String },

    /// The HTTP client could not be constructed.
    #[error("{message}")]
    Http { message: String },

    /// A GitHub API request failed (listing repositories or fetching a README).
    #[error("{message}")]
    GitHub { message: String },
}

impl Error {
    /// Stable snake_case identifier consumers branch on (the schema `errors` set).
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Usage { .. } => "usage",
            Error::Io { .. } => "io",
            Error::Http { .. } => "http",
            Error::GitHub { .. } => "github",
        }
    }

    /// Actionable remediation, when there is one.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Error::Usage { .. } => Some("see `badgevet --help` or `badgevet schema`"),
            Error::Io { .. } => Some("check the path exists and is readable"),
            Error::Http { .. } => Some("check network connectivity"),
            Error::GitHub { .. } => Some("check the owner name, or set GITHUB_TOKEN"),
        }
    }

    /// The process exit code associated with this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Io { .. } | Error::Http { .. } | Error::GitHub { .. } => 2,
            Error::Usage { .. } => 3,
        }
    }
}
