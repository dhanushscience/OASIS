//! What each known program's panels look like to UI Automation. This is the data of the C# app's
//! `Services/Context/AppCatalog.json` (version 2), written as Rust so the pure crate needs no
//! JSON reader. Each rule names a part of the focused control, or of its ancestors, whose name,
//! type, class or automation id contains some text.

/// Where a rule looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The control that has the keyboard focus.
    Focus,
    /// Any of the focus control's ancestors.
    Ancestor,
    /// The window title.
    Title,
    /// The browser address.
    Url,
}

/// A rule matches when every non-empty list has at least one entry the text contains.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub scope: Scope,
    pub name: &'static [&'static str],
    pub control_type: &'static [&'static str],
    pub class_name: &'static [&'static str],
    pub automation_id: &'static [&'static str],
}

/// The same four lists, for "not this panel when the focus looks like this".
#[derive(Debug, Clone, Copy)]
pub struct Exclusion {
    pub name: &'static [&'static str],
    pub control_type: &'static [&'static str],
    pub class_name: &'static [&'static str],
    pub automation_id: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
pub struct PanelDef {
    pub id: &'static str,
    pub label: &'static str,
    pub priority: i32,
    pub match_any: &'static [Rule],
    pub exclude_when_focus: Option<Exclusion>,
}

#[derive(Debug, Clone, Copy)]
pub struct Family {
    pub id: &'static str,
    pub display_name: &'static str,
    pub process_names: &'static [&'static str],
    pub executable_name_contains: &'static [&'static str],
    pub panels: &'static [PanelDef],
}

const NONE: &[&str] = &[];

const fn rule(scope: Scope) -> Rule {
    Rule { scope, name: NONE, control_type: NONE, class_name: NONE, automation_id: NONE }
}

const fn exclude_class(class_name: &'static [&'static str]) -> Option<Exclusion> {
    Some(Exclusion { name: NONE, control_type: NONE, class_name, automation_id: NONE })
}

