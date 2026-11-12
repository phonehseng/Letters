//! english stemming: "running", "runs" and "run" all become "run".
//!
//! this is martin porter's 1980 algorithm, written out step by step as in
//! his paper ("an algorithm for suffix stripping"). it is small, well
//! known and good enough for search: the stem is never shown to anyone,
//! it only decides which spellings count as the same word.
//!
//! words that are not plain ascii letters (other languages, numbers) are
//! returned as they are.

/// the stem of a folded (lower case, accent free) word.
pub fn stem(word: &str) -> String {
    if word.len() <= 2 || !word.bytes().all(|b| b.is_ascii_lowercase()) {
        return word.to_string();
    }
    let mut stemmer = Stemmer {
        b: word.as_bytes().to_vec(),
    };
    stemmer.step1a();
    stemmer.step1b();
    stemmer.step1c();
    stemmer.step2();
    stemmer.step3();
    stemmer.step4();
    stemmer.step5();
    String::from_utf8(stemmer.b).unwrap_or_else(|_| word.to_string())
}

struct Stemmer {
    b: Vec<u8>,
}

impl Stemmer {
    /// a consonant in porter's sense: not a, e, i, o, u, and y only after a vowel.
    fn is_consonant(&self, i: usize) -> bool {
        match self.b[i] {
            b'a' | b'e' | b'i' | b'o' | b'u' => false,
            b'y' => i == 0 || !self.is_consonant(i - 1),
            _ => true,
        }
    }

    /// the "measure" of the first `len` letters: how many vowel-consonant
    /// sequences they hold. "tree" is 0, "trouble" 1, "troubles" 2.
    fn measure(&self, len: usize) -> usize {
        let mut count = 0;
        let mut i = 0;
        while i < len && self.is_consonant(i) {
            i += 1;
        }
        loop {
            while i < len && !self.is_consonant(i) {
                i += 1;
            }
            if i >= len {
                return count;
            }
            while i < len && self.is_consonant(i) {
                i += 1;
            }
            count += 1;
        }
    }

    fn has_vowel(&self, len: usize) -> bool {
        (0..len).any(|i| !self.is_consonant(i))
    }

    fn ends_with_double_consonant(&self, len: usize) -> bool {
        len >= 2 && self.b[len - 1] == self.b[len - 2] && self.is_consonant(len - 1)
    }

    /// consonant, vowel, consonant at the end, the last not w, x or y
    /// ("hop", but not "snow").
    fn ends_cvc(&self, len: usize) -> bool {
        if len < 3 {
            return false;
        }
        let last = self.b[len - 1];
        self.is_consonant(len - 3)
            && !self.is_consonant(len - 2)
            && self.is_consonant(len - 1)
            && !matches!(last, b'w' | b'x' | b'y')
    }

    fn ends(&self, suffix: &str) -> bool {
        self.b.ends_with(suffix.as_bytes())
    }

    /// the length of the word without `suffix`.
    fn stem_len(&self, suffix: &str) -> usize {
        self.b.len() - suffix.len()
    }

    fn replace_end(&mut self, suffix: &str, with: &str) {
        let keep = self.stem_len(suffix);
        self.b.truncate(keep);
        self.b.extend_from_slice(with.as_bytes());
    }

    /// replaces the first suffix of `rules` the word ends with, when what
    /// comes before it has a measure above `min_measure`.
    fn replace_first(&mut self, rules: &[(&str, &str)], min_measure: usize) {
        for (suffix, with) in rules {
            if self.ends(suffix) {
                if self.measure(self.stem_len(suffix)) > min_measure {
                    self.replace_end(suffix, with);
                }
                return;
            }
        }
    }

    // plurals: caresses -> caress, ponies -> poni, cats -> cat
    fn step1a(&mut self) {
        if self.ends("sses") {
            self.replace_end("sses", "ss");
        } else if self.ends("ies") {
            self.replace_end("ies", "i");
        } else if self.ends("ss") {
        } else if self.ends("s") {
            self.replace_end("s", "");
        }
    }

    // past tense and -ing: agreed -> agree, hopping -> hop, filing -> file
    fn step1b(&mut self) {
        if self.ends("eed") {
            if self.measure(self.stem_len("eed")) > 0 {
                self.replace_end("eed", "ee");
            }
            return;
        }
        let suffix = if self.ends("ed") {
            "ed"
        } else if self.ends("ing") {
            "ing"
        } else {
            return;
        };
        if !self.has_vowel(self.stem_len(suffix)) {
            return;
        }
        self.replace_end(suffix, "");
        self.tidy_after_step1b();
    }

