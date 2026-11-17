//! Which part of a program has the focus: terminal, editor, AI chat, … Port of
//! `Services/Context/SectorClassifier.cs`, the workbench fallback of `WorkbenchRegionProbe.cs`
//! and the decision of `ContextPanelDetector.cs`. Pure: the OS layer reads the UI Automation tree
//! and hands over a [`UiSnapshot`].

use super::catalog::{resolve_family, Exclusion, Family, PanelDef, Rule, Scope};

/// A panel needs at least this much evidence …
const MIN_WIN_SCORE: i32 = 20;
/// … and must beat the runner-up by this much, unless something about the focus itself said so.
const DECISIVE_MARGIN: i32 = 10;

/// One control in the focus chain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiNode {
    /// 0 is the focused control, 1 its parent, and so on.
    pub depth: usize,
    pub name: String,
    pub control_type: String,
    pub automation_id: String,
    pub class_name: String,
}

/// The focused control and its ancestors (nearest first, the focus itself first).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiSnapshot {
    pub focus: UiNode,
    pub ancestors: Vec<UiNode>,
}

const SHELL_CHROME_CLASSES: [&str; 4] = ["ClientView", "NonClientView", "WinFrameView", "RootView"];

impl UiSnapshot {
    /// The focus tells nothing: unnamed, and only a pane or view.
    pub fn is_weak_focus(&self) -> bool {
        self.focus.name.trim().is_empty()
            && (contains_ci(&self.focus.control_type, "Pane") || contains_ci(&self.focus.control_type, "View"))
    }

    /// The focus is the browser-engine shell around the page, not anything inside it.
    pub fn is_shell_chrome_focus(&self) -> bool {
        let class = &self.focus.class_name;
        contains_ci(class, "Chrome_WidgetWin")
            || contains_ci(class, "Chrome_RenderWidgetHostHWND")
            || SHELL_CHROME_CLASSES.iter().any(|c| class.eq_ignore_ascii_case(c))
    }
}

/// `"none"`, `"low"`, `"medium"` or `"high"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelResult {
    pub app_family: String,
    pub app_family_display: String,
    pub panel_id: String,
    pub panel_label: String,
    pub confidence: String,
    pub sources: Vec<String>,
    pub focus_class_name: String,
    pub ancestor_hint: String,
}

