//! The Quick Launch setups kept in `profiles.json` (`QuickLaunch`), read for voice and for running.
//! Port of `QuickLaunchModel.cs` and the reading half of `QuickLaunchService.cs`.
//!
//! A window goes to a screen and either a `Layout` (as opened, maximized, a half) or a `Region`, any
//! box of the editor's grid. The editor saves with `save_setup` and `delete_setup`.

use oasis_core::actions::{ActionKind, ActionStep, ButtonAction};
use oasis_core::placement::{Layout, PixelRect, Region};
use oasis_core::quick_launch::QuickSetupRef;
use oasis_core::voice::command::safety::{step_tier, SafetyTier};
use serde_json::Value;

use crate::model::Settings;

const MAX_ITEMS: usize = 40;
/// A short pause between items so the apps do not all fight for the focus at once.
const PAUSE_MS: u32 = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    App,
    Url,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: ItemKind,
    pub target: String,
    /// For a link: the browser to use; empty is the default browser.
    pub open_with: String,
    /// Empty leaves the window on whatever screen it opens on; otherwise `main`, `other:1`, `left`, `right`, `screen:N`.
    pub screen: String,
    pub layout: Layout,
    /// A grid box, which wins over `layout` when set.
    pub region: Option<Region>,
    /// Open a new window even when the app already has one, instead of moving that one.
    pub open_another: bool,
}

impl Item {
    /// False when the item is only opened and never moved.
    pub fn wants_placement(&self) -> bool {
        !self.screen.is_empty() || self.layout != Layout::AsIs || self.region.is_some()
    }

    /// What decides how the window is placed: a region behaves like the half it is anchored to.
    pub fn effective_layout(&self) -> Layout {
        self.region.map_or(self.layout, |r| r.anchor())
    }

