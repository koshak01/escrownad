//! Sanitising HTML from the wysiwyg editor (Quill).
//!
//! The description and terms of a deal are written in an editor, so what
//! reaches the database is HTML. Rendering it on the deal page requires
//! `| safe`, and `| safe` over unchecked input is an XSS hole: any wallet
//! holder could publish a listing carrying a script.
//!
//! So the HTML is cleaned HERE, on the way in, against a whitelist of tags.
//! What reaches the template is already safe.

use std::collections::HashSet;
use std::sync::LazyLock;

/// Tags Quill can produce and that a listing description actually needs.
const ALLOWED_TAGS: &[&str] = &[
    "p",
    "br",
    "strong",
    "b",
    "em",
    "i",
    "u",
    "s",
    "ul",
    "ol",
    "li",
    "blockquote",
    "code",
    "pre",
    "h3",
    "h4",
    "a",
    "span",
];

static CLEANER: LazyLock<ammonia::Builder<'static>> = LazyLock::new(|| {
    let mut builder = ammonia::Builder::default();
    builder
        .tags(HashSet::from_iter(ALLOWED_TAGS.iter().copied()))
        // links: href only, and the schemes are restricted below
        .link_rel(Some("noopener noreferrer nofollow"))
        .url_schemes(HashSet::from_iter(["http", "https", "mailto"]));
    builder
});

/// Cleans editor HTML against a whitelist of tags.
///
/// How it works:
/// 1. strips anything outside [`ALLOWED_TAGS`] — scripts, iframes, handlers;
/// 2. keeps only safe schemes on links (http/https/mailto);
/// 3. returns `None` when nothing but markup is left, so that empty shells
///    like `<p><br></p>` never reach the database.
///
/// # Parameters
/// * `raw` — the original HTML from the wysiwyg
///
/// # Returns
/// * `Option<String>` — cleaned HTML, or `None` for empty content
pub fn rich_text(raw: &str) -> Option<String> {
    let clean = CLEANER.clean(raw).to_string();
    if is_blank(&clean) { None } else { Some(clean) }
}

/// Flat text without markup — for tables, tab titles and emails.
///
/// # Parameters
/// * `html` — the marked-up text
///
/// # Returns
/// * `String` — a single line, no tags and no double spaces
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Is the content empty: no text, no images — only hollow tags.
fn is_blank(html: &str) -> bool {
    plain_text(html).is_empty()
}
