//! Spoken Spotify commands: "pause spotify", "next track on spotify", "play loser on spotify",
//! "queue yellow". Port of `SpotifyControlIntentParser.cs`, the phrase half of
//! `MediaIntentParser.cs`, the pure rules of `SpotifyPlaybackHeuristics.cs` and the choosing rules
//! of `SpotifyPlaybackService.cs`.
//!
//! A transport word that means something elsewhere ("next", "stop", "volume 30") only goes to
//! Spotify when the phrase names Spotify or Spotify is the program running; otherwise it stays
//! with the media keys and the system volume.

const SPOTIFY_NAMES: [&str; 3] = ["spotify", "spot i", "spotfire"];

/// What a spoken phrase asks of Spotify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phrase {
    /// `pause`, `resume`, `next`, `previous`, `shuffle on|off`, `repeat`, `repeat track`,
    /// `repeat off`, `volume N`, `seek Ns`.
    Control { command: String, label: String },
    /// What to play. A leading `queue ` means add it to the queue instead.
    Play { query: String },
}

/// The phrase for `normalized` speech. `spotify_running`: Spotify is the program running.
pub fn parse(normalized: &str, spotify_running: bool) -> Option<Phrase> {
    if let Some(query) = parse_play(normalized, spotify_running) {
        return Some(Phrase::Play { query });
    }
    parse_control(normalized, spotify_running).map(|(command, label)| Phrase::Control { command, label })
}

fn strip_context(normalized: &str) -> (String, bool) {
    let mut text = normalized.trim().to_string();
    let mut named = false;
    if let Some(rest) = text.strip_prefix("spotify ") {
        text = rest.trim().to_string();
        named = true;
    }
    for suffix in [" on spotify", " in spotify", " spotify"] {
        if let Some(rest) = text.strip_suffix(suffix) {
            text = rest.trim().to_string();
            named = true;
        }
    }
    (text, named)
}

const EXACT: [(&str, &str, &str); 30] = [
    ("pause", "pause", "pause spotify"),
    ("stop", "pause", "pause spotify"),
    ("resume", "resume", "resume spotify"),
    ("continue", "resume", "resume spotify"),
    ("unpause", "resume", "resume spotify"),
    ("play", "resume", "resume spotify"),
    ("next", "next", "next track"),
    ("skip", "next", "next track"),
    ("skip next", "next", "next track"),
    ("next song", "next", "next track"),
    ("next track", "next", "next track"),
    ("previous", "previous", "previous track"),
    ("prev", "previous", "previous track"),
    ("back", "previous", "previous track"),
    ("previous song", "previous", "previous track"),
    ("previous track", "previous", "previous track"),
    ("skip back", "previous", "previous track"),
    ("shuffle", "shuffle on", "shuffle on"),
    ("shuffle on", "shuffle on", "shuffle on"),
    ("enable shuffle", "shuffle on", "shuffle on"),
    ("shuffle off", "shuffle off", "shuffle off"),
    ("disable shuffle", "shuffle off", "shuffle off"),
    ("repeat", "repeat", "repeat on"),
    ("repeat on", "repeat", "repeat on"),
    ("repeat playlist", "repeat", "repeat on"),
    ("repeat track", "repeat track", "repeat track"),
    ("repeat song", "repeat track", "repeat track"),
    ("repeat one", "repeat track", "repeat track"),
    ("repeat off", "repeat off", "repeat off"),
    ("disable repeat", "repeat off", "repeat off"),
];

fn parse_control(normalized: &str, spotify_running: bool) -> Option<(String, String)> {
    let (text, named) = strip_context(normalized);
    if let Some((_, command, label)) = EXACT.iter().find(|(phrase, _, _)| *phrase == text) {
        // "stop", "next" and "back" are the media keys' words too: without Spotify named or
        // running they are left to them.
        if named || spotify_running {
            return Some(((*command).to_string(), (*label).to_string()));
        }
        return None;
    }
    // A volume or a position is only Spotify's when Spotify is named: "volume 30" is the system's.
    if !named && !normalized.starts_with("spotify ") {
        return None;
    }
    let words: Vec<&str> = text.split(' ').collect();
    let words: Vec<&str> = match words.as_slice() {
        ["set", rest @ ..] => rest.to_vec(),
        other => other.to_vec(),
    };
    if let ["volume", rest @ ..] = words.as_slice() {
        let rest: Vec<&str> = rest.iter().copied().skip_while(|w| *w == "to").collect();
        let digits = match rest.as_slice() {
            [n] => n.strip_suffix('%').unwrap_or(n),
            [n, "percent"] => n,
            _ => return None,
        };
        let pct = digits.parse::<u32>().ok()?.min(100);
        return Some((format!("volume {pct}"), format!("spotify volume {pct}%")));
    }
    if let ["seek" | "jump", rest @ ..] = words.as_slice() {
        let rest: Vec<&str> = rest.iter().copied().skip_while(|w| *w == "to").collect();
        let (value, unit) = match rest.as_slice() {
            [v] => (*v, "s"),
            [v, "s" | "seconds" | "second"] => (*v, "s"),
            [v, "ms"] => (*v, "ms"),
            _ => return None,
        };
        value.parse::<u64>().ok()?;
        return Some((format!("seek {value}{unit}"), format!("seek {value}{unit}")));
    }
    None
}

