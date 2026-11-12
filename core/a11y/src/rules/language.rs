//! the document's language, the languages of parts of it, and text in a
//! different alphabet that isn't marked with its language.

use std::collections::BTreeMap;

use crate::content::Mark;
use crate::objects::get_text;
use crate::report::{FixKind, Location};
use crate::rules::{Collector, Context};

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    let document_lang = ctx
        .doc
        .catalog()
        .ok()
        .and_then(|catalog| get_text(ctx.doc, catalog, b"Lang"))
        .map(|lang| lang.trim().to_owned())
        .filter(|lang| !lang.is_empty());
    match &document_lang {
        None => {
            out.report(
                "missing-language",
                "The catalog has no /Lang entry.",
                Location::document(),
                Some(FixKind::SetLanguage { suggested: None }),
            );
        }
        Some(lang) if !is_valid_language_tag(lang) => {
            out.report(
                "invalid-language",
                format!("The document’s language is set to “{lang}”."),
                Location::document(),
                Some(FixKind::SetLanguage { suggested: None }),
            );
        }
        Some(_) => {}
    }
    check_part_languages(ctx, out);
    if ctx.tagged {
        check_unmarked_changes(ctx, document_lang.as_deref(), out);
    } else {
        super::skip_untagged(out, &["language-change-unmarked"]);
    }
}

/// languages set on tags or on marked content must be valid too.
fn check_part_languages(ctx: &Context, out: &mut Collector) {
    if let Some(tree) = &ctx.tree {
        for (index, element) in tree.elements.iter().enumerate() {
            if let Some(lang) = element.lang.as_deref().map(str::trim) {
                if !lang.is_empty() && !is_valid_language_tag(lang) {
                    out.report(
                        "invalid-language",
                        format!(
                            "A {} tag has its language set to “{lang}”.",
                            element.raw_type
                        ),
                        ctx.element_location(index),
                        None,
                    );
                }
            }
        }
    }
    for (page, content) in ctx.contents() {
        let bad = content.texts.iter().find(|run| {
            run.lang
                .as_deref()
                .is_some_and(|l| !l.is_empty() && !is_valid_language_tag(l))
        });
        if let Some(run) = bad {
            out.report(
                "invalid-language",
                format!(
                    "Text on page {} has its language set to “{}”.",
                    page + 1,
                    run.lang.as_deref().unwrap_or_default()
                ),
                Location::page(page, Some(run.rect)),
                None,
            );
        }
    }
}

/// finds tagged text in another alphabet whose tag says no language,
/// grouped by the tag that holds it.
fn check_unmarked_changes(ctx: &Context, document_lang: Option<&str>, out: &mut Collector) {
    let Some(tree) = &ctx.tree else {
        return;
    };
    let owners = tree.content_owners();
    // element index → (text, page)
    let mut suspicious: BTreeMap<usize, (String, u32)> = BTreeMap::new();
    for (page, content) in ctx.contents() {
        let page_id = ctx.pages[page as usize].id;
        for run in &content.texts {
            let Mark::Mcid(mcid) = run.mark else {
                continue;
            };
            if run.in_form || !run.unicode_reliable || run.lang.is_some() {
                continue;
            }
            let Some(&owner) = owners.get(&(page_id, mcid)) else {
                continue;
            };
            let lang = tree.language_of(owner).or(document_lang);
            let Some(expected) = lang.and_then(expected_scripts) else {
                continue;
            };
            if let Some(script) = dominant_script(&run.text) {
                if !expected.contains(&script) {
                    let entry = suspicious
                        .entry(owner)
                        .or_insert_with(|| (String::new(), page));
                    entry.0.push_str(&run.text);
                }
            }
        }
    }
    for (owner, (text, _page)) in suspicious.into_iter().take(50) {
        let sample: String = text.chars().take(40).collect();
        out.report(
            "language-change-unmarked",
            format!(
                "“{}” is in a different alphabet from the document’s language.",
                sample.trim()
            ),
            ctx.element_location(owner),
            None,
        );
    }
}

/// writing systems, told apart by character ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Script {
    Latin,
    Greek,
    Cyrillic,
    Arabic,
    Hebrew,
    Han,
    Kana,
    Hangul,
    Devanagari,
    Thai,
}

fn script_of(c: char) -> Option<Script> {
    let code = c as u32;
    let script = match code {
        0x41..=0x5A | 0x61..=0x7A | 0xC0..=0x24F => Script::Latin,
        0x370..=0x3FF | 0x1F00..=0x1FFF => Script::Greek,
        0x400..=0x52F => Script::Cyrillic,
        0x590..=0x5FF => Script::Hebrew,
        0x600..=0x6FF | 0x750..=0x77F => Script::Arabic,
        0x900..=0x97F => Script::Devanagari,
        0xE00..=0xE7F => Script::Thai,
        0x3040..=0x30FF => Script::Kana,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF => Script::Han,
        0xAC00..=0xD7AF | 0x1100..=0x11FF => Script::Hangul,
        _ => return None,
    };
    Some(script)
}

