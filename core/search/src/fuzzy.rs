//! typo tolerance: how many single-letter changes turn one word into another.
//!
//! "photosyntesis" is one letter away from "photosynthesis". short words
//! get no tolerance at all, because almost every three letter word is one
//! change away from dozens of others ("cat", "car", "hat"...).

/// how many typos a typed word may have and still match, by its length.
pub fn allowed_typos(word: &str) -> usize {
    match word.chars().count() {
        0..=4 => 0,
        5..=8 => 1,
        _ => 2,
    }
}

/// the edit distance between `a` and `b` (insert, delete or change one
/// letter, or swap two neighbours), or none when it is more than `limit`.
/// it gives up as soon as the limit can no longer be met, which is what
/// keeps checking a whole dictionary fast.
pub fn distance_within(a: &str, b: &str, limit: usize) -> Option<usize> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > limit {
        return None;
    }
    // three rows of the classic table are enough for swaps of neighbours
    let width = b.len() + 1;
    let mut before: Vec<usize> = vec![0; width];
    let mut previous: Vec<usize> = (0..width).collect();
    let mut current = vec![0; width];
    for i in 1..=a.len() {
        current[0] = i;
        let mut row_best = current[0];
        for j in 1..=b.len() {
            let change = usize::from(a[i - 1] != b[j - 1]);
            let mut cost = (previous[j] + 1)
                .min(current[j - 1] + 1)
                .min(previous[j - 1] + change);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cost = cost.min(before[j - 2] + 1);
            }
            current[j] = cost;
            row_best = row_best.min(cost);
        }
        if row_best > limit {
            return None;
        }
        std::mem::swap(&mut before, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }
    let result = previous[b.len()];
    (result <= limit).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_single_letter_changes() {
        assert_eq!(
            distance_within("photosyntesis", "photosynthesis", 2),
            Some(1)
        );
        assert_eq!(distance_within("kitten", "sitting", 3), Some(3));
        assert_eq!(distance_within("kitten", "sitting", 2), None);
        assert_eq!(distance_within("same", "same", 0), Some(0));
    }

    #[test]
    fn a_swap_of_neighbours_is_one_typo() {
        assert_eq!(distance_within("teh", "the", 1), Some(1));
        assert_eq!(distance_within("recieve", "receive", 1), Some(1));
    }

    #[test]
    fn short_words_get_no_tolerance() {
        assert_eq!(allowed_typos("cat"), 0);
        assert_eq!(allowed_typos("ocean"), 1);
        assert_eq!(allowed_typos("chlorophyll"), 2);
    }

    #[test]
    fn far_apart_lengths_give_up_at_once() {
        assert_eq!(distance_within("a", "abcdef", 2), None);
    }
}
