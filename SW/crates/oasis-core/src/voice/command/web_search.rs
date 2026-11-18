//! Spoken web searches: "search for mobile phones", "open chrome and search for X", "search amazon
//! for X", "look up X on wikipedia", "google X". Port of `Oasis.Core/Intent/WebSearchPhrase.cs` and
//! `Services/Intent/WebSearchIntentParser.cs`. Input is already normalized (lower case, letters,
//! digits and spaces).

use crate::actions::ActionKind;

/// A search site and how its results address is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Site {
    pub name: &'static str,
    pub url_prefix: &'static str,
    /// The query is a path segment (Google Maps), so spaces stay `%20` instead of `+`.
    pub path_query: bool,
}

impl Site {
    pub fn build_url(&self, query: &str) -> String {
        let escaped = escape_data_string(query.trim());
        let query = if self.path_query { escaped } else { escaped.replace("%20", "+") };
        format!("{}{}", self.url_prefix, query)
    }
}

/// `Uri.EscapeDataString`: everything but `A-Z a-z 0-9 - _ . ~` is percent-encoded as UTF-8.
fn escape_data_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

const fn site(name: &'static str, url_prefix: &'static str) -> Site {
    Site { name, url_prefix, path_query: false }
}

const GOOGLE: Site = site("Google", "https://www.google.com/search?q=");
const GOOGLE_IMAGES: Site = site("Google Images", "https://www.google.com/search?tbm=isch&q=");
const GOOGLE_MAPS: Site = Site { name: "Google Maps", url_prefix: "https://www.google.com/maps/search/", path_query: true };
const YOUTUBE: Site = site("YouTube", "https://www.youtube.com/results?search_query=");
const AMAZON: Site = site("Amazon", "https://www.amazon.in/s?k=");
const FLIPKART: Site = site("Flipkart", "https://www.flipkart.com/search?q=");
const WIKIPEDIA: Site = site("Wikipedia", "https://en.wikipedia.org/w/index.php?search=");
const BING: Site = site("Bing", "https://www.bing.com/search?q=");
const DUCKDUCKGO: Site = site("DuckDuckGo", "https://duckduckgo.com/?q=");
const GITHUB: Site = site("GitHub", "https://github.com/search?q=");
const REDDIT: Site = site("Reddit", "https://www.reddit.com/search/?q=");

/// Spoken names, longest first so "google maps" wins over "google".
const SITE_ALIASES: [(&str, Site); 17] = [
    ("google images", GOOGLE_IMAGES), ("google maps", GOOGLE_MAPS), ("duck duck go", DUCKDUCKGO),
    ("duckduckgo", DUCKDUCKGO), ("wikipedia", WIKIPEDIA), ("you tube", YOUTUBE), ("youtube", YOUTUBE),
    ("flipkart", FLIPKART), ("git hub", GITHUB), ("github", GITHUB), ("amazon", AMAZON), ("reddit", REDDIT),
    ("images", GOOGLE_IMAGES), ("google", GOOGLE), ("maps", GOOGLE_MAPS), ("wiki", WIKIPEDIA), ("bing", BING),
];

/// Spoken browser names → the launch token `LaunchApp browser|url` expects.
const BROWSER_ALIASES: [(&str, &str); 9] = [
    ("mozilla firefox", "firefox"), ("microsoft edge", "msedge"), ("google chrome", "chrome"),
    ("firefox", "firefox"), ("chrome", "chrome"), ("brave", "brave"), ("edge", "msedge"), ("opera", "opera"),
    ("vivaldi", "vivaldi"),
];

const OPEN_VERBS: [&str; 5] = ["open ", "launch ", "start ", "go to ", "pull up "];
const JOINERS: [&str; 3] = [" and then ", " then ", " and "];
const PREPOSITIONS: [&str; 5] = [" in ", " on ", " using ", " with ", " at "];
const LEADING_PREPOSITIONS: [&str; 5] = ["in ", "on ", "using ", "with ", "via "];
const POLITENESS: [&str; 6] = ["can you ", "could you ", "would you ", "please ", "i want to ", "i wanna "];
const TRAILING_NOISE: [&str; 4] = [" please", " for me", " now", " right now"];
const SEARCH_VERBS: [&str; 4] = ["search for ", "search ", "look up ", "lookup "];

