//! Rendering a scan report as a table (TTY) or JSON (piped).

use crate::OutputFormat;
use crate::model::{BadgeResult, Report, State};
use serde_json::json;

/// Render a report. With `only_broken`, healthy and unconfirmed rows are hidden
/// (the summary still counts every badge).
pub fn render(report: &Report, only_broken: bool, format: OutputFormat) -> String {
    let shown: Vec<&BadgeResult> = report
        .results
        .iter()
        .filter(|r| !only_broken || r.state == State::Broken)
        .collect();
    match format {
        OutputFormat::Json => json!({
            "results": shown,
            "summary": {
                "total": report.results.len(),
                "ok": report.count(State::Ok),
                "broken": report.broken(),
                "unconfirmed": report.unconfirmed(),
            }
        })
        .to_string(),
        OutputFormat::Table => table(&shown, report, only_broken),
    }
}

fn table(shown: &[&BadgeResult], report: &Report, only_broken: bool) -> String {
    if report.results.is_empty() {
        return "no badges found".to_string();
    }
    let mut rows = Vec::new();
    if shown.is_empty() {
        rows.push(format!(
            "no broken badges ({} checked)",
            report.results.len()
        ));
    } else {
        rows.push(format!(
            "{:<12} {:<22} {:<26} {}",
            "STATE", "PROVIDER", "LOCATION", "BADGE"
        ));
        for r in shown {
            let location = format!("{}:{}", r.file, r.line);
            let badge = if r.label.is_empty() { &r.url } else { &r.label };
            rows.push(format!(
                "{:<12} {:<22} {:<26} {}",
                r.state.as_str(),
                truncate(&r.provider, 22),
                truncate(&location, 26),
                truncate(badge, 44),
            ));
            if let Some(suggestion) = &r.suggestion {
                rows.push(format!("{:<12} -> {suggestion}", ""));
            }
        }
    }
    rows.push(String::new());
    let scope = if only_broken { " (broken only)" } else { "" };
    rows.push(format!(
        "{} checked{scope} · {} ok · {} broken · {} unconfirmed",
        report.results.len(),
        report.count(State::Ok),
        report.broken(),
        report.unconfirmed(),
    ));
    rows.join("\n")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let keep: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{keep}…")
    }
}
