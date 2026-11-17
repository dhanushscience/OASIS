//! Spoken window control (`docs/APP_WINDOWS.md` W8, W9, W10): "maximize this", "minimize chrome",
//! "move chrome to the other screen", "snap notepad left", "put chrome and vs code side by side",
//! "close this window", "save this layout", "restore my layout".
//!
//! A fixed list of phrases; nothing said reaches the OS as text. The result is one action,
//! `WindowCommand` with `verb|target` (an empty target is the window in front), `WindowSplit`
//! with the apps to place, or `WindowLayout` with `save|name` / `restore|name`. A move names its
//! screen as `next` or `to:<spec>`, the spec being what `placement::pick_screen` reads: `main`
//! (primary, first), `other:N` (secondary, second, third and so on: the others, left to right),
//! `left`, `right`. Whole utterance
//! only, so "maximize the plan for next year" is not a window command. "close <app>" is not here:
//! that closes the whole app and is the router's `CloseApp`, which asks first.

/// `(kind, value, label)` for a window phrase, or `None` when the phrase is something else.
pub fn plan(normalized: &str) -> Option<(&'static str, String, String)> {
    let text = normalized.strip_prefix("please ").unwrap_or(normalized).trim();
    layout(text).or_else(|| split(text)).or_else(|| single(text))
}

/// One app and the screen it should end up on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arrangement {
    pub app: String,
    /// `to:main`, `to:other:1` and so on (see the header).
    pub screen: String,
    /// "open chrome on ..." starts the app first; "move chrome to ..." only moves it.
    pub open: bool,
}

/// "open chrome on my primary screen and open notepad on my secondary screen", "put chrome on the
/// left screen and notepad on the right screen": every clause names an app and a screen, or there
/// is no arrangement (so "open chrome and search for cats" is not one). One plain "move X to ..."
/// is the `WindowCommand`, so a lone clause counts only when it opens.
pub fn arrangements(normalized: &str) -> Option<Vec<Arrangement>> {
    let text = normalized.strip_prefix("please ").unwrap_or(normalized).trim();
    let text = text.replace(" and then ", " and ").replace(", and ", " and ").replace(" then ", " and ").replace(',', " and ");
    let mut out: Vec<Arrangement> = Vec::new();
    for clause in text.split(" and ") {
        let clause = clause.trim();
        // a clause after the first may leave out its verb: "... and notepad on the secondary screen"
        let (open, rest) = match ["open ", "launch ", "start ", "run "].into_iter().find_map(|v| clause.strip_prefix(v)) {
            Some(rest) => (true, rest),
            None => match ["move ", "put ", "send ", "place "].into_iter().find_map(|v| clause.strip_prefix(v)) {
                Some(rest) => (false, rest),
                None if !out.is_empty() => (out.last()?.open, clause),
                None => return None,
            },
        };
        let (app, screen) = [" on ", " onto ", " to ", " in ", " into "].into_iter().find_map(|j| {
            let (app, place) = rest.split_once(j)?;
            Some((named(app)?, screen_of(place)?))
        })?;
        if app.is_empty() || !screen.starts_with("to:") {
            return None;
        }
        out.push(Arrangement { app, screen, open });
    }
    let opens_something = out.iter().any(|a| a.open);
    (out.len() >= 2 || opens_something).then_some(out)
}

/// "save this layout [as work]", "restore my layout", "restore the work layout".
fn layout(text: &str) -> Option<(&'static str, String, String)> {
    let words: Vec<&str> = text.split(' ').collect();
    let (verb, rest) = match words.first()? {
        &"save" | &"remember" | &"capture" => ("save", &words[1..]),
        &"restore" | &"load" | &"bring" | &"recall" | &"apply" => ("restore", &words[1..]),
        _ => return None,
    };
    // "bring back my layout"
    let rest: Vec<&str> = if rest.first() == Some(&"back") { rest[1..].to_vec() } else { rest.to_vec() };
    let at = rest.iter().position(|w| *w == "layout" || *w == "layouts")?;
    // before "layout": this / my / the / a name ("work"); after: "as work" (save) or "work" (restore)
    let before: Vec<&str> = rest[..at].iter().copied().filter(|w| !matches!(*w, "this" | "my" | "the" | "current" | "a" | "window" | "windows")).collect();
    let mut after: Vec<&str> = rest[at + 1..].to_vec();
    if after.first() == Some(&"as") || after.first() == Some(&"called") || after.first() == Some(&"named") {
        after.remove(0);
    }
    let name = if before.is_empty() { after.join(" ") } else { before.join(" ") };
    let label = if verb == "save" { "SAVE LAYOUT" } else { "RESTORE LAYOUT" };
    Some(("WindowLayout", format!("{verb}|{name}"), label.into()))
}

