//! The clispec v0.2 contract emitted by `badgevet schema`.
//!
//! Conforms to <https://clispec.dev/schema/v0.2.json> (validated by a test
//! against the vendored copy in `schemas/clispec-v0.2.json`). Keep this in sync
//! as commands, arguments, error kinds, and outcomes change.

use serde_json::{Value, json};

/// The version of The CLI Spec this document conforms to.
pub const CLISPEC_VERSION: &str = "0.2";

/// Build the clispec contract as a JSON value.
pub fn contract() -> Value {
    json!({
        "clispec": CLISPEC_VERSION,
        "name": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "global_args": [
            {
                "name": "--output",
                "type": "string",
                "enum": ["auto", "json", "text"],
                "default": "auto",
                "description": "Output format. auto = text on a TTY, JSON when piped."
            }
        ],
        "commands": [
            {
                "name": "scan",
                "description": "Scan Markdown files for status badges and report their health. The default command, invoked as `badgevet [PATH...]`. With no paths, scans README.md in the current directory; a directory is scanned recursively for Markdown files.",
                "mutating": false,
                "stability": "stable",
                "args": [
                    {"name": "path", "type": "path", "required": false, "description": "Markdown files or directories to scan (default: README.md)."},
                    {"name": "--only-broken", "type": "boolean", "default": false, "description": "Report only permanently broken badges."},
                    {"name": "--strict", "type": "boolean", "default": false, "description": "Also exit 1 on unconfirmed badges, not just broken ones."},
                    {"name": "--retries", "type": "integer", "default": 2, "description": "Re-fetch an ambiguous badge this many times before reporting it unconfirmed."},
                    {"name": "--timeout", "type": "integer", "default": 10, "description": "Per-request HTTP timeout, in seconds."}
                ],
                "output_fields": [
                    {"name": "file", "type": "path", "description": "File the badge was found in."},
                    {"name": "line", "type": "integer", "description": "1-based line number of the badge."},
                    {"name": "label", "type": "string", "description": "Alt text of the badge image."},
                    {"name": "url", "type": "string", "description": "Badge image URL."},
                    {"name": "provider", "type": "string", "description": "Badge provider (e.g. shields.io)."},
                    {"name": "state", "type": "string", "description": "ok, broken, or unconfirmed."},
                    {"name": "rendered", "type": "string", "description": "Text rendered inside the badge SVG title, when observed."},
                    {"name": "suggestion", "type": "string", "description": "Modern replacement URL for a known-dead badge, when known."}
                ]
            },
            {
                "name": "schema",
                "description": "Print this clispec contract as JSON.",
                "mutating": false,
                "stability": "stable"
            },
            {
                "name": "completions",
                "description": "Generate a shell completion script.",
                "mutating": false,
                "stability": "stable",
                "args": [
                    {"name": "shell", "type": "string", "required": true, "enum": ["bash", "zsh", "fish", "powershell", "elvish"], "description": "Target shell."}
                ]
            }
        ],
        "outcomes": [
            {"code": 1, "name": "broken_found", "description": "At least one badge is permanently broken (retired or deprecated). With --strict, unconfirmed badges also trigger this. stdout still carries the full report; no error envelope is written."}
        ],
        "errors": [
            {"kind": "usage", "exit_code": 3, "retryable": false, "description": "Invalid command-line arguments, or no scan target and no README.md."},
            {"kind": "io", "exit_code": 2, "retryable": false, "description": "A path could not be read."},
            {"kind": "http", "exit_code": 2, "retryable": true, "description": "The HTTP client could not be constructed."}
        ]
    })
}

/// The contract as a pretty-printed JSON string.
pub fn contract_json() -> String {
    serde_json::to_string_pretty(&contract()).expect("contract serializes")
}
