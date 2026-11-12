//! turning the font names stored in pdfs ("TimesNewRomanPS-BoldMT") into
//! the family names people and programs know ("Times New Roman").

/// the family a pdf font name belongs to.
pub(crate) fn family_name(pdf_name: &str) -> String {
    // a subset tag like "ABCDEF+" says only some glyphs are stored
    let name = match pdf_name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.chars().all(|c| c.is_ascii_uppercase()) => rest,
        _ => pdf_name,
    };
    // the style follows a dash or a comma: "Arial-BoldMT", "Arial,Bold"
    let base = name.split(['-', ',']).next().unwrap_or(name);
    let base = base
        .trim_end_matches("PSMT")
        .trim_end_matches("MT")
        .trim_end_matches("PS");
    let mut words = camel_case_words(base);
    while words
        .last()
        .is_some_and(|word| matches!(word.as_str(), "Bold" | "Italic" | "Oblique" | "Regular"))
        && words.len() > 1
    {
        words.pop();
    }
    let family = words.join(" ");
    if family.is_empty() {
        name.to_string()
    } else {
        family
    }
}

/// "TimesNewRoman" becomes ["Times", "New", "Roman"]; runs of capitals
/// stay together ("SegoeUI" is "Segoe UI").
fn camel_case_words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut previous: Option<char> = None;
    for c in name.chars() {
        let starts_word = match previous {
            None => true,
            Some(p) => (p.is_lowercase() && c.is_uppercase()) || c == ' ' || c == '_',
        };
        if c == ' ' || c == '_' {
            previous = Some(c);
            words.push(String::new());
            continue;
        }
        if starts_word || words.is_empty() {
            words.push(String::new());
        }
        words.last_mut().expect("just added").push(c);
        previous = Some(c);
    }
    words.retain(|word| !word.is_empty());
    words
}

/// a family office programs on windows and macos have, for the pdf
/// standard fonts that they lack: same widths, so lines keep their length.
pub(crate) fn office_family(family: &str) -> &str {
    match family {
        "Helvetica" | "Arial" => "Arial",
        "Times" | "Times Roman" | "Times New Roman" => "Times New Roman",
        "Courier" | "Courier New" => "Courier New",
        "Symbol" => "Symbol",
        "Zapf Dingbats" | "ZapfDingbats" => "Wingdings",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_and_vendor_parts_are_dropped() {
        assert_eq!(family_name("Arial-BoldMT"), "Arial");
        assert_eq!(
            family_name("TimesNewRomanPS-BoldItalicMT"),
            "Times New Roman"
        );
        assert_eq!(family_name("TimesNewRomanPSMT"), "Times New Roman");
        assert_eq!(family_name("ABCDEF+Calibri-Bold"), "Calibri");
        assert_eq!(family_name("Helvetica-Oblique"), "Helvetica");
        assert_eq!(family_name("Times-Roman"), "Times");
        assert_eq!(family_name("SegoeUI"), "Segoe UI");
        assert_eq!(family_name("Arial,Bold"), "Arial");
        assert_eq!(family_name("CalibriBold"), "Calibri");
    }

    #[test]
    fn standard_fonts_get_office_equivalents() {
        assert_eq!(office_family("Helvetica"), "Arial");
        assert_eq!(office_family("Times"), "Times New Roman");
        assert_eq!(office_family("Garamond"), "Garamond");
    }
}
