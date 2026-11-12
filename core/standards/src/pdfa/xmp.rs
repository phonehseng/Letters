//! xmp metadata (iso 16684-1) as pdf/a needs it: reading the few
//! properties the checks compare, and writing a complete packet with the
//! pdf/a identification (pdfaid) and the document information.

/// the document information both the info dictionary and xmp can hold,
/// already decoded to text. dates are pdf dates (D:YYYYMMDDHHmmSS...).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
}

/// what an xmp packet says, for the checks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct XmpFacts {
    pub part: Option<String>,
    pub conformance: Option<String>,
    pub info: DocumentInfo,
    /// the packet starts with the xpacket processing instruction
    pub has_packet_header: bool,
}

/// reads the properties pdf/a compares out of an xmp packet. attributes
/// (pdfaid:part="2") and elements (<pdfaid:part>2</pdfaid:part>) both count.
pub fn read_xmp(text: &str) -> XmpFacts {
    let info = DocumentInfo {
        title: alternative(text, "dc:title"),
        author: sequence_item(text, "dc:creator"),
        subject: alternative(text, "dc:description"),
        keywords: simple(text, "pdf:Keywords"),
        creator: simple(text, "xmp:CreatorTool"),
        producer: simple(text, "pdf:Producer"),
        creation_date: simple(text, "xmp:CreateDate").map(|date| xmp_to_pdf_date(&date)),
        modification_date: simple(text, "xmp:ModifyDate").map(|date| xmp_to_pdf_date(&date)),
    };
    XmpFacts {
        part: simple(text, "pdfaid:part"),
        conformance: simple(text, "pdfaid:conformance"),
        info,
        has_packet_header: text.trim_start().starts_with("<?xpacket begin="),
    }
}

/// a property written as an attribute or as an element with text.
fn simple(text: &str, property: &str) -> Option<String> {
    let attribute = format!("{property}=\"");
    if let Some(start) = text.find(&attribute) {
        let rest = &text[start + attribute.len()..];
        return rest.find('"').map(|end| unescape(&rest[..end]));
    }
    let attribute = format!("{property}='");
    if let Some(start) = text.find(&attribute) {
        let rest = &text[start + attribute.len()..];
        return rest.find('\'').map(|end| unescape(&rest[..end]));
    }
    element_text(text, property)
}

fn element_text(text: &str, property: &str) -> Option<String> {
    let open = format!("<{property}");
    let start = text.find(&open)?;
    let after_open = &text[start + open.len()..];
    let content_start = after_open.find('>')? + 1;
    if after_open[..content_start].ends_with("/>") {
        return None;
    }
    let content = &after_open[content_start..];
    let end = content.find(&format!("</{property}>"))?;
    Some(unescape(content[..end].trim()))
}

/// the first rdf:li of an rdf:Alt (language alternatives such as dc:title).
fn alternative(text: &str, property: &str) -> Option<String> {
    let inner = element_text(text, property)?;
    first_list_item(&inner)
}

/// the first rdf:li of an rdf:Seq (dc:creator).
fn sequence_item(text: &str, property: &str) -> Option<String> {
    alternative(text, property)
}

fn first_list_item(inner: &str) -> Option<String> {
    let start = inner.find("<rdf:li")?;
    let after = &inner[start..];
    let content_start = after.find('>')? + 1;
    let end = after.find("</rdf:li>")?;
    (end >= content_start).then(|| unescape(after[content_start..end].trim()))
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// "2026-11-12T09:30:00Z" or "2026-11-12T09:30:00+13:00" as a pdf date.
pub fn xmp_to_pdf_date(date: &str) -> String {
    let digits: String = date.chars().take(19).filter(char::is_ascii_digit).collect();
    let zone = &date[date.len().min(19)..];
    let zone = match zone {
        "" | "Z" => "Z".to_owned(),
        other => {
            let cleaned: String = other.chars().filter(|c| *c != ':').collect();
            if cleaned.len() == 5 {
                format!("{}{}'{}'", &cleaned[..1], &cleaned[1..3], &cleaned[3..5])
            } else {
                cleaned
            }
        }
    };
    format!("D:{digits}{zone}")
}

/// a pdf date as an xmp date (iso 8601). missing parts are left out, as
/// xmp allows; no zone means no zone.
pub fn pdf_to_xmp_date(date: &str) -> Option<String> {
    let body = date.trim().trim_start_matches("D:");
    let digits: String = body.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let rest = &body[digits.len()..];
    let mut out = digits[..4].to_owned();
    let parts = [(4, "-"), (6, "-"), (8, "T"), (10, ":"), (12, ":")];
    for (at, separator) in parts {
        if digits.len() >= at + 2 {
            out.push_str(separator);
            out.push_str(&digits[at..at + 2]);
        }
    }
    // a time needs at least hours and minutes in xmp
    if digits.len() == 10 {
        out.push_str(":00");
    }
    if digits.len() >= 10 {
        out.push_str(&xmp_zone(rest));
    }
    Some(out)
}

fn xmp_zone(rest: &str) -> String {
    let rest = rest.trim();
    if rest.starts_with('Z') {
        return "Z".to_owned();
    }
    let sign = match rest.chars().next() {
        Some(sign @ ('+' | '-')) => sign,
        _ => return String::new(),
    };
    let digits: String = rest.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        2 => format!("{sign}{digits}:00"),
        4.. => format!("{sign}{}:{}", &digits[..2], &digits[2..4]),
        _ => String::new(),
    }
}

