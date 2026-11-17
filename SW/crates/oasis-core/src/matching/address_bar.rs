//! Picking the address out of a browser window's text fields. Port of the selection rules in
//! `ContextTracker.GetBrowserUrl`.
//!
//! The OS layer (UI Automation on Windows) lists the window's edit fields with their accessible
//! names and current text. Which one is the address bar is decided here, the same on every OS.

use super::text::{contains_ignore_case, starts_with_ignore_case};

/// Words an address bar's accessible name contains, in any browser or language we know of
/// (Chrome "Address and search bar", Firefox "Search with Google or enter address", Edge
/// "Address and search bar", Brave "Address and search bar", Opera "Address field").
const NAME_HINTS: [&str; 5] = ["address", "search", "url", "omnibox", "location"];

/// Whether an edit field's accessible name could be a browser's address bar. The OS layer asks
/// this before reading a field's text, so it never reads the text of a page's own text boxes.
pub fn is_address_bar_name(name: &str) -> bool {
    NAME_HINTS.iter().any(|hint| contains_ignore_case(name, hint))
}

/// The first edit field that looks like the address bar, as its trimmed text.
///
/// A field qualifies if its name contains one of the hints and its text is non-empty and looks
/// like a page address (a dot, a `://`, or `localhost`): not the text of an ordinary search box.
pub fn pick_address<'a>(fields: impl IntoIterator<Item = (&'a str, &'a str)>) -> Option<String> {
    fields.into_iter().find_map(|(name, value)| {
        if !is_address_bar_name(name) {
            return None;
        }
        let value = value.trim();
        let looks_like_address = value.contains('.')
            || value.contains("://")
            || starts_with_ignore_case(value, "localhost");
        (!value.is_empty() && looks_like_address).then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(fields: &[(&str, &str)]) -> Option<String> {
        pick_address(fields.iter().copied())
    }

    #[test]
    fn takes_the_address_bar_by_its_name() {
        assert_eq!(
            pick(&[("Address and search bar", "netflix.com/browse")]).as_deref(),
            Some("netflix.com/browse")
        );
        assert_eq!(pick(&[("Omnibox", "https://a.org/x")]).as_deref(), Some("https://a.org/x"));
        assert_eq!(pick(&[("Location", "localhost:3000")]).as_deref(), Some("localhost:3000"));
    }

    #[test]
    fn names_are_matched_ignoring_case() {
        assert_eq!(pick(&[("ADDRESS FIELD", "a.com")]).as_deref(), Some("a.com"));
    }

    #[test]
    fn other_edit_fields_are_ignored() {
        // A page's own text box, even with an address-like value.
        assert_eq!(pick(&[("Comment", "see example.com")]), None);
        assert_eq!(pick(&[("Find in page", "x.y")]), None);
    }

    #[test]
    fn a_search_box_phrase_is_not_an_address() {
        // The omnibox holds what the user typed while typing; that is not a page.
        assert_eq!(pick(&[("Address and search bar", "how to cook rice")]), None);
    }

    #[test]
    fn empty_and_blank_values_are_skipped_and_the_next_field_is_tried() {
        assert_eq!(
            pick(&[("Address bar", ""), ("Search", "  "), ("URL", "  b.org/p  ")]).as_deref(),
            Some("b.org/p")
        );
    }

    #[test]
    fn the_first_qualifying_field_wins() {
        assert_eq!(
            pick(&[("Address", "first.com"), ("Search", "second.com")]).as_deref(),
            Some("first.com")
        );
    }

    #[test]
    fn nothing_found_is_none() {
        assert_eq!(pick(&[]), None);
    }
}