/// A command verb after "and"/"then" means a second clause ("search X and open Spotify"): refuse the
/// whole phrase so clause splitting handles it.
const COMMAND_VERBS: [&str; 20] = [
    "open", "close", "launch", "start", "play", "pause", "set", "turn", "mute", "unmute", "minimize", "maximize",
    "switch", "type", "press", "quit", "exit", "focus", "run", "skip",
];

/// A parsed "search for X [on site] [in browser]" request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSearchRequest {
    pub query: String,
    pub site: Site,
    pub browser: Option<&'static str>,
}

impl WebSearchRequest {
    pub fn url(&self) -> String {
        self.site.build_url(&self.query)
    }
}

pub fn try_parse(normalized: &str) -> Option<WebSearchRequest> {
    let mut text = strip(normalized);
    // "google chrome" on its own names a browser; it is not a search for "chrome".
    if text.is_empty() || try_browser(&text).is_some() {
        return None;
    }

    let mut browser: Option<&'static str> = None;
    let mut site: Option<Site> = None;

    // "open chrome and search for X" / "open amazon and search X": the opener only names where to
    // search. Anything else ("open notepad and …") is not ours.
    if let Some((opened, rest)) = peel_opener(&text) {
        if let Some(b) = try_browser(&opened) {
            browser = Some(b);
        } else {
            site = Some(try_site(&opened)?);
        }
        text = rest;
    }

    // "in Google Chrome, search for X": the browser comes first, then the search.
    if browser.is_none() {
        if let Some((b, rest)) = peel_leading_browser(&text) {
            browser = Some(b);
            text = rest;
        }
    }

    // Trailing "on amazon" / "in chrome", in either order.
    for _ in 0..2 {
        if browser.is_none() {
            if let Some((b, rest)) = peel_trailing(&text, try_browser) {
                browser = Some(b);
                text = rest;
            }
        }
        if site.is_none() {
            if let Some((s, rest)) = peel_trailing(&text, try_site) {
                site = Some(s);
                text = rest;
            }
        }
    }

    let query = peel_verb(&text, &mut site)?;
    // "search this best laptops" is a search for "best laptops" (W15).
    let query = strip_leading(&query, &["for ", "about ", "this ", "that "]);
    if is_filler_only(&query) {
        return None;
    }
    if query.is_empty() || query.len() > 200 || has_second_command(&query) {
        return None;
    }
    // "search spotify for X" belongs to the Spotify parser, not a Google search for "spotify".
    if query == "spotify" || query.starts_with("spotify ") {
        return None;
    }
    Some(WebSearchRequest { query, site: site.unwrap_or(GOOGLE), browser })
}

/// The action for a spoken search: a named browser opens the results there (`LaunchApp`
/// `browser|url`); otherwise the default browser does (`OpenUrl`). Returns `(kind, value, reply)`.
pub fn plan(normalized: &str) -> Option<(&'static str, String, String)> {
    let request = try_parse(normalized)?;
    let reply = format!("Search {} for {}.", request.site.name, request.query);
    Some(match request.browser {
        Some(browser) => (ActionKind::LaunchApp.as_str(), format!("{browser}|{}", request.url()), reply),
        None => (ActionKind::OpenUrl.as_str(), request.url(), reply),
    })
}

/// The results endpoint of a known search site, without the query, so a trusted search target
/// covers every query on that site instead of confirming each new one.
pub fn search_endpoint(url: &str) -> Option<String> {
    let value = url.trim();
    SITE_ALIASES.iter().find_map(|(_, site)| {
        let prefix = site.url_prefix;
        (value.len() > prefix.len() && value.is_char_boundary(prefix.len()) && value[..prefix.len()].eq_ignore_ascii_case(prefix))
            .then(|| prefix.to_lowercase())
    })
}

fn strip(normalized: &str) -> String {
    let mut text = strip_leading(normalized.trim(), &POLITENESS);
    for noise in TRAILING_NOISE {
        if let Some(rest) = text.strip_suffix(noise) {
            text = rest.trim_end().to_string();
        }
    }
    text
}

fn strip_leading(text: &str, prefixes: &[&str]) -> String {
    let mut text = text.to_string();
    let mut changed = true;
    while changed {
        changed = false;
        for prefix in prefixes {
            if let Some(rest) = text.strip_prefix(prefix) {
                text = rest.trim_start().to_string();
                changed = true;
            }
        }
    }
    text.trim().to_string()
}