/// "put chrome and vs code side by side", "split chrome and notepad".
fn split(text: &str) -> Option<(&'static str, String, String)> {
    let apps = if let Some(before) = text.strip_suffix(" side by side") {
        let before = ["put ", "show ", "place ", "arrange ", "have ", "open ", "set ", "line up "].into_iter().find_map(|v| before.strip_prefix(v)).unwrap_or(before);
        before.to_string()
    } else if let Some(rest) = text.strip_prefix("split ") {
        rest.trim_start_matches("the ").to_string()
    } else {
        return None;
    };
    let apps: Vec<String> = apps
        .replace(", and ", " and ")
        .replace(',', " and ")
        .split(" and ")
        .filter_map(|a| named(a.trim()).filter(|a| !a.is_empty()))
        .collect();
    if apps.len() < 2 {
        return None;
    }
    let label = format!("SPLIT {}", apps.join(" + ").to_uppercase());
    Some(("WindowSplit", apps.join(","), label))
}

/// One window: a verb on the window in front or on a named app.
fn single(text: &str) -> Option<(&'static str, String, String)> {
    // Close: only "this window". An app by name is `CloseApp`, which asks before it closes everything.
    if matches!(text, "close this window" | "close the window" | "close window" | "close current window" | "close this") {
        return Some(("WindowCommand", "close|".into(), "CLOSE WINDOW".into()));
    }
    for (said, verb) in [("maximize ", "maximize"), ("maximise ", "maximize"), ("minimize ", "minimize"), ("minimise ", "minimize"), ("restore ", "restore")] {
        if let Some(rest) = text.strip_prefix(said) {
            let who = named(rest)?;
            return Some(("WindowCommand", format!("{verb}|{who}"), label(verb, &who)));
        }
        if text == said.trim_end() {
            return Some(("WindowCommand", format!("{verb}|"), label(verb, "")));
        }
    }
    // "move chrome to the other screen", "send this to screen 2", "put it on the second monitor"
    for verb in ["move ", "send ", "put ", "shift ", "throw "] {
        let Some(rest) = text.strip_prefix(verb) else { continue };
        for joiner in [" to ", " onto ", " on ", " into ", " in "] {
            let Some((who, place)) = rest.split_once(joiner) else { continue };
            if let Some(screen) = screen_of(place) {
                let Some(who) = named(who) else { continue };
                return Some(("WindowCommand", format!("{screen}|{who}"), label("move", &who)));
            }
            if let Some(side) = side_of(place) {
                let Some(who) = named(who) else { continue };
                return Some(("WindowCommand", format!("{side}|{who}"), label("snap", &who)));
            }
        }
    }
    // "snap chrome left", "snap this to the right half"
    if let Some(rest) = text.strip_prefix("snap ") {
        let rest = rest.strip_prefix("to the ").or_else(|| rest.strip_prefix("to ")).unwrap_or(rest);
        for joiner in [" to the ", " to ", " "] {
            if let Some((who, place)) = rest.rsplit_once(joiner) {
                if let Some(side) = side_of(place) {
                    let Some(who) = named(who) else { continue };
                    return Some(("WindowCommand", format!("{side}|{who}"), label("snap", &who)));
                }
            }
        }
        if let Some(side) = side_of(rest) {
            return Some(("WindowCommand", format!("{side}|"), label("snap", "")));
        }
    }
    None
}

