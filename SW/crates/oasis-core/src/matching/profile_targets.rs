//! Port of `Matching/ProfileTargets.cs`: kind inference and `WebsiteMatch` (the website
//! catalog is `web_catalog`).
//!
//! A profile's kind (`"app"` or `"website"`) is stored on the profile. Guessing from the match
//! text is only for profiles saved before the field existed: the v1→v2 migration, and any
//! profile that still has no kind.

use super::text::{ends_with_ignore_case, is_blank, starts_with_ignore_case};
use super::url_host::absolute_host;
use super::web_catalog::{guess_url, try_resolve};
use super::YouTubeHosts;

pub const APP_KIND: &str = "app";
pub const WEBSITE_KIND: &str = "website";

/// The stored kind when there is one, otherwise the guess from the match text.
pub fn is_website(kind: Option<&str>, context_match: Option<&str>) -> bool {
    kind.is_some_and(|k| k.trim().eq_ignore_ascii_case(WEBSITE_KIND))
        || (is_blank(kind) && infer_kind(context_match) == WEBSITE_KIND)
}

/// The kind of a profile saved without one: a host or URL ("netflix.com", "https://…") or the
/// legacy bare "youtube" is a website; a process name or path is an application; "*" has none.
/// Deliberately not the website catalog: "claude", "notion" and "gemini" are desktop apps too.
pub fn infer_kind(context_match: Option<&str>) -> &'static str {
    let m = context_match.unwrap_or("").trim();
    if m.is_empty() || m == "*" {
        return "";
    }
    if m.contains('\\') || ends_with_ignore_case(m, ".exe") {
        return APP_KIND;
    }
    if YouTubeHosts::is_name(Some(m)) {
        return WEBSITE_KIND;
    }
    if m.contains('.') || m.contains("://") {
        WEBSITE_KIND
    } else {
        APP_KIND
    }
}

/// The host to store for what the user typed in website mode: a URL or domain gives its host
/// (without `www.`), a known site name gives the catalog's host, and any other bare word gets
/// `.com`.
pub fn website_match(target: &str) -> String {
    let text = target.trim();
    if text.is_empty() {
        return String::new();
    }
    if let Some(host) = absolute_host(text) {
        return strip_www(&host);
    }
    if !text.contains('.') && !text.contains('/') {
        let url = try_resolve(text).map_or_else(|| guess_url(text), str::to_string);
        return match absolute_host(&url) {
            Some(host) if !url.is_empty() => strip_www(&host),
            _ => text.to_string(),
        };
    }
    match absolute_host(&format!("https://{text}")) {
        Some(host) => strip_www(&host),
        None => text.trim_matches('/').split('/').next().unwrap_or("").to_string(),
    }
}

fn strip_www(host: &str) -> String {
    if starts_with_ignore_case(host, "www.") { host[4..].to_string() } else { host.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_website_becomes_a_host() {
        for (typed, host) in [
            ("netflix", "netflix.com"),
            ("Netflix", "netflix.com"),
            ("https://www.netflix.com/title/1", "netflix.com"),
            ("www.netflix.com", "netflix.com"),
            ("youtube", "youtube.com"),
            ("somenewsite", "somenewsite.com"),
            ("mail.proton.me/inbox", "mail.proton.me"),
            ("proton mail", "mail.proton.me"),
            ("stack exchange", "stackexchange.com"),
            ("  github  ", "github.com"),
        ] {
            assert_eq!(website_match(typed), host, "{typed}");
        }
        assert_eq!(website_match(""), "");
        assert_eq!(website_match("   "), "");
    }

    #[test]
    fn infers_the_kind_a_profile_was_shown_as() {
        assert_eq!(infer_kind(Some("*")), "");
        assert_eq!(infer_kind(None), "");
        assert_eq!(infer_kind(Some("Claude")), APP_KIND);
        assert_eq!(infer_kind(Some("netflix.com")), WEBSITE_KIND);
        assert_eq!(infer_kind(Some("youtube")), WEBSITE_KIND);
        assert_eq!(infer_kind(Some("Code")), APP_KIND);
        assert_eq!(infer_kind(Some(r"C:\apps\tool.exe")), APP_KIND);
        assert_eq!(infer_kind(Some("tool.EXE")), APP_KIND);
        assert_eq!(infer_kind(Some("https://example.com/x")), WEBSITE_KIND);
    }

    #[test]
    fn stored_kind_wins_over_the_guess() {
        assert!(is_website(Some("website"), Some("Code")));
        assert!(!is_website(Some("app"), Some("netflix.com")));
        assert!(is_website(None, Some("netflix.com")));
        assert!(!is_website(Some(""), Some("Code")));
    }
}
