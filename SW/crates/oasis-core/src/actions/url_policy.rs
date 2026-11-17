//! Port of `Services/UrlLaunchPolicy.cs`: the allowlist for URLs the app may open.
//!
//! Untrusted data (LLM plans, marketplace packages, `OpenUrl` buttons) may only open these
//! schemes. Nothing is sanitised: an unknown scheme is rejected.
//!
//! C# used .NET's `Uri` parser. Here the scheme is read by hand, and anything with control
//! characters is refused outright, so the check and the operating system can never read the
//! scheme differently. The returned text is not normalised the way `Uri.AbsoluteUri` did
//! (no added trailing slash or lower-cased host); spaces in a web address become `%20`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlScope {
    /// Pad / voice / AI `OpenUrl` steps: https, http, spotify.
    Automation,
    /// Calendar join links: the above plus conferencing protocols.
    MeetingJoin,
    /// Marketplace catalog and package download: https only.
    Catalog,
}

const AUTOMATION: [&str; 3] = ["https", "http", "spotify"];
const MEETING: [&str; 8] = ["https", "http", "spotify", "zoommtg", "msteams", "slack", "webex", "tel"];

const BLOCKED_PREFIXES: [&str; 7] =
    ["javascript:", "data:", "vbscript:", "file:", "ms-msdt:", "ms-appinstaller:", "search-ms:"];

/// The URL to open, or `None` if it may not be opened.
pub fn try_allow(raw: &str, scope: UrlScope) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    if BLOCKED_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return None;
    }

    let (scheme, candidate) = match scheme_of(value) {
        Some(scheme) => (scheme.to_ascii_lowercase(), value.to_string()),
        None => {
            // "example.com/path" with no scheme: assume https, except for the catalog.
            if scope == UrlScope::Catalog {
                return None;
            }
            ("https".to_string(), format!("https://{}", value.trim_start_matches('/')))
        }
    };

    let allowed: &[&str] = match scope {
        UrlScope::Catalog => &["https"],
        UrlScope::MeetingJoin => &MEETING,
        UrlScope::Automation => &AUTOMATION,
    };
    if !allowed.contains(&scheme.as_str()) {
        return None;
    }

    if scheme == "http" || scheme == "https" {
        let host = host_of(&candidate)?;
        if host.is_empty() || host.chars().any(char::is_whitespace) {
            return None;
        }
        return Some(candidate.replace(' ', "%20"));
    }
    Some(candidate)
}

/// The text before the first `:`, if it is a valid scheme (RFC 3986). Single letters are drive
/// letters, not schemes, and fall through to "not allowed".
fn scheme_of(value: &str) -> Option<&str> {
    let colon = value.find(':')?;
    let scheme = &value[..colon];
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then_some(scheme)
}

/// The host of `scheme://host/...` (userinfo and port removed). `None` without `//`.
fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    authority.split(':').next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto(s: &str) -> Option<String> {
        try_allow(s, UrlScope::Automation)
    }

    #[test]
    fn the_csharp_allowlist_cases() {
        assert!(auto("https://example.com").is_some());
        assert!(auto("http://example.com/path").is_some());
        assert!(auto("spotify:track:abc").is_some());
        for bad in [
            "javascript:alert(1)",
            "file:///C:/Windows/System32/calc.exe",
            "ms-msdt:something",
            "data:text/html,hi",
        ] {
            assert_eq!(auto(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_catalog_needs_https_and_a_scheme() {
        let catalog = |s| try_allow(s, UrlScope::Catalog);
        assert_eq!(catalog("http://catalog.example/index.json"), None);
        assert_eq!(catalog("catalog.example/index.json"), None);
        assert!(catalog("https://catalog.example/index.json").unwrap().starts_with("https://"));
    }

    #[test]
    fn conferencing_protocols_are_for_meeting_links_only() {
        let link = "zoommtg://zoom.us/join?action=join";
        assert!(try_allow(link, UrlScope::MeetingJoin).is_some());
        assert_eq!(try_allow(link, UrlScope::Automation), None);
        assert!(try_allow("tel:+15551234567", UrlScope::MeetingJoin).is_some());
    }

    #[test]
    fn a_bare_address_is_taken_as_https() {
        assert_eq!(auto("example.com/a").as_deref(), Some("https://example.com/a"));
        assert_eq!(auto("//example.com").as_deref(), Some("https://example.com"));
        assert_eq!(auto("  netflix.com  ").as_deref(), Some("https://netflix.com"));
    }

    #[test]
    fn other_schemes_and_blocked_prefixes_are_refused_in_any_case() {
        for bad in [
            "ftp://x.com",
            "mailto:a@b.c",
            "C:\\Windows\\notepad.exe",
            "JavaScript:alert(1)",
            "  vbscript:x",
            "search-ms:query=a",
            "ms-appinstaller:?source=http://x",
            "localhost:8080",
        ] {
            assert_eq!(auto(bad), None, "{bad}");
        }
    }

    #[test]
    fn control_characters_never_pass() {
        for bad in ["https://exa\nmple.com", "https://x.com/\u{0}", "java\tscript:alert(1)", "https://x.com\r\n--flag"] {
            assert_eq!(auto(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn web_addresses_need_a_host_and_spaces_are_escaped() {
        assert_eq!(auto("https://"), None);
        assert_eq!(auto("https:///path"), None);
        assert_eq!(auto("https://my host.com"), None);
        assert_eq!(auto("https://example.com/a b").as_deref(), Some("https://example.com/a%20b"));
        assert_eq!(auto(""), None);
        assert_eq!(auto("   "), None);
    }

    #[test]
    fn userinfo_cannot_hide_the_host() {
        assert!(auto("https://user:pw@example.com/").is_some());
        assert_eq!(auto("https://@/"), None);
    }
}