const ELECTRON_IDE: Family = Family {
    id: "electron-ide",
    display_name: "Electron IDE",
    process_names: &["Cursor", "Code", "Antigravity", "Antigravity IDE", "Windsurf", "VSCodium", "devenv"],
    executable_name_contains: &["cursor", "code", "antigravity", "windsurf", "vscodium"],
    panels: &[
        PanelDef {
            id: "terminal",
            label: "Integrated Terminal",
            priority: 30,
            match_any: &[
                Rule { class_name: &["xterm-helper-textarea", "xterm-helper", "xterm"], ..rule(Scope::Focus) },
                Rule {
                    name: &["Terminal 1", "Terminal 2", "Terminal 3", "Terminal 4", "Terminal 5", "PowerShell Extension"],
                    ..rule(Scope::Focus)
                },
                Rule { class_name: &["xterm-screen", "xterm-helpers", "terminal xterm"], ..rule(Scope::Ancestor) },
                Rule { name: &["terminal", "powershell", "cmd", "wsl", "bash"], ..rule(Scope::Focus) },
                Rule { name: &["integrated terminal", "terminal"], ..rule(Scope::Ancestor) },
                Rule { control_type: &["Terminal"], ..rule(Scope::Focus) },
            ],
            exclude_when_focus: exclude_class(&["monaco", "view-lines"]),
        },
        PanelDef {
            id: "chat",
            label: "AI Chat / Composer",
            priority: 28,
            match_any: &[
                Rule { name: &["chat", "composer", "aichat"], ..rule(Scope::Ancestor) },
                Rule { automation_id: &["chat", "composer", "aichat"], ..rule(Scope::Ancestor) },
                Rule { name: &["chat", "composer", "copilot", "ask", "assistant"], ..rule(Scope::Focus) },
                Rule {
                    class_name: &["composer-messages", "composer-input", "composer-", "aislash-editor-input"],
                    ..rule(Scope::Focus)
                },
                Rule {
                    class_name: &["composer-messages", "composer-input", "composer-", "composer-bar"],
                    ..rule(Scope::Ancestor)
                },
            ],
            exclude_when_focus: exclude_class(&["xterm", "monaco", "view-lines"]),
        },
        PanelDef {
            id: "agent",
            label: "AI Agent",
            priority: 29,
            match_any: &[
                Rule { class_name: &["antigravity-agent-side-panel"], ..rule(Scope::Ancestor) },
                Rule { name: &["Message input", "Agent Conversation"], ..rule(Scope::Focus) },
                Rule { name: &["Agent Conversation", "Message input"], ..rule(Scope::Ancestor) },
                Rule { name: &["toggle agent"], ..rule(Scope::Focus) },
                Rule { name: &["toggle agent"], ..rule(Scope::Ancestor) },
                Rule { class_name: &["antigravity"], ..rule(Scope::Focus) },
            ],
            exclude_when_focus: exclude_class(&["xterm", "monaco-list-row", "explorer-item", "view-lines", "aislash-editor-input"]),
        },
        PanelDef {
            id: "editor",
            label: "Code Editor",
            priority: 20,
            match_any: &[
                Rule { class_name: &["monaco", "view-lines", "aislash-editor-input", "monaco-editor"], ..rule(Scope::Focus) },
                Rule { control_type: &["Document"], ..rule(Scope::Focus) },
                Rule { name: &["editor", "code editor"], ..rule(Scope::Focus) },
                Rule { class_name: &["monaco", "jetski-custom-editor", "part editor"], ..rule(Scope::Ancestor) },
                Rule { automation_id: &["workbench.parts.editor"], ..rule(Scope::Ancestor) },
            ],
            exclude_when_focus: exclude_class(&["xterm"]),
        },
        PanelDef {
            id: "explorer",
            label: "File Explorer / Sidebar",
            priority: 18,
            match_any: &[
                Rule { control_type: &["TreeItem"], ..rule(Scope::Focus) },
                Rule { name: &["files explorer", "explorer"], ..rule(Scope::Ancestor) },
                Rule { automation_id: &["explorer", "sidebar"], ..rule(Scope::Ancestor) },
            ],
            exclude_when_focus: None,
        },
        PanelDef {
            id: "debug-console",
            label: "Debug Console",
            priority: 25,
            match_any: &[
                Rule { name: &["debug console"], ..rule(Scope::Focus) },
                Rule { name: &["debug console"], ..rule(Scope::Ancestor) },
            ],
            exclude_when_focus: None,
        },
        PanelDef {
            id: "output",
            label: "Output Panel",
            priority: 22,
            match_any: &[
                Rule { name: &["output"], ..rule(Scope::Focus) },
                Rule { name: &["output", "problems", "tasks"], ..rule(Scope::Ancestor) },
            ],
            exclude_when_focus: None,
        },
        PanelDef {
            id: "settings",
            label: "Settings / Preferences",
            priority: 30,
            match_any: &[
                Rule { name: &["settings", "preferences"], ..rule(Scope::Title) },
                Rule { name: &["settings", "preferences"], ..rule(Scope::Focus) },
                Rule { name: &["settings", "preferences"], ..rule(Scope::Ancestor) },
            ],
            exclude_when_focus: None,
        },
    ],
};

const BROWSER: Family = Family {
    id: "browser",
    display_name: "Web Browser",
    process_names: &["chrome", "msedge", "firefox", "brave", "opera", "vivaldi"],
    executable_name_contains: NONE,
    panels: &[
        PanelDef {
            id: "web-login",
            label: "Login / Auth",
            priority: 30,
            match_any: &[
                Rule { name: &["login", "signin", "sign-in", "auth", "oauth", "account"], ..rule(Scope::Url) },
                Rule { name: &["sign in", "log in", "login"], ..rule(Scope::Title) },
            ],
            exclude_when_focus: None,
        },
        PanelDef {
            id: "web-page",
            label: "Web Page",
            priority: 10,
            match_any: &[Rule { name: &["http"], ..rule(Scope::Url) }],
            exclude_when_focus: None,
        },
    ],
};

