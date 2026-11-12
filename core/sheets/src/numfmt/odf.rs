//! opendocument data styles (number:number-style, number:date-style...)
//! converted to and from excel format codes, so a format survives going
//! between ods and xlsx. the common shapes are covered: decimals,
//! thousands, percent, scientific, fractions, currency, text around the
//! number, colours, dates and times, and sections chosen by sign.

use super::parse::{parse, DatePart, Section, Token};
use crate::xml::{escape_attribute, escape_text, Event, Reader};

/// the data style elements for an excel code: one per extra section,
/// then the main one (named `name`) with maps to the others
pub fn excel_to_odf(code: &str, name: &str) -> String {
    let sections = parse(code);
    let mut out = String::new();
    let mut maps = String::new();
    for (index, section) in sections.iter().enumerate().skip(1) {
        let part_name = format!("{name}P{index}");
        out.push_str(&section_xml(section, &part_name, &[]));
        let condition = match (index, sections.len()) {
            (1, _) => "value()&lt;0",
            (2, _) => "value()=0",
            _ => continue,
        };
        maps.push_str(&format!(
            "<style:map style:condition=\"{condition}\" style:apply-style-name=\"{part_name}\"/>"
        ));
    }
    let first = sections.first().cloned().unwrap_or_default();
    out.push_str(&section_xml(&first, name, &[maps]));
    out
}

fn element_for(section: &Section) -> &'static str {
    if section.is_date() {
        let has_date = section
            .tokens
            .iter()
            .any(|t| matches!(t, Token::Date(p) if is_date_part(*p)));
        return if has_date {
            "number:date-style"
        } else {
            "number:time-style"
        };
    }
    if section.has(&Token::Text) {
        return "number:text-style";
    }
    if section.has(&Token::Percent) {
        return "number:percentage-style";
    }
    let currency = section
        .tokens
        .iter()
        .any(|t| matches!(t, Token::Literal(l) if is_currency(l)));
    if currency {
        "number:currency-style"
    } else {
        "number:number-style"
    }
}

fn is_date_part(part: DatePart) -> bool {
    use DatePart::*;
    matches!(
        part,
        Year2
            | Year4
            | Month
            | Month2
            | MonthShort
            | MonthLong
            | MonthLetter
            | Day
            | Day2
            | DayShort
            | DayLong
    )
}

fn is_currency(text: &str) -> bool {
    ["$", "€", "£", "¥"]
        .iter()
        .any(|symbol| text.contains(symbol))
}

fn section_xml(section: &Section, name: &str, extra: &[String]) -> String {
    let element = element_for(section);
    let mut body = String::new();
    if let Some(color) = &section.color {
        body.push_str(&format!(
            "<style:text-properties fo:color=\"{}\"/>",
            css_color(color)
        ));
    }
    if section.is_date() {
        body.push_str(&date_body(section));
    } else {
        body.push_str(&number_body(section, element));
    }
    for piece in extra {
        body.push_str(piece);
    }
    let elapsed = section.tokens.iter().any(|t| {
        matches!(
            t,
            Token::Date(
                DatePart::ElapsedHours(_)
                    | DatePart::ElapsedMinutes(_)
                    | DatePart::ElapsedSeconds(_)
            )
        )
    });
    let truncate = if elapsed {
        " number:truncate-on-overflow=\"false\""
    } else {
        ""
    };
    format!(
        "<{element} style:name=\"{}\"{truncate}>{body}</{element}>",
        escape_attribute(name)
    )
}

fn css_color(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "red" => "#ff0000",
        "blue" => "#0000ff",
        "green" => "#00ff00",
        "yellow" => "#ffff00",
        "magenta" => "#ff00ff",
        "cyan" => "#00ffff",
        "white" => "#ffffff",
        _ => "#000000",
    }
}

fn text_element(text: &str) -> String {
    format!("<number:text>{}</number:text>", escape_text(text))
}

/// literals before and after the number, and the number element itself
fn number_body(section: &Section, element: &str) -> String {
    let tokens = &section.tokens;
    let first_digit = tokens.iter().position(|t| matches!(t, Token::Digit(_)));
    let last_digit = tokens
        .iter()
        .rposition(|t| matches!(t, Token::Digit(_) | Token::DecimalPoint));
    let (Some(first_digit), Some(last_digit)) = (first_digit, last_digit) else {
        return literal_run(tokens, element);
    };
    let mut out = literal_run(&tokens[..first_digit], element);
    out.push_str(&number_element(
        &tokens[first_digit..=last_digit.max(first_digit)],
        tokens,
    ));
    out.push_str(&literal_run(&tokens[last_digit + 1..], element));
    out
}

