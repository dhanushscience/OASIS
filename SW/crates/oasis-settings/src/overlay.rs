//! The look and position of the on-screen overlay, read from `profiles.json`
//! (`OverlayAppearance`, `OverlayX`, `OverlayY`). Port of `OverlayAppearanceSettings` and the
//! saved-position rule of `OverlayWindow`.
//!
//! The values go into a web page, so each is checked: a colour must be a hex colour (WPF's
//! `#AARRGGBB` is converted to CSS's `#RRGGBBAA`), a size must be in a sensible range. A damaged
//! value falls back to the C# default instead of reaching the page.

use serde::Serialize;
use serde_json::Value;

use crate::model::Settings;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverlayAppearance {
    /// Colours of the three indicator pills, top to bottom.
    pub indicator_colors: [String; 3],
    pub indicator_width: f64,
    pub indicator_height: f64,
    pub indicator_spacing: f64,
    pub indicator_opacity: f64,
    pub label_background: String,
    pub label_text: String,
    pub label_border: String,
    pub label_font_size: f64,
    pub label_font_opacity: f64,
    pub label_bubble_opacity: f64,
    pub label_spacing: f64,
    pub label_corner_radius: f64,
    /// Where the overlay sits: `right` or `left` (bars stacked down the edge), `bottom` or `top`
    /// (bars in a row, centred). Kept as `OverlayDock` beside the look, so resetting the look keeps it.
    pub dock: String,
    /// When nothing is happening: `faint` (small and light, as in the C# app), `hidden`, or `always`.
    pub idle: String,
    /// The user dragged it somewhere; the dock then only decides how it is laid out.
    pub moved: bool,
}

pub const DOCKS: [&str; 4] = ["right", "left", "bottom", "top"];
pub const IDLE_MODES: [&str; 3] = ["faint", "hidden", "always"];

impl Default for OverlayAppearance {
    fn default() -> OverlayAppearance {
        OverlayAppearance {
            indicator_colors: ["#6E8090".into(), "#D0D8DC".into(), "#F4FAFD".into()],
            indicator_width: 8.0,
            indicator_height: 50.0,
            indicator_spacing: 14.0,
            indicator_opacity: 1.0,
            label_background: "#FFFFFF".into(),
            label_text: "#161D1F".into(),
            label_border: "#D0D8DC".into(),
            label_font_size: 14.0,
            label_font_opacity: 1.0,
            label_bubble_opacity: 1.0,
            label_spacing: 8.0,
            label_corner_radius: 22.0,
            dock: "right".into(),
            idle: "faint".into(),
            moved: false,
        }
    }
}

/// A colour a page can use, or `default`. Accepts `#RGB`, `#RRGGBB` and WPF's `#AARRGGBB`.
pub fn css_color(raw: &str, default: &str) -> String {
    let hex = raw.trim().strip_prefix('#').filter(|h| h.chars().all(|c| c.is_ascii_hexdigit()));
    match hex.map(|h| (h.len(), h)) {
        Some((3, h)) | Some((6, h)) => format!("#{}", h.to_ascii_uppercase()),
        // WPF writes alpha first; CSS wants it last.
        Some((8, h)) => format!("#{}{}", &h[2..], &h[..2]).to_ascii_uppercase(),
        _ => default.to_string(),
    }
}

fn number(section: Option<&Value>, key: &str, default: f64, min: f64, max: f64) -> f64 {
    section
        .and_then(|s| s.get(key))
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .map_or(default, |v| v.clamp(min, max))
}

fn color(section: Option<&Value>, key: &str, default: &str) -> String {
    section
        .and_then(|s| s.get(key))
        .and_then(Value::as_str)
        .map_or_else(|| default.to_string(), |raw| css_color(raw, default))
}