/// the alphabet most letters of a text are in, when there are enough
/// letters to tell.
pub(crate) fn dominant_script(text: &str) -> Option<Script> {
    let mut counts: BTreeMap<Script, usize> = BTreeMap::new();
    let mut letters = 0;
    for script in text.chars().filter_map(script_of) {
        *counts.entry(script).or_default() += 1;
        letters += 1;
    }
    let (script, count) = counts.into_iter().max_by_key(|(_, count)| *count)?;
    // single symbols like π in a formula are not a change of language
    (count >= 3 && count * 2 > letters).then_some(script)
}

/// the alphabets text in a language is normally written in, for the
/// languages this check knows. unknown languages are not checked.
fn expected_scripts(lang: &str) -> Option<&'static [Script]> {
    let primary = lang.split('-').next()?.to_ascii_lowercase();
    let scripts: &'static [Script] = match primary.as_str() {
        "en" | "fr" | "de" | "es" | "it" | "pt" | "nl" | "sv" | "da" | "no" | "nb" | "nn"
        | "fi" | "pl" | "cs" | "sk" | "hu" | "ro" | "tr" | "id" | "ms" | "vi" | "ca" | "hr"
        | "sl" | "et" | "lv" | "lt" | "ga" | "cy" | "is" | "sq" | "eu" | "gl" | "af" | "sw"
        | "tl" | "la" => &[Script::Latin],
        "el" => &[Script::Greek],
        "ru" | "uk" | "bg" | "be" | "mk" | "kk" => &[Script::Cyrillic],
        "sr" => &[Script::Cyrillic, Script::Latin],
        "ar" | "fa" | "ur" | "ps" => &[Script::Arabic],
        "he" | "yi" => &[Script::Hebrew],
        "zh" => &[Script::Han],
        "ja" => &[Script::Han, Script::Kana],
        "ko" => &[Script::Hangul, Script::Han],
        "hi" | "mr" | "ne" | "sa" => &[Script::Devanagari],
        "th" => &[Script::Thai],
        _ => return None,
    };
    Some(scripts)
}

/// whether a language code has the shape of a bcp 47 tag, like en,
/// en-US, zh-Hans-CN, sr-Latn or x-private.
pub(crate) fn is_valid_language_tag(tag: &str) -> bool {
    let subtags: Vec<&str> = tag.split('-').collect();
    let alpha = |s: &str, range: std::ops::RangeInclusive<usize>| {
        range.contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphabetic())
    };
    let alnum = |s: &str, range: std::ops::RangeInclusive<usize>| {
        range.contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric())
    };
    let Some(first) = subtags.first() else {
        return false;
    };
    // private use and the old "i-" registrations
    if first.eq_ignore_ascii_case("x") || first.eq_ignore_ascii_case("i") {
        return subtags.len() > 1 && subtags[1..].iter().all(|s| alnum(s, 1..=8));
    }
    // the grammar also allows 4 to 8 letter languages, but none are
    // registered, and "english" is a far likelier mistake than a real code
    if !alpha(first, 2..=3) {
        return false;
    }
    let mut index = 1;
    // up to three extended language subtags
    let mut extlangs = 0;
    while extlangs < 3 && index < subtags.len() && alpha(subtags[index], 3..=3) {
        index += 1;
        extlangs += 1;
    }
    if index < subtags.len() && alpha(subtags[index], 4..=4) {
        index += 1;
    }
    if index < subtags.len()
        && (alpha(subtags[index], 2..=2)
            || (subtags[index].len() == 3 && subtags[index].chars().all(|c| c.is_ascii_digit())))
    {
        index += 1;
    }
    while index < subtags.len() {
        let s = subtags[index];
        let digit_variant =
            s.len() == 4 && s.starts_with(|c: char| c.is_ascii_digit()) && alnum(s, 4..=4);
        if alnum(s, 5..=8) || digit_variant {
            index += 1;
        } else {
            break;
        }
    }
    // extensions: a single letter followed by subtags of 2 to 8
    while index < subtags.len()
        && subtags[index].len() == 1
        && !subtags[index].eq_ignore_ascii_case("x")
    {
        if !alnum(subtags[index], 1..=1) {
            return false;
        }
        index += 1;
        let start = index;
        while index < subtags.len() && alnum(subtags[index], 2..=8) {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    if index < subtags.len() && subtags[index].eq_ignore_ascii_case("x") {
        index += 1;
        let start = index;
        while index < subtags.len() && alnum(subtags[index], 1..=8) {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    index == subtags.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_language_codes_are_valid() {
        for tag in [
            "en",
            "en-US",
            "fr",
            "zh-Hans",
            "zh-Hans-CN",
            "sr-Latn-RS",
            "de-CH-1996",
            "es-419",
            "x-klingon",
            "en-US-x-twain",
        ] {
            assert!(is_valid_language_tag(tag), "{tag}");
        }
    }

    #[test]
    fn malformed_language_codes_are_invalid() {
        for tag in [
            "", "english", "en_US", "e", "en-", "-en", "en-US-", "123", "en-a",
        ] {
            assert!(!is_valid_language_tag(tag), "{tag}");
        }
    }

    #[test]
    fn the_main_alphabet_of_a_text_is_found() {
        assert_eq!(dominant_script("Hello world"), Some(Script::Latin));
        assert_eq!(dominant_script("Καλημέρα κόσμε"), Some(Script::Greek));
        assert_eq!(dominant_script("Привет"), Some(Script::Cyrillic));
        assert_eq!(dominant_script("π = 3.14"), None);
    }
}