fn literal_run(tokens: &[Token], element: &str) -> String {
    let mut out = String::new();
    let mut pending = String::new();
    for token in tokens {
        match token {
            Token::Literal(text) if element == "number:currency-style" && is_currency(text) => {
                if !pending.is_empty() {
                    out.push_str(&text_element(&std::mem::take(&mut pending)));
                }
                out.push_str(&format!(
                    "<number:currency-symbol>{}</number:currency-symbol>",
                    escape_text(text)
                ));
            }
            Token::Literal(text) => pending.push_str(text),
            Token::Space => pending.push(' '),
            Token::Percent => pending.push('%'),
            Token::Text => {
                if !pending.is_empty() {
                    out.push_str(&text_element(&std::mem::take(&mut pending)));
                }
                out.push_str("<number:text-content/>");
            }
            _ => {}
        }
    }
    if !pending.is_empty() {
        out.push_str(&text_element(&pending));
    }
    out
}

/// the number:number, number:scientific-number or number:fraction element
fn number_element(number_tokens: &[Token], all: &[Token]) -> String {
    // the mantissa ends where an exponent starts
    let mantissa_end = number_tokens
        .iter()
        .position(|t| matches!(t, Token::Exponent { .. }))
        .unwrap_or(number_tokens.len());
    let number_tokens = &number_tokens[..mantissa_end];
    let point = number_tokens.iter().position(|t| *t == Token::DecimalPoint);
    let digits = |slice: &[Token], wanted: char| {
        slice.iter().filter(|t| **t == Token::Digit(wanted)).count()
    };
    let whole = &number_tokens[..point.unwrap_or(number_tokens.len())];
    let decimals = point.map(|p| &number_tokens[p + 1..]).unwrap_or(&[]);
    let min_integer = digits(whole, '0');
    let decimal_places = decimals
        .iter()
        .filter(|t| matches!(t, Token::Digit(_)))
        .count();
    let min_decimals = digits(decimals, '0');
    let grouping = if all.contains(&Token::Thousands) {
        " number:grouping=\"true\""
    } else {
        ""
    };
    let scale = all.iter().filter(|t| **t == Token::Scale).count();
    let factor = if scale > 0 {
        format!(" number:display-factor=\"{}\"", 1000u64.pow(scale as u32))
    } else {
        String::new()
    };
    if let Some(exponent) = all.iter().position(|t| matches!(t, Token::Exponent { .. })) {
        let exponent_digits = all[exponent..]
            .iter()
            .filter(|t| matches!(t, Token::Digit(_)))
            .count();
        return format!("<number:scientific-number number:decimal-places=\"{decimal_places}\" number:min-integer-digits=\"{min_integer}\" number:min-exponent-digits=\"{exponent_digits}\"/>");
    }
    if let Some(slash) = all.iter().position(|t| *t == Token::Slash) {
        let denominator = all[slash + 1..]
            .iter()
            .take_while(|t| matches!(t, Token::Digit(_)))
            .count()
            .max(1);
        let numerator = all[..slash]
            .iter()
            .rev()
            .take_while(|t| matches!(t, Token::Digit(_)))
            .count()
            .max(1);
        return format!("<number:fraction number:min-integer-digits=\"0\" number:min-numerator-digits=\"{numerator}\" number:min-denominator-digits=\"{denominator}\"/>");
    }
    format!("<number:number number:decimal-places=\"{decimal_places}\" number:min-decimal-places=\"{min_decimals}\" number:min-integer-digits=\"{min_integer}\"{grouping}{factor}/>")
}

fn date_body(section: &Section) -> String {
    let mut out = String::new();
    let tokens = &section.tokens;
    for (index, token) in tokens.iter().enumerate() {
        let next = tokens.get(index + 1);
        let piece = match token {
            // "ss.0" is one seconds element with decimals in opendocument
            Token::Date(DatePart::Second2) | Token::Date(DatePart::Second)
                if matches!(next, Some(Token::Date(DatePart::SubSecond(_)))) =>
            {
                let Some(Token::Date(DatePart::SubSecond(digits))) = next else {
                    continue;
                };
                date_part_xml(DatePart::SubSecond(*digits))
            }
            Token::Date(DatePart::SubSecond(_)) => String::new(),
            Token::Date(part) => date_part_xml(*part),
            Token::Literal(text) => text_element(text),
            Token::Slash => text_element("/"),
            Token::Space => text_element(" "),
            Token::DecimalPoint => text_element("."),
            _ => String::new(),
        };
        out.push_str(&piece);
    }
    out
}