fn parse_play(normalized: &str, spotify_running: bool) -> Option<String> {
    // "queue yellow", "add yellow to spotify".
    if let Some(rest) = normalized.strip_prefix("queue ") {
        let query = normalize_query(&strip_spotify_tail(rest));
        return (!query.is_empty()).then(|| format!("queue {query}"));
    }
    if let Some(rest) = normalized.strip_prefix("add ") {
        let (body, tail_named) = split_spotify_tail(rest);
        if tail_named {
            let query = normalize_query(&body);
            return (!query.is_empty()).then(|| format!("queue {query}"));
        }
    }
    for name in SPOTIFY_NAMES {
        // "open spotify and play X"
        for verb in ["open", "launch", "start", "run"] {
            let head = format!("{verb} {name} ");
            if let Some(rest) = normalized.strip_prefix(&head) {
                let rest = rest.strip_prefix("and ").unwrap_or(rest);
                if let Some(query) = rest.strip_prefix("play ") {
                    return non_empty(normalize_query(query));
                }
            }
        }
        // "spotify play X"
        if let Some(query) = normalized.strip_prefix(&format!("{name} play ")) {
            return non_empty(normalize_query(query));
        }
    }
    // "play X on|in [my] spotify [app|application] [more]"
    if let Some(rest) = normalized.strip_prefix("play ") {
        for name in SPOTIFY_NAMES {
            for joiner in ["on", "in"] {
                for mine in ["my ", ""] {
                    let marker = format!(" {joiner} {mine}{name}");
                    if let Some(at) = rest.find(&marker) {
                        let after = &rest[at + marker.len()..];
                        if after.is_empty() || after.starts_with(' ') {
                            return non_empty(normalize_query(&rest[..at]));
                        }
                    }
                }
            }
        }
        if spotify_running {
            return non_empty(normalize_query(rest));
        }
    }
    None
}

fn non_empty(query: String) -> Option<String> {
    (!query.is_empty()).then_some(query)
}

/// "yellow on spotify app" → ("yellow", true).
fn split_spotify_tail(text: &str) -> (String, bool) {
    for name in SPOTIFY_NAMES {
        for joiner in ["on", "in", "to"] {
            for mine in ["my ", ""] {
                for app in [" application", " app", ""] {
                    let tail = format!(" {joiner} {mine}{name}{app}");
                    if let Some(body) = text.strip_suffix(&tail) {
                        return (body.to_string(), true);
                    }
                }
            }
        }
    }
    (text.to_string(), false)
}

fn strip_spotify_tail(text: &str) -> String {
    split_spotify_tail(text).0
}

/// A request cleaned for searching: no politeness, no "on spotify", no "the song", no leading article.
pub fn normalize_query(query: &str) -> String {
    let mut value = query.trim().trim_matches(['"', '\'']).to_string();
    let lower = |v: &str| v.to_lowercase();
    for trim in [
        " please", " for me", " now", " right now", " in my spotify application", " in my spotify app", " in my spotify",
        " on my spotify", " on spotify", " in spotify", " in spot i", " in spotify application", " in spotify app", " song",
    ] {
        if lower(&value).ends_with(trim) {
            value.truncate(value.len() - trim.len());
            value = value.trim().to_string();
        }
    }
    for prefix in ["the song ", "song called ", "track called ", "track ", "music called ", "music "] {
        if lower(&value).starts_with(prefix) {
            value = value[prefix.len()..].trim().to_string();
            break;
        }
    }
    for article in ["a ", "an ", "the "] {
        if lower(&value).starts_with(article) && value.len() > article.len() + 2 {
            value = value[article.len()..].trim().to_string();
            break;
        }
    }
    value
}

