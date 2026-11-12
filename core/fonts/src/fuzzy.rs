//! forgiving name matching: "helv" finds Helvetica, "garmond" finds
//! Garamond, "timesnew" finds Times New Roman.

/// lower case letters and digits only, so spacing and dashes don't matter.
pub fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// the words of a name, lower case.
pub fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// the optimal string alignment distance (Damerau-Levenshtein where a
/// swap of two neighbours counts as one edit), stopping early once it
/// exceeds `limit`.
pub fn edit_distance(a: &str, b: &str, limit: usize) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > limit {
        return limit + 1;
    }
    let width = b.len() + 1;
    let mut rows = vec![vec![0usize; width]; a.len() + 1];
    for (j, cell) in rows[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        rows[i][0] = i;
        let mut best = rows[i][0];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut value = (rows[i - 1][j] + 1)
                .min(rows[i][j - 1] + 1)
                .min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                value = value.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = value;
            best = best.min(value);
        }
        if best > limit {
            return limit + 1;
        }
    }
    rows[a.len()][b.len()]
}

/// how many typos a typed word of this length may have.
fn allowed_typos(length: usize) -> usize {
    match length {
        0..=3 => 0,
        // seven letters still get one typo: two would let "garmond" find
        // "HarmonyOS" as well as Garamond
        4..=7 => 1,
        _ => 2,
    }
}

/// how well one typed word matches a name, 0 for no match. higher is
/// better: the whole name, then the start of the name, then the start of
/// a word, then anywhere inside, then the same word with typos.
pub fn word_score(typed: &str, name: &str) -> u32 {
    let typed = squash(typed);
    if typed.is_empty() {
        return 0;
    }
    let whole = squash(name);
    if whole == typed {
        return 1000;
    }
    if whole.starts_with(&typed) {
        return 800 + (100 * typed.len() / whole.len().max(1)) as u32;
    }
    let name_words = words(name);
    if name_words.iter().any(|word| word.starts_with(&typed)) {
        return 600;
    }
    if whole.contains(&typed) {
        return 400;
    }
    let typos = allowed_typos(typed.chars().count());
    if typos == 0 {
        return 0;
    }
    // a typo in one word, or in the start of a word typed only partly
    let best = name_words
        .iter()
        .flat_map(|word| {
            let prefix: String = word.chars().take(typed.chars().count()).collect();
            [
                edit_distance(&typed, word, typos),
                edit_distance(&typed, &prefix, typos),
            ]
        })
        .chain(std::iter::once(edit_distance(&typed, &whole, typos)))
        .min()
        .unwrap_or(typos + 1);
    if best <= typos {
        300 - 60 * best as u32
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_count_swaps_once() {
        assert_eq!(edit_distance("garmond", "garamond", 2), 1);
        assert_eq!(edit_distance("arail", "arial", 2), 1);
        assert_eq!(edit_distance("abc", "abc", 0), 0);
        assert_eq!(edit_distance("short", "something long", 2), 3);
    }

    #[test]
    fn matches_rank_whole_then_prefix_then_typos() {
        assert_eq!(word_score("arial", "Arial"), 1000);
        assert!(word_score("helv", "Helvetica") > 800);
        assert_eq!(word_score("roman", "Times New Roman"), 600);
        assert_eq!(
            word_score("timesnew", "Times New Roman"),
            800 + 100 * 8 / 13
        );
        assert_eq!(word_score("garmond", "EB Garamond"), 240);
        assert_eq!(word_score("calbri", "Calibri"), 240);
        assert_eq!(word_score("xyz", "Arial"), 0);
        // short words get no typos, or everything would match
        assert_eq!(word_score("sas", "Sans"), 0);
    }
}
