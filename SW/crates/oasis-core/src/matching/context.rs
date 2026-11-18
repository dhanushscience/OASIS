//! What the user is looking at, as the profile matcher sees it. Port of
//! `ContextTracker.ContextInfo`. The OS layer fills it in; nothing here reads the OS.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundContext {
    /// Process name without extension ("chrome", "code").
    pub app_name: String,
    /// Address-bar text when the foreground app is a browser and it could be read.
    pub url: String,
    pub window_title: String,
    /// Which part of an app has focus (an IDE's terminal, say). Empty when no panel was
    /// detected (see `matching/panel`), which makes a multi-config profile fall back to its primary config.
    pub panel_id: String,
    pub panel_label: String,
    pub app_family: String,
    /// `"none"`, `"low"`, `"medium"` or `"high"`.
    pub detection_confidence: String,
    pub focus_class_name: String,
    pub ancestor_hint: String,
}

impl Default for ForegroundContext {
    fn default() -> ForegroundContext {
        ForegroundContext {
            app_name: String::new(),
            url: String::new(),
            window_title: String::new(),
            panel_id: String::new(),
            panel_label: String::new(),
            app_family: String::new(),
            detection_confidence: "none".into(),
            focus_class_name: String::new(),
            ancestor_hint: String::new(),
        }
    }
}

impl ForegroundContext {
    /// A context with just the parts every OS can report.
    pub fn new(app_name: &str, window_title: &str, url: &str) -> ForegroundContext {
        ForegroundContext {
            app_name: app_name.into(),
            window_title: window_title.into(),
            url: url.into(),
            ..ForegroundContext::default()
        }
    }
}