impl Default for PanelResult {
    fn default() -> PanelResult {
        PanelResult {
            app_family: "generic".into(),
            app_family_display: "Generic".into(),
            panel_id: "unknown".into(),
            panel_label: String::new(),
            confidence: "none".into(),
            sources: Vec::new(),
            focus_class_name: String::new(),
            ancestor_hint: String::new(),
        }
    }
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn matches_any(haystack: &str, needles: &[&str]) -> bool {
    !haystack.trim().is_empty() && needles.iter().any(|n| contains_ci(haystack, n))
}

/// Every non-empty list must match, and at least one list must be non-empty.
fn node_matches(node: &UiNode, name: &[&str], control_type: &[&str], class_name: &[&str], automation_id: &[&str]) -> bool {
    if !name.is_empty() && !matches_any(&node.name, name) {
        return false;
    }
    if !control_type.is_empty() && !matches_any(&node.control_type, control_type) {
        return false;
    }
    if !class_name.is_empty() && !matches_any(&node.class_name, class_name) {
        return false;
    }
    if !automation_id.is_empty() && !matches_any(&node.automation_id, automation_id) {
        return false;
    }
    name.len() + control_type.len() + class_name.len() + automation_id.len() > 0
}

fn rule_matches_node(rule: &Rule, node: &UiNode) -> bool {
    node_matches(node, rule.name, rule.control_type, rule.class_name, rule.automation_id)
}

fn is_excluded(panel: &PanelDef, ui: &UiSnapshot) -> bool {
    let Some(Exclusion { name, control_type, class_name, automation_id }) = panel.exclude_when_focus else { return false };
    node_matches(&ui.focus, name, control_type, class_name, automation_id)
}

fn score_rule(rule: &Rule, title: &str, url: &str, ui: &UiSnapshot, sources: &mut Vec<String>, panel_id: &str) -> i32 {
    let matched = match rule.scope {
        Scope::Title => matches_any(title, rule.name),
        Scope::Url => matches_any(url, rule.name),
        Scope::Focus => rule_matches_node(rule, &ui.focus),
        Scope::Ancestor => ui.ancestors.iter().any(|a| rule_matches_node(rule, a)),
    };
    if !matched {
        return 0;
    }
    let scope_name = match rule.scope {
        Scope::Title => "title",
        Scope::Url => "url",
        Scope::Focus => "focus",
        Scope::Ancestor => "ancestor",
    };
    sources.push(format!("rule:{panel_id}:{scope_name}"));
    match rule.scope {
        Scope::Focus if !rule.class_name.is_empty() || !rule.automation_id.is_empty() => 25,
        Scope::Focus => 18,
        Scope::Ancestor if ui.ancestors.iter().any(|a| a.depth <= 4 && rule_matches_node(rule, a)) => 8,
        Scope::Ancestor => 3,
        Scope::Title | Scope::Url => 12,
    }
}

fn score_panel(panel: &PanelDef, title: &str, url: &str, ui: &UiSnapshot, sources: &mut Vec<String>) -> (i32, bool) {
    let mut score = 0;
    let mut has_focus_rule = false;
    for rule in panel.match_any {
        let rule_score = score_rule(rule, title, url, ui, sources, panel.id);
        if rule_score <= 0 {
            continue;
        }
        score += rule_score + panel.priority / 5;
        if rule.scope == Scope::Focus {
            has_focus_rule = true;
        }
    }
    (score, has_focus_rule)
}

fn detect_panel<'a>(
    family: Option<&'a Family>,
    title: &str,
    url: &str,
    ui: &UiSnapshot,
    sources: &mut Vec<String>,
) -> (Option<&'a PanelDef>, i32, bool) {
    let Some(family) = family.filter(|f| !f.panels.is_empty()) else { return (None, 0, false) };

    let mut scores: Vec<(&PanelDef, i32, bool)> = Vec::new();
    for panel in family.panels {
        if is_excluded(panel, ui) {
            continue;
        }
        let (score, has_focus_rule) = score_panel(panel, title, url, ui, sources);
        if score > 0 {
            scores.push((panel, score, has_focus_rule));
        }
    }
    if scores.is_empty() {
        return (None, 0, false);
    }
    // Stable sort keeps catalog order among equal scores, as the C# list sort did in practice.
    scores.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    let (best, best_score, best_focus) = scores[0];
    let second = scores.get(1).map_or(0, |s| s.1);

    if best_score < MIN_WIN_SCORE {
        return (None, best_score, best_focus);
    }
    if best_score < second + DECISIVE_MARGIN && !best_focus {
        return (None, best_score, false);
    }
    if !best_focus && best_score < MIN_WIN_SCORE + 5 {
        return (None, best_score, false);
    }
    sources.push(format!("panel:{}", best.id));
    (Some(best), best_score, best_focus)
}

fn score_confidence(panel: Option<&PanelDef>, score: i32, has_focus_rule: bool, ui: &UiSnapshot, url: &str) -> &'static str {
    if panel.is_none() || score < MIN_WIN_SCORE {
        return if ui.focus.name.is_empty() { "none" } else { "low" };
    }
    if has_focus_rule && ui.ancestors.len() > 1 {
        "high"
    } else if has_focus_rule || !url.trim().is_empty() {
        "medium"
    } else {
        "low"
    }
}

/// The name of an ancestor that says what the panel is, for diagnostics.
fn find_ancestor_hint(ui: &UiSnapshot, panel_id: Option<&str>) -> String {
    let Some(panel_id) = panel_id.filter(|p| !p.trim().is_empty()) else { return String::new() };
    let landmarks: &[&str] = match panel_id {
        "terminal" => &["integrated terminal", "terminal"],
        "chat" => &["chat", "composer", "aichat", "agent"],
        "agent" => &["agent", "composer"],
        "editor" => &["editor", "monaco"],
        "explorer" => &["files explorer", "explorer", "sidebar"],
        _ => &[],
    };
    for node in &ui.ancestors {
        if landmarks.iter().any(|l| contains_ci(&node.name, l)) {
            return node.name.clone();
        }
    }
    ui.ancestors.iter().find(|a| !a.name.trim().is_empty()).map(|a| a.name.clone()).unwrap_or_default()
}

