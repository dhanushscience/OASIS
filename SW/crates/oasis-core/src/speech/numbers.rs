//! Dictated number words to digits: "six thousand five hundred" â†’ "6500", "seventy five
//! percent" â†’ "75%", "twenty twenty six" â†’ "2026", "three point five" â†’ "3.5", "two pm" â†’
//! "2 pm". Port of `SpokenNumbers.cs` and `SpokenNumbers.Reading.cs`.
//!
//! Numbers above ten become digits. Zero to ten said alone stay words when they count things
//! ("three pizzas", "two hours"), the usual writing style; "one" also as a pronoun ("this one",
//! "no one", "one of them"). They become digits when something marks them as a figure â€” a measure,
//! am/pm, percent, a label before it ("phase one"), or a digit next to it ("1, 2, 3"). Compounds
//! keep their words too: "three-button", "one-to-one". Commands keep their number words; the
//! command parsers read numbers themselves.

use super::text::{collapse_whitespace, token_key, tokens, trailing_punctuation};

const SMALL: [&str; 20] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
];
const TENS: [&str; 8] = ["twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
const ORDINALS: [(&str, i64); 28] = [
    ("first", 1), ("second", 2), ("third", 3), ("fourth", 4), ("fifth", 5), ("sixth", 6), ("seventh", 7),
    ("eighth", 8), ("ninth", 9), ("tenth", 10), ("eleventh", 11), ("twelfth", 12), ("thirteenth", 13),
    ("fourteenth", 14), ("fifteenth", 15), ("sixteenth", 16), ("seventeenth", 17), ("eighteenth", 18),
    ("nineteenth", 19), ("twentieth", 20), ("thirtieth", 30), ("fortieth", 40), ("fiftieth", 50),
    ("sixtieth", 60), ("seventieth", 70), ("eightieth", 80), ("ninetieth", 90), ("zeroth", 0),
];

/// Kept as a word after the figure: "75 million", "2 lakh".
const SOFT_SCALES: [&str; 7] = ["million", "billion", "trillion", "lakh", "lakhs", "crore", "crores"];
const MERIDIEMS: [&str; 3] = ["am", "pm", "o'clock"];

/// Units of time read as prose after "one" ("one day", "one second"), so "one" stays.
const TIME_UNITS: &[&str] = &[
    "second", "seconds", "minute", "minutes", "min", "mins", "hour", "hours", "hr", "hrs", "day", "days",
    "week", "weeks", "month", "months", "year", "years", "millisecond", "milliseconds", "ms", "times",
];

const MEASURE_UNITS: &[&str] = &[
    "percent", "dollar", "dollars", "rupee", "rupees", "euro", "euros", "pound", "pounds", "cent", "cents",
    "paise", "kg", "kilogram", "kilograms", "gram", "grams", "km", "kilometer", "kilometers", "kilometre",
    "kilometres", "meter", "meters", "metre", "metres", "cm", "mm", "centimeter", "centimeters",
    "millimeter", "millimeters", "mile", "miles", "inch", "inches", "foot", "feet", "gb", "mb", "kb", "tb",
    "gigabyte", "gigabytes", "megabyte", "megabytes", "volt", "volts", "amp", "amps", "watt", "watts", "hz",
    "hertz", "ohm", "ohms", "degree", "degrees", "pixel", "pixels", "px", "milli", "micro", "kilo", "mega",
    "milliamp", "milliamps", "microamp", "microamps", "kilohertz", "megahertz", "kilobytes", "kilobits", "rpm",
];

/// A noun that numbers what follows: "phase two", "button one", "step three".
const LABELS: &[&str] = &[
    "phase", "step", "version", "chapter", "page", "section", "part", "level", "option", "number", "item",
    "line", "question", "figure", "table", "button", "btn", "slide", "row", "column", "stage", "task",
    "test", "grade", "season", "episode", "room", "track", "channel", "port", "pin", "layer", "slot",
    "rule",
];

/// Short durations read as measures: "after 10 seconds", "every 30 minutes" (but "two hours").
const SHORT_DURATIONS: &[&str] = &["second", "seconds", "sec", "secs", "minute", "minutes", "min", "mins", "millisecond", "milliseconds", "ms"];

/// How many words away a figure still pulls a lone "one" into digits.
const FIGURE_REACH: usize = 3;

/// After one of these, "one" is a pronoun: "this one", "no one", "the other one".
const PRONOUN_LEADS: &[&str] =
    &["this", "that", "the", "which", "no", "any", "every", "each", "some", "another", "other", "only"];

const MONTHS: [&str; 12] = [
    "january", "february", "march", "april", "may", "june", "july", "august", "september", "october",
    "november", "december",
];

pub(crate) fn is_month(key: &str) -> bool {
    MONTHS.contains(&key)
}

fn small(word: &str) -> Option<i64> {
    SMALL.iter().position(|w| *w == word).map(|i| i as i64)
}

fn tens(word: &str) -> Option<i64> {
    TENS.iter().position(|w| *w == word).map(|i| 20 + 10 * i as i64)
}

fn ordinal(word: &str) -> Option<i64> {
    ORDINALS.iter().find(|(w, _)| *w == word).map(|(_, v)| *v)
}

fn in_set(set: &[&str], key: &str) -> bool {
    !key.is_empty() && set.contains(&key)
}

/// Cardinal number word ("two", "seventy", "hundred"); never "a", "and" or an ordinal.
pub fn is_number_word(key: &str) -> bool {
    small(key).is_some() || tens(key).is_some() || key == "hundred" || key == "thousand"
}

/// "pm", "minutes", "percent", "million": after a number these make it a quantity.
pub(crate) fn is_unit_word(key: &str) -> bool {
    in_set(&MERIDIEMS, key) || in_set(TIME_UNITS, key) || in_set(MEASURE_UNITS, key) || in_set(&SOFT_SCALES, key)
}

#[derive(Default, Clone)]
struct Piece {
    first: usize,
    last: usize,
    digits: String,
    value: i64,
    /// One spoken zero to ten on its own; converted only with supporting context.
    lone: bool,
    ordinal: bool,
    /// Hour and minutes of a two-part run ("five fifteen"), shown as 5:15 next to am/pm or "at".
    clock: Option<String>,
    /// Rest of a hyphenated token: "-dollar" in "twenty-dollar".
    suffix: String,
    /// Several figures run together ("nineteen ninety nine"); has no single value.
    joined: bool,
    convert: bool,
}

struct Atom {
    word: String,
    token: usize,
}

struct Segment {
    value: i64,
    digits: String,
    first: usize,
    last: usize,
    scaled: bool,
    ordinal: bool,
    oh: bool,
}

/// Opening punctuation, the word itself, closing punctuation: "(two," â†’ "(", "two", ",".
pub(crate) fn split(token: &str) -> (&str, &str, &str) {
    let Some(start) = token.char_indices().find(|(_, c)| c.is_alphanumeric()).map(|(i, _)| i) else {
        return (token, "", "");
    };
    let trail = trailing_punctuation(&token[start..]);
    (&token[..start], &token[start..token.len() - trail.len()], trail)
}

/// Convert number words in `text` to digits where that reads right.
pub fn apply(text: &str) -> String {
    let normalized = collapse_whitespace(text);
    let toks = tokens(&normalized);
    if toks.is_empty() {
        return normalized;
    }

    let mut pieces: Vec<Piece> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let read = read_phrase(&toks, i, &mut pieces);
        i = if read > i { read } else { i + 1 };
    }

    decide(&toks, &mut pieces);
    let (mut output, mut changed) = render(&toks, &pieces);
    changed |= join_percent(&mut output);
    if changed { output.join(" ") } else { normalized }
}

