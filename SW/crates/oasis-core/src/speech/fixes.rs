//! Words the person fixed by hand right after a dictation was typed. Given the text field as it was
//! just after typing and as it is now, find each dictated word (or short phrase) that was retyped as
//! a similar one, such as "keycat" → "KiCad", so it can go into their dictionary.

use crate::matching::levenshtein;

use super::vocabulary::is_promotable;

/// A middle part longer than this is a rewrite, not a fix.
const MAX_CHANGED_WORDS: usize = 200;

/// The `(heard, written)` pairs the person fixed inside the dictated part of `before`.
pub fn fixed_words(before: &str, after: &str, dictated: &str) -> Vec<(String, String)> {
    let dictated = dictated.trim();
    let Some(start) = before.rfind(dictated).filter(|_| !dictated.is_empty()) else { return Vec::new() };
    let span = start..start + dictated.len();

    // Only the part that changed is compared word by word, so a long document stays cheap.
    let (p, s) = common_ends(before, after);
    let (mid_a, mid_b) = (&before[p..before.len() - s], &after[p..after.len() - s]);
    let (a, b) = (tokens(mid_a), tokens(mid_b));
    if a.len() > MAX_CHANGED_WORDS || b.len() > MAX_CHANGED_WORDS {
        return Vec::new();
    }

    let mut found = Vec::new();
    for (del, ins) in hunks(&a, &b) {
        if del.is_empty() || ins.is_empty() || del.len() > 3 {
            continue;
        }
        // A fix followed straight away by new typing comes as one change: keep the fix's words.
        let ins = ins.start..ins.end.min(ins.start + del.len());
        let (from_at, to_at) = (p + a[del.start].0, p + a[del.end - 1].0 + a[del.end - 1].1.len());
        if from_at < span.start || to_at > span.end {
            continue; // the person changed their own earlier text, not what was dictated
        }
        let join = |t: &[(usize, &str)]| t.iter().map(|(_, w)| *w).collect::<Vec<_>>().join(" ");
        let from = trim_marks(&join(&a[del])).to_string();
        let to = trim_marks(&join(&b[ins])).to_string();
        if from != to && is_promotable(&from, &to) && sounds_alike(&from, &to) && !found.contains(&(from.clone(), to.clone())) {
            found.push((from, to));
        }
    }
    found
}

/// Byte lengths of the shared start and end of `a` and `b`, cut back to whole words.
fn common_ends(a: &str, b: &str) -> (usize, usize) {
    let mut p = a.char_indices().zip(b.chars()).find(|((_, x), y)| x != y).map(|((i, _), _)| i).unwrap_or(a.len().min(b.len()));
    while p > 0 && !a[..p].ends_with(char::is_whitespace) {
        p = a[..p].char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
    }
    let room = a.len().min(b.len()) - p;
    let mut s = a.chars().rev().zip(b.chars().rev()).take_while(|(x, y)| x == y).map(|(c, _)| c.len_utf8()).sum::<usize>().min(room);
    while s > 0 && !a[a.len() - s..].starts_with(char::is_whitespace) {
        s -= a[a.len() - s..].chars().next().map(char::len_utf8).unwrap_or(s);
    }
    (p, s)
}

/// Words with their byte offset.
fn tokens(text: &str) -> Vec<(usize, &str)> {
    text.split_whitespace().map(|w| (w.as_ptr() as usize - text.as_ptr() as usize, w)).collect()
}

/// The replaced stretches between `a` and `b` (a word-level longest-common-subsequence diff).
fn hunks(a: &[(usize, &str)], b: &[(usize, &str)]) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i].1 == b[j].1 { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    let (mut di, mut dj) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i].1 == b[j].1 {
            if di < i || dj < j {
                out.push((di..i, dj..j));
            }
            i += 1;
            j += 1;
            (di, dj) = (i, j);
        } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            j += 1;
        } else {
            i += 1;
        }
    }
    if di < n || dj < m {
        out.push((di..n, dj..m));
    }
    out
}

fn trim_marks(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric())
}

/// A fix of a misheard word looks like it: retyping "cat" as "dog" is a change of mind instead.
fn sounds_alike(from: &str, to: &str) -> bool {
    let (a, b) = (from.to_lowercase(), to.to_lowercase());
    let longer = a.chars().count().max(b.chars().count());
    levenshtein(&a, &b) * 10 <= longer * 6
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(before: &str, after: &str, dictated: &str) -> Vec<(String, String)> {
        fixed_words(before, after, dictated)
    }

    #[test]
    fn a_misheard_word_retyped_is_found() {
        let d = "Open keycat and the board. ";
        assert_eq!(fix(&format!("Notes: {d}"), "Notes: Open KiCad and the board. ", d), [("keycat".to_string(), "KiCad".to_string())]);
    }

    #[test]
    fn a_fix_next_to_new_typing_is_still_found() {
        let d = "send it to jon ";
        assert_eq!(fix(d, "send it to John and Mary, thanks", d), [("jon".to_string(), "John".to_string())]);
    }

    #[test]
    fn plain_typing_after_and_changes_of_mind_are_not_learned() {
        let d = "the cat sat ";
        assert!(fix(d, "the cat sat on the mat", d).is_empty());
        assert!(fix(d, "the dog sat ", d).is_empty());
        assert!(fix(d, "the Cat sat ", d).is_empty()); // only capitals
        assert!(fix(d, "", d).is_empty());
    }

    #[test]
    fn a_change_outside_the_dictated_part_is_left_alone() {
        let d = "dictated words";
        assert!(fix("teh start dictated words", "the start dictated words", d).is_empty());
    }

    #[test]
    fn the_text_not_being_found_means_nothing_to_learn() {
        assert!(fix("something else", "something elze", "not there").is_empty());
    }

    #[test]
    fn non_ascii_text_does_not_break_the_cut() {
        let d = "café keycat ";
        assert_eq!(fix(d, "café KiCad ", d), [("keycat".to_string(), "KiCad".to_string())]);
    }
}
