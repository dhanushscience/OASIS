//! The simple settings, each described once: its label, where `profiles.json` keeps it, its
//! limits, and its default. The settings screen draws itself from [`sections`] and saves through
//! [`set_field`], so adding a setting is one line here and nothing else, and every value is
//! checked against the same limits the C# window's dials and lists had.
//!
//! Settings that are lists of things (profiles, buttons, sets) are edited by `edit.rs`; this is for
//! the switches, numbers and choices.

use serde::Serialize;
use serde_json::{json, Number, Value};

use crate::edit::EditError;
use crate::model::Settings;
use crate::overlay::css_color;

/// Things the registry cannot know by itself: the microphones this computer has.
#[derive(Debug, Clone, Default)]
pub struct FieldContext {
    pub microphones: Vec<String>,
    /// Web browsers installed on this PC: `(process name, name to show)`.
    pub browsers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChoiceView {
    pub value: Value,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FieldView {
    pub id: String,
    pub label: String,
    pub hint: String,
    /// `toggle`, `number`, `choice` or `color`.
    pub kind: &'static str,
    pub value: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub options: Vec<ChoiceView>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SectionView {
    pub id: String,
    pub title: String,
    pub hint: String,
    pub fields: Vec<FieldView>,
}

#[derive(Clone)]
enum Kind {
    Toggle,
    Number { min: f64, max: f64, step: f64, integer: bool },
    Choice(Vec<(Value, String)>),
    Color,
    /// Lines of `spoken = written`, each side at most `max` characters.
    Glossary { max: usize },
}

/// Where a setting lives.
#[derive(Clone, Copy)]
enum Place {
    /// `ShowOverlayLabels`, which the settings model keeps as a typed field.
    ShowLabels,
    /// `AllowRunScriptActions`, also a typed field.
    AllowScripts,
    /// A key at the top of the file.
    Root(&'static str),
    /// A key inside a section (`Voice.Enabled`).
    In(&'static str, &'static str),
    /// Push-to-talk buttons: `Voice.PushToTalkMask` and `Voice.Mode` together.
    TalkButtons,
    /// A list of `{ Spoken, Canonical }` under this key of `Voice`.
    Glossary(&'static str),
}

struct Spec {
    id: &'static str,
    section: &'static str,
    label: &'static str,
    hint: &'static str,
    place: Place,
    default: Value,
    kind: Kind,
}

const SECTIONS: [(&str, &str, &str); 7] = [
    ("general", "General", "How the OASIS interface and the overlay behave day to day."),
    ("youtube", "YouTube", "Hidden gestures and ad skipping, for the browser extension."),
    ("pad", "Pad gestures", "Gestures that work in every program."),
    ("overlay", "Overlay look", "Colours and sizes of the bars and labels on screen."),
    ("voice", "Voice", "Push-to-talk: talk to type, or give a command."),
    ("meetings", "Meetings", "Reminders and joining from your calendar."),
    ("preferences", "Preferences", "General choices OASIS uses when you do not name one."),
];

/// The colours the overlay can use, as in the C# window.
const THEME_COLORS: [(&str, &str); 6] = [
    ("Black", "#161D1F"),
    ("Grey", "#A0A0A0"),
    ("White", "#FFFFFF"),
    ("Muted", "#6E8090"),
    ("Surface", "#F4FAFD"),
    ("Border", "#D0D8DC"),
];

fn buttons(with_off: bool) -> Vec<(Value, String)> {
    let mut list = Vec::new();
    if with_off {
        list.push((json!(-1), "Off".to_string()));
    }
    list.extend((0..3).map(|i| (json!(i), format!("Button {}", i + 1))));
    list
}

fn number(min: f64, max: f64, step: f64, integer: bool) -> Kind {
    Kind::Number { min, max, step, integer }
}

fn specs(ctx: &FieldContext, settings: &Settings) -> Vec<Spec> {
    let mut list = vec![
        Spec {
            id: "general.show_labels",
            section: "general",
            label: "Show button labels first",
            hint: "The first tap on a button shows what the three buttons do; the second tap runs it. A profile can override this.",
            place: Place::ShowLabels,
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "general.paste_dictation",
            section: "general",
            label: "Paste dictation after transcribing",
            hint: "On: what you say is typed where your cursor is. Off: it is only transcribed and kept in Insights (if history is on), and nothing is pasted.",
            place: Place::In("Voice", "PasteDictation"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "general.paste_tag",
            section: "general",
            label: "Add “[typed using project OASIS]” after pasted dictation",
            hint: "Off by default. When on, that note in square brackets is pasted right after the text you spoke. It is not added to Insights.",
            place: Place::In("Voice", "PasteTag"),
            default: json!(false),
            kind: Kind::Toggle,
        },
        Spec {
            id: "general.allow_scripts",
            section: "general",
            label: "Allow scripts",
            hint: "Lets a button run a PowerShell or AutoHotkey script you wrote. Off by default: a script can do anything a program can. Scripts never run from voice or the browser extension.",
            place: Place::AllowScripts,
            default: json!(false),
            kind: Kind::Toggle,
        },
        Spec {
            id: "general.bridge_enabled",
            section: "general",
            label: "Browser extension link",
            hint: "Lets the OASIS Bridge browser extension tell OASIS what YouTube is doing.",
            place: Place::Root("BrowserBridgeEnabled"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "general.bridge_port",
            section: "general",
            label: "Extension link port",
            hint: "Change only if another program uses 17322. The extension must use the same port.",
            place: Place::Root("BrowserBridgePort"),
            default: json!(17322),
            kind: number(1024.0, 65535.0, 1.0, true),
        },
        Spec {
            id: "youtube.auto_skip_ads",
            section: "youtube",
            label: "Skip ads automatically",
            hint: "The extension clicks Skip when it appears.",
            place: Place::In("YouTube", "AutoSkipAds"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "youtube.gestures",
            section: "youtube",
            label: "Hidden gestures",
            hint: "Hold a button for 2x speed, double-click a button for volume, while a video plays.",
            place: Place::In("YouTube", "GesturesEnabled"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "youtube.hold2x_button",
            section: "youtube",
            label: "Hold for 2x speed",
            hint: "The button to hold.",
            place: Place::In("YouTube", "Hold2xButtonIndex"),
            default: json!(1),
            kind: Kind::Choice(buttons(true)),
        },
        Spec {
            id: "pad.sounds",
            section: "pad",
            label: "Sound",
            hint: "A short sound when you press and hold to talk, and another when the words are pasted.",
            place: Place::In("Voice", "Sounds"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "pad.show_desktop_button",
            section: "pad",
            label: "Hold to show the desktop",
            hint: "Hold this button in any program to show the desktop; hold again to bring the windows back.",
            place: Place::In("PadGestures", "ShowDesktopButtonIndex"),
            default: json!(2),
            kind: Kind::Choice(buttons(true)),
        },
        Spec {
            id: "pad.show_desktop_hold_ms",
            section: "pad",
            label: "How long to hold",
            hint: "",
            place: Place::In("PadGestures", "ShowDesktopHoldMs"),
            default: json!(800),
            kind: Kind::Choice(
                [500, 800, 1000, 1500, 2000]
                    .iter()
                    .map(|ms| (json!(ms), format!("{:.1} s", f64::from(*ms) / 1000.0)))
                    .collect(),
            ),
        },
    ];

    // Where the overlay sits and how it rests. Root keys, so "Reset look" keeps them.
    list.push(Spec {
        id: "overlay.dock",
        section: "overlay",
        label: "Where the overlay sits",
        hint: "Left or right stacks the bars down that edge; top or bottom puts them in a row, centred.",
        place: Place::Root("OverlayDock"),
        default: json!("right"),
        kind: Kind::Choice(vec![
            (json!("right"), "Right edge".to_string()),
            (json!("left"), "Left edge".to_string()),
            (json!("bottom"), "Bottom, centred".to_string()),
            (json!("top"), "Top, centred".to_string()),
        ]),
    });
    list.push(Spec {
        id: "overlay.idle",
        section: "overlay",
        label: "When not in use",
        hint: "",
        place: Place::Root("OverlayIdle"),
        default: json!("faint"),
        kind: Kind::Choice(vec![
            (json!("faint"), "Faint".to_string()),
            (json!("hidden"), "Hidden".to_string()),
            (json!("always"), "Always full".to_string()),
        ]),
    });
    list.push(Spec {
        id: "overlay.popup",
        section: "overlay",
        label: "Voice pop-up",
        hint: "Across the bottom of the screen, or down its side.",
        place: Place::Root(crate::overlay::POPUP),
        default: json!("horizontal"),
        kind: Kind::Choice(vec![
            (json!("horizontal"), "Horizontal".to_string()),
            (json!("vertical"), "Vertical".to_string()),
        ]),
    });

    // The overlay look.
    /// One overlay number: id, key, label, min, max, step, default, whole numbers only.
    type Look = (&'static str, &'static str, &'static str, f64, f64, f64, f64, bool);
    let looks: [Look; 8] = [
        ("overlay.indicator_width", "IndicatorWidth", "Bar width", 4.0, 20.0, 1.0, 8.0, true),
        ("overlay.indicator_height", "IndicatorHeight", "Bar height", 16.0, 64.0, 1.0, 50.0, true),
        ("overlay.indicator_spacing", "IndicatorSpacing", "Space between bars", 2.0, 24.0, 1.0, 14.0, true),
        ("overlay.indicator_opacity", "IndicatorOpacity", "Bar opacity", 0.2, 1.0, 0.05, 1.0, false),
        ("overlay.label_font_size", "LabelFontSize", "Label text size", 12.0, 36.0, 1.0, 14.0, true),
        ("overlay.label_font_opacity", "LabelFontOpacity", "Label text opacity", 0.2, 1.0, 0.05, 1.0, false),
        ("overlay.label_bubble_opacity", "LabelBubbleOpacity", "Label bubble opacity", 0.2, 1.0, 0.05, 1.0, false),
        ("overlay.label_corner_radius", "LabelCornerRadius", "Label roundness", 0.0, 48.0, 1.0, 22.0, true),
    ];
    for (id, key, label, min, max, step, default, integer) in looks {
        list.push(Spec {
            id,
            section: "overlay",
            label,
            hint: "",
            place: Place::In("OverlayAppearance", key),
            default: if integer { json!(default as i64) } else { json!(default) },
            kind: number(min, max, step, integer),
        });
    }
    for (id, key, label, default) in [
        ("overlay.indicator_color_1", "IndicatorColor1", "Top bar colour", "#6E8090"),
        ("overlay.indicator_color_2", "IndicatorColor2", "Middle bar colour", "#D0D8DC"),
        ("overlay.indicator_color_3", "IndicatorColor3", "Bottom bar colour", "#F4FAFD"),
        ("overlay.label_background", "LabelBackgroundColor", "Label background", "#FFFFFF"),
        ("overlay.label_text", "LabelTextColor", "Label text colour", "#161D1F"),
        ("overlay.label_border", "LabelBorderColor", "Label border", "#D0D8DC"),
    ] {
        list.push(Spec {
            id,
            section: "overlay",
            label,
            hint: "",
            place: Place::In("OverlayAppearance", key),
            default: json!(default),
            kind: Kind::Color,
        });
    }

    // Preferences (W14): only browsers found on this PC; Chrome when it is there, else the system's.
    let mut browsers = vec![(json!(""), "System default".to_string())];
    browsers.extend(ctx.browsers.iter().map(|(name, shown)| (json!(name), shown.clone())));
    let has_chrome = ctx.browsers.iter().any(|(name, _)| name == "chrome");
    list.push(Spec {
        id: "preferences.browser",
        section: "preferences",
        label: "Preferred browser",
        hint: "Opens a spoken search, or \"open the browser\", when no browser is open. With one open, the page goes there as a new tab.",
        place: Place::In("Preferences", "Browser"),
        default: json!(if has_chrome { "chrome" } else { "" }),
        kind: Kind::Choice(browsers),
    });

    // Voice.
    let mut mics = vec![(json!(""), "System default".to_string())];
    mics.extend(ctx.microphones.iter().map(|m| (json!(m), m.clone())));
    let saved_mic = settings
        .extra
        .get("Voice")
        .and_then(|v| v.get("MicrophoneDeviceName"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|m| !m.is_empty());
    if let Some(saved) = saved_mic {
        if !ctx.microphones.iter().any(|m| m == saved) {
            mics.push((json!(saved), format!("{saved} (not connected)")));
        }
    }
    list.extend([
        Spec {
            id: "voice.enabled",
            section: "voice",
            label: "Push-to-talk",
            hint: "Hold the talk button, speak, let go. Needs a speech model (see the Status screen).",
            place: Place::In("Voice", "Enabled"),
            default: json!(false),
            kind: Kind::Toggle,
        },
        Spec {
            id: "voice.microphone",
            section: "voice",
            label: "Microphone",
            hint: "",
            place: Place::In("Voice", "MicrophoneDeviceName"),
            default: json!(""),
            kind: Kind::Choice(mics),
        },
        Spec {
            id: "voice.talk_buttons",
            section: "voice",
            label: "Buttons to hold",
            hint: "A short tap still runs the button's normal action.",
            place: Place::TalkButtons,
            default: json!("0"),
            kind: Kind::Choice(talk_presets()),
        },
        Spec {
            id: "voice.hold_ms",
            section: "voice",
            label: "Hold time",
            hint: "How long to hold before listening starts.",
            place: Place::In("Voice", "PushToTalkHoldMs"),
            default: json!(350),
            kind: Kind::Choice([200, 350, 500, 750, 1000].iter().map(|ms| (json!(ms), format!("{ms} ms"))).collect()),
        },
        Spec {
            id: "voice.capture_mode",
            section: "voice",
            label: "What a recording becomes",
            hint: "Auto types when a text field is focused and runs a command otherwise.",
            place: Place::In("Voice", "CaptureMode"),
            default: json!(0),
            kind: Kind::Choice(vec![
                (json!(0), "Auto".to_string()),
                (json!(1), "Always a command".to_string()),
                (json!(2), "Always typed".to_string()),
            ]),
        },
        Spec {
            id: "voice.approval_mode",
            section: "voice",
            label: "Voice commands that ask first",
            hint: "Risky commands always ask. Asking means button 1 runs it and button 2 cancels.",
            place: Place::In("Voice", "IntentApprovalMode"),
            default: json!(0),
            kind: Kind::Choice(vec![
                (json!(0), "A new app or link, the first time".to_string()),
                (json!(1), "Every command".to_string()),
                (json!(2), "Only risky ones".to_string()),
            ]),
        },
        Spec {
            id: "voice.allow_launch_app",
            section: "voice",
            label: "Let voice open apps",
            hint: "",
            place: Place::In("Voice", "AllowLaunchApp"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "voice.allow_type_text",
            section: "voice",
            label: "Let voice type text",
            hint: "For \"type …\" and \"paste …\" commands.",
            place: Place::In("Voice", "AllowTypeText"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "voice.edit_commands",
            section: "voice",
            label: "Editing commands",
            hint: "Said on their own, \"copy\", \"paste\", \"undo\", \"select all\", \"ctrl z\", \"go back\" and the like press their keys, also in a text field.",
            place: Place::In("Voice", "EditCommands"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "voice.dictation_cleanup",
            section: "voice",
            label: "Tidy up what you dictate",
            hint: "Spoken \"comma\" and \"full stop\" become symbols, number words become digits, \"um\" is dropped, \"no actually …\" keeps the correction, capitals are added.",
            place: Place::In("Voice", "DictationCleanup"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "voice.polish",
            section: "voice",
            label: "Polish with the local model",
            hint: "S1-mini tidies each sentence on the graphics card while you talk. Needs the Quality model set; switching it off also frees the card's memory.",
            place: Place::In("Voice", "PolishEnabled"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "voice.polish_tone",
            section: "voice",
            label: "Polish tone",
            hint: "How formal the polished text reads: casual is lower case with little punctuation, formal expands contractions.",
            place: Place::In("Voice", "PolishTone"),
            default: json!("balanced"),
            kind: Kind::Choice(
                [("casual", "Casual"), ("semi-casual", "Semi-casual"), ("balanced", "Balanced"), ("semi-formal", "Semi-formal"), ("formal", "Formal")]
                    .iter()
                    .map(|(v, l)| (json!(v), l.to_string()))
                    .collect(),
            ),
        },
        // For testing (V54); may go in the next update.
        Spec {
            id: "voice.polish_structure",
            section: "voice",
            label: "Polish structure",
            hint: "Prose keeps sentences; lists lets the model turn three or more items into bullets.",
            place: Place::In("Voice", "PolishStructure"),
            default: json!("lists"),
            kind: Kind::Choice([("prose", "Prose"), ("lists", "Lists")].iter().map(|(v, l)| (json!(v), l.to_string())).collect()),
        },
        Spec {
            id: "voice.polish_context",
            section: "voice",
            label: "Polish context",
            hint: "Email adds a greeting and a sign-off. Automatic uses email in mail programs and general everywhere else.",
            place: Place::In("Voice", "PolishContext"),
            default: json!("auto"),
            kind: Kind::Choice(
                [("auto", "Automatic"), ("general", "General"), ("email", "Email")].iter().map(|(v, l)| (json!(v), l.to_string())).collect(),
            ),
        },
        Spec {
            id: "voice.polish_wait_ms",
            section: "voice",
            label: "Polish wait limit",
            hint: "The longest the polish may add after you let go, while plugged in. On battery it waits 1.5 times as long, twice with Battery Saver on. Text not polished in time is typed as the rules wrote it.",
            place: Place::In("Voice", "PolishWaitMs"),
            default: json!(3000),
            kind: Kind::Choice([600, 800, 1000, 1500, 2000, 3000].iter().map(|ms| (json!(ms), format!("{ms} ms"))).collect()),
        },
        Spec {
            id: "voice.dictation_glossary",
            section: "voice",
            label: "Your own spellings",
            hint: "One per line, as: how it sounds = how to write it. For example: kay cad = KiCad",
            place: Place::Glossary("DictationGlossary"),
            default: json!(""),
            kind: Kind::Glossary { max: MAX_GLOSSARY_TEXT },
        },
        Spec {
            id: "voice.replacements",
            section: "voice",
            label: "Fragments",
            hint: "One per line, as: what you say = what to type. For example: my mail = you@example.com",
            place: Place::Glossary("Replacements"),
            default: json!(""),
            kind: Kind::Glossary { max: MAX_REPLACEMENT_TEXT },
        },
        Spec {
            id: "voice.keep_history",
            section: "voice",
            label: "Keep my dictation history",
            hint: "Saves each dictation, the recording and its text, in a folder on this PC for the Insights screen. Nothing is sent anywhere; you decide whether to share the folder.",
            place: Place::In("Voice", "KeepHistory"),
            default: json!(false),
            kind: Kind::Toggle,
        },
        Spec {
            id: "voice.keep_trace",
            section: "voice",
            label: "Keep a dictation trace (for troubleshooting)",
            hint: "Writes what you said, before and after cleanup, to a log file on this PC. Turn it off when you are done; nothing is sent anywhere.",
            place: Place::In("Voice", "KeepDictationTrace"),
            default: json!(false),
            kind: Kind::Toggle,
        },
        Spec {
            id: "meetings.reminders",
            section: "meetings",
            label: "Remind me before a meeting",
            hint: "A reminder with Join, Snooze and Dismiss appears in the OASIS window. Needs your calendar connected (below).",
            place: Place::In("Meetings", "RemindersEnabled"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "meetings.remind_minutes",
            section: "meetings",
            label: "Remind me this many minutes before",
            hint: "",
            place: Place::In("Meetings", "ReminderMinutesBefore"),
            default: json!(5),
            kind: number(1.0, 60.0, 1.0, true),
        },
        Spec {
            id: "meetings.join_minutes",
            section: "meetings",
            label: "Allow joining this many minutes before the start",
            hint: "The join button and \"join my meeting\" open a meeting from this long before it starts until it ends.",
            place: Place::In("Meetings", "JoinWindowMinutesBefore"),
            default: json!(15),
            kind: number(1.0, 120.0, 1.0, true),
        },
        Spec {
            id: "voice.stt_provider",
            section: "voice",
            label: "Where speech is turned into text",
            hint: "On this computer keeps everything private. Groq sends each recording to api.groq.com and needs an API key (below); it is faster on slow computers.",
            place: Place::In("Voice", "SttProvider"),
            default: json!(2),
            kind: Kind::Choice(vec![
                (json!(2), "On this computer (private)".to_string()),
                (json!(1), "Groq online (sends your speech to Groq)".to_string()),
            ]),
        },
        Spec {
            id: "voice.groq_fallback",
            section: "voice",
            label: "If Groq fails, use this computer",
            hint: "Offline, a wrong key or a rate limit then still gives you text. Needs the speech model installed.",
            place: Place::In("Voice", "GroqFallbackToLocal"),
            default: json!(true),
            kind: Kind::Toggle,
        },
        Spec {
            id: "voice.match_threshold",
            section: "voice",
            label: "How exact a command must be",
            hint: "Higher refuses more guesses.",
            place: Place::In("Voice", "MatchThreshold"),
            default: json!(0.72),
            kind: number(0.5, 0.95, 0.01, false),
        },
    ]);
    list
}

/// Button combinations for push-to-talk, as a list of 0-based buttons.
const TALK_PRESETS: [(&str, &[i64]); 6] = [
    ("Button 1", &[0]),
    ("Button 2", &[1]),
    ("Button 3", &[2]),
    ("Buttons 1 and 3 together", &[0, 2]),
    ("Buttons 1 and 2 together", &[0, 1]),
    ("Buttons 2 and 3 together", &[1, 2]),
];

fn mask_key(mask: &[i64]) -> String {
    mask.iter().map(i64::to_string).collect::<Vec<_>>().join("+")
}

fn talk_presets() -> Vec<(Value, String)> {
    TALK_PRESETS.iter().map(|(label, mask)| (json!(mask_key(mask)), (*label).to_string())).collect()
}

fn read_mask(settings: &Settings) -> Vec<i64> {
    settings
        .extra
        .get("Voice")
        .and_then(|v| v.get("PushToTalkMask"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_else(|| vec![0])
}

/// A stored number as plain JSON: whole numbers are integers, so 350.0 equals 350.
fn tidy(value: &Value) -> Value {
    match value.as_f64() {
        Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => json!(f as i64),
        Some(f) => Number::from_f64((f * 1000.0).round() / 1000.0).map_or(value.clone(), Value::Number),
        None => value.clone(),
    }
}

fn stored(settings: &Settings, place: Place) -> Option<&Value> {
    match place {
        Place::Root(key) => settings.extra.get(key),
        Place::In(section, key) => settings.extra.get(section)?.get(key),
        Place::ShowLabels | Place::AllowScripts | Place::TalkButtons | Place::Glossary(_) => None,
    }
}

fn current(settings: &Settings, spec: &Spec) -> Value {
    match spec.place {
        Place::ShowLabels => json!(settings.show_overlay_labels),
        Place::AllowScripts => json!(settings.allow_run_script_actions),
        Place::TalkButtons => json!(mask_key(&read_mask(settings))),
        Place::Glossary(key) => json!(glossary_text(settings, key)),
        place => stored(settings, place).filter(|v| !v.is_null()).map_or_else(|| spec.default.clone(), tidy),
    }
}

/// The screen: every section and field with its current value.
pub fn sections(settings: &Settings, ctx: &FieldContext) -> Vec<SectionView> {
    let specs = specs(ctx, settings);
    SECTIONS
        .iter()
        .map(|(id, title, hint)| SectionView {
            id: (*id).to_string(),
            title: (*title).to_string(),
            hint: (*hint).to_string(),
            fields: specs.iter().filter(|s| s.section == *id).map(|s| view(settings, s)).collect(),
        })
        .collect()
}

fn view(settings: &Settings, spec: &Spec) -> FieldView {
    let value = current(settings, spec);
    let mut field = FieldView {
        id: spec.id.to_string(),
        label: spec.label.to_string(),
        hint: spec.hint.to_string(),
        kind: "toggle",
        value: value.clone(),
        min: None,
        max: None,
        step: None,
        options: Vec::new(),
    };
    match &spec.kind {
        Kind::Toggle => {}
        Kind::Number { min, max, step, .. } => {
            field.kind = "number";
            (field.min, field.max, field.step) = (Some(*min), Some(*max), Some(*step));
        }
        Kind::Choice(options) => {
            field.kind = "choice";
            field.options = options.iter().map(|(v, l)| ChoiceView { value: v.clone(), label: l.clone() }).collect();
            // A value saved by hand or by the C# app that is not in the list stays visible.
            if !field.options.iter().any(|o| o.value == value) {
                let label = if matches!(spec.place, Place::TalkButtons) {
                    format!("Custom ({})", mask_key(&read_mask(settings)))
                } else {
                    format!("Custom ({})", value.to_string().trim_matches('"'))
                };
                field.options.push(ChoiceView { value, label });
            }
        }
        Kind::Glossary { .. } => field.kind = "glossary",
        Kind::Color => {
            field.kind = "color";
            field.options = THEME_COLORS.iter().map(|(l, hex)| ChoiceView { value: json!(hex), label: (*l).to_string() }).collect();
            let held = value.as_str().map(|s| css_color(s, "#161D1F")).unwrap_or_default();
            match field.options.iter().find(|o| o.value.as_str().is_some_and(|h| h.eq_ignore_ascii_case(&held))) {
                Some(found) => field.value = found.value.clone(),
                None => field.options.push(ChoiceView { value: json!(held), label: format!("Custom ({held})") }),
            }
        }
    }
    field
}

fn bad<T>(why: impl Into<String>) -> Result<T, EditError> {
    Err(EditError::Invalid(why.into()))
}

fn put(settings: &mut Settings, place: Place, value: Value) {
    match place {
        Place::Root(key) => {
            settings.extra.insert(key.to_string(), value);
        }
        Place::In(section, key) => {
            let entry = settings.extra.entry(section.to_string()).or_insert_with(|| Value::Object(Default::default()));
            if !entry.is_object() {
                *entry = Value::Object(Default::default());
            }
            if let Some(map) = entry.as_object_mut() {
                map.insert(key.to_string(), value);
            }
        }
        Place::ShowLabels | Place::AllowScripts | Place::TalkButtons | Place::Glossary(_) => {}
    }
}

/// Change one setting. Checks the value against the limits of that setting first, and refuses
/// anything else with a sentence for the person editing; nothing changes on a refusal.
pub fn set_field(settings: &mut Settings, id: &str, value: Value, ctx: &FieldContext) -> Result<(), EditError> {
    let specs = specs(ctx, settings);
    let Some(spec) = specs.iter().find(|s| s.id == id) else { return bad(format!("There is no setting called \"{id}\".")) };

    let checked = match &spec.kind {
        Kind::Toggle => match value.as_bool() {
            Some(b) => json!(b),
            None => return bad(format!("{} must be on or off.", spec.label)),
        },
        Kind::Number { min, max, integer, .. } => {
            let Some(n) = value.as_f64().filter(|n| n.is_finite()) else {
                return bad(format!("{} must be a number.", spec.label));
            };
            if n < *min || n > *max {
                return bad(format!("{} must be between {} and {}.", spec.label, min, max));
            }
            if *integer { json!(n.round() as i64) } else { json!((n * 100.0).round() / 100.0) }
        }
        Kind::Choice(options) => {
            let wanted = tidy(&value);
            if !options.iter().any(|(v, _)| *v == wanted) {
                return bad(format!("That is not one of the choices for {}.", spec.label));
            }
            wanted
        }
        Kind::Color => match value.as_str() {
            Some(hex) if THEME_COLORS.iter().any(|(_, c)| c.eq_ignore_ascii_case(hex)) => json!(hex.to_ascii_uppercase()),
            _ => return bad(format!("{} must be one of the listed colours.", spec.label)),
        },
        Kind::Glossary { max } => match value.as_str() {
            Some(text) => glossary_rows(text, spec.label, *max)?,
            None => return bad(format!("{} must be text.", spec.label)),
        },
    };

    match spec.place {
        Place::ShowLabels => settings.show_overlay_labels = checked.as_bool().unwrap_or(true),
        Place::AllowScripts => settings.allow_run_script_actions = checked.as_bool().unwrap_or(false),
        Place::TalkButtons => {
            let key = checked.as_str().unwrap_or("0").to_string();
            let Some((_, mask)) = TALK_PRESETS.iter().find(|(_, m)| mask_key(m) == key) else {
                return bad("That is not one of the button choices.");
            };
            put(settings, Place::In("Voice", "PushToTalkMask"), json!(mask));
            // 0 is a single button held, 1 a chord of several.
            put(settings, Place::In("Voice", "Mode"), json!(i64::from(mask.len() > 1)));
        }
        Place::Glossary(key) => put(settings, Place::In("Voice", key), checked),
        place => put(settings, place, checked),
    }
    // Choosing where it sits (even the same place again) drops a spot it was dragged to.
    if id == "overlay.dock" {
        settings.overlay_x = (-1).into();
        settings.overlay_y = (-1).into();
    }
    Ok(())
}

const MAX_GLOSSARY_ROWS: usize = 200;
const MAX_GLOSSARY_TEXT: usize = 60;
/// A replacement can be a whole address or signature line.
const MAX_REPLACEMENT_TEXT: usize = 200;

/// The saved glossary as editable lines: `spoken = written`.
fn glossary_text(settings: &Settings, key: &str) -> String {
    let rows = settings.extra.get("Voice").and_then(|v| v.get(key)).and_then(Value::as_array);
    let text = |row: &Value, key: &str| row.get(key).and_then(Value::as_str).map(str::trim).unwrap_or("").to_string();
    rows.into_iter()
        .flatten()
        .map(|r| (text(r, "Spoken"), text(r, "Canonical")))
        .filter(|(s, c)| !s.is_empty() && !c.is_empty())
        .map(|(s, c)| format!("{s} = {c}"))
        .collect::<Vec<_>>()
        .join("
")
}

/// Add one spelling to the dictionary, in place of an older one for the same spoken words.
pub fn add_glossary_word(settings: &mut Settings, spoken: &str, written: &str) -> Result<(), EditError> {
    let (spoken, written) = (spoken.trim(), written.trim());
    if spoken.contains('=') || written.contains('=') {
        return bad("A dictionary word cannot hold an = sign.");
    }
    let same = |line: &str| line.split_once('=').is_some_and(|(s, _)| s.trim().eq_ignore_ascii_case(spoken));
    let mut lines: Vec<String> = glossary_text(settings, "DictationGlossary").lines().filter(|l| !same(l)).map(String::from).collect();
    lines.push(format!("{spoken} = {written}"));
    set_field(settings, "voice.dictation_glossary", json!(lines.join("\n")), &FieldContext::default())
}

/// The lines typed in the box as the list to save. Blank lines are skipped; any other line must be
/// `spoken = written`, short, and the whole list not long. Nothing is saved if a line is refused.
fn glossary_rows(text: &str, label: &str, max: usize) -> Result<Value, EditError> {
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let Some((spoken, written)) = line.split_once('=').map(|(s, w)| (s.trim(), w.trim())) else {
            return bad(format!("{label}, line {}: write it as  how it sounds = how to write it.", n + 1));
        };
        if spoken.is_empty() || written.is_empty() {
            return bad(format!("{label}, line {}: both sides of the = need words.", n + 1));
        }
        if spoken.chars().count() > max || written.chars().count() > max {
            return bad(format!("{label}, line {}: keep each side under {max} characters.", n + 1));
        }
        rows.push(json!({ "Spoken": spoken, "Canonical": written }));
    }
    if rows.len() > MAX_GLOSSARY_ROWS {
        return bad(format!("{label}: at most {MAX_GLOSSARY_ROWS} spellings."));
    }
    Ok(Value::Array(rows))
}

/// Put the overlay's colours and sizes back to the defaults.
pub fn reset_overlay_look(settings: &mut Settings) {
    settings.extra.remove("OverlayAppearance");
}

/// Things worth saying about the current choices, without refusing them.
pub fn warnings(settings: &Settings) -> Vec<String> {
    let ctx = FieldContext::default();
    let value = |id: &str| -> Option<i64> {
        let specs = specs(&ctx, settings);
        let spec = specs.iter().find(|s| s.id == id)?;
        current(settings, spec).as_i64().filter(|i| *i >= 0)
    };
    let hold = value("youtube.hold2x_button");
    let mut notes = Vec::new();
    if let (Some(desktop), Some(hold)) = (value("pad.show_desktop_button"), hold) {
        if desktop == hold {
            notes.push("The show-desktop button is also the 2x-speed button: in a video it holds for 2x, elsewhere it shows the desktop.".to_string());
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real_like() -> Settings {
        serde_json::from_value(json!({
            "ShowOverlayLabels": true, "BrowserBridgeEnabled": true, "BrowserBridgePort": 17322,
            "YouTube": { "AutoSkipAds": true, "GesturesEnabled": true, "Hold2xButtonIndex": 2,
                         "VolumeUpButtonIndex": 1, "VolumeDownButtonIndex": 0 },
            "PadGestures": { "ShowDesktopButtonIndex": 2, "ShowDesktopHoldMs": 1000 },
            "OverlayAppearance": { "IndicatorHeight": 50, "LabelFontSize": 14, "IndicatorColor1": "#6E8090", "LabelTextColor": "#161d1f" },
            "Voice": { "Enabled": true, "PushToTalkMask": [0], "PushToTalkHoldMs": 350, "MicrophoneDeviceName": "Microphone (Logi C270 HD WebCam",
                       "IntentApprovalMode": 2, "MatchThreshold": 0.72 }
        }))
        .unwrap()
    }

    fn field(settings: &Settings, id: &str) -> FieldView {
        let ctx = FieldContext { microphones: vec!["Microphone (Realtek(R) Audio)".into()], ..Default::default() };
        sections(settings, &ctx).into_iter().flat_map(|s| s.fields).find(|f| f.id == id).expect(id)
    }

    fn set(settings: &mut Settings, id: &str, value: Value) -> Result<(), EditError> {
        set_field(settings, id, value, &FieldContext::default())
    }

    #[test]
    fn every_section_has_fields_and_every_field_a_unique_id() {
        let view = sections(&Settings::default(), &FieldContext::default());
        assert_eq!(view.len(), 7);
        let mut ids: Vec<&str> = view.iter().flat_map(|s| s.fields.iter().map(|f| f.id.as_str())).collect();
        assert!(view.iter().all(|s| !s.fields.is_empty()), "{view:?}");
        let before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate ids");
    }

    #[test]
    fn only_installed_browsers_are_offered_and_chrome_is_the_default_when_there() {
        let browser = |ctx: &FieldContext| sections(&Settings::default(), ctx).into_iter().flat_map(|s| s.fields).find(|f| f.id == "preferences.browser").unwrap();
        let both = FieldContext { browsers: vec![("chrome".into(), "Google Chrome".into()), ("firefox".into(), "Firefox".into())], ..Default::default() };
        let view = serde_json::to_value(browser(&both)).unwrap();
        assert_eq!(view["value"], json!("chrome"), "{view}");
        assert_eq!(view["options"].as_array().map(Vec::len), Some(3), "system default + the two found: {view}");
        let no_chrome = FieldContext { browsers: vec![("firefox".into(), "Firefox".into())], ..Default::default() };
        assert_eq!(serde_json::to_value(browser(&no_chrome)).unwrap()["value"], json!(""), "the system's own browser");
    }

    #[test]
    fn a_blank_settings_file_shows_the_csharp_defaults() {
        let s = Settings::default();
        assert_eq!(field(&s, "youtube.hold2x_button").value, json!(1));
        assert_eq!(field(&s, "voice.enabled").value, json!(false));
        assert_eq!(field(&s, "overlay.indicator_height").value, json!(50));
        assert_eq!(field(&s, "general.show_labels").value, json!(true));
    }

    #[test]
    fn the_owners_real_values_are_shown() {
        let s = real_like();
        assert_eq!(field(&s, "youtube.hold2x_button").value, json!(2));
        assert_eq!(field(&s, "pad.show_desktop_hold_ms").value, json!(1000));
        assert_eq!(field(&s, "voice.approval_mode").value, json!(2));
        assert_eq!(field(&s, "voice.talk_buttons").value, json!("0"));
        assert_eq!(field(&s, "overlay.label_text").value, json!("#161D1F"), "matched without regard to case");
    }

    #[test]
    fn a_microphone_that_is_not_plugged_in_stays_visible() {
        let mic = field(&real_like(), "voice.microphone");
        assert_eq!(mic.value, json!("Microphone (Logi C270 HD WebCam"));
        assert!(mic.options.iter().any(|o| o.label.ends_with("(not connected)")), "{:?}", mic.options);
        assert!(mic.options.iter().any(|o| o.label == "System default"));
        assert!(mic.options.iter().any(|o| o.label == "Microphone (Realtek(R) Audio)"));
    }

    #[test]
    fn switches_and_choices_are_saved_where_the_csharp_app_keeps_them() {
        let mut s = real_like();
        set(&mut s, "youtube.gestures", json!(false)).unwrap();
        set(&mut s, "youtube.hold2x_button", json!(1)).unwrap();
        set(&mut s, "general.bridge_enabled", json!(false)).unwrap();
        set(&mut s, "voice.capture_mode", json!(2)).unwrap();
        assert_eq!(s.extra["YouTube"]["GesturesEnabled"], json!(false));
        assert_eq!(s.extra["YouTube"]["Hold2xButtonIndex"], json!(1));
        assert_eq!(s.extra["YouTube"]["AutoSkipAds"], json!(true), "the rest of the section is untouched");
        assert_eq!(s.extra["BrowserBridgeEnabled"], json!(false));
        assert_eq!(s.extra["Voice"]["CaptureMode"], json!(2));
    }

    #[test]
    fn show_labels_is_the_typed_field() {
        let mut s = Settings::default();
        set(&mut s, "general.show_labels", json!(false)).unwrap();
        assert!(!s.show_overlay_labels);
    }

    #[test]
    fn scripts_are_off_until_switched_on() {
        let mut s = Settings::default();
        assert_eq!(field(&s, "general.allow_scripts").value, json!(false));
        set(&mut s, "general.allow_scripts", json!(true)).unwrap();
        assert!(s.allow_run_script_actions);
    }

    #[test]
    fn numbers_are_held_to_the_limits_of_the_c_sharp_dials() {
        let mut s = Settings::default();
        assert!(set(&mut s, "overlay.indicator_width", json!(12)).is_ok());
        assert_eq!(s.extra["OverlayAppearance"]["IndicatorWidth"], json!(12));
        for (id, bad) in [("overlay.indicator_width", 3), ("overlay.indicator_width", 21), ("overlay.label_font_size", 11), ("general.bridge_port", 80), ("general.bridge_port", 70000)] {
            assert!(matches!(set(&mut s, id, json!(bad)), Err(EditError::Invalid(_))), "{id} {bad}");
        }
        assert!(set(&mut s, "overlay.indicator_opacity", json!(0.1)).is_err(), "never nearly invisible");
        assert!(set(&mut s, "overlay.indicator_opacity", json!("high")).is_err());
        assert!(set(&mut s, "overlay.indicator_opacity", json!(0.55)).is_ok());
    }

    #[test]
    fn a_refused_change_changes_nothing() {
        let mut s = real_like();
        let before = s.clone();
        assert!(set(&mut s, "youtube.hold2x_button", json!(7)).is_err());
        assert!(set(&mut s, "youtube.gestures", json!("yes")).is_err());
        assert!(set(&mut s, "no.such.setting", json!(1)).is_err());
        assert!(set(&mut s, "overlay.label_text", json!("red; background:url(x)")).is_err());
        assert_eq!(s, before);
    }

    #[test]
    fn colours_must_be_from_the_list() {
        let mut s = Settings::default();
        assert!(set(&mut s, "overlay.label_text", json!("#ffffff")).is_ok());
        assert_eq!(s.extra["OverlayAppearance"]["LabelTextColor"], json!("#FFFFFF"));
        assert!(set(&mut s, "overlay.label_text", json!("#123456")).is_err());
    }

    #[test]
    fn a_colour_set_by_hand_is_still_shown() {
        let s: Settings = serde_json::from_value(json!({ "OverlayAppearance": { "LabelBorderColor": "#80123456" } })).unwrap();
        let f = field(&s, "overlay.label_border");
        assert!(f.options.iter().any(|o| o.label.starts_with("Custom")), "{:?}", f.options);
    }

    #[test]
    fn the_talk_buttons_set_the_mask_and_the_mode_together() {
        let mut s = real_like();
        set(&mut s, "voice.talk_buttons", json!("0+2")).unwrap();
        assert_eq!(s.extra["Voice"]["PushToTalkMask"], json!([0, 2]));
        assert_eq!(s.extra["Voice"]["Mode"], json!(1));
        set(&mut s, "voice.talk_buttons", json!("1")).unwrap();
        assert_eq!(s.extra["Voice"]["PushToTalkMask"], json!([1]));
        assert_eq!(s.extra["Voice"]["Mode"], json!(0));
        assert!(set(&mut s, "voice.talk_buttons", json!("0+1+2")).is_err());
    }

    #[test]
    fn a_hand_made_talk_combination_stays_selectable() {
        let s: Settings = serde_json::from_value(json!({ "Voice": { "PushToTalkMask": [0, 1, 2] } })).unwrap();
        let f = field(&s, "voice.talk_buttons");
        assert_eq!(f.value, json!("0+1+2"));
        assert!(f.options.iter().any(|o| o.label == "Custom (0+1+2)"));
    }

    #[test]
    fn dictation_cleanup_and_the_glossary_are_edited_here_and_read_by_the_voice_settings() {
        let mut s = Settings::default();
        assert_eq!(field(&s, "voice.dictation_cleanup").value, json!(true));
        set(&mut s, "voice.dictation_cleanup", json!(false)).unwrap();
        set(&mut s, "voice.dictation_glossary", json!("kay cad = KiCad

  my name = Dhanush ")).unwrap();
        let voice = crate::voice::VoiceSettings::from(&s);
        assert!(!voice.dictation_cleanup);
        assert_eq!(
            voice.dictation_glossary,
            vec![("kay cad".to_string(), "KiCad".to_string()), ("my name".to_string(), "Dhanush".to_string())]
        );
        assert_eq!(field(&s, "voice.dictation_glossary").value, json!("kay cad = KiCad
my name = Dhanush"));
        assert_eq!(field(&s, "voice.dictation_glossary").kind, "glossary");
    }

    #[test]
    fn the_editing_commands_switch_is_on_by_default_and_read_by_the_router() {
        let mut s = Settings::default();
        let f = field(&s, "voice.edit_commands");
        assert_eq!((f.value, f.kind), (json!(true), "toggle"));
        set(&mut s, "voice.edit_commands", json!(false)).unwrap();
        assert_eq!(s.extra["Voice"]["EditCommands"], json!(false));
        assert!(!crate::voice_commands::router_settings(&s).edit_commands);
    }

    #[test]
    fn a_word_fixed_by_hand_joins_the_glossary_in_place_of_an_older_one() {
        let mut s = Settings::default();
        set(&mut s, "voice.dictation_glossary", json!("keycat = Kicad\nfoo = Foo")).unwrap();
        add_glossary_word(&mut s, "Keycat", "KiCad").unwrap();
        assert_eq!(field(&s, "voice.dictation_glossary").value, json!("foo = Foo\nKeycat = KiCad"));
        assert!(add_glossary_word(&mut s, "a=b", "c").is_err());
    }

    #[test]
    fn a_bad_glossary_line_changes_nothing() {
        let mut s = Settings::default();
        set(&mut s, "voice.dictation_glossary", json!("a = b")).unwrap();
        for bad in ["no equals sign", "= written", "spoken =", &format!("{} = x", "y".repeat(61))] {
            assert!(set(&mut s, "voice.dictation_glossary", json!(bad)).is_err(), "{bad}");
        }
        assert!(set(&mut s, "voice.dictation_glossary", json!(5)).is_err());
        assert_eq!(field(&s, "voice.dictation_glossary").value, json!("a = b"));
    }

    #[test]
    fn the_overlay_can_be_reset_to_the_defaults() {
        let mut s = real_like();
        reset_overlay_look(&mut s);
        assert!(!s.extra.contains_key("OverlayAppearance"));
        assert_eq!(field(&s, "overlay.indicator_height").value, json!(50));
    }

    #[test]
    fn the_overlay_module_reads_what_this_screen_writes() {
        let mut s = Settings::default();
        set(&mut s, "overlay.indicator_width", json!(11)).unwrap();
        set(&mut s, "overlay.indicator_color_1", json!("#a0a0a0")).unwrap();
        let look = crate::overlay::overlay_appearance(&s);
        assert_eq!(look.indicator_width, 11.0);
        assert_eq!(look.indicator_colors[0], "#A0A0A0");
    }

    #[test]
    fn clashing_buttons_get_a_note_but_are_allowed() {
        let mut s = real_like();
        assert!(warnings(&s).iter().any(|w| w.contains("show-desktop")), "your layout shares button 3: {:?}", warnings(&s));
        set(&mut s, "youtube.hold2x_button", json!(0)).unwrap();
        assert!(warnings(&s).is_empty(), "no clash once they differ: {:?}", warnings(&s));
    }
}