fn decide(toks: &[String], pieces: &mut [Piece]) {
    for piece in pieces.iter_mut() {
        let (_, _, last_trail) = split(&toks[piece.last]);
        let open = last_trail.is_empty() && piece.suffix.is_empty();
        let next = if open && piece.last + 1 < toks.len() { token_key(&toks[piece.last + 1]) } else { String::new() };
        let prev = if piece.first > 0 && split(&toks[piece.first - 1]).2.is_empty() {
            token_key(&toks[piece.first - 1])
        } else {
            String::new()
        };

        if piece.clock.is_some() && (in_set(&MERIDIEMS, &next) || prev == "at") {
            piece.digits = piece.clock.clone().unwrap_or_default();
        }
        if !piece.convert {
            continue;
        }
        if piece.ordinal {
            // "the third option" reads better spelled out; "the 21st" does not.
            piece.convert = piece.value >= 10;
            continue;
        }
        if !piece.lone {
            continue;
        }
        // Above ten, a figure wherever it stands; ten and under need a reason.
        piece.convert = piece.suffix.is_empty()
            && (piece.value > 10
                || in_set(&MERIDIEMS, &next)
                || prev == "at" && piece.value >= 1
                || (piece.value >= 2 && in_set(SHORT_DURATIONS, &next))
                || in_set(&SOFT_SCALES, &next)
                || in_set(MEASURE_UNITS, &next)
                || in_set(LABELS, &prev));
    }

    // "one or 2", "1, 2, 3", "one, maybe 2": next to a figure, "one" is a figure too. Mixed
    // "one â€¦ 2" in one breath reads as a mistake; far apart it is just prose.
    let mut figure: Vec<bool> =
        toks.iter().map(|t| split(t).1.chars().next().is_some_and(|c| c.is_ascii_digit())).collect();
    for piece in pieces.iter().filter(|p| p.convert) {
        for f in &mut figure[piece.first..=piece.last] {
            *f = true;
        }
    }

    for piece in pieces.iter_mut() {
        if piece.convert || !piece.lone || !piece.suffix.is_empty() {
            continue;
        }
        let lead = if piece.first > 0 { token_key(&toks[piece.first - 1]) } else { String::new() };
        let follow = if piece.last + 1 < toks.len() { token_key(&toks[piece.last + 1]) } else { String::new() };
        let one = piece.value <= 1;
        if (one && PRONOUN_LEADS.contains(&lead.as_str()))
            || (one && matches!(follow.as_str(), "of" | "more" | "thing" | "new"))
            || in_set(TIME_UNITS, &follow)
        {
            continue;
        }
        let from = piece.first.saturating_sub(FIGURE_REACH);
        let to = (piece.last + FIGURE_REACH).min(toks.len() - 1);
        if (from..=to).any(|t| figure[t]) {
            piece.convert = true;
            for f in &mut figure[piece.first..=piece.last] {
                *f = true;
            }
        }
    }
}