/// The overlay's appearance, with C# defaults for anything missing or out of range.
///
/// `LabelOpacity` is the pre-split single opacity; the migration copies it into the two new
/// ones, so only they are read.
pub fn overlay_appearance(settings: &Settings) -> OverlayAppearance {
    let d = OverlayAppearance::default();
    let s = settings.extra.get("OverlayAppearance").filter(|v| v.is_object());
    OverlayAppearance {
        indicator_colors: [
            color(s, "IndicatorColor1", &d.indicator_colors[0]),
            color(s, "IndicatorColor2", &d.indicator_colors[1]),
            color(s, "IndicatorColor3", &d.indicator_colors[2]),
        ],
        indicator_width: number(s, "IndicatorWidth", d.indicator_width, 2.0, 80.0),
        indicator_height: number(s, "IndicatorHeight", d.indicator_height, 10.0, 200.0),
        indicator_spacing: number(s, "IndicatorSpacing", d.indicator_spacing, 0.0, 80.0),
        indicator_opacity: number(s, "IndicatorOpacity", d.indicator_opacity, 0.05, 1.0),
        label_background: color(s, "LabelBackgroundColor", &d.label_background),
        label_text: color(s, "LabelTextColor", &d.label_text),
        label_border: color(s, "LabelBorderColor", &d.label_border),
        label_font_size: number(s, "LabelFontSize", d.label_font_size, 8.0, 40.0),
        label_font_opacity: number(s, "LabelFontOpacity", d.label_font_opacity, 0.05, 1.0),
        label_bubble_opacity: number(s, "LabelBubbleOpacity", d.label_bubble_opacity, 0.05, 1.0),
        label_spacing: number(s, "LabelSpacing", d.label_spacing, 0.0, 80.0),
        label_corner_radius: number(s, "LabelCornerRadius", d.label_corner_radius, 0.0, 60.0),
        dock: one_of(settings, "OverlayDock", &DOCKS),
        idle: one_of(settings, "OverlayIdle", &IDLE_MODES),
        moved: overlay_position(settings).is_some(),
    }
}

/// The voice pop-up (the HUD) runs down the side (`vertical`) or across the bottom. A root key of its
/// own: absent (every file from before the choice existed) is horizontal, as it always was.
pub const POPUP: &str = "VoicePopup";

pub fn popup_vertical(settings: &Settings) -> bool {
    settings.extra.get(POPUP).and_then(Value::as_str).is_some_and(|v| v.trim().eq_ignore_ascii_case("vertical"))
}

/// A word from a short list, or the list's first (the default).
fn one_of(settings: &Settings, key: &str, allowed: &[&str]) -> String {
    let saved = settings.extra.get(key).and_then(Value::as_str).map(str::trim).unwrap_or("");
    allowed.iter().find(|a| a.eq_ignore_ascii_case(saved)).unwrap_or(&allowed[0]).to_string()
}

