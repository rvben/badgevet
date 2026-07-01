//! Which image hosts count as badges, and the curated map of known-dead badge
//! routes to their modern replacements.

/// Return the host portion of a URL, without scheme, userinfo, or port.
pub fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    (!host.is_empty()).then_some(host)
}

/// The badge provider for a URL, as a stable display name, or `None` if the URL
/// is not from a recognized badge host.
pub fn provider_name(url: &str) -> Option<&'static str> {
    match host_of(url)? {
        "img.shields.io" | "shields.io" => Some("shields.io"),
        "vsmarketplacebadges.dev" => Some("vsmarketplacebadges.dev"),
        "badgen.net" | "flat.badgen.net" => Some("badgen.net"),
        _ => None,
    }
}

/// Whether an image URL should be treated as a status badge.
pub fn is_badge_url(url: &str) -> bool {
    provider_name(url).is_some()
}

/// Suggest a modern replacement for a known-dead badge URL, if one exists.
///
/// shields.io retired its Visual Studio Marketplace routes; vsmarketplacebadges.dev
/// serves the same version, downloads, and rating metrics.
pub fn suggest(url: &str) -> Option<String> {
    const MARKER: &str = "/visual-studio-marketplace/";
    let host = host_of(url)?;
    if host != "img.shields.io" && host != "shields.io" {
        return None;
    }
    let pos = url.find(MARKER)?;
    let rest = &url[pos + MARKER.len()..];
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let mut segs = rest.split('/');
    let metric = segs.next()?;
    let ext = segs.next()?;
    if ext.is_empty() {
        return None;
    }
    let kind = match metric {
        "v" => "version",
        "d" => "downloads-short",
        "r" => "rating",
        _ => return None,
    };
    Some(format!("https://vsmarketplacebadges.dev/{kind}/{ext}.svg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_shields_hosts() {
        assert_eq!(
            provider_name("https://img.shields.io/crates/v/rumdl"),
            Some("shields.io")
        );
        assert_eq!(
            provider_name("https://shields.io/badge/x-y-blue"),
            Some("shields.io")
        );
        assert!(is_badge_url(
            "https://vsmarketplacebadges.dev/version/rvben.rumdl.svg"
        ));
        assert!(is_badge_url("https://badgen.net/npm/v/express"));
    }

    #[test]
    fn ignores_non_badge_images() {
        assert_eq!(provider_name("https://example.com/logo.png"), None);
        assert_eq!(provider_name("./docs/screenshot.png"), None);
        assert!(!is_badge_url(
            "https://raw.githubusercontent.com/x/y/main/logo.svg"
        ));
    }

    #[test]
    fn host_parsing_strips_port_and_userinfo() {
        assert_eq!(
            host_of("https://user@img.shields.io:443/x"),
            Some("img.shields.io")
        );
        assert_eq!(
            host_of("https://img.shields.io/x?y=z"),
            Some("img.shields.io")
        );
    }

    #[test]
    fn suggests_vsmarketplace_replacement() {
        assert_eq!(
            suggest("https://img.shields.io/visual-studio-marketplace/v/rvben.rumdl"),
            Some("https://vsmarketplacebadges.dev/version/rvben.rumdl.svg".into())
        );
        assert_eq!(
            suggest("https://img.shields.io/visual-studio-marketplace/d/rvben.rumdl"),
            Some("https://vsmarketplacebadges.dev/downloads-short/rvben.rumdl.svg".into())
        );
        assert_eq!(
            suggest("https://img.shields.io/visual-studio-marketplace/r/rvben.rumdl"),
            Some("https://vsmarketplacebadges.dev/rating/rvben.rumdl.svg".into())
        );
    }

    #[test]
    fn no_suggestion_for_unknown_dead_badges() {
        assert_eq!(
            suggest("https://img.shields.io/github/stars/rvben/rumdl"),
            None
        );
        assert_eq!(
            suggest("https://vsmarketplacebadges.dev/version/rvben.rumdl.svg"),
            None
        );
    }
}