fn render(toks: &[String], pieces: &[Piece]) -> (Vec<String>, bool) {
    let mut changed = false;
    let mut output = Vec::with_capacity(toks.len());
    let mut p = 0;
    let mut i = 0;
    while i < toks.len() {
        if p < pieces.len() && pieces[p].first == i {
            let piece = &pieces[p];
            p += 1;
            if piece.convert {
                output.push(format!(
                    "{}{}{}{}",
                    split(&toks[piece.first]).0,
                    piece.digits,
                    piece.suffix,
                    split(&toks[piece.last]).2
                ));
                changed = true;
            } else {
                output.extend(toks[piece.first..=piece.last].iter().cloned());
            }
            i = piece.last + 1;
            continue;
        }
        output.push(toks[i].clone());
        i += 1;
    }
    (output, changed)
}

/// "75 percent" â†’ "75%", also for digits the model wrote itself.
fn join_percent(output: &mut Vec<String>) -> bool {
    let mut changed = false;
    let mut i = 1;
    while i < output.len() {
        let (lead, core, trail) = split(&output[i]);
        if !lead.is_empty() || !core.eq_ignore_ascii_case("percent") {
            i += 1;
            continue;
        }
        if !output[i - 1].chars().next_back().is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
            continue;
        }
        let trail = trail.to_string();
        output[i - 1].push('%');
        output[i - 1].push_str(&trail);
        output.remove(i);
        changed = true;
    }
    changed
}

