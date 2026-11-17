//! Which profile, and which of its configs, applies to what the user is looking at. Port of
//! `Services/ProfileMatcher.cs`.
//!
//! Lives beside the profile model because it reads it; everything it needs from the world
//! arrives in a [`ForegroundContext`] and the YouTube playback token, so it is pure.
//!
//! Results are indexes into the profile list (and the profile's config list), which keeps them
//! valid to hold while the settings are borrowed elsewhere.

use oasis_core::matching::profile_targets::{infer_kind, is_website, WEBSITE_KIND};
use oasis_core::matching::text::{contains_ignore_case, is_blank};
use oasis_core::matching::url_host::{
    site_label, try_get_url_host, url_host_or_path_contains,
};
use oasis_core::matching::{BrowserProcesses, ForegroundContext, YouTubeHosts};

use crate::model::{ConfigProfile, Profile};

/// The profile and config that apply right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveProfile {
    pub profile: usize,
    /// `None` only for a profile saved with no configs at all.
    pub config: Option<usize>,
}

/// Whether any profile matches on a website, which is what makes reading the address bar worth
/// doing. `youtube` has no dot but still needs the address-bar check.
pub fn has_url_based_profiles(profiles: &[Profile]) -> bool {
    profiles.iter().any(|profile| {
        let m = profile.context_match.trim();
        !m.is_empty()
            && m != "*"
            && (YouTubeHosts::is_context_match(Some(m)) || is_website(Some(&profile.kind), Some(m)))
    })
}

/// Whether any profile has an extra set of buttons chosen by a panel or a window title, which is
/// what makes reading the focused control worth doing (port of `ContextTracker.NeedsPanelProbe`).
pub fn has_panel_profiles(profiles: &[Profile]) -> bool {
    profiles.iter().filter(|p| p.multi_config_enabled && !p.config_profiles.is_empty()).any(|profile| {
        profile.config_profiles.iter().filter(|c| !c.is_primary()).any(|config| {
            let m = config.context_match.trim();
            !config.panel_id.trim().is_empty() || (!m.is_empty() && m != "*")
        })
    })
}

pub fn resolve_active_profile(
    profiles: &[Profile],
    context: &ForegroundContext,
    watch_token: &str,
) -> Option<ActiveProfile> {
    let profile = matching_profile(profiles, context)?;
    Some(ActiveProfile { profile, config: matching_config(&profiles[profile], context, watch_token) })
}

/// The profile for this context. Website profiles are tried first: a `chrome` profile matches
/// every Chrome window, so listed before `netflix.com` it would hide it. Then application
/// profiles, then the first wildcard profile.
pub fn matching_profile(profiles: &[Profile], context: &ForegroundContext) -> Option<usize> {
    for website_pass in [true, false] {
        for (index, profile) in profiles.iter().enumerate() {
            if is_wildcard(&profile.context_match) {
                continue;
            }
            if is_website(Some(&profile.kind), Some(&profile.context_match)) != website_pass {
                continue;
            }
            if matches_context(&profile.context_match, website_pass, context) {
                return Some(index);
            }
        }
    }
    profiles.iter().position(|p| is_blank(Some(&p.context_match)) || p.context_match == "*")
}

fn is_wildcard(context_match: &str) -> bool {
    is_blank(Some(context_match)) || context_match == "*"
}

/// The config of `app` for this context.
pub fn matching_config(
    app: &Profile,
    context: &ForegroundContext,
    watch_token: &str,
) -> Option<usize> {
    let primary = app.primary_config_index();
    if !app.multi_config_enabled {
        return primary;
    }
    let configs = &app.config_profiles;

    // A YouTube-gated config needs YouTube in front, or a browser tab that is not clearly some
    // other site (the address bar read sometimes comes back empty mid-hold).
    let media_when = if watch_token.is_empty() {
        ""
    } else if is_youtube_foreground(context)
        || (BrowserProcesses::looks_like_browser(Some(&context.app_name))
            && !is_clearly_non_youtube_foreground(context))
    {
        watch_token
    } else {
        ""
    };

    if !media_when.is_empty() {
        let hit = configs.iter().enumerate().find(|(_, config)| {
            !config.is_primary()
                && config.normalized_when() == media_when
                && (matches_config(config, context, true)
                    || matches_config(config, context, false)
                    || matches_media_only(config))
        });
        if let Some((index, _)) = hit {
            return Some(index);
        }
    }

    if should_use_primary_only(context) {
        return primary;
    }

    for require_panel in [true, false] {
        let hit = configs.iter().enumerate().find(|(_, config)| {
            !config.is_primary()
                && !config.has_media_when()
                && matches_config(config, context, require_panel)
        });
        if let Some((index, _)) = hit {
            return Some(index);
        }
    }
    primary
}