/// Classify what has the focus in a program.
pub fn classify(process_name: &str, process_path: Option<&str>, title: &str, url: &str, ui: &UiSnapshot) -> PanelResult {
    let family = resolve_family(process_name, process_path);
    let mut sources = Vec::new();
    if let Some(f) = family {
        sources.push(format!("family:{}", f.id));
    }
    let (panel, score, has_focus_rule) = detect_panel(family, title, url, ui, &mut sources);
    let confidence = score_confidence(panel, score, has_focus_rule, ui, url);
    PanelResult {
        app_family: family.map_or("generic", |f| f.id).to_string(),
        app_family_display: family.map_or("Generic", |f| f.display_name).to_string(),
        panel_id: panel.map_or("unknown", |p| p.id).to_string(),
        panel_label: panel.map_or("", |p| p.label).to_string(),
        confidence: confidence.to_string(),
        sources,
        focus_class_name: ui.focus.class_name.clone(),
        ancestor_hint: find_ancestor_hint(ui, panel.map(|p| p.id)),
    }
}

// ---- the workbench fallback --------------------------------------------------------------------

/// A control seen while scanning a whole IDE window for telltale names and classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbenchNode {
    pub depth: usize,
    pub name: String,
    pub class_name: String,
    pub automation_id: String,
    /// The control, or something within a few levels below it, has the keyboard focus.
    pub has_focus_within: bool,
}

const NAME_LANDMARKS: [(&str, &str); 14] = [
    ("Integrated Terminal", "terminal"),
    ("Terminal", "terminal"),
    ("Debug Console", "debug-console"),
    ("Chat", "chat"),
    ("Composer", "chat"),
    ("AI Chat", "chat"),
    ("Agent Conversation", "agent"),
    ("Message input", "agent"),
    ("Agent", "agent"),
    ("Toggle Agent", "agent"),
    ("Problems", "output"),
    ("Output", "output"),
    ("Files Explorer", "explorer"),
    ("Explorer", "explorer"),
];

const CLASS_SIGNALS: [(&str, &str, &str, i32); 15] = [
    ("xterm-helper-textarea", "terminal", "Integrated Terminal", 40),
    ("xterm-screen", "terminal", "Integrated Terminal", 35),
    ("terminal xterm", "terminal", "Integrated Terminal", 34),
    ("composer-messages", "chat", "AI Chat / Composer", 38),
    ("composer-input", "chat", "AI Chat / Composer", 37),
    ("composer-bar", "chat", "AI Chat / Composer", 36),
    ("aislash-editor-input", "chat", "AI Chat / Composer", 35),
    ("antigravity-agent-side-panel", "agent", "AI Agent", 39),
    ("Message input", "agent", "AI Agent", 38),
    ("Agent Conversation", "agent", "AI Agent", 37),
    ("monaco-editor", "editor", "Code Editor", 30),
    ("view-lines", "editor", "Code Editor", 28),
    ("jetski-custom-editor", "editor", "Code Editor", 28),
    ("workbench.parts.editor", "editor", "Code Editor", 26),
    ("explorer-item", "explorer", "File Explorer / Sidebar", 24),
];

fn landmark_label(panel_id: &str, fallback: &str) -> String {
    match panel_id {
        "terminal" => "Integrated Terminal",
        "agent" => "AI Agent",
        "chat" => "AI Chat / Composer",
        "editor" => "Code Editor",
        "explorer" => "File Explorer / Sidebar",
        "output" => "Output Panel",
        "debug-console" => "Debug Console",
        _ => fallback,
    }
    .to_string()
}

struct Hit {
    panel_id: &'static str,
    label: String,
    signal: &'static str,
    class_name: String,
    depth: usize,
    priority: i32,
    focus_within: bool,
}