fn group(value: i64) -> String {
    let s = value.to_string();
    if value < 10_000 {
        return s;
    }
    let (sign, digits) = s.strip_prefix('-').map_or(("", s.as_str()), |d| ("-", d));
    let mut out = String::new();
    for (n, c) in digits.chars().enumerate() {
        if n > 0 && (digits.len() - n) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{sign}{out}")
}

fn ordinal_suffix(value: i64) -> &'static str {
    if (11..=13).contains(&(value % 100)) {
        return "th";
    }
    match value % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

// ---- Reading: find a run of number words and turn it into figures. ----

/// Reads the number phrase starting at `start`; returns the next unread token.
fn read_phrase(toks: &[String], start: usize, pieces: &mut Vec<Piece>) -> usize {
    let mut atoms: Vec<Atom> = Vec::new();
    let mut suffix = String::new();
    let mut j = start;
    while j < toks.len() {
        let (lead, core, trail) = split(&toks[j]);
        if j > start && !lead.is_empty() {
            break;
        }
        let lowered = core.to_lowercase();
        let parts: Vec<&str> = lowered.split('-').collect();
        let mut numeric = 0;
        while numeric < parts.len() && is_atom(&parts, numeric, &atoms, toks, start, j) {
            numeric += 1;
        }

        if numeric == 0 {
            if parts.len() != 1 || !trail.is_empty() || !is_glue(parts[0], &atoms, toks, j) {
                break;
            }
            atoms.push(Atom { word: parts[0].to_string(), token: j });
            j += 1;
            continue;
        }

        for part in &parts[..numeric] {
            atoms.push(Atom { word: (*part).to_string(), token: j });
        }
        j += 1;
        if numeric < parts.len() {
            // "twenty-dollar": the figure ends inside the token.
            let rest = parts[numeric..].join("-");
            let cut = lowered.len().saturating_sub(rest.len() + 1);
            suffix = lowered.get(cut..).unwrap_or("").to_string();
            break;
        }
        if !trail.is_empty() || ordinal(parts[numeric - 1]).is_some() {
            break;
        }
    }

    if atoms.iter().all(|a| !is_number_word(&a.word) && ordinal(&a.word).is_none()) {
        return start;
    }

    let Some(segments) = read_segments(&atoms) else { return j };
    let mut built = merge(segments);
    // A token split across two figures cannot be rewritten; leave the phrase as spoken.
    for k in 1..built.len() {
        if built[k].first <= built[k - 1].last {
            return j;
        }
    }
    if let Some(last) = built.last_mut() {
        last.suffix = suffix.clone();
        if suffix.is_empty() {
            j = j.max(read_decimal(toks, last));
        }
    }
    pieces.extend(built);
    j
}

fn is_atom(parts: &[&str], index: usize, atoms: &[Atom], toks: &[String], start: usize, token: usize) -> bool {
    let word = parts[index];
    if is_number_word(word) {
        return true;
    }
    if ordinal(word).is_none() {
        return false;
    }
    if word != "second" {
        return true;
    }
    // "thirty second" is 30 s far more often than the 32nd; only a date-like frame makes it one.
    let after_tens = if index > 0 {
        tens(parts[index - 1]).is_some()
    } else {
        atoms.last().is_some_and(|a| tens(&a.word).is_some())
    };
    if !after_tens {
        return false;
    }
    let before = if start > 0 { token_key(&toks[start - 1]) } else { String::new() };
    let after = if token + 1 < toks.len() { token_key(&toks[token + 1]) } else { String::new() };
    before == "the" || after == "of" || is_month(&before) || is_month(&after)
}

/// "a hundred", "two hundred and five", "nineteen oh five": words that only count inside a number.
fn is_glue(word: &str, atoms: &[Atom], toks: &[String], token: usize) -> bool {
    if token + 1 >= toks.len() {
        return false;
    }
    let (next_lead, next_core, _) = split(&toks[token + 1]);
    if !next_lead.is_empty() {
        return false;
    }
    let lowered = next_core.to_lowercase();
    let next = lowered.split('-').next().unwrap_or("");
    match word {
        "a" | "an" => atoms.is_empty() && matches!(next, "hundred" | "thousand"),
        "and" => {
            atoms.last().is_some_and(|a| matches!(a.word.as_str(), "hundred" | "thousand"))
                && (small(next).is_some() || tens(next).is_some())
        }
        "oh" => {
            atoms.last().is_some_and(|a| is_number_word(&a.word)) && small(next).is_some_and(|d| d <= 9)
        }
        _ => false,
    }
}

fn read_segments(atoms: &[Atom]) -> Option<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut a = 0;
    while a < atoms.len() {
        if atoms[a].word == "oh" {
            segments.push(Segment {
                value: 0,
                digits: "0".into(),
                first: atoms[a].token,
                last: atoms[a].token,
                scaled: false,
                ordinal: false,
                oh: true,
            });
            a += 1;
            continue;
        }
        let first = a;
        let (value, scaled, is_ordinal) = try_read_cardinal(atoms, &mut a)?;
        segments.push(Segment {
            value,
            digits: value.to_string(),
            first: atoms[first].token,
            last: atoms[a - 1].token,
            scaled,
            ordinal: is_ordinal,
            oh: false,
        });
    }
    Some(segments)
}