    fn tidy_after_step1b(&mut self) {
        let len = self.b.len();
        if self.ends("at") || self.ends("bl") || self.ends("iz") {
            self.b.push(b'e');
        } else if self.ends_with_double_consonant(len)
            && !matches!(self.b[len - 1], b'l' | b's' | b'z')
        {
            self.b.pop();
        } else if self.measure(len) == 1 && self.ends_cvc(len) {
            self.b.push(b'e');
        }
    }

    // happy -> happi, so it matches "happiness" after step 2
    fn step1c(&mut self) {
        if self.ends("y") && self.has_vowel(self.stem_len("y")) {
            self.replace_end("y", "i");
        }
    }

    fn step2(&mut self) {
        const RULES: &[(&str, &str)] = &[
            ("ational", "ate"),
            ("tional", "tion"),
            ("enci", "ence"),
            ("anci", "ance"),
            ("izer", "ize"),
            ("abli", "able"),
            ("alli", "al"),
            ("entli", "ent"),
            ("eli", "e"),
            ("ousli", "ous"),
            ("ization", "ize"),
            ("ation", "ate"),
            ("ator", "ate"),
            ("alism", "al"),
            ("iveness", "ive"),
            ("fulness", "ful"),
            ("ousness", "ous"),
            ("aliti", "al"),
            ("iviti", "ive"),
            ("biliti", "ble"),
        ];
        self.replace_first(RULES, 0);
    }

    fn step3(&mut self) {
        const RULES: &[(&str, &str)] = &[
            ("icate", "ic"),
            ("ative", ""),
            ("alize", "al"),
            ("iciti", "ic"),
            ("ical", "ic"),
            ("ful", ""),
            ("ness", ""),
        ];
        self.replace_first(RULES, 0);
    }

    fn step4(&mut self) {
        const SUFFIXES: &[&str] = &[
            "al", "ance", "ence", "er", "ic", "able", "ible", "ant", "ement", "ment", "ent", "ion",
            "ou", "ism", "ate", "iti", "ous", "ive", "ize",
        ];
        // longest first, so "ement" wins over "ment" and "ent"
        let mut best: Option<&str> = None;
        for suffix in SUFFIXES {
            if self.ends(suffix) && best.is_none_or(|b| suffix.len() > b.len()) {
                best = Some(suffix);
            }
        }
        let Some(suffix) = best else {
            return;
        };
        let keep = self.stem_len(suffix);
        if self.measure(keep) <= 1 {
            return;
        }
        // "-ion" goes only after s or t: adoption -> adopt, but not "lion"
        if suffix == "ion" && !(keep > 0 && matches!(self.b[keep - 1], b's' | b't')) {
            return;
        }
        self.b.truncate(keep);
    }

    fn step5(&mut self) {
        let len = self.b.len();
        if self.ends("e") {
            let m = self.measure(len - 1);
            if m > 1 || (m == 1 && !self.ends_cvc(len - 1)) {
                self.b.pop();
            }
        }
        let len = self.b.len();
        if self.measure(len) > 1 && self.ends_with_double_consonant(len) && self.ends("l") {
            self.b.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::stem;

    #[test]
    fn examples_from_porters_paper() {
        let cases = [
            ("caresses", "caress"),
            ("ponies", "poni"),
            ("cats", "cat"),
            ("feed", "feed"),
            ("agreed", "agre"),
            ("plastered", "plaster"),
            ("motoring", "motor"),
            ("sing", "sing"),
            ("conflated", "conflat"),
            ("hopping", "hop"),
            ("falling", "fall"),
            ("filing", "file"),
            ("happy", "happi"),
            ("relational", "relat"),
            ("conditional", "condit"),
            ("digitizer", "digit"),
            ("hopefulness", "hope"),
            ("adoption", "adopt"),
            ("generalization", "gener"),
            ("controlling", "control"),
            ("roll", "roll"),
        ];
        for (word, expected) in cases {
            assert_eq!(stem(word), expected, "stem of {word}");
        }
    }

    #[test]
    fn forms_of_one_word_share_a_stem() {
        let forms = [
            "connect",
            "connected",
            "connecting",
            "connection",
            "connections",
        ];
        let stems: Vec<String> = forms.iter().map(|w| stem(w)).collect();
        assert!(stems.iter().all(|s| s == "connect"), "{stems:?}");
        assert_eq!(stem("running"), stem("run"));
        assert_eq!(stem("photosynthesis"), stem("photosynthesis"));
    }

    #[test]
    fn non_english_words_are_left_alone() {
        assert_eq!(stem("привет"), "привет");
        assert_eq!(stem("2024"), "2024");
        assert_eq!(stem("is"), "is");
    }
}