fn date_part_xml(part: DatePart) -> String {
    use DatePart::*;
    let long = " number:style=\"long\"";
    match part {
        Year2 => "<number:year/>".into(),
        Year4 => format!("<number:year{long}/>"),
        Month => "<number:month/>".into(),
        Month2 => format!("<number:month{long}/>"),
        MonthShort => "<number:month number:textual=\"true\"/>".into(),
        MonthLong | MonthLetter => format!("<number:month{long} number:textual=\"true\"/>"),
        Day => "<number:day/>".into(),
        Day2 => format!("<number:day{long}/>"),
        DayShort => "<number:day-of-week/>".into(),
        DayLong => format!("<number:day-of-week{long}/>"),
        Hour | ElapsedHours(1) => "<number:hours/>".into(),
        Hour2 | ElapsedHours(_) => format!("<number:hours{long}/>"),
        Minute | ElapsedMinutes(1) => "<number:minutes/>".into(),
        Minute2 | ElapsedMinutes(_) => format!("<number:minutes{long}/>"),
        Second | ElapsedSeconds(1) => "<number:seconds/>".into(),
        Second2 | ElapsedSeconds(_) => format!("<number:seconds{long}/>"),
        SubSecond(digits) => format!("<number:seconds{long} number:decimal-places=\"{digits}\"/>"),
        AmPm | AP => "<number:am-pm/>".into(),
    }
}

/// an excel code for a data style element; `lookup` finds other data
/// styles by name (for the sections a style:map points to)
pub fn odf_to_excel(xml: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut sections = Vec::new();
    let main = section_code(xml);
    let mut negative = None;
    let mut zero = None;
    for (condition, target) in maps(xml) {
        let code = lookup(&target).map(|xml| section_code(&xml));
        if condition.contains("<0") {
            negative = code;
        } else if condition.contains("=0") && !condition.contains(">=") {
            zero = code;
        } else if condition.contains(">=0") || condition.contains(">0") {
            // libreoffice puts the positive part in a map and the negative in the main style
            negative = Some(main.clone());
            sections.push(code.unwrap_or_default());
        }
    }
    if sections.is_empty() {
        sections.push(main);
    }
    if let Some(negative) = negative {
        sections.push(negative);
    }
    if let Some(zero) = zero {
        sections.push(zero);
    }
    let code = sections.join(";");
    if code.is_empty() {
        "General".into()
    } else {
        code
    }
}

fn maps(xml: &str) -> Vec<(String, String)> {
    let mut reader = Reader::new(xml);
    let mut found = Vec::new();
    while let Ok(Some(event)) = reader.next() {
        if let Event::Start(tag) = event {
            if tag.local() == "map" {
                if let (Some(condition), Some(target)) = (
                    tag.attr("style:condition"),
                    tag.attr("style:apply-style-name"),
                ) {
                    found.push((condition.into_owned(), target.into_owned()));
                }
            }
        }
    }
    found
}

/// the excel code of one data style element, without its maps
fn section_code(xml: &str) -> String {
    let mut reader = Reader::new(xml);
    let mut out = String::new();
    let mut root = "";
    let mut in_text = false;
    while let Ok(Some(event)) = reader.next() {
        match event {
            Event::Start(tag) => {
                if root.is_empty() {
                    root = if tag.local() == "percentage-style" {
                        "percent"
                    } else {
                        "other"
                    };
                    if let Some(color) = color_of(xml) {
                        out.push_str(&format!("[{color}]"));
                    }
                    continue;
                }
                in_text = matches!(tag.local(), "text" | "currency-symbol");
                out.push_str(&piece_code(&tag));
            }
            Event::Text(text) if in_text => out.push_str(&quoted(&text)),
            Event::End { .. } => in_text = false,
            _ => {}
        }
    }
    if root == "percent" && !out.contains('%') {
        out.push('%');
    }
    out
}

fn color_of(xml: &str) -> Option<&'static str> {
    let lower = xml.to_ascii_lowercase();
    let at = lower.find("fo:color=\"")?;
    match &lower[at + 10..at + 17] {
        "#ff0000" => Some("Red"),
        "#0000ff" => Some("Blue"),
        "#00ff00" => Some("Green"),
        _ => None,
    }
}

