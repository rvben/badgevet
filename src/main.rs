//! badgevet CLI.
//!
//! Follows The CLI Spec (clispec.dev): text on a TTY, JSON when piped,
//! structured error envelopes on the last line of stderr, a `schema`
//! subcommand, and mutation markers. Finding broken badges is a non-zero
//! `outcome` (exit 1), not an error.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use badgevet::{
    Error, GitHubScope, OutputFormat, Request, ReqwestHttp, RetryPolicy, Source, render, run,
    schema,
};
use clap::error::ErrorKind as ClapErrorKind;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "badgevet",
    version,
    about = "Find retired and broken status badges in Markdown that link checkers miss",
    long_about = "Find retired and broken status badges in Markdown that link checkers miss.\n\nA retired shields.io badge returns HTTP 200 with an SVG that reads \"retired badge\", so ordinary link checkers pass it. badgevet reads the rendered badge instead.\n\nRun `badgevet schema` for the machine-readable contract (clispec.dev).",
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Markdown files or directories to scan (default: README.md in the cwd).
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,

    /// Scan an owner's GitHub repositories instead of local paths.
    #[arg(long, value_name = "OWNER", conflicts_with = "paths")]
    github: Option<String>,

    /// With --github: include forks.
    #[arg(long)]
    include_forks: bool,

    /// With --github: include archived repositories.
    #[arg(long)]
    include_archived: bool,

    /// With --github: include private repositories (needs a token with repo scope).
    #[arg(long)]
    include_private: bool,

    /// Report only permanently broken badges.
    #[arg(long)]
    only_broken: bool,

    /// Also exit 1 on unconfirmed badges, not just broken ones.
    #[arg(long)]
    strict: bool,

    /// Re-fetch an ambiguous badge this many times before reporting it unconfirmed.
    #[arg(long, default_value_t = 2)]
    retries: u32,

    /// Per-request HTTP timeout, in seconds.
    #[arg(long, default_value_t = 10)]
    timeout: u64,

    /// Output format; auto = text on a TTY, JSON when piped.
    #[arg(long, short = 'o', value_enum, default_value = "auto", global = true)]
    output: CliOutput,
}

#[derive(Subcommand)]
enum Command {
    /// Print the machine-readable contract (clispec.dev) as JSON.
    Schema,
    /// Generate a shell completion script.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum CliOutput {
    Auto,
    Json,
    Text,
}

impl CliOutput {
    fn resolve(self) -> OutputFormat {
        match self {
            CliOutput::Json => OutputFormat::Json,
            CliOutput::Text => OutputFormat::Table,
            CliOutput::Auto => {
                if std::io::stdout().is_terminal() {
                    OutputFormat::Table
                } else {
                    OutputFormat::Json
                }
            }
        }
    }
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => return handle_clap_error(e),
    };

    match &cli.command {
        Some(Command::Schema) => {
            println!("{}", schema::contract_json());
            return ExitCode::SUCCESS;
        }
        Some(Command::Completions { shell }) => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(*shell, &mut cmd, name, &mut std::io::stdout());
            return ExitCode::SUCCESS;
        }
        None => {}
    }

    let http = match ReqwestHttp::new(Duration::from_secs(cli.timeout)) {
        Ok(http) => http,
        Err(err) => {
            emit_error(&err);
            return ExitCode::from(err.exit_code() as u8);
        }
    };

    let source = match cli.github.clone() {
        Some(owner) => Source::GitHub(GitHubScope {
            owner,
            include_forks: cli.include_forks,
            include_archived: cli.include_archived,
            include_private: cli.include_private,
        }),
        None => Source::Paths(cli.paths.clone()),
    };

    let request = Request {
        source,
        format: cli.output.resolve(),
        strict: cli.strict,
        only_broken: cli.only_broken,
        retry: RetryPolicy {
            retries: cli.retries,
            backoff: Duration::from_millis(400),
        },
    };

    match run(&http, &request) {
        Ok(report) => {
            let out = render(&report, request.only_broken, request.format);
            let _ = writeln!(std::io::stdout(), "{out}");
            ExitCode::from(report.exit_code(request.strict) as u8)
        }
        Err(err) => {
            emit_error(&err);
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

/// Help and version print normally and exit 0; every other clap failure becomes
/// a structured `usage` error envelope.
fn handle_clap_error(e: clap::Error) -> ExitCode {
    match e.kind() {
        ClapErrorKind::DisplayHelp
        | ClapErrorKind::DisplayVersion
        | ClapErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            let _ = e.print();
            ExitCode::SUCCESS
        }
        _ => {
            let err = Error::Usage {
                message: e.to_string().trim().to_string(),
            };
            emit_error(&err);
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

/// Write the clispec error envelope as the last line of stderr.
fn emit_error(err: &Error) {
    let mut error = serde_json::Map::new();
    error.insert("kind".into(), json!(err.kind()));
    error.insert("message".into(), json!(err.to_string()));
    error.insert("exit_code".into(), json!(err.exit_code()));
    if let Some(hint) = err.hint() {
        error.insert("hint".into(), json!(hint));
    }
    eprintln!("{}", json!({ "error": error }));
}
