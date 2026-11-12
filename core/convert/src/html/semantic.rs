//! easy to edit html: the document's structure as plain, accessible
//! html. headings are h1 to h6, lists are ul and ol (nested), tables have
//! header cells with scope, pictures are figures with captions and alt
//! text, links are links, footnotes are gathered at the end. no positions
//! and almost no styling, so the text reflows on any screen.

use std::io::Write;

use letters_ocr::{CancelFlag, Progress};

use super::{escape, write_str, Assets, HtmlExportOptions};
use crate::source::PdfSource;
use crate::structure::model::{runs_text, Block, BlockContent, Figure, ListItem, Table, TextRun};
use crate::structure::{analyze_structure, StructureOptions};
use crate::ConvertError;

const STYLE: &str = "body { max-width: 42em; margin: 2em auto; padding: 0 1em; font-family: Georgia, 'Times New Roman', serif; line-height: 1.5; color: #222; }\n\
table { border-collapse: collapse; margin: 1em 0; }\n\
th, td { border: 1px solid #999; padding: .25em .6em; vertical-align: top; }\n\
th { background: #f0f0f0; text-align: left; }\n\
figure { margin: 1.5em 0; }\n\
figure img { max-width: 100%; height: auto; }\n\
figcaption, caption { font-size: .9em; color: #555; }\n\
.footnotes { font-size: .9em; border-top: 1px solid #ccc; margin-top: 2em; }\n";

