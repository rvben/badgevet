//! badgevet: find retired and broken status badges in Markdown that link
//! checkers miss.
//!
//! Ordinary link checkers only see HTTP status, but a retired shields.io badge
//! returns `200 OK` with an SVG whose title reads "retired badge". badgevet
//! fetches each badge and reads that rendered title instead.
//!
//! The whole pipeline is reachable through [`run`], which is generic over the
//! [`Http`] seam so tests drive it with canned responses (no network).

mod classify;
mod error;
mod fetch;
mod markdown;
mod model;
mod output;
mod provider;
pub mod schema;

pub use error::Error;
pub use fetch::{Http, ReqwestHttp, RetryPolicy};
pub use model::{Badge, BadgeResult, Report, State};
pub use output::render;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

/// The maximum number of badges fetched concurrently.
const MAX_WORKERS: usize = 8;

/// Rendered output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
}

/// A complete badgevet request.
#[derive(Debug, Clone)]
pub struct Request {
    /// Files or directories to scan. Empty means "README.md in the cwd".
    pub paths: Vec<PathBuf>,
    pub format: OutputFormat,
    /// Also treat unconfirmed badges as a failure in the exit code.
    pub strict: bool,
    /// Render only broken badges.
    pub only_broken: bool,
    pub retry: RetryPolicy,
}

/// Run a scan and return the report. `strict` and `only_broken` affect the exit
/// code and rendering respectively, both applied by the caller.
pub fn run(http: &dyn Http, req: &Request) -> Result<Report, Error> {
    let files = resolve_files(&req.paths)?;

    let mut badges: Vec<Badge> = Vec::new();
    for file in &files {
        let content = std::fs::read_to_string(file).map_err(|e| Error::Io {
            path: file.display().to_string(),
            message: e.to_string(),
        })?;
        let name = file.display().to_string();
        badges.extend(
            markdown::extract_images(&content, &name)
                .into_iter()
                .filter(|b| provider::is_badge_url(&b.url)),
        );
    }

    let mut seen = BTreeSet::new();
    let unique: Vec<String> = badges
        .iter()
        .map(|b| b.url.clone())
        .filter(|u| seen.insert(u.clone()))
        .collect();
    let checked = check_all(http, &unique, req.retry);

    let results = badges
        .into_iter()
        .map(|b| {
            let (state, rendered) = checked
                .get(&b.url)
                .cloned()
                .unwrap_or((State::Unconfirmed, None));
            let suggestion = (state == State::Broken)
                .then(|| provider::suggest(&b.url))
                .flatten();
            BadgeResult {
                provider: provider::provider_name(&b.url)
                    .unwrap_or("unknown")
                    .to_string(),
                file: b.file,
                line: b.line,
                label: b.label,
                url: b.url,
                state,
                rendered,
                suggestion,
            }
        })
        .collect();

    Ok(Report { results })
}

/// Check every unique URL, up to [`MAX_WORKERS`] at a time.
fn check_all(
    http: &dyn Http,
    urls: &[String],
    retry: RetryPolicy,
) -> HashMap<String, (State, Option<String>)> {
    if urls.is_empty() {
        return HashMap::new();
    }
    let results: Mutex<HashMap<String, (State, Option<String>)>> = Mutex::new(HashMap::new());
    let next = AtomicUsize::new(0);
    let sleep = |d: Duration| thread::sleep(d);
    let workers = urls.len().min(MAX_WORKERS);

    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(url) = urls.get(i) else { break };
                    let outcome = fetch::check(http, url, retry, &sleep);
                    results.lock().unwrap().insert(url.clone(), outcome);
                }
            });
        }
    });

    results.into_inner().unwrap()
}

/// Expand the requested paths into a concrete list of Markdown files.
fn resolve_files(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, Error> {
    if inputs.is_empty() {
        let readme = PathBuf::from("README.md");
        if readme.is_file() {
            return Ok(vec![readme]);
        }
        return Err(Error::Usage {
            message: "no paths given and no README.md in the current directory".to_string(),
        });
    }

    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            collect_markdown(input, &mut files);
        } else if input.is_file() {
            files.push(input.clone());
        } else {
            return Err(Error::Io {
                path: input.display().to_string(),
                message: "no such file or directory".to_string(),
            });
        }
    }
    Ok(files)
}

/// Recursively collect Markdown files under `dir`, skipping VCS and build dirs.
fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if name.starts_with('.') || matches!(name, "node_modules" | "target") {
                continue;
            }
            collect_markdown(&path, out);
        } else if is_markdown(&path) {
            out.push(path);
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}
