//! what the export dialog asks for, and what it gets back.
//!
//! both cross the bridge to the ui as json with camelCase names, so the
//! dialog's choices map one to one onto these types.

use std::path::PathBuf;

use letters_convert::{ColorMode, ExportStyle, ImageFormat, PageSelection, SheetLayout};
use serde::{Deserialize, Serialize};

/// one export: the format with its settings, the pages, and where to.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportJob {
    pub request: ExportRequest,
    /// zero based pages, or every page
    #[serde(default)]
    pub pages: PageSelection,
    /// the file the save panel returned. for pictures, the folder and
    /// name the page pictures are made from ("Essay.png" gives
    /// "Essay - page 1.png" and so on)
    pub destination: PathBuf,
}

/// the format and its settings.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "format", rename_all = "camelCase")]
pub enum ExportRequest {
    /// a .docx, exact match or easy to edit
    Word {
        style: ExportStyle,
    },
    /// an easy to edit .odt
    OpenDocument,
    Excel {
        #[serde(default)]
        layout: SheetLayout,
    },
    PowerPoint,
    Images {
        image: ImageFormat,
        dpi: f32,
        #[serde(default)]
        color: ColorMode,
    },
    WebPage {
        style: ExportStyle,
        /// pictures and fonts inside the one .html file
        #[serde(default = "yes", rename = "singleFile")]
        single_file: bool,
    },
    /// reading order is easy to edit, keep layout is exact match
    Text {
        style: ExportStyle,
    },
}

fn yes() -> bool {
    true
}

impl ExportRequest {
    /// the extension the destination should have.
    pub fn extension(&self) -> &'static str {
        match self {
            ExportRequest::Word { .. } => "docx",
            ExportRequest::OpenDocument => "odt",
            ExportRequest::Excel { .. } => "xlsx",
            ExportRequest::PowerPoint => "pptx",
            ExportRequest::Images { image, .. } => match image {
                ImageFormat::Png => "png",
                ImageFormat::Jpeg { .. } => "jpg",
                ImageFormat::Tiff { .. } => "tiff",
            },
            ExportRequest::WebPage { .. } => "html",
            ExportRequest::Text { .. } => "txt",
        }
    }
}

/// what happened, in plain words, for the dialog's result line.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOutcome {
    /// every file made, in order
    pub files: Vec<PathBuf>,
    /// one or two sentences: what was made and how good it is
    pub summary: String,
    /// things the person should know (fonts that could not be embedded,
    /// pictures that stayed pictures), one sentence each
    pub notes: Vec<String>,
    /// the honest check, when the engine made one ("Checked with Word:
    /// every letter within 0.09 pt.")
    pub verification: Option<String>,
    /// the measured numbers behind `verification`, for "How close is it?"
    pub closeness: Option<Closeness>,
    /// fonts replaced by a similar one, each "“X”: why"
    pub stand_ins: Vec<String>,
}

/// how close an exact match word document came, measured through word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Closeness {
    pub glyphs: usize,
    pub max_pt: f64,
    pub mean_pt: f64,
    pub p99_pt: f64,
    /// fractions of glyphs, 0 to 1
    pub within_0_025: f64,
    pub within_0_06: f64,
    pub within_0_125: f64,
    /// every glyph within 0.125 pt, mean under 0.05 pt, and the same fonts
    pub pass: bool,
}

/// how far an export has got, sent to the ui while it runs.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    /// 0 to 1
    pub fraction: f32,
    /// the page being worked on, counted from 1, when there is one
    pub page: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialogs_json_reads_as_a_job() {
        let job: ExportJob = serde_json::from_str(
            r#"{"request":{"format":"images","image":{"kind":"jpeg","quality":80},"dpi":150,"color":"grayscale"},
                "pages":{"pages":[0,2]},"destination":"C:/out/Essay.jpg"}"#,
        )
        .unwrap();
        assert_eq!(job.pages, PageSelection::Pages(vec![0, 2]));
        assert_eq!(job.request.extension(), "jpg");
        let word: ExportRequest =
            serde_json::from_str(r#"{"format":"word","style":"easyToEdit"}"#).unwrap();
        assert_eq!(
            word,
            ExportRequest::Word {
                style: ExportStyle::EasyToEdit
            }
        );
        let page: ExportRequest =
            serde_json::from_str(r#"{"format":"webPage","style":"exactMatch"}"#).unwrap();
        assert_eq!(
            page,
            ExportRequest::WebPage {
                style: ExportStyle::ExactMatch,
                single_file: true
            }
        );
    }
}
