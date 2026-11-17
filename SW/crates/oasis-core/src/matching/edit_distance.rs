//! Port of `Matching/EditDistance.cs`.
//!
//! Exact, case-sensitive Levenshtein distance, used by app-launch discovery, the spoken
//! app-name resolver and the voice router. Works on Unicode scalar values (`char`); the C#
//! version works on UTF-16 code units. The two agree for everything outside rare astral-plane
//! characters, which do not appear in app names.

/// Insertions, deletions and substitutions needed to turn `a` into `b`.
pub fn levenshtein(a: &str, b: &str) -> usize {
    levenshtein_bounded(a, b, usize::MAX)
}

/// As [`levenshtein`], but each input is cut to `max_len` characters first, to bound the work
/// on long window titles.
pub fn levenshtein_bounded(a: &str, b: &str, max_len: usize) -> usize {
    let a: Vec<char> = a.chars().take(max_len).collect();
    let b: Vec<char> = b.chars().take(max_len).collect();

    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let mut costs: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut last = costs[0];
        costs[0] = i;
        for j in 1..=b.len() {
            let temp = costs[j];
            let cost = usize::from(a[i - 1] != b[j - 1]);
            costs[j] = (costs[j] + 1).min(costs[j - 1] + 1).min(last + cost);
            last = temp;
        }
    }
    costs[b.len()]
}

/// 1.0 for equal strings down to 0.0 for nothing in common; 0.0 when either is empty.
pub fn similarity(a: &str, b: &str) -> f64 {
    let (la, lb) = (a.chars().count(), b.chars().count());
    if la == 0 || lb == 0 {
        return 0.0;
    }
    1.0 - levenshtein(a, b) as f64 / la.max(lb) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_the_number_of_single_character_edits() {
        for (a, b, expected) in [
            ("", "", 0),
            ("abc", "", 3),
            ("", "abc", 3),
            ("kicad", "kicad", 0),
            ("keycad", "kicad", 2),
            ("kitten", "sitting", 3),
            ("flaw", "lawn", 2),
        ] {
            assert_eq!(levenshtein(a, b), expected, "{a} -> {b}");
            assert_eq!(levenshtein(b, a), expected, "{b} -> {a}");
        }
    }

    #[test]
    fn comparison_is_case_sensitive() {
        assert_eq!(levenshtein("Spotify", "spotify"), 1);
    }

    #[test]
    fn long_inputs_are_cut_before_they_are_compared() {
        let a = format!("{}x", "a".repeat(100));
        let b = format!("{}y", "a".repeat(100));
        assert_eq!(levenshtein(&a, &b), 1);
        assert_eq!(levenshtein_bounded(&a, &b, 48), 0);
    }

    #[test]
    fn similarity_runs_from_zero_to_one() {
        for (a, b, expected) in [
            ("", "abc", 0.0),
            ("abc", "", 0.0),
            ("abc", "abc", 1.0),
            ("abcd", "abcf", 0.75),
            ("abc", "xyz", 0.0),
        ] {
            assert!((similarity(a, b) - expected).abs() < 1e-9, "{a} {b}");
        }
    }
}
