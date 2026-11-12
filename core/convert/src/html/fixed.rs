//! exact match html: each page a box of the page's size, its text as
//! real spans placed where the pdf draws them, over a picture of
//! everything else (drawn from a copy of the page with the text taken
//! out, so nothing shows twice).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use letters_ocr::{CancelFlag, Progress};

use super::fonts::{covers, embedded_font, embedding_allowed};
use super::{escape, number, write_str, Assets, HtmlExportOptions, HtmlReport};
use crate::content::read_page_drawing;
use crate::content::strip::{remaining_paint, write_background_copy, Removals, TempPdf};
use crate::images::{encode_jpeg, encode_png};
use crate::render::{render_at_dpi, PageRenderer};
use crate::source::PdfSource;
use crate::structure::fonts::office_family;
use crate::structure::fragments::Fragment;
use crate::structure::model::{GenericFamily, TextRun};
use crate::structure::{page_number_map, read_page};
use crate::ConvertError;

/// where the baseline sits below the top of a line box one font size
/// tall, as a share of the size. browsers place it by the font's own
/// ascent and descent; 0.84 is right for arial, helvetica and times
/// within a few hundredths, which is a fraction of a point at body sizes.
pub(crate) const BASELINE_SHARE: f32 = 0.84;

/// a font from the pdf the page can use, by its css name.
struct PageFont {
    css_name: String,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn write(
    source: &PdfSource,
    renderer: &mut dyn PageRenderer,
    options: &HtmlExportOptions,
    out: &mut impl Write,
    assets: &mut Assets,
    report: &mut HtmlReport,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<(), ConvertError> {
    let pages = options.pages.resolve(source.page_count())?;
    let page_numbers = page_number_map(source);

    // first look: which fonts the text uses, and which pages draw
    // anything besides text
    let mut font_chars: BTreeMap<String, (String, BTreeSet<char>)> = BTreeMap::new();
    let mut needs_background = Vec::new();
    for page in &pages {
        check_cancel(cancel)?;
        let objects = letters_edit::page_objects(source.document(), *page)?;
        for run in objects
            .text_blocks
            .iter()
            .flat_map(|b| &b.lines)
            .flat_map(|l| &l.runs)
        {
            let entry = font_chars
                .entry(run.font.id.clone())
                .or_insert_with(|| (run.font.name.clone(), BTreeSet::new()));
            entry.1.extend(run.text.chars());
        }
        if options.background && draws_more_than_text(source, *page)? {
            needs_background.push(*page);
        }
    }
    let fonts = page_fonts(source, options, assets, report, &font_chars)?;
    let background = if needs_background.is_empty() {
        None
    } else {
        let plan: Vec<_> = needs_background
            .iter()
            .map(|page| source.page_id(*page).map(|id| (id, Removals::default())))
            .collect::<Result<_, _>>()?;
        Some(write_background_copy(source.document(), &plan)?)
    };

    write_head(out, source, &fonts)?;
    for (done, page) in pages.iter().enumerate() {
        check_cancel(cancel)?;
        let read = read_page(source, *page, &page_numbers)?;
        let background_src = match &background {
            Some(copy) if needs_background.contains(page) => {
                report.pages_with_background.push(*page);
                Some(background_picture(
                    renderer,
                    copy,
                    *page,
                    (read.width, read.height),
                    options,
                    assets,
                    !read.drawing.images.is_empty(),
                )?)
            }
            _ => None,
        };
        write_page(
            out,
            *page,
            read.width,
            read.height,
            background_src.as_deref(),
            &read.fragments,
            &fonts,
        )?;
        progress(Progress {
            page: *page,
            done: done + 1,
            total: pages.len(),
        });
    }
    write_str(out, "</body>\n</html>\n")
}

fn check_cancel(cancel: &CancelFlag) -> Result<(), ConvertError> {
    if cancel.is_cancelled() {
        Err(ConvertError::Cancelled)
    } else {
        Ok(())
    }
}

/// whether the page paints anything once its text is gone: paths,
/// pictures, shadings, or annotations other than links.
fn draws_more_than_text(source: &PdfSource, page: usize) -> Result<bool, ConvertError> {
    let page_id = source.page_id(page)?;
    let drawing = read_page_drawing(source.document(), page_id)?;
    if remaining_paint(&drawing, &Removals::default()) > 0 {
        return Ok(true);
    }
    let doc = source.document();
    let annotations = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|p| p.get(b"Annots").ok())
        .and_then(|a| crate::source::resolve(doc, a).as_array().ok())
        .cloned()
        .unwrap_or_default();
    Ok(annotations.iter().any(|a| {
        let subtype = crate::source::resolve(doc, a)
            .as_dict()
            .ok()
            .and_then(|d| d.get(b"Subtype").ok())
            .and_then(|s| s.as_name().ok());
        subtype != Some(b"Link")
    }))
}

/// decides for each font of the pdf whether the page can use it, saves
/// the ones it can (and offers the others that allow it as files), and
/// says why any were left out.
fn page_fonts(
    source: &PdfSource,
    options: &HtmlExportOptions,
    assets: &mut Assets,
    report: &mut HtmlReport,
    font_chars: &BTreeMap<String, (String, BTreeSet<char>)>,
) -> Result<BTreeMap<String, (PageFont, String)>, ConvertError> {
    let mut fonts = BTreeMap::new();
    if !options.embed_fonts {
        return Ok(fonts);
    }
    for (index, (font_ref, (name, chars))) in font_chars.iter().enumerate() {
        let Some(font) = embedded_font(source.document(), font_ref) else {
            continue;
        };
        if !embedding_allowed(&font.bytes) {
            report.font_notes.push(format!(
                "The font “{name}” doesn’t allow being copied, so a similar font is used instead."
            ));
            continue;
        }
        let text: String = chars.iter().collect();
        let usable = covers(&font.bytes, &text);
        let file_name = format!("{}.{}", crate::naming::safe_base_name(name), font.extension);
        if !usable {
            report.font_notes.push(format!(
                "The copy of “{name}” in this PDF can’t be read by browsers, so a similar font is used instead."
            ));
            if !options.self_contained {
                // offered as a file all the same: it is allowed, and may
                // help someone rebuild the document
                assets.add(&file_name, font.mime, &font.bytes)?;
            }
            continue;
        }
        let src = assets.add(&file_name, font.mime, &font.bytes)?;
        report.fonts_used.push(name.clone());
        fonts.insert(
            font_ref.clone(),
            (
                PageFont {
                    css_name: format!("lf{index}"),
                },
                src,
            ),
        );
    }
    Ok(fonts)
}

fn write_head(
    out: &mut impl Write,
    source: &PdfSource,
    fonts: &BTreeMap<String, (PageFont, String)>,
) -> Result<(), ConvertError> {
    let title = source.title().unwrap_or_else(|| "Document".into());
    let mut head = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n<title>{}</title>\n<style>\n",
        escape(&title)
    );
    head.push_str(
        "body { margin: 0; padding: 16px 0; background: #e6e6e6; }\n\
         .page { position: relative; margin: 0 auto 16px; background: #fff; overflow: hidden; box-shadow: 0 1px 4px rgba(0,0,0,.25); }\n\
         .page > img.background { position: absolute; left: 0; top: 0; width: 100%; height: 100%; }\n\
         .page span { position: absolute; white-space: pre; line-height: 1; transform-origin: 0 0; }\n\
         .page a { color: inherit; text-decoration: none; }\n",
    );
    for (font, src) in fonts.values() {
        head.push_str(&format!(
            "@font-face {{ font-family: \"{}\"; src: url(\"{}\"); }}\n",
            font.css_name, src
        ));
    }
    head.push_str("</style>\n</head>\n<body>\n");
    write_str(out, &head)
}