/// The foreground is a YouTube page: its address, or a `- YouTube` title (a Google result that
/// merely mentions the word does not count).
pub fn is_youtube_foreground(context: &ForegroundContext) -> bool {
    try_get_url_host(&context.url).is_some_and(|host| YouTubeHosts::is_host(Some(&host)))
        || title_looks_like_youtube_watch(&context.window_title)
}

/// We can positively tell the foreground tab is some other website.
pub fn is_clearly_non_youtube_foreground(context: &ForegroundContext) -> bool {
    if is_youtube_foreground(context) {
        return false;
    }
    if try_get_url_host(&context.url).is_some_and(|host| !YouTubeHosts::is_host(Some(&host))) {
        return true;
    }
    is_search_results_title(context.window_title.trim())
}

fn is_search_results_title(title: &str) -> bool {
    [" - Google Search", " - Bing", " - DuckDuckGo"]
        .iter()
        .any(|marker| contains_ignore_case(title, marker))
}

/// `Video - YouTube` and `Video - YouTube - Brave` count; search results do not.
fn title_looks_like_youtube_watch(title: &str) -> bool {
    let text = title.trim();
    !text.is_empty() && !is_search_results_title(text) && contains_ignore_case(text, " - YouTube")
}

/// A YouTube-gated config with no panel and no match still applies on playback state alone.
fn matches_media_only(config: &ConfigProfile) -> bool {
    config.has_media_when()
        && config.panel_id.trim().is_empty()
        && (config.context_match.trim().is_empty() || config.context_match.trim() == "*")
}

fn should_use_primary_only(context: &ForegroundContext) -> bool {
    context.panel_id.trim().is_empty()
        || context.panel_id == "unknown"
        || matches!(context.detection_confidence.as_str(), "none" | "low")
}

fn matches_config(config: &ConfigProfile, context: &ForegroundContext, require_panel: bool) -> bool {
    if !config.panel_id.trim().is_empty() {
        if !config.panel_id.eq_ignore_ascii_case(&context.panel_id) {
            return false;
        }
        if require_panel {
            let has_hints =
                !config.focus_class_name.trim().is_empty() || !config.ancestor_hint.trim().is_empty();
            if has_hints && !matches_signature_hints(config, context) {
                return false;
            }
        }
        return true;
    }
    if require_panel {
        return false;
    }
    let m = config.context_match.trim();
    !m.is_empty() && m != "*" && matches_title_or_url(m, context)
}

fn matches_signature_hints(config: &ConfigProfile, context: &ForegroundContext) -> bool {
    if !config.focus_class_name.trim().is_empty()
        && contains_ignore_case(&context.focus_class_name, &config.focus_class_name)
    {
        return true;
    }
    if !config.ancestor_hint.trim().is_empty()
        && !context.ancestor_hint.trim().is_empty()
        && contains_ignore_case(&context.ancestor_hint, &config.ancestor_hint)
    {
        return true;
    }
    config.focus_class_name.trim().is_empty() && config.ancestor_hint.trim().is_empty()
}

/// A website profile matches the browser's address (or, when that cannot be read, the page
/// title); an application profile matches the process name or window title. Which one a profile
/// is comes from its stored kind, not from its match text: the Claude desktop app (match
/// `Claude`) is an application even though claude.ai is a website.
fn matches_context(context_match: &str, website: bool, context: &ForegroundContext) -> bool {
    if website {
        if YouTubeHosts::is_context_match(Some(context_match)) {
            return is_youtube_foreground(context);
        }
        // The address host or path, never titles or query strings.
        return url_host_or_path_contains(&context.url, context_match)
            || bare_site_title_fallback(context_match, context);
    }
    contains_ignore_case(&context.url, context_match)
        || contains_ignore_case(&context.window_title, context_match)
        || contains_ignore_case(&context.app_name, context_match)
}

fn matches_title_or_url(context_match: &str, context: &ForegroundContext) -> bool {
    if YouTubeHosts::is_context_match(Some(context_match)) {
        return is_youtube_foreground(context);
    }
    if looks_like_host_or_url_fragment(context_match) {
        return url_host_or_path_contains(&context.url, context_match)
            || bare_site_title_fallback(context_match, context);
    }
    contains_ignore_case(&context.url, context_match)
        || contains_ignore_case(&context.window_title, context_match)
}

