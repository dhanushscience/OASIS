//! Shared tokenization for cleanup. Port of `TranscriptText.cs` (the parts cleanup uses).

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '%'
}

/// Words only: runs of letters, digits, `'` and `%`.
pub fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.trim().chars() {
        if is_word_char(c) {
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Whitespace-separated tokens with punctuation still attached ("Kee-cadd," is one token).
pub fn tokens(text: &str) -> Vec<String> {
    collapse_whitespace(text).split(' ').filter(|t| !t.is_empty()).map(str::to_string).collect()
}

/// Comparison key: the token's word characters, lower-cased. Empty for pure punctuation.
pub fn token_key(token: &str) -> String {
    token.chars().filter(|c| is_word_char(*c)).flat_map(char::to_lowercase).collect()
}

/// Punctuation after the last word character: "no," → ",", "right?" → "?".
pub fn trailing_punctuation(token: &str) -> &str {
    let end = token.char_indices().rev().find(|(_, c)| is_word_char(*c)).map_or(0, |(i, c)| i + c.len_utf8());
    &token[end..]
}

/// Replace a token's trailing punctuation, keeping its word characters as written.
pub fn with_trailing_punctuation(token: &str, punctuation: &str) -> String {
    let keep = token.len() - trailing_punctuation(token).len();
    format!("{}{}", &token[..keep], punctuation)
}

/// True when the token closes a sentence: "done.", "right?". The full stop of an abbreviation
/// ("p.m.", "e.g.", "I.T.") belongs to the word and does not count.
pub fn ends_sentence(token: &str) -> bool {
    let trail = trailing_punctuation(token);
    if trail.contains(['!', '?']) {
        return true;
    }
    if !trail.contains('.') {
        return false;
    }
    let word = &token[..token.len() - trail.len()];
    !word.contains('.') || word.chars().any(|c| c.is_ascii_digit())
}

/// Trim, join runs of whitespace to one space, and drop a space before closing punctuation.
pub fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending = false;
    let mut started = false;
    for c in text.trim().chars() {
        if c.is_whitespace() {
            pending = started;
            continue;
        }
        if pending && !is_punctuation_attach(c) {
            out.push(' ');
        }
        pending = false;
        started = true;
        out.push(c);
    }
    out
}

/// Lower-case for matching. Falls back to ASCII-only folding when full lower-casing would change
/// the byte length, so offsets found in the folded copy stay valid in the original.
pub fn fold_case(text: &str) -> String {
    let lower = text.to_lowercase();
    if lower.len() == text.len() { lower } else { text.to_ascii_lowercase() }
}

/// [`collapse_whitespace`] per line, for text that layout already broke into lines. Blank lines
/// survive one at a time (a paragraph break).
pub fn collapse_whitespace_keep_lines(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(text.len());
    let mut blank = false;
    for line in text.split('\n') {
        let collapsed = collapse_whitespace(line);
        if collapsed.is_empty() {
            blank = !out.is_empty();
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank { "\n\n" } else { "\n" });
        }
        out.push_str(&collapsed);
        blank = false;
    }
    out
}

fn is_punctuation_attach(c: char) -> bool {
    matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '\'')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_collapses_and_punctuation_attaches() {
        assert_eq!(collapse_whitespace("  hello   world ,  ok "), "hello world, ok");
        assert_eq!(collapse_whitespace("   "), "");
    }

    #[test]
    fn token_helpers() {
        assert_eq!(tokens("Kee-cadd, right?"), vec!["Kee-cadd,", "right?"]);
        assert_eq!(token_key("Don't,"), "don't");
        assert_eq!(token_key("..."), "");
        assert_eq!(trailing_punctuation("no,"), ",");
        assert_eq!(trailing_punctuation("no"), "");
        assert_eq!(with_trailing_punctuation("no,", "."), "no.");
        assert_eq!(words("Hi, it's 50% off"), vec!["Hi", "it's", "50%", "off"]);
    }
}