/// Which screen a phrase names: `next`, or `to:<spec>` (see the header). "primary", "main" and "first"
/// are the main screen; "secondary", "second", "third" and so on are the others, left to right;
/// "left" and "right" are by position; "next", "other" and "another" go to the following screen.
fn screen_of(place: &str) -> Option<String> {
    let place = strip_article(place.trim());
    let words: Vec<&str> = place.split(' ').filter(|w| !w.is_empty()).collect();
    let is_noun = |w: &str| matches!(w, "screen" | "monitor" | "display");
    let qualifier = match words.as_slice() {
        [n] if is_noun(n) => "next",
        [q] if matches!(*q, "primary" | "main" | "secondary") => *q,
        [q, n] if is_noun(n) => *q,
        [n, q] if is_noun(n) => *q,
        _ => return None,
    };
    let spec = match qualifier {
        "next" | "other" | "another" => return Some("next".into()),
        "primary" | "main" => "main".to_string(),
        "secondary" => "other:1".to_string(),
        "left" => "left".to_string(),
        "right" => "right".to_string(),
        n => match number(n)? {
            1 => "main".to_string(),
            k => format!("other:{}", k - 1),
        },
    };
    Some(format!("to:{spec}"))
}

fn strip_article(text: &str) -> &str {
    ["the ", "my ", "our ", "your "].into_iter().find_map(|a| text.strip_prefix(a)).map_or(text, str::trim_start)
}

fn number(word: &str) -> Option<u32> {
    let n = match word {
        "one" | "first" | "1st" => 1,
        "two" | "second" | "2nd" => 2,
        "three" | "third" | "3rd" => 3,
        "four" | "fourth" | "4th" => 4,
        digits => digits.parse().ok()?,
    };
    (1..=8).contains(&n).then_some(n)
}

/// "left", "the right half", "top side" → `left`, `right`, `top`, `bottom`.
fn side_of(place: &str) -> Option<&'static str> {
    let place = strip_article(place.trim());
    let word = place.strip_suffix(" half").or_else(|| place.strip_suffix(" side")).or_else(|| place.strip_suffix(" of the screen")).unwrap_or(place);
    Some(match word {
        "left" => "left",
        "right" => "right",
        "top" | "upper" => "top",
        "bottom" | "lower" => "bottom",
        _ => return None,
    })
}

/// The app a phrase names; empty for the window in front ("this", "it", "the window", …).
fn target(spoken: &str) -> String {
    let spoken = spoken.trim().trim_start_matches("the ").trim();
    if matches!(spoken, "" | "this" | "it" | "that" | "this window" | "that window" | "window" | "current window" | "the window" | "this app" | "this one") {
        return String::new();
    }
    spoken.strip_suffix(" window").or_else(|| spoken.strip_suffix(" app")).unwrap_or(spoken).trim().to_string()
}

/// [`target`], or `None` when it reads like a sentence and not an app ("the file from backup").
fn named(spoken: &str) -> Option<String> {
    let who = target(spoken);
    let sentence = who.split_whitespace().count() > 3 || [" from ", " for ", " to ", " of ", " in ", " with ", " at ", " by ", " the ", " my "].iter().any(|w| format!(" {who} ").contains(w));
    (!sentence).then_some(who)
}

