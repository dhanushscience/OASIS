//! Port of `Services/DisplayLabelFormatter.cs`: the label shown for a button must fit the
//! overlay's pill (14 characters). It is shown in capitals; a longer one is abbreviated word by
//! word, then trimmed, as a last resort.

/// The room a label has on the overlay.
pub const DEFAULT_MAX_LENGTH: usize = 14;

/// Words that carry little meaning and are dropped first.
const LOW_VALUE_WORDS: [&str; 24] = [
    "A", "AN", "THE", "FOR", "TO", "OF", "IN", "ON", "AT", "BY", "WITH", "AND", "OR", "USE",
    "USING", "SETUP", "SET", "UP", "FROM", "INTO", "YOUR", "THIS", "THAT", "VIA",
];

const ABBREVIATIONS: [(&str, &str); 19] = [
    ("PROTOTYPE", "PROTO"),
    ("AUTOMATION", "AUTO"),
    ("AUTOMATE", "AUTO"),
    ("INFORMATION", "INFO"),
    ("DOCUMENTATION", "DOCS"),
    ("IMPLEMENTATION", "IMPL"),
    ("IMPLEMENT", "IMPL"),
    ("CONFIGURATION", "CONFIG"),
    ("CONFIGURE", "CONFIG"),
    ("APPLICATION", "APP"),
    ("WORKFLOW", "FLOW"),
    ("DISPLAY", "DISP"),
    ("WINDOW", "WIN"),
    ("DETECTION", "DETECT"),
    ("MANAGEMENT", "MGMT"),
    ("MANAGE", "MGMT"),
    ("SETTINGS", "SET"),
    ("PROFILE", "PROF"),
    ("PROFILES", "PROF"),
];

/// `"-"` for a blank label, otherwise the label in capitals, shortened to `max_length`.
pub fn fit(label: &str, max_length: usize) -> String {
    if label.trim().is_empty() {
        return "-".to_string();
    }
    let normalised = label.to_uppercase().trim().replace(['_', '/'], " ");
    if normalised.chars().count() <= max_length {
        normalised
    } else {
        fit_single(&normalised, max_length)
    }
}

fn fit_single(label: &str, max_length: usize) -> String {
    let words: Vec<&str> = label.split([' ', '-']).filter(|w| !w.is_empty()).collect();
    match words.as_slice() {
        [] => return "-".to_string(),
        [only] => return fit_word(only, max_length),
        _ => {}
    }

    let abbreviated: Vec<&str> = words.iter().map(|w| abbreviate(w)).collect();
    let compact = join_distinct(&abbreviated);
    if compact.chars().count() <= max_length {
        return compact;
    }

    let mut kept: Vec<&str> =
        abbreviated.iter().copied().filter(|w| !LOW_VALUE_WORDS.contains(w)).collect();
    if kept.is_empty() {
        kept = vec![abbreviated[abbreviated.len() - 1]];
    }
    let compact = join_distinct(&kept);
    if compact.chars().count() <= max_length {
        return compact;
    }

    // As many of the last words as fit.
    for keep in (1..=kept.len()).rev() {
        let compact = join_distinct(&kept[kept.len() - keep..]);
        if compact.chars().count() <= max_length {
            return compact;
        }
    }

    if kept.len() >= 2 {
        let acronym: String = kept[..kept.len() - 1].iter().filter_map(|w| w.chars().next()).collect();
        let last = kept[kept.len() - 1];
        let compact = if acronym.is_empty() { last.to_string() } else { format!("{acronym} {last}") };
        if compact.chars().count() <= max_length {
            return compact;
        }
    }
    fit_word(kept[kept.len() - 1], max_length)
}

fn abbreviate(word: &str) -> &str {
    ABBREVIATIONS.iter().find(|(long, _)| *long == word).map_or(word, |(_, short)| short)
}

fn join_distinct(words: &[&str]) -> String {
    let mut distinct: Vec<&str> = Vec::with_capacity(words.len());
    for word in words {
        if distinct.last() != Some(word) {
            distinct.push(word);
        }
    }
    distinct.join(" ")
}

fn fit_word(word: &str, max_length: usize) -> String {
    word.chars().take(max_length).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit14(label: &str) -> String {
        fit(label, DEFAULT_MAX_LENGTH)
    }

    #[test]
    fn a_short_label_is_just_put_in_capitals() {
        assert_eq!(fit14("copy"), "COPY");
        assert_eq!(fit14("  vs code  "), "VS CODE");
        assert_eq!(fit14("Show_Desk/now"), "SHOW DESK NOW");
        // Too long: the leading word goes, as in C#.
        assert_eq!(fit14("Show_Desktop/now"), "DESKTOP NOW");
        // Hyphens are kept when the label already fits.
        assert_eq!(fit14("vs-code"), "VS-CODE");
    }

    #[test]
    fn a_blank_label_is_a_dash() {
        assert_eq!(fit14(""), "-");
        assert_eq!(fit14("   "), "-");
    }

    #[test]
    fn a_long_single_word_is_cut() {
        assert_eq!(fit14("internationalization"), "INTERNATIONALI");
    }

    #[test]
    fn long_words_are_abbreviated_and_low_value_words_dropped() {
        assert_eq!(fit14("documentation setup for the project"), "DOCS PROJECT");
        assert_eq!(fit14("open configuration"), "OPEN CONFIG");
    }

    #[test]
    fn when_nothing_else_works_the_last_words_that_fit_are_kept() {
        assert_eq!(fit14("visual studio code insiders edition"), "EDITION");
    }

    #[test]
    fn repeated_neighbouring_words_collapse_after_abbreviating() {
        // WINDOW becomes WIN twice in a row, and the repeat is dropped.
        assert_eq!(fit14("window window manager"), "WIN MANAGER");
    }

    #[test]
    fn the_limit_counts_characters_not_bytes() {
        let fifteen = "é".repeat(15);
        assert_eq!(fit14(&fifteen).chars().count(), 14);
    }

    #[test]
    fn a_label_exactly_at_the_limit_is_untouched_and_one_over_is_shortened() {
        assert_eq!(fit14("exactly fourte"), "EXACTLY FOURTE");
        assert_eq!(fit14("exactly fourtee"), "FOURTEE");
    }

    #[test]
    fn a_smaller_budget_is_honoured() {
        assert_eq!(fit("desktop", 4), "DESK");
    }
}