/// Scan an IDE window's controls for names and classes that give a panel away, and pick the one
/// with the keyboard focus (then the deepest, then the strongest). `None` if nothing is there.
pub fn workbench_panel(app_family: &str, nodes: &[WorkbenchNode]) -> Option<PanelResult> {
    if app_family != "electron-ide" {
        return None;
    }
    let mut hits: Vec<Hit> = Vec::new();
    for node in nodes {
        let haystack = format!("{} {} {}", node.name, node.class_name, node.automation_id);
        for (landmark, panel_id) in NAME_LANDMARKS {
            if contains_ci(&node.name, landmark) {
                hits.push(Hit {
                    panel_id,
                    label: landmark_label(panel_id, landmark),
                    signal: landmark,
                    class_name: node.class_name.clone(),
                    depth: node.depth,
                    priority: 20 + node.depth as i32,
                    focus_within: node.has_focus_within,
                });
            }
        }
        for (pattern, panel_id, label, priority) in CLASS_SIGNALS {
            if contains_ci(&haystack, pattern) {
                hits.push(Hit {
                    panel_id,
                    label: label.to_string(),
                    signal: pattern,
                    class_name: node.class_name.clone(),
                    depth: node.depth,
                    priority: priority + node.depth as i32,
                    focus_within: node.has_focus_within,
                });
            }
        }
    }
    let best = hits.into_iter().max_by_key(|h| (h.focus_within, h.depth, h.priority))?;
    Some(PanelResult {
        app_family: app_family.to_string(),
        app_family_display: String::new(),
        panel_id: best.panel_id.to_string(),
        panel_label: best.label,
        confidence: "medium".into(),
        sources: vec![format!("workbench:{}", best.signal)],
        focus_class_name: best.class_name,
        ancestor_hint: best.signal.to_string(),
    })
}

/// Whether the IDE window should be scanned as a second opinion: the first look found no panel,
/// or only weakly, or the focus said nothing.
pub fn should_probe_workbench(result: &PanelResult, ui: &UiSnapshot) -> bool {
    result.app_family == "electron-ide"
        && (result.panel_id == "unknown"
            || matches!(result.confidence.as_str(), "none" | "low")
            || ui.is_weak_focus()
            || ui.is_shell_chrome_focus())
}

