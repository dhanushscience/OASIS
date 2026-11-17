//! Port of `Matching/YouTubeHosts.cs`.
//!
//! What counts as YouTube, in one place. The profile matcher, the intent normalizer and the
//! browser-open parser each used to carry their own host and spoken-name checks and had
//! drifted (the normalizer did not know "yt music").

use super::text::{contains_ignore_case, ends_with_ignore_case, is_blank};

pub struct YouTubeHosts;

impl YouTubeHosts {
    pub const HOME: &'static str = "https://www.youtube.com";
    pub const MUSIC: &'static str = "https://music.youtube.com";

    /// The bare site name as a profile match or catalog key.
    pub fn is_name(value: Option<&str>) -> bool {
        value.is_some_and(|v| v.trim().eq_ignore_ascii_case("youtube"))
    }

    /// A host name, without scheme or path: youtube.com, any subdomain of it, or youtu.be.
    pub fn is_host(host: Option<&str>) -> bool {
        if is_blank(host) {
            return false;
        }
        let host = host.unwrap_or("");
        host.eq_ignore_ascii_case("youtu.be")
            || host.eq_ignore_ascii_case("youtube.com")
            || ends_with_ignore_case(host, ".youtube.com")
    }

    /// A profile's stored match that targets YouTube. The bare name "youtube" has no dot but
    /// still needs the address-bar check, so it counts alongside the hosts.
    pub fn is_context_match(context_match: Option<&str>) -> bool {
        let value = context_match.unwrap_or("").trim();
        Self::is_name(Some(value))
            || value.eq_ignore_ascii_case("youtu.be")
            || contains_ignore_case(value, "youtube.com")
    }

    /// What a person says for the site itself: "youtube", "youtu" or "yt".
    pub fn is_spoken_site(token: Option<&str>) -> bool {
        let v = token.unwrap_or("").trim();
        ["youtube", "youtu", "yt"]
            .iter()
            .any(|s| v.eq_ignore_ascii_case(s))
    }

    /// What a person says for the music site: "youtube music" or "yt music".
    pub fn is_spoken_music(token: Option<&str>) -> bool {
        let v = token.unwrap_or("").trim();
        ["youtube music", "yt music"]
            .iter()
            .any(|s| v.eq_ignore_ascii_case(s))
    }
}

#[cfg(test)]
mod tests {
    use super::YouTubeHosts as Y;

    #[test]
    fn these_hosts_are_youtube() {
        for h in [
            "youtube.com",
            "www.youtube.com",
            "music.youtube.com",
            "m.youtube.com",
            "YOUTUBE.COM",
            "youtu.be",
        ] {
            assert!(Y::is_host(Some(h)), "{h}");
        }
    }

    #[test]
    fn these_hosts_are_not() {
        for h in [
            None,
            Some(""),
            Some("  "),
            Some("netflix.com"),
            Some("notyoutube.com"),
            Some("youtube.com.example.org"),
            Some("youtu.be.example.org"),
        ] {
            assert!(!Y::is_host(h), "{h:?}");
        }
    }

    #[test]
    fn these_profile_matches_target_youtube() {
        for m in [
            "youtube",
            "YouTube",
            "  youtube ",
            "youtu.be",
            "youtube.com",
            "https://www.youtube.com/watch?v=abc",
        ] {
            assert!(Y::is_context_match(Some(m)), "{m}");
        }
        for m in [None, Some(""), Some("*"), Some("netflix"), Some("chrome")] {
            assert!(!Y::is_context_match(m), "{m:?}");
        }
    }

    #[test]
    fn the_bare_name_is_only_the_bare_name() {
        for (v, e) in [
            (Some("youtube"), true),
            (Some("YouTube"), true),
            (Some(" youtube "), true),
            (Some("youtube.com"), false),
            (Some("youtube music"), false),
            (None, false),
        ] {
            assert_eq!(Y::is_name(v), e, "{v:?}");
        }
    }

    #[test]
    fn spoken_names() {
        for (v, e) in [
            (Some("youtube"), true),
            (Some("youtu"), true),
            (Some("YT"), true),
            (Some("youtube music"), false),
            (Some("yt music"), false),
            (Some("netflix"), false),
            (None, false),
        ] {
            assert_eq!(Y::is_spoken_site(v), e, "{v:?}");
        }
        for (v, e) in [
            (Some("youtube music"), true),
            (Some("YT Music"), true),
            (Some("youtube"), false),
            (Some("music"), false),
            (None, false),
        ] {
            assert_eq!(Y::is_spoken_music(v), e, "{v:?}");
        }
    }

    #[test]
    fn the_two_addresses_are_the_ones_the_app_opens() {
        assert_eq!(Y::HOME, "https://www.youtube.com");
        assert_eq!(Y::MUSIC, "https://music.youtube.com");
    }
}