/// True when the request names no track, artist or playlist ("play some music").
pub fn is_generic_query(query: &str) -> bool {
    const GENERIC: [&str; 22] = [
        "music", "some music", "any music", "my music", "good music", "some good music", "songs", "some songs", "any songs",
        "a song", "some song", "any song", "something", "anything", "something good", "something nice", "random",
        "random song", "random music", "spotify", "spotify music", "",
    ];
    let mut q = query.trim().trim_matches(['"', '\'']).to_lowercase();
    if let Some(rest) = q.strip_prefix("spotify:") {
        q = rest.trim().to_string();
    }
    for tail in [" please", " for me", " now"] {
        if let Some(rest) = q.strip_suffix(tail) {
            q = rest.trim().to_string();
        }
    }
    GENERIC.contains(&q.as_str())
}

/// What a request is for, and the words to search with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryKind {
    Track,
    UserPlaylist,
    LikedSongs,
    Album,
    Artist,
    Queue,
}

fn is_liked_songs(lower: &str) -> bool {
    matches!(lower, "my liked songs" | "liked songs" | "my library" | "my songs" | "liked" | "my liked" | "liked tracks")
}

fn strip_library_phrases(query: &str) -> String {
    let mut value = query.trim().trim_matches(['"', '\'']).to_string();
    for suffix in [
        " in my library", " from my library", " in my profile", " in my spotify", " on spotify", " in spotify", " in spot i",
        " in spotify application", " playlist", " my playlist", " song",
    ] {
        if value.to_lowercase().ends_with(suffix) {
            value.truncate(value.len() - suffix.len());
            value = value.trim().to_string();
        }
    }
    if value.to_lowercase().starts_with("playlist ") {
        value = value["playlist ".len()..].trim().to_string();
    }
    if value.to_lowercase().starts_with("my ") {
        value = value["my ".len()..].trim().to_string();
    }
    value
}

fn strip_kind_word(value: &str, kind: &str) -> String {
    let mut result = value.trim().to_string();
    let lower = result.to_lowercase();
    if lower.starts_with(&format!("{kind} ")) {
        result = result[kind.len() + 1..].trim().to_string();
    }
    if result.to_lowercase().ends_with(&format!(" {kind}")) {
        result.truncate(result.len() - kind.len() - 1);
        result = result.trim().to_string();
    }
    result
}

pub fn classify_query(query: &str) -> (QueryKind, String) {
    let trimmed = query.trim();
    let lower = trimmed.to_lowercase();
    if is_liked_songs(&lower) {
        return (QueryKind::LikedSongs, String::new());
    }
    if lower.starts_with("queue ") {
        return (QueryKind::Queue, strip_library_phrases(trimmed["queue ".len()..].trim()));
    }
    let stripped = strip_library_phrases(trimmed);
    let stripped_lower = stripped.to_lowercase();
    if is_liked_songs(&stripped_lower) {
        return (QueryKind::LikedSongs, String::new());
    }
    if lower.contains(" album") || stripped_lower.starts_with("album ") || stripped_lower.ends_with(" album") {
        return (QueryKind::Album, strip_kind_word(&stripped, "album"));
    }
    if lower.contains(" artist") || stripped_lower.starts_with("artist ") || stripped_lower.ends_with(" artist") {
        return (QueryKind::Artist, strip_kind_word(&stripped, "artist"));
    }
    let wants_playlist = ["my library", "my playlist", "in my profile", "from my library", "in my spotify"].iter().any(|p| lower.contains(p))
        || stripped_lower.contains("playlist")
        || lower.contains(" playlist");
    if wants_playlist {
        return (QueryKind::UserPlaylist, stripped);
    }
    (QueryKind::Track, stripped)
}

/// How well a name matches what was asked for. Higher is better.
pub fn score_name(hay: &str, needle: &str) -> f64 {
    let mut score = 0.0;
    if hay == needle {
        score += 120.0;
    } else if hay.contains(needle) {
        score += 80.0;
    } else if needle.contains(hay) {
        score += 60.0;
    }
    for token in needle.split(' ').filter(|t| t.chars().count() >= 3) {
        if hay.contains(token) {
            score += 15.0;
        }
    }
    score
}

