//! the result line of the export dialog: what was made, in plain words,
//! from each engine's own report.

use letters_convert::{HtmlReport, PptxReport, XlsxReport};

use super::easy_edit::EasyEditReport;

/// "1 page", "3 pages".
pub fn count(number: usize, noun: &str) -> String {
    match number {
        1 => format!("1 {noun}"),
        _ => format!("{number} {noun}s"),
    }
}

/// "a, b and c".
fn list(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// "12 slides. Text is real text; 3 charts were kept as pictures."
pub fn slides(report: &PptxReport) -> String {
    let with_background = report
        .slides
        .iter()
        .filter(|slide| slide.background)
        .count();
    let mut sentence = format!("{}. Text is real text", count(report.slides.len(), "slide"));
    if with_background > 0 {
        sentence.push_str(&format!(
            "; {} kept what couldn’t be rebuilt as a picture behind the text",
            if with_background == 1 {
                "1 slide".to_string()
            } else {
                format!("{with_background} slides")
            }
        ));
    }
    sentence.push('.');
    if report.slides.iter().any(|slide| slide.scaled) {
        sentence.push_str(" Pages of another size were fitted to the slide.");
    }
    sentence
}

pub fn workbook(report: &XlsxReport) -> String {
    if report.tables == 0 {
        return format!(
            "{} with the page’s text; no tables were found, so there are no cells to calculate with.",
            count(report.sheets.len(), "sheet")
        );
    }
    format!(
        "{} with {} as real cells, numbers and dates included.",
        count(report.sheets.len(), "sheet"),
        count(report.tables, "table")
    )
}

pub fn web_page(report: &HtmlReport, pages: usize, exact: bool) -> String {
    if exact {
        let with_picture = report.pages_with_background.len();
        let mut sentence = format!(
            "A web page of {} with real, selectable text",
            count(pages, "page")
        );
        if with_picture > 0 {
            sentence.push_str(" over a picture of the drawings");
        }
        sentence.push('.');
        sentence
    } else {
        format!(
            "A web page of headings, paragraphs, lists and tables from {}.",
            count(pages, "page")
        )
    }
}

pub fn pictures(files: usize, pages: usize) -> String {
    if files == 1 && pages > 1 {
        format!("One picture file with {}.", count(pages, "page"))
    } else {
        format!("{}, one for each page.", count(files, "picture"))
    }
}

pub fn text(characters: usize, layout: bool) -> String {
    let how = if layout {
        "with each line where it was on the page"
    } else {
        "in reading order"
    };
    format!("Plain text {how}, {}.", count(characters, "character"))
}

/// "2 pages became 3 headings, 14 paragraphs, 2 lists and 1 table."
pub fn easy_to_edit(report: &EasyEditReport) -> String {
    let mut parts = Vec::new();
    for (number, noun) in [
        (report.headings, "heading"),
        (report.paragraphs, "paragraph"),
        (report.lists, "list"),
        (report.tables, "table"),
        (report.pictures, "picture"),
        (report.footnotes, "footnote"),
    ] {
        if number > 0 {
            parts.push(count(number, noun));
        }
    }
    if parts.is_empty() {
        return format!(
            "{} had no text to rebuild; if they are scanned, recognise the text first.",
            count(report.pages, "page")
        );
    }
    let mut sentence = format!("{} became {}.", count(report.pages, "page"), list(&parts));
    if report.header || report.footer {
        let which = match (report.header, report.footer) {
            (true, true) => "header and footer",
            (true, false) => "header",
            _ => "footer",
        };
        sentence.push_str(&format!(" The repeated {which} became a real {which}."));
    }
    sentence
}

/// one note per thing the person should know about an easy to edit file.
pub fn easy_to_edit_notes(report: &EasyEditReport) -> Vec<String> {
    let mut notes = Vec::new();
    if report.pictures_left_out > 0 {
        notes.push(format!(
            "{} couldn’t be copied out of the PDF and {} left out.",
            count(report.pictures_left_out, "picture"),
            if report.pictures_left_out == 1 {
                "was"
            } else {
                "were"
            }
        ));
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_convert::SlideReport;

    fn slide(background: bool) -> SlideReport {
        SlideReport {
            page: 0,
            text_boxes: 3,
            pictures: 0,
            shapes: 0,
            background,
            in_background: vec![],
            scaled: false,
        }
    }

    #[test]
    fn slides_say_what_stayed_a_picture() {
        let report = PptxReport {
            slide_width: 720.0,
            slide_height: 540.0,
            slides: vec![slide(false), slide(true), slide(true)],
        };
        assert_eq!(slides(&report), "3 slides. Text is real text; 2 slides kept what couldn’t be rebuilt as a picture behind the text.");
    }

    #[test]
    fn easy_to_edit_lists_only_what_there_is() {
        let report = EasyEditReport {
            pages: 2,
            headings: 3,
            paragraphs: 14,
            tables: 1,
            footer: true,
            ..EasyEditReport::default()
        };
        assert_eq!(easy_to_edit(&report), "2 pages became 3 headings, 14 paragraphs and 1 table. The repeated footer became a real footer.");
        let empty = EasyEditReport {
            pages: 1,
            ..EasyEditReport::default()
        };
        assert!(easy_to_edit(&empty).contains("recognise the text"));
    }

    #[test]
    fn counting_words() {
        assert_eq!(count(1, "page"), "1 page");
        assert_eq!(count(0, "table"), "0 tables");
        assert_eq!(pictures(1, 4), "One picture file with 4 pages.");
    }
}
