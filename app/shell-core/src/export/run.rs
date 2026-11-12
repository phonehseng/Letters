//! running one export: the right engine for the chosen format, with
//! progress, cancelling, and a plain result line.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use letters_convert::{
    analyze_structure, pdf_to_html, pdf_to_images, pdf_to_pptx, pdf_to_text, pdf_to_xlsx,
    CancelFlag, ExportStyle, HtmlExportOptions, HtmlMode, ImageExportOptions, ImageFormat,
    PageSelection, PdfSource, PptxExportOptions, Progress, StructureOptions, TextExportOptions,
    TextMode, XlsxExportOptions,
};
use letters_office::FileKind;

use super::easy_edit::{self, PictureSource};
use super::exact::{self, ExactPlaces};
use super::renderer::WorkerRenderer;
use super::request::{ExportJob, ExportOutcome, ExportProgress, ExportRequest};
use super::summary;

/// what an export needs from the app besides the job.
pub struct ExportTools {
    pub worker: letters_pdf::RenderWorker,
    /// where the exact match engine keeps its measurements and work files
    pub exact: ExactPlaces,
    pub progress: Arc<dyn Fn(ExportProgress) + Send + Sync>,
    pub cancel: CancelFlag,
    /// the exact match engine's own stop switch, flipped with `cancel`
    pub exact_cancel: letters_exact::CancelFlag,
}

/// exports `source` (the pdf at `pdf`, as it is shown, edits included)
/// as the job asks.
pub fn run_export(
    pdf: &Path,
    job: &ExportJob,
    tools: &ExportTools,
) -> Result<ExportOutcome, String> {
    let source = PdfSource::open(pdf).map_err(|error| error.to_string())?;
    let pages = job
        .pages
        .resolve(source.page_count())
        .map_err(|error| error.to_string())?
        .len();
    let progress = tools.progress.clone();
    let mut on_page = move |step: Progress| {
        progress(ExportProgress {
            fraction: if step.total == 0 {
                0.0
            } else {
                step.done as f32 / step.total as f32
            },
            page: Some(step.page + 1),
        })
    };
    let cancel = &tools.cancel;
    let destination = with_extension(&job.destination, job.request.extension());
    let failed = |error: letters_convert::ConvertError| error.to_string();
    match &job.request {
        ExportRequest::Word {
            style: ExportStyle::ExactMatch,
        } => {
            if job.pages != PageSelection::All {
                return Err("Exact Match makes the whole document. Choose All Pages, or use Easy to Edit for some pages.".into());
            }
            let render = Some(super::renderer::exact_renderer(
                tools.worker.clone(),
                tools.exact.work_dir.clone(),
            ));
            let use_word = crate::writer::export::word_installed();
            exact::convert_exact(
                pdf,
                &destination,
                &tools.exact,
                use_word,
                render,
                tools.progress.clone(),
                tools.exact_cancel.clone(),
            )
        }
        ExportRequest::Word {
            style: ExportStyle::EasyToEdit,
        } => easy_to_edit(
            &source,
            &job.pages,
            FileKind::Docx,
            &destination,
            &mut on_page,
            cancel,
        ),
        ExportRequest::OpenDocument => easy_to_edit(
            &source,
            &job.pages,
            FileKind::Odt,
            &destination,
            &mut on_page,
            cancel,
        ),
        ExportRequest::Excel { layout } => {
            let options = XlsxExportOptions {
                pages: job.pages.clone(),
                layout: *layout,
            };
            let report = pdf_to_xlsx(&source, &options, &destination, &mut on_page, cancel)
                .map_err(failed)?;
            Ok(made(
                vec![destination],
                summary::workbook(&report),
                report.notes.clone(),
            ))
        }
        ExportRequest::PowerPoint => {
            let mut renderer = WorkerRenderer::new(tools.worker.clone());
            let options = PptxExportOptions {
                pages: job.pages.clone(),
                ..PptxExportOptions::default()
            };
            let report = pdf_to_pptx(
                &source,
                &mut renderer,
                &options,
                &destination,
                &mut on_page,
                cancel,
            )
            .map_err(failed)?;
            let notes = report
                .slides
                .iter()
                .flat_map(|slide| {
                    slide
                        .in_background
                        .iter()
                        .map(|what| format!("Slide {}: {what}", slide.page + 1))
                })
                .take(6)
                .collect();
            Ok(made(vec![destination], summary::slides(&report), notes))
        }
        ExportRequest::Images { image, dpi, color } => {
            let mut renderer = WorkerRenderer::new(tools.worker.clone());
            let options = ImageExportOptions {
                format: *image,
                dpi: *dpi,
                pages: job.pages.clone(),
                color: *color,
            };
            let (folder, base) = folder_and_base(&destination);
            let files = pdf_to_images(
                &source,
                &mut renderer,
                &options,
                &folder,
                &base,
                &mut on_page,
                cancel,
            )
            .map_err(failed)?;
            let line = summary::pictures(files.len(), pages);
            Ok(made(files, line, picture_notes(image)))
        }
        ExportRequest::WebPage { style, single_file } => {
            let mut renderer = WorkerRenderer::new(tools.worker.clone());
            let exact = *style == ExportStyle::ExactMatch;
            let options = HtmlExportOptions {
                mode: if exact {
                    HtmlMode::ExactMatch
                } else {
                    HtmlMode::EasyToEdit
                },
                pages: job.pages.clone(),
                self_contained: *single_file,
                ..HtmlExportOptions::default()
            };
            let report = pdf_to_html(
                &source,
                &mut renderer,
                &options,
                &destination,
                &mut on_page,
                cancel,
            )
            .map_err(failed)?;
            let line = summary::web_page(&report, pages, exact);
            Ok(made(report.files.clone(), line, report.font_notes.clone()))
        }
        ExportRequest::Text { style } => {
            let layout = *style == ExportStyle::ExactMatch;
            let options = TextExportOptions {
                pages: job.pages.clone(),
                mode: if layout {
                    TextMode::Layout
                } else {
                    TextMode::ReadingOrder
                },
                ..TextExportOptions::default()
            };
            let text = pdf_to_text(&source, &options, &mut on_page, cancel).map_err(failed)?;
            crate::writer::save::write_atomically(text.as_bytes(), &destination)?;
            Ok(made(
                vec![destination],
                summary::text(text.chars().count(), layout),
                Vec::new(),
            ))
        }
    }
}