#[allow(clippy::too_many_arguments)]
fn background_picture(
    renderer: &mut dyn PageRenderer,
    copy: &TempPdf,
    page: usize,
    size: (f32, f32),
    options: &HtmlExportOptions,
    assets: &mut Assets,
    has_photos: bool,
) -> Result<String, ConvertError> {
    let image = render_at_dpi(renderer, copy.path(), page, size, options.background_dpi)?;
    let rgb = image.to_rgb();
    let mut bytes = Vec::new();
    // photos are much smaller as jpeg; lines and flat colour stay sharp as png
    let (name, mime) = if has_photos {
        encode_jpeg(
            &mut bytes,
            image.width,
            image.height,
            &rgb,
            false,
            options.background_dpi,
            85,
        )?;
        (format!("page-{}.jpg", page + 1), "image/jpeg")
    } else {
        encode_png(
            &mut bytes,
            image.width,
            image.height,
            &rgb,
            false,
            options.background_dpi,
        )?;
        (format!("page-{}.png", page + 1), "image/png")
    };
    assets.add(&name, mime, &bytes)
}

fn write_page(
    out: &mut impl Write,
    page: usize,
    width: f32,
    height: f32,
    background: Option<&str>,
    fragments: &[Fragment],
    fonts: &BTreeMap<String, (PageFont, String)>,
) -> Result<(), ConvertError> {
    let mut html = format!(
        "<section class=\"page\" id=\"page-{n}\" aria-label=\"Page {n}\" style=\"width:{}pt;height:{}pt\">\n",
        number(width),
        number(height),
        n = page + 1
    );
    if let Some(src) = background {
        html.push_str(&format!(
            "<img class=\"background\" src=\"{src}\" alt=\"\" />\n"
        ));
    }
    for fragment in fragments {
        for (run, rect) in fragment.placed_runs() {
            if run.text.trim().is_empty() {
                continue;
            }
            html.push_str(&span(fragment, &run, &rect, fonts));
            html.push('\n');
        }
    }
    html.push_str("</section>\n");
    write_str(out, &html)
}