/// the dates of two sources stand for the same moment: compared by their
/// digits and zone, so "D:20261112093000Z" equals "D:20261112093000+00'00'".
pub fn same_date(a: &str, b: &str) -> bool {
    let normal = |date: &str| {
        let xmp = pdf_to_xmp_date(date).unwrap_or_default();
        xmp.replace("+00:00", "Z")
    };
    normal(a) == normal(b)
}

/// a complete xmp packet for pdf/a with the given identification and info.
pub fn write_xmp(part: u8, conformance: &str, info: &DocumentInfo) -> String {
    let mut description = String::new();
    let mut push = |text: String| {
        description.push_str(&text);
        description.push('\n');
    };
    push(format!("   <pdfaid:part>{part}</pdfaid:part>"));
    push(format!(
        "   <pdfaid:conformance>{conformance}</pdfaid:conformance>"
    ));
    let language_alternative = |name: &str, value: &str| {
        format!(
            "   <{name}><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></{name}>",
            escape(value)
        )
    };
    if let Some(title) = &info.title {
        push(language_alternative("dc:title", title));
    }
    if let Some(author) = &info.author {
        push(format!(
            "   <dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>",
            escape(author)
        ));
    }
    if let Some(subject) = &info.subject {
        push(language_alternative("dc:description", subject));
    }
    let simple_properties = [
        ("pdf:Keywords", info.keywords.clone()),
        ("pdf:Producer", info.producer.clone()),
        ("xmp:CreatorTool", info.creator.clone()),
        (
            "xmp:CreateDate",
            info.creation_date.as_deref().and_then(pdf_to_xmp_date),
        ),
        (
            "xmp:ModifyDate",
            info.modification_date.as_deref().and_then(pdf_to_xmp_date),
        ),
    ];
    for (name, value) in simple_properties {
        if let Some(value) = value {
            push(format!("   <{name}>{}</{name}>", escape(&value)));
        }
    }
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
 <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
  <rdf:Description rdf:about=\"\"\n\
    xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"\n\
    xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n\
    xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\"\n\
    xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n\
{description}  </rdf:Description>\n\
 </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"w\"?>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_packets_read_back() {
        let info = DocumentInfo {
            title: Some("Trip & Permission".into()),
            author: Some("Ms Teacher".into()),
            creation_date: Some("D:20261112093000+13'00'".into()),
            ..Default::default()
        };
        let packet = write_xmp(2, "U", &info);
        let facts = read_xmp(&packet);
        assert_eq!(facts.part.as_deref(), Some("2"));
        assert_eq!(facts.conformance.as_deref(), Some("U"));
        assert_eq!(facts.info.title.as_deref(), Some("Trip & Permission"));
        assert_eq!(facts.info.author.as_deref(), Some("Ms Teacher"));
        assert!(same_date(
            facts.info.creation_date.as_deref().unwrap(),
            "D:20261112093000+13'00'"
        ));
        assert!(facts.has_packet_header);
    }

    #[test]
    fn attributes_count_too() {
        let facts = read_xmp("<rdf:Description pdfaid:part=\"1\" pdfaid:conformance='B'/>");
        assert_eq!(
            (facts.part.as_deref(), facts.conformance.as_deref()),
            (Some("1"), Some("B"))
        );
    }

    #[test]
    fn dates_convert_both_ways() {
        assert_eq!(
            pdf_to_xmp_date("D:20261112093000Z").unwrap(),
            "2026-11-12T09:30:00Z"
        );
        assert_eq!(pdf_to_xmp_date("D:2026").unwrap(), "2026");
        assert_eq!(
            xmp_to_pdf_date("2026-11-12T09:30:00+13:00"),
            "D:20261112093000+13'00'"
        );
        assert!(same_date("D:20261112093000Z", "D:20261112093000+00'00'"));
    }
}