/// Combine the first look with the workbench scan, as `ContextPanelDetector.Detect` does.
pub fn merge_workbench(first: PanelResult, fallback: Option<PanelResult>) -> PanelResult {
    let Some(fallback) = fallback else { return first };
    let mut sources = first.sources.clone();
    sources.extend(fallback.sources.clone());
    PanelResult {
        app_family: first.app_family,
        app_family_display: first.app_family_display,
        panel_id: fallback.panel_id,
        panel_label: fallback.panel_label,
        confidence: fallback.confidence,
        sources,
        focus_class_name: if fallback.focus_class_name.trim().is_empty() { first.focus_class_name } else { fallback.focus_class_name },
        ancestor_hint: if fallback.ancestor_hint.trim().is_empty() { first.ancestor_hint } else { fallback.ancestor_hint },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(depth: usize, name: &str, class_name: &str) -> UiNode {
        UiNode { depth, name: name.into(), class_name: class_name.into(), ..UiNode::default() }
    }

    fn snapshot(focus: UiNode, ancestors: &[UiNode]) -> UiSnapshot {
        let mut chain = vec![focus.clone()];
        chain.extend(ancestors.iter().cloned());
        UiSnapshot { focus, ancestors: chain }
    }

    #[test]
    fn the_integrated_terminal_is_found_from_its_text_input() {
        let ui = snapshot(node(0, "Terminal 1, powershell", "xterm-helper-textarea"), &[node(1, "", "xterm-screen"), node(2, "Terminal", "")]);
        let r = classify("Code", None, "main.rs - Visual Studio Code", "", &ui);
        assert_eq!((r.panel_id.as_str(), r.confidence.as_str()), ("terminal", "high"), "{r:?}");
        assert_eq!(r.app_family, "electron-ide");
        assert_eq!(r.focus_class_name, "xterm-helper-textarea");
        assert!(r.sources.contains(&"panel:terminal".to_string()));
    }

    #[test]
    fn the_code_editor_is_found_and_the_terminal_is_excluded_there() {
        let ui = snapshot(node(0, "", "monaco-editor view-lines"), &[node(1, "editor", "")]);
        let r = classify("Cursor", None, "x", "", &ui);
        assert_eq!(r.panel_id, "editor", "{r:?}");
    }

    #[test]
    fn the_ai_agent_panel_is_found_in_antigravity() {
        let ui = snapshot(node(0, "Message input", ""), &[node(1, "Agent Conversation", "antigravity-agent-side-panel")]);
        let r = classify("Antigravity IDE", None, "x", "", &ui);
        assert_eq!(r.panel_id, "agent", "{r:?}");
        assert_eq!(r.panel_label, "AI Agent");
    }

    #[test]
    fn the_chat_composer_is_found_in_cursor() {
        let ui = snapshot(node(0, "", "aislash-editor-input"), &[node(1, "", "composer-bar")]);
        let r = classify("Cursor", None, "x", "", &ui);
        assert_eq!(r.panel_id, "chat", "{r:?}");
    }

    #[test]
    fn an_unknown_program_has_no_panel() {
        let r = classify("spotify", None, "Spotify", "", &snapshot(node(0, "Search", ""), &[]));
        assert_eq!((r.panel_id.as_str(), r.app_family.as_str()), ("unknown", "generic"));
    }

    #[test]
    fn a_focus_that_says_nothing_is_not_guessed() {
        let r = classify("Code", None, "x", "", &UiSnapshot::default());
        assert_eq!((r.panel_id.as_str(), r.confidence.as_str()), ("unknown", "none"));
    }

    #[test]
    fn a_web_login_page_is_found_from_the_address() {
        let r = classify("chrome", None, "Sign in - Google Accounts", "https://accounts.google.com/signin", &UiSnapshot::default());
        assert_eq!(r.panel_id, "web-login", "{r:?}");
        assert_eq!(r.confidence, "medium", "an address backs it up");
    }

    #[test]
    fn weak_and_shell_focus_are_told_apart_from_a_real_one() {
        let weak = UiSnapshot { focus: UiNode { control_type: "ControlType.Pane".into(), ..UiNode::default() }, ancestors: vec![] };
        assert!(weak.is_weak_focus());
        assert!(!snapshot(node(0, "Terminal", ""), &[]).is_weak_focus());
        let shell = snapshot(node(0, "", "Chrome_RenderWidgetHostHWND"), &[]);
        assert!(shell.is_shell_chrome_focus());
        assert!(!snapshot(node(0, "", "xterm"), &[]).is_shell_chrome_focus());
    }

    fn workbench(nodes: &[(usize, &str, &str, bool)]) -> Vec<WorkbenchNode> {
        nodes
            .iter()
            .map(|(depth, name, class_name, focus)| WorkbenchNode {
                depth: *depth,
                name: (*name).into(),
                class_name: (*class_name).into(),
                automation_id: String::new(),
                has_focus_within: *focus,
            })
            .collect()
    }

    #[test]
    fn the_workbench_scan_prefers_what_has_the_keyboard_focus() {
        let nodes = workbench(&[(3, "Integrated Terminal", "", false), (5, "Chat", "composer-bar", true)]);
        let r = workbench_panel("electron-ide", &nodes).unwrap();
        assert_eq!((r.panel_id.as_str(), r.confidence.as_str()), ("chat", "medium"));
    }

    #[test]
    fn the_workbench_scan_is_only_for_ides_and_needs_something_to_find() {
        assert!(workbench_panel("browser", &workbench(&[(1, "Terminal", "", true)])).is_none());
        assert!(workbench_panel("electron-ide", &workbench(&[(1, "Nothing here", "", true)])).is_none());
    }

    #[test]
    fn the_second_opinion_is_asked_only_when_the_first_was_weak() {
        let strong = PanelResult { app_family: "electron-ide".into(), panel_id: "terminal".into(), confidence: "high".into(), ..Default::default() };
        let ui = snapshot(node(0, "Terminal", "xterm"), &[]);
        assert!(!should_probe_workbench(&strong, &ui));
        let weak = PanelResult { panel_id: "unknown".into(), ..strong.clone() };
        assert!(should_probe_workbench(&weak, &ui));
        let browser = PanelResult { app_family: "browser".into(), panel_id: "unknown".into(), ..Default::default() };
        assert!(!should_probe_workbench(&browser, &ui), "never outside an IDE");
    }

    #[test]
    fn merging_takes_the_second_opinion_and_keeps_what_it_lacks() {
        let first = PanelResult { app_family: "electron-ide".into(), focus_class_name: "x".into(), ..Default::default() };
        let second = PanelResult { panel_id: "terminal".into(), panel_label: "Integrated Terminal".into(), confidence: "medium".into(), ..Default::default() };
        let merged = merge_workbench(first.clone(), Some(second));
        assert_eq!((merged.panel_id.as_str(), merged.app_family.as_str(), merged.focus_class_name.as_str()), ("terminal", "electron-ide", "x"));
        assert_eq!(merge_workbench(first.clone(), None), first);
    }
}
