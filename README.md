# badgevet

[![CI](https://github.com/rvben/badgevet/actions/workflows/ci.yml/badge.svg)](https://github.com/rvben/badgevet/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/badgevet.svg)](https://crates.io/crates/badgevet)
[![clispec](https://img.shields.io/badge/clispec-v0.2-blue)](https://clispec.dev)

Find retired and broken status badges in Markdown that link checkers miss.

A retired shields.io badge returns **HTTP 200** with a valid SVG whose title
reads `retired badge`, so ordinary link checkers (lychee, markdown-link-check)
mark it OK. badgevet fetches each badge and reads the *rendered* SVG title
instead, so it can tell a dead badge from a healthy one.

## Install

```sh
cargo install badgevet
# or a prebuilt binary, no recompile:
cargo binstall badgevet
```

## Usage

```sh
badgevet                    # scan README.md in the current directory
badgevet docs/ README.md    # scan files and directories (dirs recurse for *.md)
badgevet ~/Projects/mine    # one command scans every README under all your local repos
badgevet --only-broken .    # report only permanently broken badges
badgevet | jq .             # JSON when piped
```

### Check every repo you own

Point badgevet at an owner and it checks the **canonical published README** of
each of their repositories - no cloning needed. It reads the same README GitHub
shows the world, so it catches badge rot even in repos you rarely touch.

```sh
badgevet --github rvben              # all your public, non-fork, non-archived repos
badgevet --github rvben --only-broken
```

Set `GITHUB_TOKEN` (or `GH_TOKEN`) to lift GitHub's unauthenticated rate limit;
required for `--include-private`. Widen the default scope with
`--include-forks`, `--include-archived`, `--include-private`.

Example:

```text
STATE        PROVIDER               LOCATION                   BADGE
broken       shields.io             README.md:3                Version
             -> https://vsmarketplacebadges.dev/version/rvben.rumdl.svg

3 checked · 2 ok · 1 broken · 0 unconfirmed
```

## How it classifies

Each badge lands in one of three states:

| State | Meaning | Fails CI? |
| --- | --- | --- |
| `ok` | The badge renders a real value. | no |
| `broken` | A deterministic dead state (`retired`, `deprecated`). | **yes (exit 1)** |
| `unconfirmed` | Could not be verified: an ambiguous `invalid` / `inaccessible` / empty title, or a transient network failure. Retried with backoff first. | no (unless `--strict`) |

The distinction is the point. shields.io renders `invalid` both for a genuinely
bad badge and when its upstream API is merely rate-limiting, so `unconfirmed`
never fails your build by default. Only an explicit, permanent dead state does.

When a known-dead pattern has a modern replacement (e.g. shields.io's retired
Visual Studio Marketplace routes), badgevet prints the suggested URL.

## Fixing

`scan` never touches your files. The separate `fix` command applies those
suggestions in place:

```sh
badgevet fix              # rewrite broken badges in README.md
badgevet fix docs/        # ...or across a directory of Markdown
```

It swaps only the badge image URL (leaving the surrounding link untouched),
changes only broken badges that have a known replacement, and is idempotent.
Broken badges with no known replacement are left alone and reported as
`unfixable` (exit 1). `fix` is local-only; it does not work with `--github`.
Since `scan` is read-only and `fix` mutates, they are separate commands with
honest `mutating` markers in the [schema](https://clispec.dev).

## Options

| Flag | Default | Description |
| --- | --- | --- |
| `--github <owner>` | - | Scan an owner's GitHub repos instead of local paths. |
| `--include-forks` | off | With `--github`: include forks. |
| `--include-archived` | off | With `--github`: include archived repos. |
| `--include-private` | off | With `--github`: include private repos (needs a token). |
| `--only-broken` | off | Report only permanently broken badges. |
| `--strict` | off | Also exit 1 on `unconfirmed` badges. |
| `--retries <n>` | `2` | Re-fetch an ambiguous badge before giving up. |
| `--timeout <secs>` | `10` | Per-request HTTP timeout. |
| `-o, --output <fmt>` | `auto` | `auto` (text on a TTY, JSON when piped), `json`, `text`. |

## Exit codes

| code | meaning |
| --- | --- |
| `0` | no broken badges |
| `1` | at least one badge is permanently broken (or, with `--strict`, unconfirmed) |
| `2` | a path could not be read, or the HTTP client failed to build |
| `3` | usage error |

Exit `1` is an [outcome](https://clispec.dev), not an error: stdout still carries
the full report and no error envelope is written. This makes badgevet a natural
CI or pre-commit gate.

## For agents (clispec)

badgevet follows [The CLI Spec](https://clispec.dev): structured output on
stdout, structured error envelopes on the last line of stderr, and a `schema`
subcommand whose output validates against `clispec.dev/schema/v0.2.json`
(checked by the test suite).

```sh
badgevet schema
```

## License

MIT
