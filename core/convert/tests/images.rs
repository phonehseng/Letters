//! pdf pages to png, jpeg and tiff, drawn by letters-pdf.

mod support;

use std::fs;

use letters_convert::*;
use support::*;

const RED: [u8; 3] = [255, 0, 0];

/// pages with a red square one inch from the top left, and a different
/// size per page so each picture can be told apart.
fn three_page_pdf(name: &str) -> (std::path::PathBuf, PdfSource) {
    let dir = temp_dir(name);
    let path = dir.join("squares.pdf");
    let pages: Vec<TestPage> = (0..3)
        .map(|index| {
            let mut page = TestPage::sized(612.0, 792.0 - index as f32 * 72.0);
            page.fill_rect(72.0, 72.0, 72.0, 72.0, RED);
            page
        })
        .collect();
    write_pdf(&path, &pages);
    let source = PdfSource::open(&path).unwrap();
    (dir, source)
}

fn export(
    source: &PdfSource,
    folder: &std::path::Path,
    options: &ImageExportOptions,
) -> Result<Vec<std::path::PathBuf>, ConvertError> {
    let mut renderer = TestRenderer::new();
    pdf_to_images(
        source,
        &mut renderer,
        options,
        folder,
        "Squares",
        &mut |_| {},
        &CancelFlag::new(),
    )
}

#[test]
fn png_pages_have_the_right_size_pixels_and_resolution() {
    let (dir, source) = three_page_pdf("png");
    let options = ImageExportOptions {
        dpi: 144.0,
        ..Default::default()
    };
    let files = export(&source, &dir, &options).unwrap();
    let names: Vec<_> = files
        .iter()
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "Squares - page 1.png",
            "Squares - page 2.png",
            "Squares - page 3.png"
        ]
    );

    let bytes = fs::read(&files[0]).unwrap();
    let image = decode_png(&bytes);
    // 144 dpi is two pixels per point
    assert_eq!((image.width, image.height), (1224, 1584));
    assert_eq!(image.channels, 3);
    // the square covers 144..288 pixels; its middle is red, outside white
    assert_eq!(image.rgb(216, 216), RED);
    assert_eq!(image.rgb(100, 100), [255, 255, 255]);
    assert_eq!(image.rgb(300, 216), [255, 255, 255]);
    // the second page is an inch shorter
    let second = decode_png(&fs::read(&files[1]).unwrap());
    assert_eq!((second.width, second.height), (1224, 1440));

    // pHYs: 144 dpi is 5669 pixels per metre
    let reader = png::Decoder::new(std::io::Cursor::new(&bytes))
        .read_info()
        .unwrap();
    let dims = reader.info().pixel_dims.unwrap();
    assert_eq!(dims.xppu, 5669);
    assert_eq!(dims.unit, png::Unit::Meter);
}

#[test]
fn grayscale_png_has_one_channel() {
    let (dir, source) = three_page_pdf("gray");
    let options = ImageExportOptions {
        dpi: 72.0,
        color: ColorMode::Grayscale,
        pages: PageSelection::Pages(vec![0]),
        ..Default::default()
    };
    let files = export(&source, &dir, &options).unwrap();
    let image = decode_png(&fs::read(&files[0]).unwrap());
    assert_eq!(image.channels, 1);
    // red is 76 in gray (0.299 of full brightness)
    assert_eq!(image.rgb(108, 108), [76, 76, 76]);
    assert_eq!(image.rgb(10, 10), [255, 255, 255]);
}

#[test]
fn jpeg_keeps_quality_size_and_resolution() {
    let (dir, source) = three_page_pdf("jpeg");
    let mut sizes = Vec::new();
    for quality in [20, 95] {
        let folder = dir.join(format!("q{quality}"));
        fs::create_dir_all(&folder).unwrap();
        let options = ImageExportOptions {
            format: ImageFormat::Jpeg { quality },
            dpi: 300.0,
            pages: PageSelection::Pages(vec![2]),
            ..Default::default()
        };
        let files = export(&source, &folder, &options).unwrap();
        assert_eq!(
            files[0].file_name().unwrap().to_string_lossy(),
            "Squares - page 3.jpg"
        );
        let bytes = fs::read(&files[0]).unwrap();
        sizes.push(bytes.len());
        // jfif header: units 1 (dots per inch), then x and y density
        assert_eq!(&bytes[6..11], b"JFIF\0");
        assert_eq!(bytes[13], 1);
        assert_eq!(u16::from_be_bytes([bytes[14], bytes[15]]), 300);
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert_eq!(decoded.dimensions(), (2550, 2700));
        let middle = decoded.get_pixel(450, 450).0;
        assert!(
            middle[0] > 230 && middle[1] < 30 && middle[2] < 30,
            "{middle:?}"
        );
    }
    assert!(
        sizes[0] < sizes[1],
        "lower quality should be smaller: {sizes:?}"
    );
}