/// Where the user put the overlay, in screen units, or `None` for "never moved" (the overlay then
/// sits at the right edge, vertically centred). `-1` is the stored "not set".
pub fn overlay_position(settings: &Settings) -> Option<(f64, f64)> {
    let (x, y) = (settings.overlay_x.as_f64()?, settings.overlay_y.as_f64()?);
    (x >= 0.0 && y >= 0.0).then_some((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn with(appearance: Value) -> Settings {
        let mut s = Settings::default();
        s.extra.insert("OverlayAppearance".into(), appearance);
        s
    }

    #[test]
    fn missing_settings_give_the_csharp_defaults() {
        let a = overlay_appearance(&Settings::default());
        assert_eq!(a, OverlayAppearance::default());
        assert_eq!((a.indicator_width, a.indicator_height, a.indicator_spacing), (8.0, 50.0, 14.0));
        assert_eq!(a.label_corner_radius, 22.0);
    }

    #[test]
    fn the_owners_real_appearance_is_read() {
        let a = overlay_appearance(&with(json!({
            "IndicatorColor1": "#6E8090", "IndicatorColor2": "#D0D8DC", "IndicatorColor3": "#F4FAFD",
            "IndicatorWidth": 8, "IndicatorHeight": 50, "IndicatorSpacing": 14, "IndicatorOpacity": 1,
            "LabelBackgroundColor": "#FFFFFF", "LabelTextColor": "#161D1F", "LabelBorderColor": "#D0D8DC",
            "LabelFontSize": 14, "LabelFontOpacity": 1, "LabelBubbleOpacity": 1, "LabelOpacity": 1,
            "LabelSpacing": 14, "LabelCornerRadius": 22
        })));
        assert_eq!(a.label_spacing, 14.0);
        assert_eq!(a.indicator_colors[2], "#F4FAFD");
    }

    #[test]
    fn colours_are_checked_and_wpf_alpha_first_is_converted() {
        assert_eq!(css_color("#abc", "#000000"), "#ABC");
        assert_eq!(css_color(" #6e8090 ", "#000000"), "#6E8090");
        assert_eq!(css_color("#80FF0000", "#000000"), "#FF000080", "50% red: alpha moves to the end");
        for bad in ["red", "#12", "#12345", "#GGGGGG", "url(x)", "#fff; background:url(x)", "", "rgb(1,2,3)"] {
            assert_eq!(css_color(bad, "#123456"), "#123456", "{bad}");
        }
    }

    #[test]
    fn numbers_outside_a_sensible_range_are_clamped_and_junk_is_ignored() {
        let a = overlay_appearance(&with(json!({
            "IndicatorWidth": 9999, "IndicatorHeight": -5, "LabelFontSize": "big", "IndicatorOpacity": 0,
            "LabelCornerRadius": null
        })));
        assert_eq!(a.indicator_width, 80.0);
        assert_eq!(a.indicator_height, 10.0);
        assert_eq!(a.label_font_size, 14.0);
        assert_eq!(a.indicator_opacity, 0.05, "never fully invisible: the overlay is the sign OASIS runs");
        assert_eq!(a.label_corner_radius, 22.0);
    }

    #[test]
    fn dock_and_idle_are_read_from_their_own_keys_and_junk_falls_back() {
        let mut s = Settings::default();
        assert_eq!((overlay_appearance(&s).dock.as_str(), overlay_appearance(&s).idle.as_str()), ("right", "faint"));
        s.extra.insert("OverlayDock".into(), json!("Bottom"));
        s.extra.insert("OverlayIdle".into(), json!("hidden"));
        assert_eq!((overlay_appearance(&s).dock.as_str(), overlay_appearance(&s).idle.as_str()), ("bottom", "hidden"));
        s.extra.insert("OverlayDock".into(), json!("middle"));
        assert_eq!(overlay_appearance(&s).dock, "right");
    }

    #[test]
    fn an_older_file_keeps_the_horizontal_pop_up() {
        let mut s = Settings::default();
        assert!(!popup_vertical(&s), "absent is horizontal");
        s.extra.insert(POPUP.into(), json!("sideways"));
        assert!(!popup_vertical(&s));
        s.extra.insert(POPUP.into(), json!("Vertical"));
        assert!(popup_vertical(&s));
    }

    #[test]
    fn the_position_is_the_saved_one_or_none() {
        let at = |x: i32, y: i32| Settings { overlay_x: x.into(), overlay_y: y.into(), ..Settings::default() };
        assert_eq!(overlay_position(&at(0, 717)), Some((0.0, 717.0)));
        assert_eq!(overlay_position(&at(-1, -1)), None);
        assert_eq!(overlay_position(&at(10, -1)), None);
        assert_eq!(overlay_position(&Settings::default()), None);
    }

    #[test]
    fn a_fractional_position_from_wpf_is_read() {
        let s = Settings { overlay_x: serde_json::Number::from_f64(12.5).unwrap(), overlay_y: 300.into(), ..Settings::default() };
        assert_eq!(overlay_position(&s), Some((12.5, 300.0)));
    }
}