fn peel_opener(text: &str) -> Option<(String, String)> {
    for verb in OPEN_VERBS {
        let Some(remainder) = text.strip_prefix(verb) else { continue };
        for joiner in JOINERS {
            let Some(idx) = remainder.find(joiner).filter(|i| *i > 0) else { continue };
            let rest = remainder[idx + joiner.len()..].trim();
            return (!rest.is_empty()).then(|| (remainder[..idx].trim().to_string(), rest.to_string()));
        }
    }
    None
}

fn peel_leading_browser(text: &str) -> Option<(&'static str, String)> {
    for preposition in LEADING_PREPOSITIONS {
        let Some(after) = text.strip_prefix(preposition) else { continue };
        let after = strip_leading(after, &["my ", "the "]);
        for (spoken, token) in BROWSER_ALIASES {
            let Some(rest) = after.strip_prefix(&format!("{spoken} ")) else { continue };
            let rest = strip_leading(rest, &["browser "]);
            return (!rest.is_empty()).then_some((token, rest));
        }
    }
    None
}

fn peel_trailing<T>(text: &str, resolve: fn(&str) -> Option<T>) -> Option<(T, String)> {
    for preposition in PREPOSITIONS {
        let Some(idx) = text.rfind(preposition).filter(|i| *i > 0) else { continue };
        if let Some(value) = resolve(text[idx + preposition.len()..].trim()) {
            return Some((value, text[..idx].trim().to_string()));
        }
    }
    None
}