fn looks_like_host_or_url_fragment(context_match: &str) -> bool {
    let value = context_match.trim();
    if value.is_empty() || value.contains('\\') || value.to_ascii_lowercase().ends_with(".exe") {
        return false;
    }
    infer_kind(Some(value)) == WEBSITE_KIND
}

/// A website profile (`netflix.com` or `netflix`) also matches a browser's page title when the
/// address bar could not be read: UI Automation often returns nothing (the tree is built on first
/// use, the bar is hidden in fullscreen video). Only for an empty address, and never for a search
/// results page that merely mentions the site.
fn bare_site_title_fallback(site: &str, context: &ForegroundContext) -> bool {
    if !context.url.trim().is_empty()
        || !BrowserProcesses::looks_like_browser(Some(&context.app_name))
        || context.window_title.trim().is_empty()
    {
        return false;
    }
    let label = site_label(site);
    label.len() >= 3
        && !is_search_results_title(&context.window_title)
        && contains_ignore_case(&context.window_title, &label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis_core::bridge::ConfigWhen;

    fn profile(name: &str, context_match: &str) -> Profile {
        Profile { name: name.into(), context_match: context_match.into(), ..Default::default() }
    }

    fn profiles(matches: &[&str]) -> Vec<Profile> {
        matches.iter().map(|m| profile(m, m)).collect()
    }

    fn name_of(list: &[Profile], context: &ForegroundContext) -> Option<String> {
        matching_profile(list, context).map(|i| list[i].name.clone())
    }

    fn ctx(app: &str, title: &str, url: &str) -> ForegroundContext {
        ForegroundContext::new(app, title, url)
    }

    // ---- ports of ProfileMatcherWebsiteTests -------------------------------------------

    #[test]
    fn a_website_profile_beats_an_earlier_browser_profile() {
        let list = profiles(&["*", "chrome", "netflix.com"]);
        let c = ctx("chrome", "The Mentalist - Netflix", "netflix.com/watch/80014185?trackId=1");
        assert_eq!(name_of(&list, &c).as_deref(), Some("netflix.com"));
    }

    #[test]
    fn the_browser_profile_still_applies_on_other_sites() {
        let list = profiles(&["*", "chrome", "netflix.com"]);
        assert_eq!(name_of(&list, &ctx("chrome", "Example", "example.org")).as_deref(), Some("chrome"));
    }

    #[test]
    fn an_app_named_like_a_website_matches_its_own_window() {
        let mut list = profiles(&["*"]);
        list.push(Profile { kind: "app".into(), ..profile("Claude", "Claude") });
        assert_eq!(name_of(&list, &ctx("claude", "Claude", "")).as_deref(), Some("Claude"));
    }

    #[test]
    fn a_website_profile_never_matches_a_desktop_window_by_its_name() {
        let mut list = profiles(&["*"]);
        list.push(Profile { kind: "website".into(), ..profile("Claude web", "claude.ai") });
        assert_eq!(name_of(&list, &ctx("claude", "Claude", "")).as_deref(), Some("*"));
    }

    #[test]
    fn netflix_matches_by_title_when_the_address_is_unreadable() {
        let list = profiles(&["*", "chrome", "netflix.com"]);
        assert_eq!(name_of(&list, &ctx("chrome", "Netflix", "")).as_deref(), Some("netflix.com"));
    }

    // ---- behaviour read from the C# source ---------------------------------------------

    /// The owner's real case (W2b): a ChatGPT tab titled for the conversation, not the site.
    #[test]
    fn a_website_profile_matches_by_address_when_the_title_says_nothing_about_the_site() {
        let list = profiles(&["*", "chatgpt.com"]);
        let tab = |url| ctx("brave", "Blindfolded Project Build - Brave", url);
        assert_eq!(name_of(&list, &tab("chatgpt.com/c/6aee8d8a-099c")).as_deref(), Some("chatgpt.com"));
        assert_eq!(name_of(&list, &tab("")).as_deref(), Some("*"), "the title alone cannot tell");
    }

    #[test]
    fn the_title_fallback_is_not_used_for_a_search_results_page() {
        let list = profiles(&["*", "netflix.com"]);
        let c = ctx("chrome", "netflix - Google Search", "");
        assert_eq!(name_of(&list, &c).as_deref(), Some("*"));
    }

    #[test]
    fn the_title_fallback_is_only_for_browsers_and_only_without_an_address() {
        let list = profiles(&["*", "netflix.com"]);
        assert_eq!(name_of(&list, &ctx("notepad", "Netflix notes", "")).as_deref(), Some("*"));
        assert_eq!(name_of(&list, &ctx("chrome", "Netflix", "example.org")).as_deref(), Some("*"));
    }

    #[test]
    fn an_application_profile_matches_process_title_or_address_text() {
        let list = profiles(&["*", "code"]);
        assert_eq!(name_of(&list, &ctx("code", "main.rs", "")).as_deref(), Some("code"));
        assert_eq!(name_of(&list, &ctx("notepad", "my code.txt", "")).as_deref(), Some("code"));
        assert_eq!(name_of(&list, &ctx("notepad", "todo", "")).as_deref(), Some("*"));
    }

    #[test]
    fn no_wildcard_profile_and_no_match_gives_nothing() {
        assert_eq!(name_of(&profiles(&["chrome"]), &ctx("notepad", "x", "")), None);
        assert_eq!(matching_profile(&[], &ctx("a", "b", "")), None);
    }

    #[test]
    fn a_blank_match_counts_as_the_wildcard() {
        let list = vec![profile("Blank", "  "), profile("Other", "chrome")];
        assert_eq!(name_of(&list, &ctx("notepad", "", "")).as_deref(), Some("Blank"));
    }

    #[test]
    fn a_youtube_profile_needs_real_youtube_in_front() {
        let list = profiles(&["*", "youtube"]);
        let yes = ctx("brave", "Cats - YouTube - Brave", "");
        let by_url = ctx("chrome", "Anything", "youtube.com/watch?v=1");
        let search = ctx("chrome", "youtube cats - Google Search", "");
        assert_eq!(name_of(&list, &yes).as_deref(), Some("youtube"));
        assert_eq!(name_of(&list, &by_url).as_deref(), Some("youtube"));
        assert_eq!(name_of(&list, &search).as_deref(), Some("*"));
    }

    #[test]
    fn the_address_is_read_only_when_a_website_profile_exists() {
        assert!(has_url_based_profiles(&profiles(&["*", "netflix.com"])));
        assert!(has_url_based_profiles(&profiles(&["youtube"])));
        assert!(!has_url_based_profiles(&profiles(&["*", "chrome", "code"])));
        let kind = Profile { kind: "website".into(), ..profile("W", "netflix") };
        assert!(has_url_based_profiles(&[kind]));
    }

    // ---- configs -----------------------------------------------------------------------

    fn config(name: &str, context_match: &str) -> ConfigProfile {
        ConfigProfile { name: name.into(), context_match: context_match.into(), ..Default::default() }
    }

    fn multi(configs: Vec<ConfigProfile>) -> Profile {
        Profile { multi_config_enabled: true, config_profiles: configs, ..profile("P", "chrome") }
    }

    fn config_name(app: &Profile, c: &ForegroundContext, token: &str) -> Option<String> {
        matching_config(app, c, token).map(|i| app.config_profiles[i].name.clone())
    }

    #[test]
    fn without_multi_config_the_primary_is_always_used() {
        let mut app = multi(vec![config("Docs", "docs"), config("Main", "*")]);
        app.multi_config_enabled = false;
        assert_eq!(config_name(&app, &ctx("chrome", "docs", ""), "").as_deref(), Some("Main"));
    }

    #[test]
    fn a_config_matching_the_title_wins_once_a_panel_is_known_and_otherwise_primary_applies() {
        let app = multi(vec![config("Main", "*"), config("Docs", "docs")]);
        let mut c = ctx("chrome", "My docs", "");
        // No panel detection (the probes are not ported): primary only.
        assert_eq!(config_name(&app, &c, "").as_deref(), Some("Main"));
        c.panel_id = "editor".into();
        c.detection_confidence = "high".into();
        assert_eq!(config_name(&app, &c, "").as_deref(), Some("Docs"));
        c.window_title = "elsewhere".into();
        assert_eq!(config_name(&app, &c, "").as_deref(), Some("Main"));
    }

    #[test]
    fn a_panel_config_needs_the_panel_and_its_hints() {
        let mut terminal = config("Terminal", "");
        terminal.panel_id = "terminal".into();
        terminal.focus_class_name = "xterm".into();
        let app = multi(vec![config("Main", "*"), terminal]);

        let mut c = ctx("code", "x", "");
        c.panel_id = "TERMINAL".into();
        c.detection_confidence = "high".into();
        c.focus_class_name = "other".into();
        // Hint missing: the strict pass rejects it, the relaxed pass accepts the panel alone.
        assert_eq!(config_name(&app, &c, "").as_deref(), Some("Terminal"));
        c.panel_id = "editor".into();
        assert_eq!(config_name(&app, &c, "").as_deref(), Some("Main"));
        c.panel_id = "unknown".into();
        assert_eq!(config_name(&app, &c, "").as_deref(), Some("Main"));
    }

    #[test]
    fn a_youtube_gated_config_applies_while_youtube_plays() {
        let mut playing = config("Playing", "");
        playing.when = "playing".into();
        let mut ended = config("Ended", "");
        ended.when = "youtube.ended".into();
        let app = multi(vec![config("Main", "*"), playing, ended]);

        let on_youtube = ctx("chrome", "Cats - YouTube", "youtube.com/watch?v=1");
        assert_eq!(config_name(&app, &on_youtube, ConfigWhen::PLAYING).as_deref(), Some("Playing"));
        assert_eq!(config_name(&app, &on_youtube, ConfigWhen::ENDED).as_deref(), Some("Ended"));
        assert_eq!(config_name(&app, &on_youtube, "").as_deref(), Some("Main"));
    }

    #[test]
    fn a_gated_config_holds_while_the_address_is_unreadable_but_not_on_another_site() {
        let mut playing = config("Playing", "");
        playing.when = "playing".into();
        let app = multi(vec![config("Main", "*"), playing]);

        let blank_address = ctx("chrome", "Some tab", "");
        assert_eq!(config_name(&app, &blank_address, ConfigWhen::PLAYING).as_deref(), Some("Playing"));
        let other_site = ctx("chrome", "Example", "example.org");
        assert_eq!(config_name(&app, &other_site, ConfigWhen::PLAYING).as_deref(), Some("Main"));
        let search = ctx("chrome", "cats - Google Search", "");
        assert_eq!(config_name(&app, &search, ConfigWhen::PLAYING).as_deref(), Some("Main"));
        let not_a_browser = ctx("notepad", "x", "");
        assert_eq!(config_name(&app, &not_a_browser, ConfigWhen::PLAYING).as_deref(), Some("Main"));
    }

    #[test]
    fn gated_configs_are_skipped_by_the_ordinary_passes() {
        let mut gated = config("Gated", "docs");
        gated.when = "playing".into();
        let app = multi(vec![config("Main", "*"), gated]);
        let mut c = ctx("chrome", "docs", "");
        c.panel_id = "editor".into();
        c.detection_confidence = "high".into();
        assert_eq!(config_name(&app, &c, "").as_deref(), Some("Main"));
    }

    #[test]
    fn a_profile_with_no_configs_has_no_active_config() {
        let app = Profile { multi_config_enabled: true, ..profile("P", "x") };
        assert_eq!(matching_config(&app, &ctx("a", "b", ""), ""), None);
    }

    #[test]
    fn resolving_gives_the_profile_and_config_indexes() {
        let mut list = profiles(&["*", "chrome"]);
        list[1].config_profiles = vec![config("Main", "*")];
        let active = resolve_active_profile(&list, &ctx("chrome", "x", ""), "").unwrap();
        assert_eq!(active, ActiveProfile { profile: 1, config: Some(0) });
        assert_eq!(resolve_active_profile(&[], &ctx("a", "b", ""), ""), None);
    }

    #[test]
    fn youtube_foreground_helpers() {
        assert!(is_youtube_foreground(&ctx("brave", "Video - YouTube - Brave", "")));
        assert!(is_youtube_foreground(&ctx("chrome", "x", "https://music.youtube.com/")));
        assert!(!is_youtube_foreground(&ctx("chrome", "youtube - Bing", "")));
        assert!(is_clearly_non_youtube_foreground(&ctx("chrome", "x", "example.org")));
        assert!(is_clearly_non_youtube_foreground(&ctx("chrome", "a - DuckDuckGo", "")));
        assert!(!is_clearly_non_youtube_foreground(&ctx("chrome", "x", "")));
        assert!(!is_clearly_non_youtube_foreground(&ctx("chrome", "x", "youtu.be/abc")));
    }
}
