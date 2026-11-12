//! the render callback, with the pdfium library Letters ships: vector
//! content word can't draw becomes pictures, and the verification compares
//! pixels.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use letters_exact::{convert, ConvertOptions, RenderedPage};
use pdfium_render::prelude::{PdfColor, PdfRenderConfig, Pdfium};

/// pdfium can be bound once per process, so every test shares it.
static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

fn pdfium_dir() -> PathBuf {
    match std::env::var_os("LETTERS_PDFIUM_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium"),
    }
}

fn pdfium() -> Result<&'static Pdfium, String> {
    PDFIUM
        .get_or_init(|| {
            let library = Pdfium::pdfium_platform_library_name_at_path(&pdfium_dir());
            Pdfium::bind_to_library(library)
                .map(Pdfium::new)
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// draws page 0 of a pdf on a transparent background.
fn render(pdf: &[u8], dpi: f32) -> Result<RenderedPage, String> {
    let document = pdfium()?
        .load_pdf_from_byte_slice(pdf, None)
        .map_err(|e| e.to_string())?;
    let page = document.pages().get(0).map_err(|e| e.to_string())?;
    let config = PdfRenderConfig::new()
        .scale_page_by_factor(dpi / 72.0)
        .set_clear_color(PdfColor::new(255, 255, 255, 0))
        .render_form_data(false);
    let bitmap = page
        .render_with_config(&config)
        .map_err(|e| e.to_string())?;
    Ok(RenderedPage {
        width: bitmap.width() as u32,
        height: bitmap.height() as u32,
        rgba: bitmap.as_rgba_bytes(),
    })
}

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
        .join(name)
}

#[test]
fn vectors_word_cannot_draw_become_pictures() {
    if !pdfium_dir().exists() {
        eprintln!("pdfium isn't in vendor/pdfium; run tools/fetch-pdfium first");
        return;
    }
    let out =
        std::env::temp_dir().join(format!("letters-exact-render-{}.docx", std::process::id()));
    let options = ConvertOptions {
        use_word: false,
        render: Some(Arc::new(render)),
        ..ConvertOptions::default()
    };
    let report = convert(&corpus("slides.pdf"), &out, &options).unwrap();
    // the prototype: 9 shapes, 10 vector items in 3 pictures (corpus_report.md)
    let shapes: usize = report.pages.iter().map(|p| p.shapes).sum();
    let rasterised: usize = report.pages.iter().map(|p| p.rasterised_items).sum();
    let pictures: usize = report.pages.iter().map(|p| p.raster_pictures).sum();
    let left_out: usize = report.pages.iter().map(|p| p.vectors_left_out).sum();
    let problems: Vec<&String> = report.pages.iter().flat_map(|p| &p.problems).collect();
    assert_eq!(
        (shapes, rasterised, pictures, left_out),
        (9, 10, 3, 0),
        "{problems:?}"
    );
    let bytes = std::fs::read(&out).unwrap();
    let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let pngs = archive
        .file_names()
        .filter(|name| name.as_ref().is_ok_and(|name| name.ends_with(".png")))
        .count();
    assert!(pngs >= 3, "{pngs} png pictures");
    let _ = std::fs::remove_file(&out);
}

#[test]
#[ignore = "starts microsoft word"]
fn the_verification_compares_pixels_when_pages_can_be_drawn() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target-exact/word-tests/render");
    std::fs::create_dir_all(&dir).unwrap();
    let options = ConvertOptions {
        verify: true,
        render: Some(Arc::new(render)),
        work_dir: Some(dir.join("work")),
        ..ConvertOptions::default()
    };
    let report = convert(&corpus("form.pdf"), &dir.join("form.docx"), &options).unwrap();
    let verification = report.verification.unwrap();
    println!("{}", verification.summary);
    for page in &verification.pages {
        println!(
            "page {}: ssim {:.4}, mean |diff| {:.3}, {} of {} ink pixels moved",
            page.page + 1,
            page.ssim,
            page.mean_abs_diff,
            page.pixels_over_noise,
            page.ink_pixels
        );
    }
    assert_eq!(verification.pages.len(), 1);
    assert!(verification.pages[0].ssim > 0.9);
}
