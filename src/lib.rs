//! badgevet: find retired and broken status badges in Markdown that link
//! checkers miss.
//!
//! Ordinary link checkers only see HTTP status, but a retired shields.io badge
//! returns `200 OK` with an SVG whose title reads "retired badge". badgevet
//! fetches each badge and reads that rendered title instead.
//!
//! The whole pipeline is reachable through [`run`], which is generic over the
//! [`Http`] seam so tests drive it with canned responses (no network). A scan
//! draws its Markdown either from local paths or, with [`Source::GitHub`], from
//! the canonical READMEs of an owner's repositories.

mod classify;
mod error;
mod fetch;
mod github;
mod markdown;
mod model;
mod output;
mod provider;
pub mod schema;

pub use error::Error;
pub use fetch::{Http, ReqwestHttp, RetryPolicy};
pub use github::GitHubScope;
pub use model::{AppliedFix, Badge, BadgeResult, FixResult, Report, State};
pub use output::{render, render_fix};

use std::collections::{BTreeMap, BTreeSet, HashMap};
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

/// Where a scan draws its Markdown from.
#[derive(Debug, Clone)]
pub enum Source {
    /// Local files and directories (directories recurse for Markdown).
    Paths(Vec<PathBuf>),
    /// The canonical READMEs of an owner's GitHub repositories.
    GitHub(GitHubScope),
}

/// A complete badgevet request.
#[derive(Debug, Clone)]
pub struct Request {
    pub source: Source,
    pub format: OutputFormat,
    /// Also treat unconfirmed badges as a failure in the exit code.
    pub strict: bool,
    /// Render only broken badges.
    pub only_broken: bool,
    pub retry: RetryPolicy,
}

/// A piece of Markdown to scan, with a display name (a path or `owner/repo`).
#[derive(Debug)]
pub(crate) struct Document {
    pub name: String,
    pub content: String,
}

/// Run a scan and return the report. `strict` and `only_broken` affect the exit
/// code and rendering respectively, both applied by the caller.
pub fn run(http: &dyn Http, req: &Request) -> Result<Report, Error> {
    let documents = match &req.source {
        Source::Paths(paths) => local_documents(paths)?,
        Source::GitHub(scope) => github::fetch_readmes(http, scope)?,
    };

    let mut badges: Vec<Badge> = Vec::new();
    for doc in &documents {
        badges.extend(
            markdown::extract_images(&doc.content, &doc.name)
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

/// Rewrite each broken badge that has a known replacement, in place, in its
/// local file. Returns what was changed plus how many broken badges had no known
/// replacement (and so were left untouched). Intended for local-path reports;
/// the badge `file` must be a readable path.
pub fn apply_fixes(report: &Report) -> Result<FixResult, Error> {
    let mut by_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut unfixable = 0;
    for r in &report.results {
        if r.state != State::Broken {
            continue;
        }
        match &r.suggestion {
            Some(new) => {
                by_file
                    .entry(r.file.clone())
                    .or_default()
                    .push((r.url.clone(), new.clone()));
            }
            None => unfixable += 1,
        }
    }

    let mut fixed = Vec::new();
    for (file, replacements) in by_file {
        let mut content = std::fs::read_to_string(&file).map_err(|e| Error::Io {
            path: file.clone(),
            message: e.to_string(),
        })?;
        let mut seen = BTreeSet::new();
        let mut changed = false;
        for (old, new) in replacements {
            if !seen.insert(old.clone()) {
                continue;
            }
            let (updated, replaced) = replace_badge_url(&content, &old, &new);
            if replaced > 0 {
                content = updated;
                changed = true;
                fixed.push(AppliedFix {
                    file: file.clone(),
                    old,
                    new,
                });
            }
        }
        if changed {
            std::fs::write(&file, content).map_err(|e| Error::Io {
                path: file.clone(),
                message: e.to_string(),
            })?;
        }
    }

    Ok(FixResult { fixed, unfixable })
}

/// Replace `old` with `new`, but only where `old` appears as a Markdown image or
/// link destination `](old)` or an HTML `src` attribute value. This scopes the
/// rewrite to badge references, leaving the URL untouched if it also appears in
/// prose or a code block. Returns the new content and the number of replacements.
fn replace_badge_url(content: &str, old: &str, new: &str) -> (String, usize) {
    let mut result = content.to_string();
    let mut count = 0;
    let patterns = [
        (format!("]({old})"), format!("]({new})")),
        (format!("src=\"{old}\""), format!("src=\"{new}\"")),
        (format!("src='{old}'"), format!("src='{new}'")),
    ];
    for (from, to) in patterns {
        let matches = result.matches(&from).count();
        if matches > 0 {
            result = result.replace(&from, &to);
            count += matches;
        }
    }
    (result, count)
}

/// Check every unique URL, up to [`MAX_WORKERS`] at a time.
fn check_all(
    http: &dyn Http,
    urls: &[String],
    retry: RetryPolicy,
) -> HashMap<String, (State, Option<String>)> {
    let sleep = |d: Duration| thread::sleep(d);
    let outcomes = parallel_map(urls, MAX_WORKERS, |_, url| {
        fetch::check(http, url, retry, &sleep)
    });
    urls.iter().cloned().zip(outcomes).collect()
}

/// Map `f` over `items` using at most `max_workers` threads, preserving order.
pub(crate) fn parallel_map<T, R, F>(items: &[T], max_workers: usize, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(usize, &T) -> R + Sync,
{
    if items.is_empty() {
        return Vec::new();
    }
    let slots: Vec<Mutex<Option<R>>> = (0..items.len()).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let workers = max_workers.min(items.len());

    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let value = f(i, item);
                    *slots[i].lock().unwrap() = Some(value);
                }
            });
        }
    });

    slots
        .into_iter()
        .map(|slot| slot.into_inner().unwrap().expect("every slot filled"))
        .collect()
}

/// Read the requested local inputs into scannable documents. A path of `-`
/// reads Markdown from stdin.
fn local_documents(paths: &[PathBuf]) -> Result<Vec<Document>, Error> {
    let read_stdin = paths.iter().any(|p| p.as_os_str() == "-");
    let file_inputs: Vec<PathBuf> = paths
        .iter()
        .filter(|p| p.as_os_str() != "-")
        .cloned()
        .collect();

    let mut documents = Vec::new();
    if read_stdin {
        let mut content = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut content).map_err(|e| {
            Error::Io {
                path: "<stdin>".to_string(),
                message: e.to_string(),
            }
        })?;
        documents.push(Document {
            name: "<stdin>".to_string(),
            content,
        });
    }

    // Resolve file inputs; fall back to the default README only when nothing at
    // all (no files and no stdin) was requested.
    if !file_inputs.is_empty() || !read_stdin {
        for file in resolve_files(&file_inputs)? {
            let content = std::fs::read_to_string(&file).map_err(|e| Error::Io {
                path: file.display().to_string(),
                message: e.to_string(),
            })?;
            documents.push(Document {
                name: file.display().to_string(),
                content,
            });
        }
    }
    Ok(documents)
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