pub(super) fn write(
    source: &PdfSource,
    options: &HtmlExportOptions,
    out: &mut impl Write,
    assets: &mut Assets,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<(), ConvertError> {
    let structure = analyze_structure(
        source,
        &StructureOptions {
            pages: options.pages.clone(),
            dehyphenate: options.dehyphenate,
            ..Default::default()
        },
        progress,
        cancel,
    )?;
    let title = structure.title.clone().unwrap_or_else(|| "Document".into());
    write_str(
        out,
        &format!(
            "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n<title>{}</title>\n<style>\n{STYLE}</style>\n</head>\n<body>\n<main>\n",
            escape(&title)
        ),
    )?;
    let mut notes = Vec::new();
    for block in &structure.blocks {
        if cancel.is_cancelled() {
            return Err(ConvertError::Cancelled);
        }
        match &block.content {
            BlockContent::Footnote { marker, runs } => notes.push((block.page, marker, runs)),
            _ => write_str(out, &block_html(source, block, assets)?)?,
        }
    }
    write_str(out, "</main>\n")?;
    if !notes.is_empty() {
        let mut html = String::from("<section class=\"footnotes\" aria-label=\"Notes\">\n");
        for (page, marker, runs) in notes {
            html.push_str(&format!(
                "<p id=\"note-{}-{}\"><sup>{}</sup> {}</p>\n",
                page + 1,
                escape(marker.trim_end_matches(['.', ')'])),
                escape(marker),
                inline(runs)
            ));
        }
        html.push_str("</section>\n");
        write_str(out, &html)?;
    }
    write_str(out, "</body>\n</html>\n")
}

fn block_html(
    source: &PdfSource,
    block: &Block,
    assets: &mut Assets,
) -> Result<String, ConvertError> {
    Ok(match &block.content {
        BlockContent::Heading { level, runs } => {
            let level = (*level).clamp(1, 6);
            format!("<h{level}>{}</h{level}>\n", inline(runs))
        }
        BlockContent::Paragraph { runs } => format!("<p>{}</p>\n", inline(runs)),
        BlockContent::List { items } => list_html(items),
        BlockContent::Table(table) => table_html(table),
        BlockContent::Figure(figure) => figure_html(source, block.page, figure, assets)?,
        BlockContent::Footnote { .. } => String::new(),
    })
}

/// runs as inline html: links, bold, italics and superscripts. sizes,
/// colours and fonts are left to the reader's style.
pub(crate) fn inline(runs: &[TextRun]) -> String {
    let mut html = String::new();
    let mut index = 0;
    while index < runs.len() {
        // runs that look the same once sizes and colours are dropped are
        // written as one, so markup isn't split needlessly
        let mut text = runs[index].text.clone();
        let mut next = index + 1;
        while next < runs.len() && same_markup(&runs[index], &runs[next]) {
            text.push_str(&runs[next].text);
            next += 1;
        }
        html.push_str(&marked_up(&runs[index], &text));
        index = next;
    }
    html
}

fn same_markup(a: &TextRun, b: &TextRun) -> bool {
    a.bold == b.bold && a.italic == b.italic && a.superscript == b.superscript && a.link == b.link
}

fn marked_up(run: &TextRun, text: &str) -> String {
    let mut html = escape(text);
    if text.trim().is_empty() {
        return html;
    }
    if run.superscript {
        html = format!("<sup>{html}</sup>");
    }
    if run.italic {
        html = format!("<em>{html}</em>");
    }
    if run.bold {
        html = format!("<strong>{html}</strong>");
    }
    if let Some(link) = &run.link {
        html = format!("<a href=\"{}\">{html}</a>", escape(link));
    }
    html
}

/// nested lists from the items' levels: a deeper item opens a list
/// inside the item before it.
fn list_html(items: &[ListItem]) -> String {
    let mut html = String::new();
    // the lists open right now, outermost first: whether each is ordered
    let mut open: Vec<bool> = Vec::new();
    for item in items {
        let depth = usize::from(item.level) + 1;
        while open.len() > depth {
            close_list(&mut html, &mut open);
        }
        if open.len() == depth && open.last() != Some(&item.ordered) {
            close_list(&mut html, &mut open);
        }
        if open.len() == depth {
            html.push_str("</li>\n");
        }
        while open.len() < depth {
            html.push_str(if item.ordered { "<ol>\n" } else { "<ul>\n" });
            open.push(item.ordered);
        }
        html.push_str(&format!("<li>{}", inline(&item.runs)));
    }
    while !open.is_empty() {
        close_list(&mut html, &mut open);
    }
    html
}

fn close_list(html: &mut String, open: &mut Vec<bool>) {
    let ordered = open.pop().unwrap_or(false);
    html.push_str(if ordered {
        "</li>\n</ol>\n"
    } else {
        "</li>\n</ul>\n"
    });
}

fn table_html(table: &Table) -> String {
    let mut html = String::from("<table>\n");
    if let Some(caption) = &table.caption {
        html.push_str(&format!("<caption>{}</caption>\n", inline(caption)));
    }
    // which grid places a merged cell already covers
    let mut covered = vec![false; table.rows * table.columns];
    for cell in &table.cells {
        for row in cell.row..(cell.row + cell.row_span).min(table.rows) {
            for column in cell.column..(cell.column + cell.column_span).min(table.columns) {
                covered[row * table.columns + column] = true;
            }
        }
    }
    for row in 0..table.rows {
        let header = row < table.header_rows;
        if row == 0 && header {
            html.push_str("<thead>\n");
        }
        if row == table.header_rows {
            html.push_str("<tbody>\n");
        }
        html.push_str("<tr>");
        for column in 0..table.columns {
            let tag = if header { "th" } else { "td" };
            match table.cell(row, column) {
                Some(cell) => {
                    let mut attributes = String::new();
                    if header {
                        attributes.push_str(if cell.column_span > 1 {
                            " scope=\"colgroup\""
                        } else {
                            " scope=\"col\""
                        });
                    }
                    if cell.column_span > 1 {
                        attributes.push_str(&format!(" colspan=\"{}\"", cell.column_span));
                    }
                    if cell.row_span > 1 {
                        attributes.push_str(&format!(" rowspan=\"{}\"", cell.row_span));
                    }
                    html.push_str(&format!(
                        "<{tag}{attributes}>{}</{tag}>",
                        inline(&cell.runs)
                    ));
                }
                None if !covered[row * table.columns + column] => {
                    html.push_str(&format!("<{tag}></{tag}>"))
                }
                None => {}
            }
        }
        html.push_str("</tr>\n");
        if header && row + 1 == table.header_rows {
            html.push_str("</thead>\n");
        }
    }
    if table.rows > table.header_rows {
        html.push_str("</tbody>\n");
    }
    html.push_str("</table>\n");
    html
}

/// a picture with its caption. the alt text is the caption, or says
/// where the picture came from when there is none.
fn figure_html(
    source: &PdfSource,
    page: usize,
    figure: &Figure,
    assets: &mut Assets,
) -> Result<String, ConvertError> {
    let caption = figure.caption.as_ref().map(|runs| runs_text(runs));
    let alt = caption
        .clone()
        .unwrap_or_else(|| format!("Picture from page {}", page + 1));
    let picture = match letters_edit::extract_image(source.document(), page, &figure.image_id) {
        Ok(picture) => picture,
        // a picture that can't be saved out (an unusual format) is
        // described instead of dropped silently
        Err(error) => {
            return Ok(format!(
                "<p><em>[A picture from page {} couldn’t be copied: {}]</em></p>\n",
                page + 1,
                escape(&error.to_string())
            ))
        }
    };
    let mime = if picture.format == "jpeg" {
        "image/jpeg"
    } else {
        "image/png"
    };
    let extension = if picture.format == "jpeg" {
        "jpg"
    } else {
        "png"
    };
    let src = assets.add(
        &format!("picture-page-{}.{extension}", page + 1),
        mime,
        &picture.bytes,
    )?;
    let mut html = format!(
        "<figure>\n<img src=\"{src}\" alt=\"{}\" width=\"{}\" height=\"{}\" />\n",
        escape(&alt),
        figure.pixel_width,
        figure.pixel_height
    );
    if let Some(runs) = &figure.caption {
        html.push_str(&format!("<figcaption>{}</figcaption>\n", inline(runs)));
    }
    html.push_str("</figure>\n");
    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Rgb;
    use crate::structure::model::GenericFamily;

    fn run(text: &str, bold: bool) -> TextRun {
        TextRun {
            text: text.into(),
            font_family: "Helvetica".into(),
            generic_family: GenericFamily::SansSerif,
            bold,
            italic: false,
            size: 11.0,
            color: Rgb::BLACK,
            link: None,
            superscript: false,
            font_ref: String::new(),
        }
    }

    fn item(level: u8, ordered: bool, text: &str) -> ListItem {
        ListItem {
            level,
            ordered,
            marker: "•".into(),
            runs: vec![run(text, false)],
            rect: Default::default(),
        }
    }

    #[test]
    fn lists_nest_by_level() {
        let html = list_html(&[
            item(0, false, "a"),
            item(1, true, "b"),
            item(1, true, "c"),
            item(0, false, "d"),
        ]);
        assert_eq!(
            html,
            "<ul>\n<li>a<ol>\n<li>b</li>\n<li>c</li>\n</ol>\n</li>\n<li>d</li>\n</ul>\n"
        );
    }

    #[test]
    fn runs_become_strong_and_links() {
        let mut linked = run("site", false);
        linked.link = Some("https://example.org".into());
        let html = inline(&[run("Go ", true), run("now ", true), linked]);
        assert_eq!(
            html,
            "<strong>Go now </strong><a href=\"https://example.org\">site</a>"
        );
    }
}