/// A found item: what to show and what to score on.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub uri: String,
    pub name: String,
    /// Artists (tracks and albums) or owner (playlists); part of the text matched against.
    pub by: String,
    pub popularity: u32,
}

/// The best track for a request: name match first, popularity as a tie-breaker.
pub fn pick_track<'a>(tracks: &'a [Candidate], query: &str) -> Option<&'a Candidate> {
    let needle = query.trim().to_lowercase();
    let mut best: Option<(&Candidate, f64)> = None;
    for track in tracks {
        let mut score = score_name(&format!("{} {}", track.name, track.by).to_lowercase(), &needle) + f64::from(track.popularity) * 0.5;
        if track.name.eq_ignore_ascii_case(query.trim()) {
            score += 80.0;
        }
        if best.is_none_or(|(_, s)| score > s) {
            best = Some((track, score));
        }
    }
    best.map(|(t, _)| t).or_else(|| tracks.first())
}

/// The best playlist, or none if nothing is close (a score of at least 40).
pub fn pick_playlist<'a>(items: &'a [Candidate], query: &str) -> Option<&'a Candidate> {
    let needle = query.trim().to_lowercase();
    items
        .iter()
        .map(|p| (p, score_name(&p.name.to_lowercase(), &needle)))
        .fold(None::<(&Candidate, f64)>, |best, next| match best {
            Some((_, s)) if s >= next.1 => best,
            _ => Some(next),
        })
        .filter(|(_, s)| *s >= 40.0)
        .map(|(p, _)| p)
}

/// The best album or artist: close enough (30) or else the first result.
pub fn pick_album_or_artist<'a>(items: &'a [Candidate], query: &str) -> Option<&'a Candidate> {
    let needle = query.trim().to_lowercase();
    let best = items
        .iter()
        .map(|a| (a, score_name(&format!("{} {}", a.name, a.by).trim().to_lowercase(), &needle)))
        .fold(None::<(&Candidate, f64)>, |best, next| match best {
            Some((_, s)) if s >= next.1 => best,
            _ => Some(next),
        });
    match best {
        Some((item, score)) if score >= 30.0 => Some(item),
        _ => items.first(),
    }
}

/// A playback device Spotify knows about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub active: bool,
}

/// "Play in Spotify" means this PC: the first computer, else any device that has an id.
pub fn pick_device(devices: &[Device]) -> Option<&Device> {
    devices
        .iter()
        .find(|d| d.kind.eq_ignore_ascii_case("Computer") && !d.id.is_empty())
        .or_else(|| devices.iter().find(|d| !d.id.is_empty()))
}