/// one run as an absolutely placed span (inside a link when it is one).
fn span(
    fragment: &Fragment,
    run: &TextRun,
    rect: &crate::geometry::Rect,
    fonts: &BTreeMap<String, (PageFont, String)>,
) -> String {
    let size = run.size.max(1.0);
    // a raised run's baseline sits as far above the line's as its box does
    let baseline = fragment.baseline - (fragment.rect.bottom() - rect.bottom()).max(0.0);
    let (left, top, turn) = if fragment.rotated {
        let angle = fragment.angle.to_radians();
        let drop = BASELINE_SHARE * size;
        (
            fragment.origin.0 + drop * angle.sin(),
            fragment.origin.1 - drop * angle.cos(),
            format!("transform:rotate({}deg);", number(fragment.angle)),
        )
    } else {
        (rect.x, baseline - BASELINE_SHARE * size, String::new())
    };
    let mut style = format!(
        "left:{}pt;top:{}pt;font-size:{}pt;font-family:{};{turn}",
        number(left),
        number(top),
        number(size),
        font_stack(run, fonts)
    );
    if run.bold {
        style.push_str("font-weight:bold;");
    }
    if run.italic {
        style.push_str("font-style:italic;");
    }
    if fragment.invisible {
        // the text of a scan's text layer: selectable, but the picture
        // behind it shows the letters
        style.push_str("color:transparent;");
    } else if run.color != crate::content::Rgb::BLACK {
        style.push_str(&format!("color:#{};", run.color.hex()));
    }
    let span = format!(
        "<span style=\"{}\">{}</span>",
        escape(&style),
        escape(&run.text)
    );
    match &run.link {
        Some(link) => format!("<a href=\"{}\">{span}</a>", escape(link)),
        None => span,
    }
}

/// the pdf's own font when the page can use it, then the family by name,
/// a common font with the same widths, and the generic family.
fn font_stack(run: &TextRun, fonts: &BTreeMap<String, (PageFont, String)>) -> String {
    let mut stack = Vec::new();
    if let Some((font, _)) = fonts.get(&run.font_ref) {
        stack.push(format!("'{}'", font.css_name));
    }
    stack.push(format!("'{}'", run.font_family.replace('\'', "")));
    let office = office_family(&run.font_family);
    if office != run.font_family {
        stack.push(format!("'{office}'"));
    }
    stack.push(
        match run.generic_family {
            GenericFamily::Serif => "serif",
            GenericFamily::SansSerif => "sans-serif",
            GenericFamily::Monospace => "monospace",
        }
        .to_string(),
    );
    stack.join(",")
}