/// Returns (value, scaled, ordinal).
fn try_read_cardinal(atoms: &[Atom], a: &mut usize) -> Option<(i64, bool, bool)> {
    let start = *a;
    let mut scaled = false;
    let mut is_ordinal = false;
    let mut value;
    if matches!(atoms[*a].word.as_str(), "a" | "an") {
        *a += 1;
        value = 1;
    } else if matches!(atoms[*a].word.as_str(), "hundred" | "thousand") {
        value = 1;
    } else {
        let (v, o) = try_read_below_hundred(atoms, a)?;
        value = v;
        is_ordinal = o;
    }

    if !is_ordinal && *a < atoms.len() && atoms[*a].word == "hundred" {
        *a += 1;
        value = value.saturating_mul(100);
        scaled = true;
        value = value.saturating_add(read_tail(atoms, a, &mut is_ordinal));
    }

    if !is_ordinal && *a < atoms.len() && atoms[*a].word == "thousand" {
        *a += 1;
        value = value.saturating_mul(1000);
        scaled = true;
        let mark = *a;
        let mut rest = read_tail(atoms, a, &mut is_ordinal);
        if !is_ordinal && *a > mark && *a < atoms.len() && atoms[*a].word == "hundred" {
            *a += 1;
            rest = rest.saturating_mul(100).saturating_add(read_tail(atoms, a, &mut is_ordinal));
        }
        value = value.saturating_add(rest);
    }

    let ok = *a > start && (scaled || !matches!(atoms[start].word.as_str(), "a" | "an"));
    ok.then_some((value, scaled, is_ordinal))
}

/// Optional "and" plus a number under 100 after a scale word; 0 when nothing follows.
fn read_tail(atoms: &[Atom], a: &mut usize, is_ordinal: &mut bool) -> i64 {
    let mark = *a;
    if *a < atoms.len() && atoms[*a].word == "and" {
        *a += 1;
    }
    if *a < atoms.len() {
        if let Some((tail, o)) = try_read_below_hundred(atoms, a) {
            *is_ordinal = o;
            return tail;
        }
        *is_ordinal = false;
    }
    *a = mark;
    0
}