/// A spoken seek in milliseconds: `30s`, `30000ms`, or bare seconds.
pub fn seek_ms(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    if let Some(ms) = raw.strip_suffix("ms") {
        return ms.trim().parse().ok();
    }
    let seconds = raw.strip_suffix('s').unwrap_or(raw).trim();
    seconds.parse::<u64>().ok().map(|s| s * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control(text: &str, running: bool) -> Option<String> {
        match parse(text, running) {
            Some(Phrase::Control { command, .. }) => Some(command),
            _ => None,
        }
    }

    fn play(text: &str, running: bool) -> Option<String> {
        match parse(text, running) {
            Some(Phrase::Play { query }) => Some(query),
            _ => None,
        }
    }

    #[test]
    fn transport_words_go_to_spotify_only_when_it_is_named_or_running() {
        assert_eq!(control("pause spotify", false).as_deref(), Some("pause"));
        assert_eq!(control("next track on spotify", false).as_deref(), Some("next"));
        assert_eq!(control("spotify shuffle off", false).as_deref(), Some("shuffle off"));
        assert_eq!(control("next", false), None, "left to the media keys");
        assert_eq!(control("next", true).as_deref(), Some("next"));
        assert_eq!(control("repeat song", true).as_deref(), Some("repeat track"));
        assert_eq!(control("hello spotify", false), None);
    }

    #[test]
    fn volume_and_seek_need_spotify_named() {
        assert_eq!(control("spotify volume 40", false).as_deref(), Some("volume 40"));
        assert_eq!(control("set volume to 40 percent on spotify", false).as_deref(), Some("volume 40"));
        assert_eq!(control("spotify volume 500", false).as_deref(), Some("volume 100"));
        assert_eq!(control("volume 40", true), None, "the system volume");
        assert_eq!(control("seek 30 on spotify", false).as_deref(), Some("seek 30s"));
        assert_eq!(control("spotify jump to 5000 ms", false).as_deref(), Some("seek 5000ms"));
        assert_eq!(control("spotify seek soon", false), None);
    }

    #[test]
    fn play_phrases() {
        assert_eq!(play("play loser on spotify", false).as_deref(), Some("loser"));
        assert_eq!(play("play the song yellow by coldplay in my spotify app", false).as_deref(), Some("yellow by coldplay"));
        assert_eq!(play("open spotify and play believer", false).as_deref(), Some("believer"));
        assert_eq!(play("spotify play numb", false).as_deref(), Some("numb"));
        assert_eq!(play("play numb on spot i", false).as_deref(), Some("numb"), "speech slip for spotify");
        assert_eq!(play("play numb", false), None, "no Spotify named or running");
        assert_eq!(play("play numb", true).as_deref(), Some("numb"));
        assert_eq!(play("queue yellow on spotify", false).as_deref(), Some("queue yellow"));
        assert_eq!(play("add yellow to spotify", false).as_deref(), Some("queue yellow"));
        assert_eq!(play("add milk to the list", false), None);
        assert_eq!(play("play", true), None, "a bare play is a resume");
        assert_eq!(control("play", true).as_deref(), Some("resume"));
    }

    #[test]
    fn queries_are_cleaned() {
        assert_eq!(normalize_query("  the song Yellow please "), "Yellow");
        assert_eq!(normalize_query("a thousand years in spotify"), "thousand years");
        assert_eq!(normalize_query("the"), "the", "too short to strip");
        assert!(is_generic_query("some music"));
        assert!(is_generic_query("spotify: music please"));
        assert!(!is_generic_query("yellow"));
    }

    #[test]
    fn requests_are_classified() {
        assert_eq!(classify_query("my liked songs"), (QueryKind::LikedSongs, String::new()));
        assert_eq!(classify_query("queue yellow"), (QueryKind::Queue, "yellow".into()));
        assert_eq!(classify_query("album abbey road"), (QueryKind::Album, "abbey road".into()));
        assert_eq!(classify_query("coldplay artist"), (QueryKind::Artist, "coldplay".into()));
        assert_eq!(classify_query("my gym playlist"), (QueryKind::UserPlaylist, "gym".into()));
        assert_eq!(classify_query("yellow"), (QueryKind::Track, "yellow".into()));
    }

    fn cand(name: &str, by: &str, popularity: u32) -> Candidate {
        Candidate { uri: format!("spotify:track:{name}"), name: name.into(), by: by.into(), popularity }
    }

    #[test]
    fn the_best_match_wins_and_a_poor_playlist_is_refused() {
        let tracks = [cand("Yellow Submarine", "The Beatles", 60), cand("Yellow", "Coldplay", 80)];
        assert_eq!(pick_track(&tracks, "yellow").unwrap().by, "Coldplay", "the exact name and the popular one");
        assert_eq!(pick_track(&[], "x"), None);
        let lists = [cand("Gym Mix", "me", 0), cand("Chill", "me", 0)];
        assert_eq!(pick_playlist(&lists, "gym").unwrap().name, "Gym Mix");
        assert!(pick_playlist(&lists, "zzz").is_none());
        let albums = [cand("Abbey Road", "The Beatles", 0), cand("Help", "The Beatles", 0)];
        assert_eq!(pick_album_or_artist(&albums, "help").unwrap().name, "Help");
        assert_eq!(pick_album_or_artist(&albums, "qqq").unwrap().name, "Abbey Road", "else the first");
    }

    #[test]
    fn the_computer_is_the_device_to_play_on() {
        let d = |id: &str, kind: &str, active: bool| Device { id: id.into(), name: id.into(), kind: kind.into(), active };
        let devices = [d("phone", "Smartphone", true), d("", "Computer", false), d("pc", "computer", false)];
        assert_eq!(pick_device(&devices).unwrap().id, "pc");
        assert_eq!(pick_device(&[d("phone", "Smartphone", false)]).unwrap().id, "phone");
        assert!(pick_device(&[d("", "Computer", false)]).is_none());
    }

    #[test]
    fn seeks() {
        assert_eq!(seek_ms("30s"), Some(30_000));
        assert_eq!(seek_ms("1500ms"), Some(1500));
        assert_eq!(seek_ms("45"), Some(45_000));
        assert_eq!(seek_ms("soon"), None);
    }
}
