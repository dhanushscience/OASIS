//! The action kinds, as one list. A profile stores a kind as plain text (`profiles.json` is shared
//! with the C# app and may hold kinds this build does not know), so the text stays what is saved;
//! this enum is only how the code reads it, so a misspelt kind is a compile error, not a button
//! that silently does nothing.

/// Declares the kinds once: the enum, the list used to read text back, and `as_str`. A kind's
/// stored text is its variant name, so one list cannot drift from the other two.
macro_rules! kinds {
    ($($(#[$doc:meta])* $kind:ident,)*) => {
        /// Every kind the code produces or matches. `Unknown` is anything else, kept as text by its owner.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum ActionKind {
            $($(#[$doc])* $kind,)*
            Unknown,
        }

        /// Every kind but `Unknown`, for reading text back.
        const ALL: &[ActionKind] = &[$(ActionKind::$kind,)*];

        impl ActionKind {
            /// The text to store. Producers use this so they cannot drift from what consumers read.
            /// `Unknown` is empty: it has no text of its own. `const` lets the menu lists in
            /// oasis-settings be built from it.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(ActionKind::$kind => stringify!($kind),)*
                    ActionKind::Unknown => "",
                }
            }
        }
    };
}

kinds! {
    Shortcut, OpenUrl, LaunchApp, CloseApp, CloseWindow, WindowCommand, WindowSplit, WindowLayout,
    RunScript, AutoHotkey, MouseClick, PasteText, OpenSettings, JoinMeeting, QuickLaunch, LockPc,
    Chime, PlaySound, Radio, Brightness, Theme, Delay, Automation, Reminder, CancelReminders,
    SpotifyPlay, SpotifyControl,
    /// Removed step kind; an old file's step is skipped, not failed.
    WaitRelease,
    /// Named only so the safety check can refuse them; nothing runs them.
    DeleteFile,
    MoveFile,
}

use ActionKind::*;

impl ActionKind {
    /// Read a stored kind exactly, as the runner always did (C# compared exactly). Two old
    /// spellings are accepted: `Launch App` and `Wait for Release`.
    pub fn of(text: &str) -> ActionKind {
        match text {
            "Launch App" => LaunchApp,
            "Wait for Release" => WaitRelease,
            _ => ALL.iter().copied().find(|k| k.as_str() == text).unwrap_or(Unknown),
        }
    }

    /// Read a kind ignoring ASCII case, for the sites that always did (`eq_ignore_ascii_case`).
    /// No aliases: `Launch App` stays `Unknown` here, as it never matched there.
    pub fn of_ignore_case(text: &str) -> ActionKind {
        ALL.iter().copied().find(|k| k.as_str().eq_ignore_ascii_case(text)).unwrap_or(Unknown)
    }

    /// Any kind the Spotify lane takes: the text starts with `Spotify`, known or not.
    pub fn is_spotify_text(text: &str) -> bool {
        text.starts_with("Spotify")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips() {
        for &kind in ALL {
            let name = kind.as_str();
            assert!(!name.is_empty());
            assert_eq!(ActionKind::of(name), kind);
            assert_eq!(ActionKind::of_ignore_case(name), kind);
        }
        assert_eq!(Unknown.as_str(), "");
    }

    #[test]
    fn exact_reads_aliases_and_ignore_case_does_not() {
        assert_eq!(ActionKind::of("Launch App"), LaunchApp);
        assert_eq!(ActionKind::of("Wait for Release"), WaitRelease);
        assert_eq!(ActionKind::of_ignore_case("Launch App"), Unknown);
    }

    #[test]
    fn case_and_unknown() {
        assert_eq!(ActionKind::of("launchapp"), Unknown);
        assert_eq!(ActionKind::of_ignore_case("launchAPP"), LaunchApp);
        assert_eq!(ActionKind::of(""), Unknown);
        assert_eq!(ActionKind::of("SomethingNew"), Unknown);
        assert_eq!(ActionKind::of_ignore_case(" LaunchApp"), Unknown);
        assert!(ActionKind::is_spotify_text("SpotifyNext") && !ActionKind::is_spotify_text("spotify"));
    }
}
