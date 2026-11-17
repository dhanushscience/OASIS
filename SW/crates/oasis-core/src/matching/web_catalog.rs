//! Port of `Intent/WebSiteCatalog.cs`: spoken or typed website names → addresses ("proton mail" →
//! mail.proton.me). Without it a site name became `https://proton mail.com`, or was fuzzy-matched
//! to an unrelated app. Lookup ignores case, spaces and punctuation, so "proton mail",
//! "protonmail" and "Proton-Mail" are one key.

use super::YouTubeHosts;

/// `(key, address)`, keys already in compact form.
const SITES: &[(&str, &str)] = &[
    ("protonmail", "https://mail.proton.me"),
    ("proton", "https://mail.proton.me"),
    ("protonmailcom", "https://mail.proton.me"),
    ("protondrive", "https://drive.proton.me"),
    ("protoncalendar", "https://calendar.proton.me"),
    ("gmail", "https://mail.google.com"),
    ("googlemail", "https://mail.google.com"),
    ("outlook", "https://outlook.live.com/mail"),
    ("hotmail", "https://outlook.live.com/mail"),
    ("yahoomail", "https://mail.yahoo.com"),
    ("googledrive", "https://drive.google.com"),
    ("googledocs", "https://docs.google.com"),
    ("googlesheets", "https://sheets.google.com"),
    ("googleslides", "https://slides.google.com"),
    ("googlecalendar", "https://calendar.google.com"),
    ("googlemaps", "https://www.google.com/maps"),
    ("googlephotos", "https://photos.google.com"),
    ("googlemeet", "https://meet.google.com"),
    ("googletranslate", "https://translate.google.com"),
    ("google", "https://www.google.com"),
    ("bing", "https://www.bing.com"),
    ("duckduckgo", "https://duckduckgo.com"),
    ("github", "https://github.com"),
    ("gitlab", "https://gitlab.com"),
    ("stackoverflow", "https://stackoverflow.com"),
    ("reddit", "https://www.reddit.com"),
    ("twitter", "https://x.com"),
    ("x", "https://x.com"),
    ("facebook", "https://www.facebook.com"),
    ("instagram", "https://www.instagram.com"),
    ("linkedin", "https://www.linkedin.com"),
    ("pinterest", "https://www.pinterest.com"),
    ("twitch", "https://www.twitch.tv"),
    ("whatsappweb", "https://web.whatsapp.com"),
    ("telegramweb", "https://web.telegram.org"),
    ("chatgpt", "https://chatgpt.com"),
    ("claude", "https://claude.ai"),
    ("gemini", "https://gemini.google.com"),
    ("perplexity", "https://www.perplexity.ai"),
    ("youtube", YouTubeHosts::HOME),
    ("netflix", "https://www.netflix.com"),
    ("hulu", "https://www.hulu.com"),
    ("disneyplus", "https://www.disneyplus.com"),
    ("jiohotstar", "https://www.hotstar.com"),
    ("primevideo", "https://www.primevideo.com"),
    ("hotstar", "https://www.hotstar.com"),
    ("amazon", "https://www.amazon.in"),
    ("flipkart", "https://www.flipkart.com"),
    ("wikipedia", "https://en.wikipedia.org"),
    ("notion", "https://www.notion.so"),
    ("figma", "https://www.figma.com"),
    ("canva", "https://www.canva.com"),
    ("spotifyweb", "https://open.spotify.com"),
];

/// Lower-case letters and digits only.
pub fn compact(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The address for a spoken site name, if the catalog knows it.
pub fn try_resolve(spoken: &str) -> Option<&'static str> {
    let key = compact(spoken);
    if key.is_empty() {
        return None;
    }
    SITES.iter().find(|(k, _)| *k == key).map(|(_, url)| *url)
}

/// Best guess for a name the catalog does not know: the words run together plus `.com`
/// ("stack exchange" → `https://stackexchange.com`). Spaces never reach a URL.
pub fn guess_url(spoken: &str) -> String {
    let key = compact(spoken);
    if key.is_empty() {
        String::new()
    } else {
        format!("https://{key}.com")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_names_resolve_whatever_the_spacing_case_or_punctuation() {
        for spoken in ["proton mail", "protonmail", "Proton-Mail", "PROTON  MAIL"] {
            assert_eq!(try_resolve(spoken), Some("https://mail.proton.me"), "{spoken}");
        }
        assert_eq!(try_resolve("Netflix"), Some("https://www.netflix.com"));
        assert_eq!(try_resolve("youtube"), Some("https://www.youtube.com"));
        assert_eq!(try_resolve("x"), Some("https://x.com"));
    }

    #[test]
    fn unknown_and_empty_names_do_not_resolve() {
        assert_eq!(try_resolve("somenewsite"), None);
        assert_eq!(try_resolve(""), None);
        assert_eq!(try_resolve("  ---  "), None);
    }

    #[test]
    fn a_guess_runs_the_words_together_and_adds_com() {
        assert_eq!(guess_url("stack exchange"), "https://stackexchange.com");
        assert_eq!(guess_url("Some New-Site"), "https://somenewsite.com");
        assert_eq!(guess_url(""), "");
    }

    #[test]
    fn compact_keeps_letters_and_digits_only() {
        assert_eq!(compact("Proton-Mail 2!"), "protonmail2");
    }

    #[test]
    fn the_table_matches_the_csharp_catalog_and_has_no_blank_or_duplicate_keys() {
        assert_eq!(SITES.len(), 53, "the C# catalog has 53 entries");
        let mut seen = std::collections::HashSet::new();
        assert!(SITES.iter().all(|(k, url)| !k.is_empty() && url.starts_with("https://") && seen.insert(*k)));
        assert!(SITES.iter().all(|(k, _)| *k == compact(k)), "every key is already compact");
    }
}