fn label(verb: &str, who: &str) -> String {
    let who = if who.is_empty() { "THIS" } else { who };
    format!("{} {}", verb.to_uppercase(), who.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Option<(String, String)> {
        plan(text).map(|(k, v, _)| (k.to_string(), v))
    }
    fn is(text: &str, kind: &str, value: &str) {
        assert_eq!(p(text), Some((kind.to_string(), value.to_string())), "{text}");
    }

    #[test]
    fn a_verb_acts_on_the_window_in_front_or_a_named_app() {
        is("maximize", "WindowCommand", "maximize|");
        is("maximize this", "WindowCommand", "maximize|");
        is("maximise this window", "WindowCommand", "maximize|");
        is("minimize chrome", "WindowCommand", "minimize|chrome");
        is("minimize vs code", "WindowCommand", "minimize|vs code");
        is("restore notepad", "WindowCommand", "restore|notepad");
        is("close this window", "WindowCommand", "close|");
    }

    #[test]
    fn moving_between_screens() {
        is("move chrome to the other screen", "WindowCommand", "next|chrome");
        is("move this to the next monitor", "WindowCommand", "next|");
        is("send notepad to screen 2", "WindowCommand", "to:other:1|notepad");
        is("put chrome on the second monitor", "WindowCommand", "to:other:1|chrome");
        is("move chrome to my primary screen", "WindowCommand", "to:main|chrome");
        is("move chrome to the main display", "WindowCommand", "to:main|chrome");
        is("move notepad to the secondary screen", "WindowCommand", "to:other:1|notepad");
        is("move notepad to secondary", "WindowCommand", "to:other:1|notepad");
        is("send this to the first screen", "WindowCommand", "to:main|");
        is("send this to screen one", "WindowCommand", "to:main|");
        is("move chrome to the third monitor", "WindowCommand", "to:other:2|chrome");
        is("move chrome to the left screen", "WindowCommand", "to:left|chrome");
        is("move chrome to the right monitor", "WindowCommand", "to:right|chrome");
        is("move it to the other display", "WindowCommand", "next|");
    }

    #[test]
    fn snapping_to_a_side() {
        is("snap chrome left", "WindowCommand", "left|chrome");
        is("snap this to the right half", "WindowCommand", "right|");
        is("snap to the left", "WindowCommand", "left|");
        is("move notepad to the left half", "WindowCommand", "left|notepad");
        is("put chrome on the right side", "WindowCommand", "right|chrome");
        is("snap vs code to the top", "WindowCommand", "top|vs code");
    }

    #[test]
    fn two_or_more_apps_share_the_screen() {
        is("put chrome and vs code side by side", "WindowSplit", "chrome,vs code");
        is("chrome and notepad side by side", "WindowSplit", "chrome,notepad");
        is("split chrome and notepad", "WindowSplit", "chrome,notepad");
        is("split chrome, vs code and spotify", "WindowSplit", "chrome,vs code,spotify");
        assert_eq!(p("put chrome side by side"), None, "one app is not a split");
    }

    #[test]
    fn layouts_are_saved_and_restored_by_name() {
        is("save this layout", "WindowLayout", "save|");
        is("save my layout as work", "WindowLayout", "save|work");
        is("save the work layout", "WindowLayout", "save|work");
        is("restore my layout", "WindowLayout", "restore|");
        is("restore work layout", "WindowLayout", "restore|work");
        is("restore my work layout", "WindowLayout", "restore|work");
        is("load the coding layout", "WindowLayout", "restore|coding");
    }

    #[test]
    fn several_apps_each_on_its_own_screen() {
        let a = |text: &str| arrangements(text).map(|v| v.into_iter().map(|a| (a.app, a.screen, a.open)).collect::<Vec<_>>());
        let t = |app: &str, screen: &str, open: bool| (app.to_string(), screen.to_string(), open);
        assert_eq!(
            a("open chrome in my primary screen and open notepad in my secondary screen"),
            Some(vec![t("chrome", "to:main", true), t("notepad", "to:other:1", true)])
        );
        assert_eq!(
            a("open chrome on the first screen and notepad on the second screen"),
            Some(vec![t("chrome", "to:main", true), t("notepad", "to:other:1", true)])
        );
        assert_eq!(a("open notepad on my secondary screen"), Some(vec![t("notepad", "to:other:1", true)]));
        assert_eq!(
            a("put chrome on the left screen and notepad on the right screen"),
            Some(vec![t("chrome", "to:left", false), t("notepad", "to:right", false)])
        );
        assert_eq!(
            a("move chrome to the primary screen then open spotify on the secondary screen"),
            Some(vec![t("chrome", "to:main", false), t("spotify", "to:other:1", true)])
        );
        // one plain move is a WindowCommand; none of these is an arrangement
        for other in ["move chrome to the primary screen", "open chrome and search for cats", "open chrome", "open chrome and notepad", "open chrome on the moon", "search for chrome on my primary screen"] {
            assert_eq!(a(other), None, "{other}");
        }
    }

    #[test]
    fn anything_else_is_left_alone() {
        for other in [
            "close chrome", "open chrome", "maximize the plan for next year", "move the meeting to friday",
            "put the kettle on", "restore the file from backup", "save the document", "snap a photo", "split the bill", "",
        ] {
            assert_eq!(p(other), None, "{other}");
        }
    }
}