const VISUAL_STUDIO: Family = Family {
    id: "visual-studio",
    display_name: "Visual Studio",
    process_names: &["devenv"],
    executable_name_contains: NONE,
    panels: &[
        PanelDef {
            id: "editor",
            label: "Editor",
            priority: 10,
            match_any: &[Rule { control_type: &["Document"], ..rule(Scope::Focus) }],
            exclude_when_focus: None,
        },
        PanelDef {
            id: "terminal",
            label: "Terminal",
            priority: 20,
            match_any: &[Rule { name: &["terminal"], ..rule(Scope::Ancestor) }],
            exclude_when_focus: None,
        },
    ],
};

const OFFICE: Family = Family {
    id: "office",
    display_name: "Microsoft Office",
    process_names: &["WINWORD", "EXCEL", "POWERPNT", "OUTLOOK"],
    executable_name_contains: NONE,
    panels: &[PanelDef {
        id: "document",
        label: "Document",
        priority: 10,
        match_any: &[Rule { control_type: &["Document", "Edit"], ..rule(Scope::Focus) }],
        exclude_when_focus: None,
    }],
};

/// The families, in the order they are tried.
pub const FAMILIES: [Family; 4] = [ELECTRON_IDE, BROWSER, VISUAL_STUDIO, OFFICE];

fn process_token(value: &str) -> String {
    value.chars().filter(|c| c.is_alphanumeric()).collect()
}

fn process_name_matches(pattern: &str, candidate: &str) -> bool {
    if pattern.trim().is_empty() || candidate.trim().is_empty() {
        return false;
    }
    if pattern.eq_ignore_ascii_case(candidate) {
        return true;
    }
    let (p, c) = (process_token(pattern).to_lowercase(), process_token(candidate).to_lowercase());
    !p.is_empty() && !c.is_empty() && (c.contains(&p) || p.contains(&c))
}

impl Family {
    pub fn matches_process(&self, process_name: &str, process_path: Option<&str>) -> bool {
        if self.process_names.iter().any(|p| process_name_matches(p, process_name)) {
            return true;
        }
        let Some(path) = process_path.filter(|p| !p.trim().is_empty()) else { return false };
        let file = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if self.process_names.iter().any(|p| process_name_matches(p, &file)) {
            return true;
        }
        let lower = path.to_lowercase();
        self.executable_name_contains.iter().any(|hint| lower.contains(hint))
    }
}

/// The first family that claims this program.
pub fn resolve_family(process_name: &str, process_path: Option<&str>) -> Option<&'static Family> {
    static ALL: [Family; 4] = FAMILIES;
    ALL.iter().find(|f| f.matches_process(process_name, process_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ides_and_browsers_are_recognised_by_process_name() {
        for name in ["Code", "code", "Cursor", "Antigravity", "Windsurf", "VSCodium"] {
            assert_eq!(resolve_family(name, None).map(|f| f.id), Some("electron-ide"), "{name}");
        }
        for name in ["chrome", "msedge", "brave", "firefox"] {
            assert_eq!(resolve_family(name, None).map(|f| f.id), Some("browser"), "{name}");
        }
        assert_eq!(resolve_family("WINWORD", None).map(|f| f.id), Some("office"));
        assert!(resolve_family("spotify", None).is_none());
        assert!(resolve_family("", None).is_none());
    }

    #[test]
    fn a_process_name_with_spaces_or_dots_still_matches() {
        assert_eq!(resolve_family("Antigravity IDE", None).map(|f| f.id), Some("electron-ide"));
        assert_eq!(resolve_family("anti-gravity", None).map(|f| f.id), Some("electron-ide"));
    }

    #[test]
    fn the_program_path_can_identify_a_program_too() {
        let family = resolve_family("weird", Some(r"C:\Users\x\AppData\Local\Programs\cursor\Cursor.exe"));
        assert_eq!(family.map(|f| f.id), Some("electron-ide"));
        assert!(resolve_family("weird", Some(r"C:\Tools\unrelated.exe")).is_none());
    }

    #[test]
    fn the_catalog_has_the_panels_the_owners_profiles_name() {
        let ide = resolve_family("code", None).unwrap();
        for id in ["terminal", "chat", "agent", "editor", "explorer"] {
            assert!(ide.panels.iter().any(|p| p.id == id), "{id}");
        }
    }
}