/// literal text in a code: plain when safe, quoted otherwise
fn quoted(text: &str) -> String {
    let plain = text
        .chars()
        .all(|c| " $-+/():,%".contains(c) || c == '€' || c == '£');
    if plain || text.is_empty() {
        text.to_string()
    } else {
        format!("\"{}\"", text.replace('"', ""))
    }
}

fn piece_code(tag: &crate::xml::Tag) -> String {
    let long = tag.attr("number:style").as_deref() == Some("long");
    let textual = tag.attr("number:textual").as_deref() == Some("true");
    let number = |name: &str| {
        tag.attr(name)
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
    };
    match tag.local() {
        "number" => {
            let decimals = number("number:decimal-places");
            let min_decimals = tag
                .attr("number:min-decimal-places")
                .and_then(|v| v.parse().ok())
                .unwrap_or(decimals);
            let integers = number("number:min-integer-digits");
            let grouping = tag.attr("number:grouping").as_deref() == Some("true");
            let mut code = whole_part_code(integers, grouping);
            if decimals > 0 {
                code.push('.');
                code.push_str(&"0".repeat(min_decimals.min(decimals)));
                code.push_str(&"#".repeat(decimals.saturating_sub(min_decimals)));
            }
            let factor = tag
                .attr("number:display-factor")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(1);
            let mut scale = factor;
            while scale >= 1000 {
                code.push(',');
                scale /= 1000;
            }
            code
        }
        "scientific-number" => {
            let decimals = number("number:decimal-places");
            let exponent = number("number:min-exponent-digits").max(1);
            format!(
                "0{}{}E+{}",
                if decimals > 0 { "." } else { "" },
                "0".repeat(decimals),
                "0".repeat(exponent)
            )
        }
        "fraction" => {
            let numerator = number("number:min-numerator-digits").max(1);
            let denominator = number("number:min-denominator-digits").max(1);
            format!("# {}/{}", "?".repeat(numerator), "?".repeat(denominator))
        }
        "year" => if long { "yyyy" } else { "yy" }.into(),
        "month" if textual => if long { "mmmm" } else { "mmm" }.into(),
        "month" => if long { "mm" } else { "m" }.into(),
        "day" => if long { "dd" } else { "d" }.into(),
        "day-of-week" => if long { "dddd" } else { "ddd" }.into(),
        "hours" => if long { "hh" } else { "h" }.into(),
        "minutes" => if long { "mm" } else { "m" }.into(),
        "seconds" => {
            let decimals = number("number:decimal-places");
            let base = if long { "ss" } else { "s" };
            if decimals > 0 {
                format!("{base}.{}", "0".repeat(decimals))
            } else {
                base.into()
            }
        }
        "am-pm" => "AM/PM".into(),
        "boolean" => "\"TRUE\";\"TRUE\";\"FALSE\"".into(),
        "text-content" => "@".into(),
        _ => String::new(),
    }
}

/// "0", "00", "#" or with grouping "#,##0", "#,#00"
fn whole_part_code(integers: usize, grouping: bool) -> String {
    if !grouping {
        return if integers == 0 {
            "#".into()
        } else {
            "0".repeat(integers)
        };
    }
    // at least four places so the comma has three digits after it
    let mut places = "#".repeat(4usize.saturating_sub(integers)) + &"0".repeat(integers);
    places.insert(places.len() - 3, ',');
    places
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(code: &str) -> String {
        let xml = excel_to_odf(code, "N1");
        let parts: Vec<(String, String)> = split_styles(&xml);
        let main = parts
            .iter()
            .find(|(name, _)| name == "N1")
            .map(|(_, xml)| xml.clone())
            .unwrap();
        odf_to_excel(&main, &|name| {
            parts
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, x)| x.clone())
        })
    }

    /// the separate style elements of `excel_to_odf`'s output, by name
    fn split_styles(xml: &str) -> Vec<(String, String)> {
        let wrapped = format!("<root>{xml}</root>");
        let fragment = crate::xml::Fragment::parse(&wrapped).unwrap();
        fragment
            .children
            .iter()
            .map(|(_, child)| {
                let part = crate::xml::Fragment::parse(child).unwrap();
                (
                    part.attribute("style:name").unwrap().to_string(),
                    child.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn common_formats_survive_both_ways() {
        for code in [
            "0.00",
            "#,##0",
            "#,##0.00",
            "0%",
            "0.00%",
            "0.00E+00",
            "yyyy-mm-dd",
            "d mmm yyyy",
            "h:mm:ss",
            "mm:ss.0",
            "0.00;[Red]-0.00",
            "@",
        ] {
            assert_eq!(round_trip(code), code, "{code}");
        }
    }
}
