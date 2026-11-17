//! Small string helpers that mirror .NET `StringComparison.OrdinalIgnoreCase`.
//!
//! The C# code compares ASCII identifiers (process names, hosts, URL schemes) case-insensitively.
//! These helpers do the same for ASCII; non-ASCII letters compare exactly, which is what the
//! inputs here (hosts, process names, origins) need.

pub fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

pub fn ends_with_ignore_case(s: &str, suffix: &str) -> bool {
    s.len() >= suffix.len()
        && s.as_bytes()[s.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes())
}

pub fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

/// `string.IsNullOrWhiteSpace`.
pub fn is_blank(s: Option<&str>) -> bool {
    s.is_none_or(|v| v.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert!(starts_with_ignore_case(
            "Chrome-Extension://x",
            "chrome-extension://"
        ));
        assert!(!starts_with_ignore_case("ch", "chrome"));
        assert!(ends_with_ignore_case("a.YouTube.com", ".youtube.com"));
        assert!(contains_ignore_case(
            "https://WWW.YOUTUBE.COM/watch",
            "youtube.com"
        ));
        assert!(is_blank(None) && is_blank(Some("  ")) && !is_blank(Some("x")));
    }
}