/// Returns (value, ordinal).
fn try_read_below_hundred(atoms: &[Atom], a: &mut usize) -> Option<(i64, bool)> {
    if *a >= atoms.len() {
        return None;
    }
    let word = atoms[*a].word.as_str();
    if let Some(t) = tens(word) {
        *a += 1;
        let mut value = t;
        let mut is_ordinal = false;
        if *a < atoms.len() {
            let next = atoms[*a].word.as_str();
            if let Some(unit) = small(next).filter(|u| (1..=9).contains(u)) {
                value += unit;
                *a += 1;
            } else if let Some(nth) = ordinal(next).filter(|n| *n <= 9) {
                value += nth;
                is_ordinal = true;
                *a += 1;
            }
        }
        return Some((value, is_ordinal));
    }
    if let Some(v) = small(word) {
        *a += 1;
        return Some((v, false));
    }
    if let Some(v) = ordinal(word) {
        *a += 1;
        return Some((v, true));
    }
    None
}

/// Figures said side by side without a scale word are one string of digits: "nineteen ninety
/// nine" â†’ 1999, "eighty eighty" â†’ 8080, "nine one one" â†’ 911. Two lone digits ("one two") are
/// too little to go on and stay separate.
fn merge(segments: Vec<Segment>) -> Vec<Piece> {
    let joinable = |s: &Segment| !s.scaled && !s.ordinal && s.value < 100;
    let mut pieces = Vec::new();
    let mut i = 0;
    while i < segments.len() {
        let mut end = i;
        while end < segments.len() && joinable(&segments[end]) {
            end += 1;
        }
        let run = end - i;
        let two_lone_digits = run == 2 && segments[i].value <= 9 && segments[i + 1].value <= 9;
        if run >= 2 && !two_lone_digits {
            let digits: String = segments[i..end].iter().map(|s| s.digits.as_str()).collect();
            let head_len = segments[i].digits.len();
            let minutes = &digits[head_len..];
            let mut clock = None;
            if (1..=12).contains(&segments[i].value)
                && !segments[i].oh
                && minutes.len() == 2
                && minutes.parse::<i64>().is_ok_and(|m| m <= 59)
            {
                clock = Some(format!("{}:{}", segments[i].digits, minutes));
            }
            pieces.push(Piece {
                first: segments[i].first,
                last: segments[end - 1].last,
                digits,
                joined: true,
                convert: true,
                clock,
                ..Piece::default()
            });
            i = end;
            continue;
        }
        let s = &segments[i];
        pieces.push(Piece {
            first: s.first,
            last: s.last,
            value: s.value,
            digits: if s.ordinal { format!("{}{}", s.digits, ordinal_suffix(s.value)) } else { group(s.value) },
            lone: s.value <= 10 && !s.scaled,
            ordinal: s.ordinal,
            // A stray "oh" is the exclamation, never a zero.
            convert: !s.oh,
            ..Piece::default()
        });
        i += 1;
    }
    pieces
}