fn peel_verb(text: &str, site: &mut Option<Site>) -> Option<String> {
    // "search amazon for X" / "search on youtube for X".
    for verb in ["search on ", "search in ", "search "] {
        let Some(remainder) = text.strip_prefix(verb) else { continue };
        for (spoken, candidate) in SITE_ALIASES {
            if let Some(query) = remainder.strip_prefix(&format!("{spoken} for ")) {
                site.get_or_insert(candidate);
                return Some(query.trim().to_string());
            }
        }
    }

    for verb in SEARCH_VERBS {
        if let Some(query) = text.strip_prefix(verb) {
            return Some(query.trim().to_string());
        }
    }

    // "youtube search lofi" / "google maps coffee near me" / "google mobile phones".
    for (spoken, candidate) in SITE_ALIASES {
        let Some(after) = text.strip_prefix(&format!("{spoken} ")) else { continue };
        let remainder = strip_leading(after, &["search for ", "search "]);
        let named_with_verb = remainder.len() < after.len();
        // A bare "<site> X" is only a search for the sites people use as a verb.
        if !named_with_verb && candidate != GOOGLE && candidate != GOOGLE_MAPS && candidate != GOOGLE_IMAGES {
            continue;
        }
        site.get_or_insert(candidate);
        return Some(remainder);
    }

    // "find X on amazon": "find" alone is too broad ("find my files"), so it needs a site.
    if site.is_some() {
        if let Some(rest) = text.strip_prefix("find ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

const FILLERS: [&str; 5] = ["for", "about", "this", "that", "it"];

const STOP_WORDS: [&str; 8] = ["a", "an", "the", "uh", "um", "of", "to", "and"];

/// Nothing but fillers, or fillers and one stop word or one letter ("search for a").
fn is_filler_only(words: &str) -> bool {
    let rest: Vec<&str> = words.split_whitespace().filter(|w| !FILLERS.contains(w)).collect();
    match rest[..] {
        [] => true,
        [one] => one.chars().count() == 1 || STOP_WORDS.contains(&one),
        _ => false,
    }
}

/// "search", "search this", "search for": a search with nothing to search for. It is ignored, not
/// typed and not asked about (W15, W18).
pub fn is_empty_search(normalized: &str) -> bool {
    let text = strip(normalized);
    ["search", "look up", "lookup"].iter().any(|verb| {
        text.strip_prefix(verb).is_some_and(|rest| (rest.is_empty() || rest.starts_with(' ')) && is_filler_only(rest))
    })
}

fn has_second_command(query: &str) -> bool {
    JOINERS.iter().any(|joiner| {
        query.find(joiner).is_some_and(|idx| {
            let after = &query[idx + joiner.len()..];
            COMMAND_VERBS.iter().any(|verb| after == *verb || after.starts_with(&format!("{verb} ")))
        })
    })
}

fn strip_suffix_any(text: &str, suffixes: &[&str]) -> String {
    let value = text.trim();
    suffixes.iter().find_map(|s| value.strip_suffix(s)).map_or(value, str::trim).to_string()
}

fn try_site(spoken: &str) -> Option<Site> {
    let value = strip_suffix_any(spoken, &[" website", " site", " dot com", " com"]);
    SITE_ALIASES.iter().find(|(name, _)| *name == value).map(|(_, s)| *s)
}

fn try_browser(spoken: &str) -> Option<&'static str> {
    let value = strip_suffix_any(spoken, &[" browser", " app", " application"]);
    BROWSER_ALIASES.iter().find(|(name, _)| *name == value).map(|(_, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search() {
        let cases = [
            ("search for mobile phones", "mobile phones", "https://www.google.com/search?q=mobile+phones", None),
            ("open google chrome and search for mobile phones", "mobile phones", "https://www.google.com/search?q=mobile+phones", Some("chrome")),
            ("open chrome and search for salt and pepper", "salt and pepper", "https://www.google.com/search?q=salt+and+pepper", Some("chrome")),
            ("can you search for mobile phones in brave please", "mobile phones", "https://www.google.com/search?q=mobile+phones", Some("brave")),
            ("search amazon for wireless earbuds", "wireless earbuds", "https://www.amazon.in/s?k=wireless+earbuds", None),
            ("open amazon and search for iphone 16", "iphone 16", "https://www.amazon.in/s?k=iphone+16", None),
            ("search for lofi music on youtube", "lofi music", "https://www.youtube.com/results?search_query=lofi+music", None),
            ("youtube search lofi beats", "lofi beats", "https://www.youtube.com/results?search_query=lofi+beats", None),
            ("look up black holes on wikipedia", "black holes", "https://en.wikipedia.org/w/index.php?search=black+holes", None),
            ("google best laptops 2026", "best laptops 2026", "https://www.google.com/search?q=best+laptops+2026", None),
            ("search for coffee shops on google maps in edge", "coffee shops", "https://www.google.com/maps/search/coffee%20shops", Some("msedge")),
            ("find running shoes on flipkart", "running shoes", "https://www.flipkart.com/search?q=running+shoes", None),
            ("search for weather in delhi", "weather in delhi", "https://www.google.com/search?q=weather+in+delhi", None),
        ];
        for (transcript, query, url, browser) in cases {
            let r = try_parse(transcript).unwrap_or_else(|| panic!("{transcript}"));
            assert_eq!((r.query.as_str(), r.url().as_str(), r.browser), (query, url, browser), "{transcript}");
        }
    }

    #[test]
    fn leaves_other_commands_alone() {
        for t in [
            "open chrome", "google chrome", "open youtube in brave", "open notepad and search for cats",
            "search for mobile phones and open spotify", "find my files", "search spotify for loser",
            "play lofi on youtube", "search", "",
        ] {
            assert!(try_parse(t).is_none(), "{t}");
        }
    }

    #[test]
    fn a_named_browser_launches_it_with_the_results() {
        let (kind, value, _) = plan("open google chrome and search for mobile phones").unwrap();
        assert_eq!((kind, value.as_str()), ("LaunchApp", "chrome|https://www.google.com/search?q=mobile+phones"));
        let (kind, value, reply) = plan("search for mobile phones").unwrap();
        assert_eq!((kind, value.as_str()), ("OpenUrl", "https://www.google.com/search?q=mobile+phones"));
        assert_eq!(reply, "Search Google for mobile phones.");
    }

    #[test]
    fn search_this_searches_the_words_after_it_and_alone_is_nothing() {
        assert_eq!(try_parse("search this best laptops").map(|r| r.query), Some("best laptops".into()));
        for empty in ["search this", "search for", "search", "look up this", "search for this"] {
            assert!(try_parse(empty).is_none(), "{empty}");
            assert!(is_empty_search(empty), "{empty}");
        }
        for empty in ["search for a", "search the", "search for uh", "search x", "look up an"] {
            assert!(is_empty_search(empty), "{empty}");
        }
        assert!(!is_empty_search("search for ai") && !is_empty_search("search for a laptop"));
        assert!(!is_empty_search("search best laptops") && !is_empty_search("searching for answers is hard"));
    }

    #[test]
    fn endpoints_cover_every_query_on_a_site() {
        assert_eq!(search_endpoint("https://www.google.com/search?q=laptops").as_deref(), Some("https://www.google.com/search?q="));
        assert_eq!(search_endpoint("https://www.google.com/url?q=https://example.com"), None);
    }
}
