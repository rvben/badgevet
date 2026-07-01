//! Fetching and checking a single badge.
//!
//! [`Http`] is the seam: [`ReqwestHttp`] hits the network, while tests
//! substitute a fake that returns canned SVG bodies, so the classification and
//! retry logic is exercised entirely offline.

use crate::classify::{Verdict, classify_title};
use crate::model::State;
use std::time::Duration;

/// A minimal HTTP GET. `Ok(body)` on 2xx, `Err(reason)` on non-2xx or transport
/// failure. A failed fetch is not fatal: the caller treats it as ambiguous.
pub trait Http: Sync {
    fn get(&self, url: &str) -> Result<String, String>;
}

/// The real client: reqwest blocking with rustls.
pub struct ReqwestHttp {
    client: reqwest::blocking::Client,
}

impl ReqwestHttp {
    pub fn new(timeout: Duration) -> Result<Self, crate::Error> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("badgevet/", env!("CARGO_PKG_VERSION")))
            .timeout(timeout)
            .build()
            .map_err(|e| crate::Error::Http {
                message: e.to_string(),
            })?;
        Ok(Self { client })
    }
}

impl Http for ReqwestHttp {
    fn get(&self, url: &str) -> Result<String, String> {
        let resp = self.client.get(url).send().map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}"));
        }
        resp.text().map_err(|e| e.to_string())
    }
}

/// How many times to re-fetch a badge whose result is ambiguous, and how long to
/// wait between attempts (linear backoff).
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub retries: u32,
    pub backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            retries: 2,
            backoff: Duration::from_millis(400),
        }
    }
}

/// Check one badge URL: fetch, read its `<title>`, classify. Deterministic
/// dead/healthy states return on the first response; ambiguous ones are retried
/// and, if never resolved, become [`State::Unconfirmed`].
///
/// `sleep` is injected so tests drive the retry path without real delays.
pub fn check(
    http: &dyn Http,
    url: &str,
    policy: RetryPolicy,
    sleep: &dyn Fn(Duration),
) -> (State, Option<String>) {
    let mut last: Option<String> = None;
    for attempt in 0..=policy.retries {
        match http.get(url) {
            Ok(body) => match classify_title(&extract_title(&body).unwrap_or_default()) {
                Verdict::Healthy(msg) => return (State::Ok, Some(msg)),
                Verdict::Broken(msg) => return (State::Broken, Some(msg)),
                Verdict::Ambiguous(msg) => {
                    last = Some(if msg.is_empty() {
                        "no title in response".to_string()
                    } else {
                        msg
                    });
                }
            },
            Err(reason) => last = Some(reason),
        }
        if attempt < policy.retries {
            sleep(policy.backoff * (attempt + 1));
        }
    }
    (State::Unconfirmed, last)
}

/// Extract the text of the first `<title>...</title>` element in an SVG/HTML body.
pub fn extract_title(body: &str) -> Option<String> {
    let lower = body.to_lowercase();
    let open = lower.find("<title")?;
    let text_start = open + lower[open..].find('>')? + 1;
    let close = text_start + lower[text_start..].find("</title>")?;
    Some(body[text_start..close].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeHttp {
        responses: HashMap<String, Result<String, String>>,
    }

    impl Http for FakeHttp {
        fn get(&self, url: &str) -> Result<String, String> {
            self.responses
                .get(url)
                .cloned()
                .unwrap_or_else(|| Err("unmapped url".into()))
        }
    }

    fn svg(title: &str) -> String {
        format!("<svg xmlns=\"http://www.w3.org/2000/svg\"><title>{title}</title></svg>")
    }

    fn no_sleep(_: Duration) {}

    #[test]
    fn extracts_svg_title() {
        assert_eq!(
            extract_title(&svg("crates.io: v1.0")),
            Some("crates.io: v1.0".to_string())
        );
        assert_eq!(extract_title("<svg></svg>"), None);
    }

    #[test]
    fn healthy_badge_returns_ok() {
        let http = FakeHttp {
            responses: HashMap::from([("u".to_string(), Ok(svg("build: passing")))]),
        };
        let (state, rendered) = check(&http, "u", RetryPolicy::default(), &no_sleep);
        assert_eq!(state, State::Ok);
        assert_eq!(rendered.as_deref(), Some("build: passing"));
    }

    #[test]
    fn retired_badge_returns_broken_without_retrying() {
        let http = FakeHttp {
            responses: HashMap::from([("u".to_string(), Ok(svg("marketplace: retired badge")))]),
        };
        let (state, _) = check(&http, "u", RetryPolicy::default(), &no_sleep);
        assert_eq!(state, State::Broken);
    }

    #[test]
    fn persistently_invalid_badge_becomes_unconfirmed() {
        let http = FakeHttp {
            responses: HashMap::from([("u".to_string(), Ok(svg("stars: invalid")))]),
        };
        let policy = RetryPolicy {
            retries: 2,
            backoff: Duration::ZERO,
        };
        let (state, rendered) = check(&http, "u", policy, &no_sleep);
        assert_eq!(state, State::Unconfirmed);
        assert_eq!(rendered.as_deref(), Some("stars: invalid"));
    }

    #[test]
    fn transport_error_becomes_unconfirmed() {
        let http = FakeHttp {
            responses: HashMap::new(),
        };
        let (state, _) = check(&http, "missing", RetryPolicy::default(), &no_sleep);
        assert_eq!(state, State::Unconfirmed);
    }
}
