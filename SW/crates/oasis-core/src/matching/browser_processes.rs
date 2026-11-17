//! Port of `Matching/BrowserProcesses.cs`.
//!
//! Which processes are web browsers. Five places used to keep their own list and had drifted,
//! so a browser one of them missed was treated as an ordinary application: no address read, no
//! website-profile match.
//!
//! [`BrowserProcesses::is_known`] answers "is this process exactly one of the browsers we know".
//! [`BrowserProcesses::looks_like_browser`] is looser, for names that carry more than the
//! process name ("chrome_proxy", "Opera GX"); it matches by substring, except for Arc, whose
//! name sits inside too many other words ("search").
//!
//! Cross-platform note: on macOS and Linux process names differ ("Google Chrome",
//! "google-chrome", "firefox-esr", "Arc"). [`BrowserProcesses::normalize`] strips ".exe" and
//! ".app"; the substring rule already covers "google-chrome" and "Google Chrome".

pub struct BrowserProcesses;

impl BrowserProcesses {
    /// Process names, lower case, without ".exe".
    pub const NAMES: [&'static str; 7] = [
        "chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "arc",
    ];

    const ARC: &'static str = "arc";

    /// Trimmed, lower case, without a trailing ".exe" (Windows) or ".app" (macOS bundle).
    pub fn normalize(process: Option<&str>) -> String {
        let name = process.unwrap_or("").trim().to_lowercase();
        for suffix in [".exe", ".app"] {
            if let Some(stripped) = name.strip_suffix(suffix) {
                return stripped.to_string();
            }
        }
        name
    }

    pub fn is_known(process: Option<&str>) -> bool {
        let name = Self::normalize(process);
        !name.is_empty() && Self::NAMES.contains(&name.as_str())
    }

    pub fn looks_like_browser(name: Option<&str>) -> bool {
        let lower = Self::normalize(name);
        if lower.is_empty() {
            return false;
        }
        if Self::NAMES.contains(&lower.as_str()) {
            return true;
        }
        Self::NAMES
            .iter()
            .any(|browser| *browser != Self::ARC && lower.contains(browser))
    }
}

#[cfg(test)]
mod tests {
    use super::BrowserProcesses as B;

    #[test]
    fn every_known_browser_is_known() {
        for p in B::NAMES {
            assert!(B::is_known(Some(p)), "{p}");
        }
    }

    #[test]
    fn a_process_name_is_normalised_before_it_is_compared() {
        for p in ["Chrome.exe", "  msedge  ", "FIREFOX", "Arc.app"] {
            assert!(B::is_known(Some(p)), "{p}");
        }
    }

    #[test]
    fn anything_else_is_not_known() {
        for p in [
            None,
            Some(""),
            Some("   "),
            Some("notepad"),
            Some("explorer"),
            Some("chrome_proxy"),
            Some("Opera GX"),
        ] {
            assert!(!B::is_known(p), "{p:?}");
        }
    }

    #[test]
    fn a_name_that_contains_a_browser_looks_like_one() {
        for p in [
            "chrome_proxy",
            "Opera GX",
            "msedgewebview2",
            "firefox-esr",
            "arc",
            "Google Chrome",
            "google-chrome",
        ] {
            assert!(B::looks_like_browser(Some(p)), "{p}");
        }
    }

    #[test]
    fn arc_is_matched_only_as_a_whole_name() {
        for p in [
            None,
            Some(""),
            Some("notepad"),
            Some("search"),
            Some("archive"),
            Some("Spotify"),
        ] {
            assert!(!B::looks_like_browser(p), "{p:?}");
        }
    }
}