    /// The visible rectangle asked for on `work`; `None` for as-opened and maximized.
    pub fn target_rect(&self, work: PixelRect) -> Option<PixelRect> {
        match (self.region, self.layout) {
            (Some(r), _) => Some(r.rect(work)),
            (None, Layout::LeftHalf | Layout::RightHalf) => Some(oasis_core::placement::target(work, self.layout)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    pub id: String,
    pub name: String,
    /// Run once after OASIS starts.
    pub start_with_oasis: bool,
    pub voice_phrases: Vec<String>,
    pub items: Vec<Item>,
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

/// Every setup that has an id, a name and at least one usable item.
pub fn setups(settings: &Settings) -> Vec<Setup> {
    let Some(list) = settings.extra.get("QuickLaunch").and_then(Value::as_array) else { return Vec::new() };
    list.iter()
        .filter_map(|s| {
            let items: Vec<Item> = s
                .get("Items")
                .and_then(Value::as_array)?
                .iter()
                .take(MAX_ITEMS)
                .filter_map(|i| {
                    let kind = match i.get("Kind").and_then(Value::as_i64).unwrap_or(0) {
                        0 => ItemKind::App,
                        1 => ItemKind::Url,
                        2 => ItemKind::Folder,
                        _ => return None,
                    };
                    let target = text(i, "Target");
                    (!target.is_empty()).then(|| Item {
                        kind,
                        target,
                        open_with: text(i, "OpenWith"),
                        screen: text(i, "Screen"),
                        layout: Layout::from_number(i.get("Layout").and_then(Value::as_i64).unwrap_or(0)),
                        region: i.get("Region").and_then(|r| {
                            let n = |k: &str| r.get(k).and_then(Value::as_i64);
                            Region::new(n("X")?, n("Y")?, n("W")?, n("H")?)
                        }),
                        open_another: i.get("OpenAnother").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect();
            let (id, name) = (text(s, "Id"), text(s, "Name"));
            if id.is_empty() || name.is_empty() || items.is_empty() {
                return None;
            }
            let voice_phrases = s
                .get("VoicePhrases")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).take(8).collect())
                .unwrap_or_default();
            let start_with_oasis = s.get("StartWithOasis").and_then(Value::as_bool).unwrap_or(false);
            Some(Setup { id, name, start_with_oasis, voice_phrases, items })
        })
        .collect()
}

/// What the voice router listens for.
pub fn voice_refs(settings: &Settings) -> Vec<QuickSetupRef> {
    setups(settings)
        .into_iter()
        .map(|s| QuickSetupRef { id: s.id, phrases: std::iter::once(s.name).chain(s.voice_phrases).collect() })
        .collect()
}

/// The setup marked "start with OASIS": only the first marked one counts.
pub fn startup_setup(settings: &Settings) -> Option<Setup> {
    setups(settings).into_iter().find(|s| s.start_with_oasis)
}

/// A setup by its id, or failing that by its name.
pub fn find(settings: &Settings, id_or_name: &str) -> Option<Setup> {
    let key = id_or_name.trim();
    let all = setups(settings);
    all.iter().find(|s| s.id.eq_ignore_ascii_case(key)).or_else(|| all.iter().find(|s| s.name.eq_ignore_ascii_case(key))).cloned()
}

/// The action step an item amounts to, which is what the safety rules judge.
pub fn step_of(item: &Item) -> (&'static str, String) {
    match item.kind {
        ItemKind::App | ItemKind::Folder => (ActionKind::LaunchApp.as_str(), item.target.clone()),
        ItemKind::Url if item.open_with.is_empty() => (ActionKind::OpenUrl.as_str(), item.target.clone()),
        ItemKind::Url => (ActionKind::LaunchApp.as_str(), format!("{}|{}", item.open_with, item.target)),
    }
}

/// The setup without the items that may not start from here (an app that names a shell or a script
/// host), and a sentence for each one left out. The rule is the one spoken commands follow.
pub fn without_blocked(setup: &Setup) -> (Setup, Vec<String>) {
    let mut kept = setup.clone();
    let mut skipped = Vec::new();
    kept.items.retain(|item| {
        let (kind, value) = step_of(item);
        let blocked = step_tier(kind, &value) == SafetyTier::Blocked;
        if blocked {
            skipped.push(format!("{} was left out because it is not allowed to start from here", item.target));
        }
        !blocked
    });
    (kept, skipped)
}

/// The setup as steps of one automation, and the items left out (with why). Never runs a script or
/// a shell. Used when there is no desktop to place windows on.
pub fn to_action(setup: &Setup) -> (ButtonAction, Vec<String>) {
    let (setup, skipped) = without_blocked(setup);
    let mut steps = Vec::new();
    for item in &setup.items {
        let (kind, value) = step_of(item);
        if !steps.is_empty() {
            steps.push(ActionStep { kind: ActionKind::Delay.as_str().into(), value: PAUSE_MS.to_string(), ..Default::default() });
        }
        steps.push(ActionStep { kind: kind.into(), value, ..Default::default() });
    }
    (ButtonAction { kind: ActionKind::Automation.as_str().into(), value: String::new(), steps }, skipped)
}


// ---- the editor ------------------------------------------------------------------------------

/// What the editor sends for one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemEdit {
    /// `app`, `url` or `folder`.
    pub kind: String,
    pub target: String,
    pub open_with: String,
    pub screen: String,
    /// 0 as opened, 1 maximized, 2 left half, 3 right half.
    pub layout: i64,
    pub region: Option<Region>,
    pub open_another: bool,
}

/// What the editor sends for a setup. `id` is empty for a new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupEdit {
    pub id: String,
    pub name: String,
    /// What to say, besides the name; commas separate several.
    pub call_name: String,
    pub start_with_oasis: bool,
    pub items: Vec<ItemEdit>,
}

/// The phrases a setup answers to, as the router will see them: the name, then the call names.
fn phrases_of(name: &str, call_name: &str) -> Vec<String> {
    std::iter::once(name.trim().to_string()).chain(call_name.split(',').map(|p| p.trim().to_string())).filter(|p| !p.is_empty()).collect()
}

fn item_json(item: &ItemEdit, old: Option<&Value>) -> Result<Value, String> {
    let kind = match item.kind.as_str() {
        "app" => 0,
        "url" => 1,
        "folder" => 2,
        other => return Err(format!("'{other}' is not something Quick Launch can open.")),
    };
    let target = item.target.trim();
    if target.is_empty() {
        return Err("Every item needs an app, link or folder.".into());
    }
    // Keep what the editor does not know about (a field a newer version added).
    let mut json = old.cloned().unwrap_or_else(|| serde_json::json!({}));
    let obj = json.as_object_mut().ok_or("An item is not an object.")?;
    obj.insert("Kind".into(), kind.into());
    obj.insert("Target".into(), target.into());
    obj.insert("OpenWith".into(), item.open_with.trim().into());
    obj.insert("Screen".into(), item.screen.trim().into());
    obj.insert("Layout".into(), item.layout.clamp(0, 3).into());
    obj.insert("OpenAnother".into(), item.open_another.into());
    match item.region {
        Some(r) => obj.insert("Region".into(), serde_json::json!({ "X": r.x, "Y": r.y, "W": r.w, "H": r.h })),
        None => obj.remove("Region"),
    };
    Ok(json)
}

fn new_id(name: &str, taken: &[String]) -> String {
    let base: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    let base = if base.is_empty() { "setup".to_string() } else { base };
    let mut id = base.clone();
    let mut n = 2;
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&id)) {
        id = format!("{base}{n}");
        n += 1;
    }
    id
}

impl Settings {
    /// Create or change a setup and return its id. A name or phrase another setup already answers to
    /// is refused: two setups said the same way would be a coin toss.
    pub fn save_setup(&mut self, edit: SetupEdit) -> Result<String, String> {
        let name = edit.name.trim();
        if name.is_empty() {
            return Err("A setup needs a name.".into());
        }
        // the window stops typing at these; this refuses the rest
        if name.chars().count() > 40 {
            return Err("A setup name can be at most 40 characters.".into());
        }
        if edit.call_name.chars().count() > 80 {
            return Err("The voice command can be at most 80 characters.".into());
        }
        if edit.items.is_empty() {
            return Err("Add at least one app to open.".into());
        }
        if edit.items.len() > MAX_ITEMS {
            return Err(format!("A setup can open up to {MAX_ITEMS} things."));
        }
        let phrases = phrases_of(name, &edit.call_name);
        for phrase in &phrases {
            let letters = oasis_core::quick_launch::normalize(phrase).chars().filter(|c| c.is_alphanumeric()).count();
            if letters < oasis_core::quick_launch::MIN_PHRASE_LETTERS {
                return Err(format!("“{phrase}” is too short to say: use at least {} letters.", oasis_core::quick_launch::MIN_PHRASE_LETTERS));
            }
        }
        let list = self.extra.entry("QuickLaunch".to_string()).or_insert_with(|| Value::Array(Vec::new()));
        let list = list.as_array_mut().ok_or("QuickLaunch in profiles.json is not a list.")?;
        let position = list.iter().position(|s| !edit.id.is_empty() && text(s, "Id").eq_ignore_ascii_case(&edit.id));
        for (i, other) in list.iter().enumerate() {
            if Some(i) == position {
                continue;
            }
            let theirs: Vec<String> = std::iter::once(text(other, "Name"))
                .chain(other.get("VoicePhrases").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string))
                .map(|p| oasis_core::quick_launch::normalize(&p))
                .collect();
            if let Some(clash) = phrases.iter().find(|p| theirs.contains(&oasis_core::quick_launch::normalize(p))) {
                return Err(format!("“{}” already opens “{}”. Pick a different name or phrase.", clash, text(other, "Name")));
            }
        }
        let old_items: Vec<Value> = position.and_then(|i| list[i].get("Items").and_then(Value::as_array).cloned()).unwrap_or_default();
        let mut items = Vec::new();
        for (n, item) in edit.items.iter().enumerate() {
            items.push(item_json(item, old_items.get(n))?);
        }
        let mut json = position.map_or_else(|| serde_json::json!({}), |i| list[i].clone());
        let ids: Vec<String> = list.iter().map(|s| text(s, "Id")).collect();
        let id = position.map_or_else(|| new_id(name, &ids), |i| text(&list[i], "Id"));
        let obj = json.as_object_mut().ok_or("A setup is not an object.")?;
        obj.insert("Id".into(), id.clone().into());
        obj.insert("Name".into(), name.into());
        obj.insert("VoicePhrases".into(), Value::Array(phrases.iter().skip(1).map(|p| Value::String(p.clone())).collect()));
        obj.insert("StartWithOasis".into(), edit.start_with_oasis.into());
        obj.insert("Items".into(), Value::Array(items));
        match position {
            Some(i) => list[i] = json,
            None => list.push(json),
        }
        Ok(id)
    }

    pub fn delete_setup(&mut self, id: &str) -> Result<(), String> {
        let list = self.extra.get_mut("QuickLaunch").and_then(Value::as_array_mut).ok_or("That setup is no longer there.")?;
        let before = list.len();
        list.retain(|s| !text(s, "Id").eq_ignore_ascii_case(id));
        if list.len() == before { Err("That setup is no longer there.".into()) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn settings(quick: Value) -> Settings {
        serde_json::from_value(json!({ "QuickLaunch": quick })).unwrap()
    }

    fn sample() -> Settings {
        settings(json!([
            { "Id": "p", "Name": "Project Mode", "VoicePhrases": ["time to work", " "], "Items": [
                { "Kind": 0, "Target": "code" },
                { "Kind": 1, "Target": "https://github.com", "OpenWith": "brave" },
                { "Kind": 1, "Target": "https://example.com" },
                { "Kind": 2, "Target": "C:\\Work" },
                { "Kind": 0, "Target": "" },
                { "Kind": 9, "Target": "x" }
            ] },
            { "Id": "empty", "Name": "Nothing", "Items": [] },
            { "Name": "No id", "Items": [{ "Kind": 0, "Target": "a" }] }
        ]))
    }

    #[test]
    fn only_setups_with_an_id_a_name_and_an_item_are_kept() {
        let all = setups(&sample());
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].items.len(), 4);
        assert_eq!(all[0].voice_phrases, ["time to work"]);
        assert_eq!(voice_refs(&sample())[0].phrases, ["Project Mode", "time to work"]);
        assert!(setups(&Settings::default()).is_empty());
    }

    #[test]
    fn the_first_setup_marked_to_start_with_oasis_is_the_one() {
        let s = settings(json!([
            { "Id": "a", "Name": "A", "Items": [{ "Kind": 0, "Target": "x" }] },
            { "Id": "b", "Name": "B", "StartWithOasis": true, "Items": [{ "Kind": 0, "Target": "x" }] },
            { "Id": "c", "Name": "C", "StartWithOasis": true, "Items": [{ "Kind": 0, "Target": "x" }] }
        ]));
        assert_eq!(startup_setup(&s).unwrap().id, "b");
        assert!(startup_setup(&sample()).is_none());
    }

    #[test]
    fn a_setup_is_found_by_id_then_by_name() {
        assert_eq!(find(&sample(), "P").unwrap().name, "Project Mode");
        assert_eq!(find(&sample(), "project mode").unwrap().id, "p");
        assert!(find(&sample(), "nope").is_none());
    }

    #[test]
    fn the_action_opens_each_item_with_a_pause_between() {
        let (action, skipped) = to_action(&setups(&sample())[0]);
        assert!(skipped.is_empty());
        let steps: Vec<(&str, &str)> = action.steps.iter().map(|s| (s.kind.as_str(), s.value.as_str())).collect();
        assert_eq!(
            steps,
            [
                ("LaunchApp", "code"),
                ("Delay", "400"),
                ("LaunchApp", "brave|https://github.com"),
                ("Delay", "400"),
                ("OpenUrl", "https://example.com"),
                ("Delay", "400"),
                ("LaunchApp", "C:\\Work"),
            ]
        );
    }

    #[test]
    fn an_app_that_names_a_shell_is_left_out_and_said() {
        let s = settings(json!([{ "Id": "x", "Name": "Odd", "Items": [
            { "Kind": 0, "Target": "powershell.exe" }, { "Kind": 0, "Target": "notepad" }] }]));
        let (action, skipped) = to_action(&setups(&s)[0]);
        assert_eq!(action.steps.len(), 1);
        assert_eq!(skipped.len(), 1);
    }

    fn item_edit(target: &str) -> ItemEdit {
        ItemEdit { kind: "app".into(), target: target.into(), open_with: String::new(), screen: String::new(), layout: 0, region: None, open_another: false }
    }

    fn setup_edit(name: &str, call: &str) -> SetupEdit {
        SetupEdit { id: String::new(), name: name.into(), call_name: call.into(), start_with_oasis: false, items: vec![item_edit("code")] }
    }

    #[test]
    fn a_saved_setup_reads_back_with_its_region_and_phrases() {
        let mut s = Settings::default();
        let mut edit = setup_edit("Project Mode", "time to work, work time");
        edit.items[0].screen = "other:1".into();
        edit.items[0].region = Region::new(500, 0, 500, 500);
        let id = s.save_setup(edit).unwrap();
        let got = &setups(&s)[0];
        assert_eq!((got.id.as_str(), got.name.as_str()), (id.as_str(), "Project Mode"));
        assert_eq!(got.voice_phrases, ["time to work", "work time"]);
        assert_eq!(got.items[0].region, Region::new(500, 0, 500, 500));
        assert!(got.items[0].wants_placement());
    }

    #[test]
    fn editing_keeps_the_id_and_unknown_fields_and_refuses_a_second_setup_said_the_same() {
        let mut s = settings(json!([{ "Id": "a", "Name": "Alpha", "Extra": 7, "Items": [{ "Kind": 0, "Target": "x", "Future": true }] }]));
        let mut edit = setup_edit("Alpha two", "");
        edit.id = "a".into();
        assert_eq!(s.save_setup(edit).unwrap(), "a");
        assert_eq!(s.extra["QuickLaunch"][0]["Extra"], 7);
        assert_eq!(s.extra["QuickLaunch"][0]["Items"][0]["Future"], true);
        assert!(s.save_setup(setup_edit("alpha  two", "")).unwrap_err().contains("already opens"));
        assert!(s.save_setup(setup_edit("Beta", "Alpha Two")).unwrap_err().contains("already opens"));
        assert!(s.save_setup(setup_edit("ab", "")).unwrap_err().contains("too short"));
        assert!(s.save_setup(SetupEdit { items: vec![], ..setup_edit("Gamma", "") }).is_err());
    }

    #[test]
    fn a_setup_can_be_deleted() {
        let mut s = Settings::default();
        let id = s.save_setup(setup_edit("Project Mode", "")).unwrap();
        assert!(s.delete_setup(&id).is_ok());
        assert!(setups(&s).is_empty() && s.delete_setup(&id).is_err());
    }
}
