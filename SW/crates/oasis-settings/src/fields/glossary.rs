//! The dictionary box (`spoken = written` lines) under Voice, and adding one word to it.

use super::*;

pub(super) const MAX_GLOSSARY_ROWS: usize = 200;

pub(super) const MAX_GLOSSARY_TEXT: usize = 60;

/// A replacement can be a whole address or signature line.
pub(super) const MAX_REPLACEMENT_TEXT: usize = 200;

/// The saved glossary as editable lines: `spoken = written`.
pub(super) fn glossary_text(settings: &Settings, key: &str) -> String {
    let rows = settings.extra.get("Voice").and_then(|v| v.get(key)).and_then(Value::as_array);
    let text = |row: &Value, key: &str| row.get(key).and_then(Value::as_str).map(str::trim).unwrap_or("").to_string();
    rows.into_iter()
        .flatten()
        .map(|r| (text(r, "Spoken"), text(r, "Canonical")))
        .filter(|(s, c)| !s.is_empty() && !c.is_empty())
        .map(|(s, c)| if s == c { c } else { format!("{s} = {c}") })
        .collect::<Vec<_>>()
        .join("
")
}

/// Add one spelling to the dictionary, in place of an older one for the same spoken words.
/// An empty `spoken` adds the word on its own: it is heard the same as it is written.
pub fn add_glossary_word(settings: &mut Settings, spoken: &str, written: &str) -> Result<(), EditError> {
    let written = written.trim();
    let spoken = if spoken.trim().is_empty() { written } else { spoken.trim() };
    if spoken.contains('=') || written.contains('=') {
        return bad("A dictionary word cannot hold an = sign.");
    }
    let same = |line: &str| line.split('=').next().is_some_and(|s| s.trim().eq_ignore_ascii_case(spoken));
    let mut lines: Vec<String> = glossary_text(settings, "DictationGlossary").lines().filter(|l| !same(l)).map(String::from).collect();
    lines.push(if spoken == written { written.to_string() } else { format!("{spoken} = {written}") });
    set_field(settings, "voice.dictation_glossary", json!(lines.join("\n")), &FieldContext::default())
}

/// The lines typed in the box as the list to save. Blank lines are skipped; any other line must be
/// `spoken = written` (or, with `bare_ok`, a word on its own, heard as it is written), short, and the
/// whole list not long. Nothing is saved if a line is refused.
pub(super) fn glossary_rows(text: &str, label: &str, max: usize, bare_ok: bool) -> Result<Value, EditError> {
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let pair = match line.split_once('=') {
            Some((s, w)) => Some((s.trim(), w.trim())),
            None if bare_ok => Some((line.trim(), line.trim())),
            None => None,
        };
        let Some((spoken, written)) = pair else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::test_settings::*;

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
    fn a_word_fixed_by_hand_joins_the_glossary_in_place_of_an_older_one() {
        let mut s = Settings::default();
        set(&mut s, "voice.dictation_glossary", json!("keycat = Kicad\nfoo = Foo")).unwrap();
        add_glossary_word(&mut s, "Keycat", "KiCad").unwrap();
        assert_eq!(field(&s, "voice.dictation_glossary").value, json!("foo = Foo\nKeycat = KiCad"));
        assert!(add_glossary_word(&mut s, "a=b", "c").is_err());
    }

    #[test]
    fn a_dictionary_word_can_stand_alone_but_a_fragment_cannot() {
        let mut s = Settings::default();
        set(&mut s, "voice.dictation_glossary", json!("Shashank
kay cad = KiCad")).unwrap();
        assert_eq!(field(&s, "voice.dictation_glossary").value, json!("Shashank
kay cad = KiCad"));
        assert_eq!(crate::voice::VoiceSettings::from(&s).dictation_glossary[0], ("Shashank".to_string(), "Shashank".to_string()));
        add_glossary_word(&mut s, "", "Tim").unwrap();
        assert_eq!(field(&s, "voice.dictation_glossary").value, json!("Shashank
kay cad = KiCad
Tim"));
        assert!(set(&mut s, "voice.replacements", json!("my mail")).is_err());
    }

    #[test]
    fn a_bad_glossary_line_changes_nothing() {
        let mut s = Settings::default();
        set(&mut s, "voice.dictation_glossary", json!("a = b")).unwrap();
        for bad in ["= written", "spoken =", &format!("{} = x", "y".repeat(61))] {
            assert!(set(&mut s, "voice.dictation_glossary", json!(bad)).is_err(), "{bad}");
        }
        assert!(set(&mut s, "voice.dictation_glossary", json!(5)).is_err());
        assert_eq!(field(&s, "voice.dictation_glossary").value, json!("a = b"));
    }
}