fn tiff_pages(path: &std::path::Path) -> Vec<(u32, u32, u16, f64)> {
    use tiff::decoder::Decoder;
    use tiff::tags::Tag;
    let mut decoder = Decoder::new(std::io::BufReader::new(fs::File::open(path).unwrap())).unwrap();
    let mut pages = Vec::new();
    loop {
        let (width, height) = decoder.dimensions().unwrap();
        let compression = decoder.get_tag_u32(Tag::Compression).unwrap() as u16;
        let resolution = match decoder.get_tag(Tag::XResolution).unwrap() {
            tiff::decoder::ifd::Value::Rational(n, d) => f64::from(n) / f64::from(d),
            other => panic!("unexpected resolution {other:?}"),
        };
        decoder.read_image().unwrap();
        pages.push((width, height, compression, resolution));
        if !decoder.more_images() {
            break;
        }
        decoder.next_image().unwrap();
    }
    pages
}

#[test]
fn multi_page_tiff_holds_every_page() {
    let (dir, source) = three_page_pdf("tiff");
    let options = ImageExportOptions {
        format: ImageFormat::Tiff {
            compression: TiffCompression::Lzw,
            multi_page: true,
        },
        dpi: 72.0,
        ..Default::default()
    };
    let files = export(&source, &dir, &options).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file_name().unwrap(), "Squares.tiff");
    let pages = tiff_pages(&files[0]);
    // lzw is compression 5 in tiff's numbering
    assert_eq!(
        pages,
        [
            (612, 792, 5, 72.0),
            (612, 720, 5, 72.0),
            (612, 648, 5, 72.0)
        ]
    );
}

#[test]
fn single_page_tiffs_use_deflate() {
    let (dir, source) = three_page_pdf("tiff-single");
    let options = ImageExportOptions {
        format: ImageFormat::Tiff {
            compression: TiffCompression::Deflate,
            multi_page: false,
        },
        dpi: 100.0,
        pages: PageSelection::Pages(vec![0, 1]),
        ..Default::default()
    };
    let files = export(&source, &dir, &options).unwrap();
    assert_eq!(files.len(), 2);
    // deflate is compression 8
    assert_eq!(tiff_pages(&files[1]), [(850, 1000, 8, 100.0)]);
}

#[test]
fn files_already_there_are_never_replaced() {
    let (dir, source) = three_page_pdf("no-overwrite");
    fs::write(dir.join("Squares - page 1.png"), b"keep me").unwrap();
    let options = ImageExportOptions {
        dpi: 36.0,
        pages: PageSelection::Pages(vec![0]),
        ..Default::default()
    };
    let files = export(&source, &dir, &options).unwrap();
    assert_eq!(
        files[0].file_name().unwrap().to_string_lossy(),
        "Squares - page 1 (2).png"
    );
    assert_eq!(
        fs::read(dir.join("Squares - page 1.png")).unwrap(),
        b"keep me"
    );
}

#[test]
fn progress_is_reported_and_cancelling_leaves_nothing() {
    let (dir, source) = three_page_pdf("cancel");
    let cancel = CancelFlag::new();
    let mut seen = Vec::new();
    let mut renderer = TestRenderer::new();
    let options = ImageExportOptions {
        dpi: 36.0,
        ..Default::default()
    };
    let result = pdf_to_images(
        &source,
        &mut renderer,
        &options,
        &dir,
        "Squares",
        &mut |progress| {
            seen.push(progress);
            if progress.done == 2 {
                cancel.cancel();
            }
        },
        &cancel,
    );
    assert_eq!(result, Err(ConvertError::Cancelled));
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].total, 3);
    // only the pdf is left: the two finished pictures were removed again
    let left: Vec<_> = fs::read_dir(&dir).unwrap().collect();
    assert_eq!(left.len(), 1);
    // one page was drawn per page finished, never more at once
    assert_eq!(renderer.pages_drawn, 2);
}

#[test]
fn locked_empty_and_bad_settings_are_explained() {
    let dir = temp_dir("failures");
    let locked = dir.join("locked.pdf");
    write_encrypted_pdf(&locked);
    let error = PdfSource::open(&locked).err().unwrap();
    assert_eq!(error, ConvertError::Locked);
    assert_eq!(
        error.to_string(),
        "This PDF is locked with a password. Unlock it first, then try again."
    );

    let empty = dir.join("empty.pdf");
    write_empty_pdf(&empty);
    assert_eq!(
        PdfSource::open(&empty).err(),
        Some(ConvertError::EmptyDocument)
    );

    let (dir, source) = three_page_pdf("bad-settings");
    let options = ImageExportOptions {
        format: ImageFormat::Jpeg { quality: 0 },
        ..Default::default()
    };
    assert!(matches!(
        export(&source, &dir, &options),
        Err(ConvertError::InvalidSetting(_))
    ));
    let options = ImageExportOptions {
        pages: PageSelection::Pages(vec![7]),
        ..Default::default()
    };
    assert_eq!(
        export(&source, &dir, &options),
        Err(ConvertError::UnknownPage(7))
    );
}
