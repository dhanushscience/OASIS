//! Reading the host and path out of what a browser's address bar shows. Port of
//! `ProfileMatcher.TryGetUrlHost`, `SiteLabel` and the path half of `UrlHostOrPathMatches`.
//!
//! Browsers often show the address without its scheme (`youtube.com/watch?v=…`), and UI
//! Automation sometimes returns odd strings, so this is deliberately forgiving.

use super::text::contains_ignore_case;

/// The host of `raw`, lower case. Accepts `https://host/…`, `//host/…` and `host/…`.
///
/// An address with a scheme is trusted to have a host; one without must look like a domain
/// (contain a dot), so a search phrase typed in the bar is not mistaken for a site.
pub fn try_get_url_host(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.contains("://") {
        if let Some(host) = host_of(raw) {
            return Some(host);
        }
    }
    let candidate = if raw.starts_with("//") {
        format!("https:{raw}")
    } else if !raw.contains("://") {
        format!("https://{}", raw.trim_start_matches('/'))
    } else {
        raw.to_string()
    };
    host_of(&candidate).filter(|host| host.contains('.'))
}

/// The host of an address that has a scheme and `://` (`https://www.netflix.com/x`), lower case.
/// `None` for anything else, including `mail.proton.me/inbox`, which has no scheme.
pub fn absolute_host(raw: &str) -> Option<String> {
    let raw = raw.trim();
    raw.contains("://").then(|| host_of(raw)).flatten()
}

/// The host of `scheme://[user@]host[:port]/…`.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed.split(']').next()?
    } else {
        authority.split(':').next()?
    };
    let host = host.trim();
    (!host.is_empty() && !host.chars().any(char::is_whitespace)).then(|| host.to_lowercase())
}

/// The path of an address, `/` if it has none. The query and fragment are not part of it.
pub fn url_path(raw: &str) -> String {
    let raw = raw.trim();
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("https://{}", raw.trim_start_matches('/'))
    };
    let Some((_, rest)) = with_scheme.split_once("://") else { return "/".into() };
    let after_authority = rest.find(['/', '?', '#']).map_or("", |i| &rest[i..]);
    let path = after_authority.split(['?', '#']).next().unwrap_or("");
    if path.is_empty() { "/".into() } else { path.into() }
}

/// Whether `fragment` appears in the address's host or path, never in the query string.
pub fn url_host_or_path_contains(url: &str, fragment: &str) -> bool {
    if url.trim().is_empty() || fragment.trim().is_empty() {
        return false;
    }
    let needle = fragment.trim();
    let Some(host) = try_get_url_host(url) else {
        // Last resort for odd strings: what comes before any ? or #.
        let lower = url.trim().to_lowercase();
        let before_query = lower.split(['?', '#']).next().unwrap_or("");
        return before_query.contains(&needle.to_lowercase());
    };
    let path = url_path(url);
    // `youtube.com/shorts` spans host and path, so it is also tried against both joined.
    contains_ignore_case(&host, needle)
        || contains_ignore_case(&path, needle)
        || contains_ignore_case(&format!("{host}{path}"), needle.trim_start_matches("https://").trim_start_matches("http://"))
}

/// Whether a stored match names a page inside a site (`youtube.com/shorts`), not just the site.
pub fn has_path(context_match: &str) -> bool {
    let m = context_match.trim();
    m.split_once("://").map_or(m, |(_, rest)| rest).trim_end_matches('/').contains('/')
}

/// The name a site puts in its page titles: `www.netflix.com/browse` → `netflix`.
pub fn site_label(site: &str) -> String {
    let mut host = site.trim();
    if let Some(i) = host.find("://") {
        host = &host[i + 3..];
    }
    let host = host.split(['/', '?', '#']).next().unwrap_or("");
    let parts: Vec<&str> = host.split('.').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2 { parts[parts.len() - 2].to_string() } else { host.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_can_span_host_and_path() {
        assert!(url_host_or_path_contains("https://www.youtube.com/shorts/abc?x=1", "youtube.com/shorts"));
        assert!(!url_host_or_path_contains("https://www.youtube.com/watch?v=shorts", "youtube.com/shorts"));
        assert!(has_path("youtube.com/shorts") && has_path("https://a.com/x/") && !has_path("youtube.com/") && !has_path("netflix"));
    }

    #[test]
    fn hosts_from_full_and_scheme_less_addresses() {
        let host = |s| try_get_url_host(s);
        assert_eq!(host("https://www.YouTube.com/watch?v=1").as_deref(), Some("www.youtube.com"));
        assert_eq!(host("youtube.com/watch?v=1").as_deref(), Some("youtube.com"));
        assert_eq!(host("//netflix.com/browse").as_deref(), Some("netflix.com"));
        assert_eq!(host("https://user:pw@example.org:8080/x").as_deref(), Some("example.org"));
        assert_eq!(host("http://localhost:3000/").as_deref(), Some("localhost"));
    }

    #[test]
    fn a_bare_phrase_is_not_a_site() {
        assert_eq!(try_get_url_host("how to cook rice"), None);
        assert_eq!(try_get_url_host("localhost:3000"), None);
        assert_eq!(try_get_url_host("  "), None);
        assert_eq!(try_get_url_host(""), None);
    }

    #[test]
    fn matches_the_host_or_the_path_but_not_the_query() {
        assert!(url_host_or_path_contains("netflix.com/watch/80014185?trackId=1", "netflix.com"));
        assert!(url_host_or_path_contains("https://github.com/rust-lang/rust", "rust-lang"));
        assert!(!url_host_or_path_contains("https://example.com/?q=netflix.com", "netflix.com"));
        assert!(!url_host_or_path_contains("", "x"));
        assert!(!url_host_or_path_contains("example.org", ""));
    }

    #[test]
    fn a_phrase_in_the_bar_is_matched_before_its_query_part_only() {
        assert!(url_host_or_path_contains("netflix movies?x=1", "netflix"));
        assert!(!url_host_or_path_contains("movies?x=netflix", "netflix"));
    }

    #[test]
    fn paths() {
        assert_eq!(url_path("https://a.com/x/y?z=1#f"), "/x/y");
        assert_eq!(url_path("a.com"), "/");
        assert_eq!(url_path("a.com?q=1"), "/");
    }

    #[test]
    fn the_label_a_site_uses_in_titles() {
        assert_eq!(site_label("netflix.com"), "netflix");
        assert_eq!(site_label("www.netflix.com/browse"), "netflix");
        assert_eq!(site_label("https://music.youtube.com/x"), "youtube");
        assert_eq!(site_label("netflix"), "netflix");
    }
}