/// "three point one four" â†’ 3.14. Returns the next unread token.
fn read_decimal(toks: &[String], piece: &mut Piece) -> usize {
    if piece.ordinal || piece.joined || !piece.convert {
        return piece.last + 1;
    }
    if !split(&toks[piece.last]).2.is_empty() || piece.last + 2 >= toks.len() {
        return piece.last + 1;
    }
    if !toks[piece.last + 1].eq_ignore_ascii_case("point") {
        return piece.last + 1;
    }

    let mut fraction = String::new();
    let mut j = piece.last + 2;
    while j < toks.len() {
        let (lead, core, trail) = split(&toks[j]);
        let word = core.to_lowercase();
        let digit = if word == "oh" { Some(0) } else { small(&word).filter(|d| *d <= 9) };
        let Some(digit) = digit.filter(|_| lead.is_empty()) else { break };
        fraction.push_str(&digit.to_string());
        j += 1;
        if !trail.is_empty() {
            break;
        }
    }
    if fraction.is_empty() {
        return piece.last + 1;
    }
    piece.digits = format!("{}.{}", piece.value, fraction);
    piece.last = j - 1;
    piece.lone = false;
    piece.clock = None;
    j
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(cases: &[(&str, &str)]) {
        for (raw, expected) in cases {
            assert_eq!(apply(raw), *expected, "{raw}");
        }
    }

    #[test]
    fn number_words_become_digits() {
        check(&[
            ("the total amount is six thousand five hundred", "the total amount is 6500"),
            ("bring the total to two thousand four hundred.", "bring the total to 2400."),
            ("scored seventy-five million dollars.", "scored 75 million dollars."),
            ("Hundred percent greatness.", "100% greatness."),
            ("Set brightness seventy five.", "Set brightness 75."),
            ("one hundred and five people", "105 people"),
            ("fifty thousand rupees", "50,000 rupees"),
            ("If I have ten minutes I should be able to explain.", "If I have 10 minutes I should be able to explain."),
            ("ten out of ten", "ten out of ten"),
            ("it is 50 percent done", "it is 50% done"),
        ]);
    }

    #[test]
    fn figures_said_side_by_side_are_one_string_of_digits() {
        check(&[
            ("twenty twenty six", "2026"),
            ("back in nineteen ninety nine", "back in 1999"),
            ("port eighty eighty", "port 8080"),
            ("call nine nine eight seven", "call 9987"),
            ("error four oh four", "error 404"),
        ]);
    }

    #[test]
    fn small_numbers_are_digits_when_they_measure() {
        check(&[
            ("Tuesday two pm", "Tuesday 2 pm"),
            ("Wednesday three p.m.", "Wednesday 3 p.m."),
            ("meet at five fifteen", "meet at 5:15"),
            ("seven thirty am works", "7:30 am works"),
            ("three point five volts", "3.5 volts"),
            ("wait two minutes", "wait 2 minutes"),
            ("Commit Phase One.", "Commit Phase 1."),
            ("press button two", "press button 2"),
            ("five to ten people", "five to ten people"),
            ("the twenty first of May", "the 21st of May"),
            ("I know what these three buttons do.", "I know what these three buttons do."),
            ("One, two, three, four.", "One, two, three, four."),
            ("oh five of them left", "oh five of them left"),
            ("a thirty-second dictation", "a 30-second dictation"),
        ]);
    }

    #[test]
    fn a_lone_one_stays_a_word() {
        for raw in [
            "a three-button macro pad",
            "One second click will do the action.",
            "an exact one-to-one replica",
            "If I have one minute I should be able to explain.",
            "this one is better and no one came",
            "For example, one of the first things that we have to do",
            "the first and second time",
            "It's 3.5 volts, not 5.",
        ] {
            assert_eq!(apply(raw), raw);
        }
    }

    #[test]
    fn counting_prose_and_digit_helpers() {
        check(&[
            ("One, two, three, go!", "One, two, three, go!"),
            ("I have one, maybe two, cars.", "I have one, maybe two, cars."), ("one or 12 cars", "1 or 12 cars"),
            ("One, I do not know.", "One, I do not know."),
            ("One thing I noticed, two of the buttons are swapped.", "One thing I noticed, two of the buttons are swapped."),
        ]);
        assert_eq!(group(1_234_567), "1,234,567");
        assert_eq!(group(9999), "9999");
        assert_eq!(ordinal_suffix(12), "th");
        assert_eq!(ordinal_suffix(21), "st");
        assert!(is_number_word("seventy") && !is_number_word("and") && !is_number_word("first"));
    }
}
