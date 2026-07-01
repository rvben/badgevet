//! Extract image references from Markdown, with source line numbers.
//!
//! Both Markdown images (`![alt](url)`, including reference-style) and inline
//! `<img src="url">` HTML are captured. Filtering to badge hosts is left to the
//! caller, so this module stays provider-agnostic and easy to test.

use crate::model::Badge;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Extract every image reference from `content`, tagging each with `file` and a
/// 1-based line number.
pub fn extract_images(content: &str, file: &str) -> Vec<Badge> {
    let lines = LineIndex::new(content);
    let mut badges = Vec::new();
    // (url, line, accumulated alt text) for the image currently being parsed.
    let mut current: Option<(String, usize, String)> = None;

    for (event, range) in Parser::new_ext(content, Options::empty()).into_offset_iter() {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                current = Some((
                    dest_url.to_string(),
                    lines.line_of(range.start),
                    String::new(),
                ));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, alt)) = current.as_mut() {
                    alt.push_str(&text);
                }
            }
            Event::End(TagEnd::Image) => {
                if let Some((url, line, alt)) = current.take() {
                    badges.push(Badge {
                        file: file.to_string(),
                        line,
                        label: alt.trim().to_string(),
                        url,
                    });
                }
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                for (offset, url, alt) in img_tags(&html) {
                    badges.push(Badge {
                        file: file.to_string(),
                        line: lines.line_of(range.start + offset),
                        label: alt.trim().to_string(),
                        url,
                    });
                }
            }
            _ => {}
        }
    }
    badges
}

/// Byte-offset to 1-based line-number lookup.
struct LineIndex {
    /// Byte offset of the start of each line.
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(content: &str) -> Self {
        let mut starts = vec![0];
        for (i, b) in content.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        Self { starts }
    }

    fn line_of(&self, offset: usize) -> usize {
        // Number of line-starts at or before `offset` is the 1-based line number.
        self.starts.partition_point(|&s| s <= offset).max(1)
    }
}

/// Find every `<img>` tag in an HTML fragment, returning `(byte offset within
/// the fragment, src, alt)` for each one that has a `src`.
fn img_tags(html: &str) -> Vec<(usize, String, String)> {
    let lower = html.to_lowercase();
    let mut out = Vec::new();
    let mut search = 0;
    while let Some(rel) = lower[search..].find("<img") {
        let start = search + rel;
        let end = html[start..].find('>').map_or(html.len(), |e| start + e);
        let tag = &html[start..end];
        if let Some(src) = attr_value(tag, "src") {
            let alt = attr_value(tag, "alt").unwrap_or_default();
            out.push((start, src, alt));
        }
        search = end;
    }
    out
}

/// Read the value of an HTML attribute from a single tag, honoring double,
/// single, or unquoted values. Only whole-word attribute names match, so `src`
/// does not match inside `data-src`.
fn attr_value(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let mut from = 0;
    loop {
        let idx = from + lower[from..].find(attr)?;
        from = idx + attr.len();
        // The attribute name must be preceded by a boundary (not part of a
        // longer name like `data-src`).
        let boundary = idx == 0
            || !matches!(tag.as_bytes()[idx - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_');
        if !boundary {
            continue;
        }
        let after = tag[from..].trim_start();
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let (quote, body) = match value.strip_prefix('"') {
            Some(rest) => ('"', rest),
            None => match value.strip_prefix('\'') {
                Some(rest) => ('\'', rest),
                None => {
                    let stop = value.find(char::is_whitespace).unwrap_or(value.len());
                    return Some(value[..stop].to_string());
                }
            },
        };
        let stop = body.find(quote).unwrap_or(body.len());
        return Some(body[..stop].to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(content: &str) -> Vec<String> {
        extract_images(content, "README.md")
            .into_iter()
            .map(|b| b.url)
            .collect()
    }

    #[test]
    fn extracts_linked_badge_with_line_and_alt() {
        let md = "# Title\n\n[![Build](https://img.shields.io/x)](https://ci)\n";
        let badges = extract_images(md, "README.md");
        assert_eq!(badges.len(), 1);
        assert_eq!(badges[0].url, "https://img.shields.io/x");
        assert_eq!(badges[0].label, "Build");
        assert_eq!(badges[0].line, 3);
    }

    #[test]
    fn extracts_plain_and_reference_style_images() {
        let md = "![a](https://img.shields.io/a)\n\n![b][ref]\n\n[ref]: https://img.shields.io/b\n";
        assert_eq!(
            urls(md),
            vec!["https://img.shields.io/a", "https://img.shields.io/b"]
        );
    }

    #[test]
    fn extracts_html_img_tags() {
        let md =
            "<p align=\"center\">\n  <img alt=\"cov\" src=\"https://img.shields.io/c\">\n</p>\n";
        let badges = extract_images(md, "README.md");
        assert_eq!(badges.len(), 1);
        assert_eq!(badges[0].url, "https://img.shields.io/c");
        assert_eq!(badges[0].label, "cov");
        assert_eq!(badges[0].line, 2);
    }

    #[test]
    fn src_does_not_match_data_src() {
        let tag = "<img data-src=\"decoy\" src=\"real\">";
        assert_eq!(attr_value(tag, "src"), Some("real".to_string()));
    }

    #[test]
    fn no_images_yields_empty() {
        assert!(urls("just some **markdown** with [a link](https://example.com)").is_empty());
    }
}