fn made(files: Vec<PathBuf>, summary: String, notes: Vec<String>) -> ExportOutcome {
    ExportOutcome {
        files,
        summary,
        notes,
        ..ExportOutcome::default()
    }
}

fn picture_notes(image: &ImageFormat) -> Vec<String> {
    match image {
        ImageFormat::Jpeg { quality } if *quality < 60 => {
            vec!["A low quality setting makes small files, but text can look blurred.".into()]
        }
        _ => Vec::new(),
    }
}

/// the pdf's structure rebuilt as a .docx or .odt.
fn easy_to_edit(
    source: &PdfSource,
    pages: &PageSelection,
    kind: FileKind,
    destination: &Path,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<ExportOutcome, String> {
    let (document, report) = easy_to_edit_document(source, pages, progress, cancel)?;
    let bytes = letters_office::save(&document, kind).map_err(|error| error.to_string())?;
    crate::writer::save::write_atomically(&bytes, destination)?;
    Ok(ExportOutcome {
        files: vec![destination.to_path_buf()],
        summary: summary::easy_to_edit(&report),
        notes: summary::easy_to_edit_notes(&report),
        ..ExportOutcome::default()
    })
}

/// the office model of a pdf, for Easy to Edit files and for opening a
/// pdf in the writer.
pub fn easy_to_edit_document(
    source: &PdfSource,
    pages: &PageSelection,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<(letters_office::model::Document, easy_edit::EasyEditReport), String> {
    let options = StructureOptions {
        pages: pages.clone(),
        ..StructureOptions::default()
    };
    let structure =
        analyze_structure(source, &options, progress, cancel).map_err(|error| error.to_string())?;
    Ok(easy_edit::build_document(&structure, &PdfPictures(source)))
}

/// the pdf's own pictures, copied out by letters-edit.
struct PdfPictures<'a>(&'a PdfSource);

impl PictureSource for PdfPictures<'_> {
    fn picture(&self, page: usize, image_id: &str) -> Option<Vec<u8>> {
        letters_edit::extract_image(self.0.document(), page, image_id)
            .ok()
            .map(|image| image.bytes)
    }
}

/// the destination with the format's extension, so "Essay" saved as a
/// word document is "Essay.docx".
pub fn with_extension(path: &Path, extension: &str) -> PathBuf {
    let has_it = path.extension().is_some_and(|current| {
        current.to_string_lossy().eq_ignore_ascii_case(extension)
            || (extension == "jpg" && current.to_string_lossy().eq_ignore_ascii_case("jpeg"))
            || (extension == "tiff" && current.to_string_lossy().eq_ignore_ascii_case("tif"))
            || (extension == "html" && current.to_string_lossy().eq_ignore_ascii_case("htm"))
    });
    if has_it {
        path.to_path_buf()
    } else {
        path.with_extension(extension)
    }
}

/// "C:/out/Essay.png" as the folder "C:/out" and the name "Essay".
fn folder_and_base(path: &Path) -> (PathBuf, String) {
    let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let base = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Page".into());
    (folder, base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_is_added_only_when_missing() {
        assert_eq!(
            with_extension(Path::new("a/Essay"), "docx"),
            PathBuf::from("a/Essay.docx")
        );
        assert_eq!(
            with_extension(Path::new("a/Essay.DOCX"), "docx"),
            PathBuf::from("a/Essay.DOCX")
        );
        assert_eq!(
            with_extension(Path::new("a/p.jpeg"), "jpg"),
            PathBuf::from("a/p.jpeg")
        );
        assert_eq!(
            with_extension(Path::new("a/p.pdf"), "odt"),
            PathBuf::from("a/p.odt")
        );
    }

    #[test]
    fn page_pictures_are_named_from_the_destination() {
        let (folder, base) = folder_and_base(Path::new("C:/out/Essay.png"));
        assert_eq!(folder, PathBuf::from("C:/out"));
        assert_eq!(base, "Essay");
    }

    #[test]
    fn a_study_guide_becomes_an_easy_to_edit_document() {
        let dir = crate::test_pdf::temp_dir("easy-edit-guide");
        let pdf = dir.join("guide.pdf");
        std::fs::write(&pdf, super::super::test_pdfs::study_guide_pdf()).unwrap();
        let source = PdfSource::open(&pdf).unwrap();
        let (document, report) = easy_to_edit_document(
            &source,
            &PageSelection::All,
            &mut |_| {},
            &CancelFlag::new(),
        )
        .unwrap();
        assert!(report.headings >= 2, "{report:?}");
        assert_eq!(report.lists, 1, "{report:?}");
        assert_eq!(report.tables, 1, "{report:?}");
        let text: Vec<String> = document
            .blocks
            .iter()
            .filter_map(|b| b.paragraph().map(|p| p.plain_text()))
            .collect();
        assert!(
            text.iter().any(|t| t.contains("Water from the roots")),
            "{text:?}"
        );
        let docx = letters_office::write_docx(&document).unwrap();
        assert!(letters_office::read_docx(&docx).is_ok());
    }
}
