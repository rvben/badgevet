//! End-to-end tests of the compiled binary that need no network: the clispec
//! contract, help, completions, and the error/exit-code envelope. Badge-checking
//! behavior is covered offline against the library in `scan.rs`.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_badgevet");

struct Output {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_in(dir: &Path, args: &[&str]) -> Output {
    let out = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn binary");
    Output {
        code: out.status.code().unwrap(),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

fn run(args: &[&str]) -> Output {
    run_in(Path::new(env!("CARGO_MANIFEST_DIR")), args)
}

fn run_with_stdin(args: &[&str], input: &str) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn binary");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    Output {
        code: out.status.code().unwrap(),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

fn error_envelope(stderr: &str) -> serde_json::Value {
    let last = stderr.lines().last().expect("stderr has an error line");
    serde_json::from_str::<serde_json::Value>(last).expect("error envelope is JSON")["error"]
        .clone()
}

#[test]
fn schema_is_clispec_v0_2() {
    let out = run(&["schema"]);
    assert_eq!(out.code, 0);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["clispec"], "0.2");
    assert_eq!(v["commands"][0]["name"], "scan");
    assert_eq!(v["outcomes"][0]["code"], 1);
}

#[test]
fn help_mentions_schema() {
    let out = run(&["--help"]);
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("schema"));
}

#[test]
fn completions_bash_generates_script() {
    let out = run(&["completions", "bash"]);
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("badgevet"));
}

#[test]
fn no_target_and_no_readme_exits_3() {
    let dir = tempfile::tempdir().unwrap();
    let out = run_in(dir.path(), &[]);
    assert_eq!(out.code, 3, "stdout: {} stderr: {}", out.stdout, out.stderr);
    assert_eq!(error_envelope(&out.stderr)["kind"], "usage");
}

#[test]
fn missing_path_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = run_in(dir.path(), &["does-not-exist.md"]);
    assert_eq!(out.code, 2);
    assert_eq!(error_envelope(&out.stderr)["kind"], "io");
}

#[test]
fn file_without_badges_reports_none() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("doc.md"),
        "# Title\n\nJust text and a [link](https://example.com).\n",
    )
    .unwrap();
    // stdout is piped (not a TTY), so output is JSON.
    let out = run_in(dir.path(), &["doc.md"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["summary"]["total"], 0);
}

#[test]
fn stdin_dash_reads_markdown() {
    // Markdown with no badge-host images: no network, empty report, exit 0.
    let out = run_with_stdin(
        &["-"],
        "# Title\n\n[link](https://example.com) and prose.\n",
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["summary"]["total"], 0);
}
