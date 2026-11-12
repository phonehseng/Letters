//! drawing a line of glyphs at 11 pt and 300 dpi with the crate's own
//! rasteriser, and comparing two drawings (ssim, differing pixels).

use super::*;

/// one line of text as outlines in pixels (y down), from per-character
/// glyphs, laid out by their advances.
pub(crate) fn line_segments(
    text: &str,
    glyphs: &dyn Fn(char) -> Option<GlyphData>,
    baseline: f64,
    margin: f64,
) -> (Vec<Segment>, f64) {
    let mut segments = Vec::new();
    let mut pen = 0.0;
    for c in text.chars() {
        let Some(glyph) = glyphs(c) else { continue };
        let place = |x: f64, y: f64| {
            (
                ((x + pen) * PIXELS_PER_EM + margin) as f32,
                (baseline - y * PIXELS_PER_EM) as f32,
            )
        };
        for contour in &glyph.shape.contours {
            let pieces = contour.pieces();
            let Some(first) = pieces.first() else {
                continue;
            };
            let (x, y) = place(first.from.0, first.from.1);
            segments.push(Segment::Move(x, y));
            for piece in &pieces {
                let (x, y) = place(piece.to.0, piece.to.1);
                match piece.control {
                    None => segments.push(Segment::Line(x, y)),
                    Some(c) => {
                        let (cx, cy) = place(c.0, c.1);
                        segments.push(Segment::Quad(cx, cy, x, y));
                    }
                }
            }
            segments.push(Segment::Close);
        }
        pen += glyph.advance;
    }
    (segments, pen * PIXELS_PER_EM + 2.0 * margin)
}

/// the mean structural similarity over 8 × 8 windows (every 4 pixels)
/// that have ink in either picture; blank windows would only flatter it.
pub(crate) fn ssim(a: &AlphaBitmap, b: &AlphaBitmap) -> f64 {
    let c1 = (0.01f64 * 255.0).powi(2);
    let c2 = (0.03f64 * 255.0).powi(2);
    let mut total = 0.0;
    let mut windows = 0usize;
    let mut y = 0;
    while y + 8 <= a.height {
        let mut x = 0;
        while x + 8 <= a.width {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab, mut ink) =
                (0.0, 0.0, 0.0, 0.0, 0.0, false);
            for dy in 0..8 {
                for dx in 0..8 {
                    let va = f64::from(a.at(x + dx, y + dy));
                    let vb = f64::from(b.at(x + dx, y + dy));
                    ink |= va > 0.0 || vb > 0.0;
                    sa += va;
                    sb += vb;
                    saa += va * va;
                    sbb += vb * vb;
                    sab += va * vb;
                }
            }
            if ink {
                let n = 64.0;
                let (ma, mb) = (sa / n, sb / n);
                let (va, vb) = (saa / n - ma * ma, sbb / n - mb * mb);
                let cov = sab / n - ma * mb;
                total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                    / ((ma * ma + mb * mb + c1) * (va + vb + c2));
                windows += 1;
            }
            x += 4;
        }
        y += 4;
    }
    if windows == 0 {
        1.0
    } else {
        total / windows as f64
    }
}

/// the share of inked pixels (in either picture) whose coverage differs
/// by more than an eighth.
pub(crate) fn differing_pixels(a: &AlphaBitmap, b: &AlphaBitmap) -> f64 {
    let (mut inked, mut differing) = (0usize, 0usize);
    for (va, vb) in a.pixels.iter().zip(&b.pixels) {
        if *va > 32 || *vb > 32 {
            inked += 1;
            if va.abs_diff(*vb) > 32 {
                differing += 1;
            }
        }
    }
    if inked == 0 {
        0.0
    } else {
        differing as f64 / inked as f64 * 100.0
    }
}

pub(crate) fn dump(folder: &Path, name: &str, bitmap: &AlphaBitmap) {
    let mut bytes = format!("P5\n{} {}\n255\n", bitmap.width, bitmap.height).into_bytes();
    bytes.extend(bitmap.pixels.iter().map(|v| 255 - v));
    std::fs::create_dir_all(folder).ok();
    std::fs::write(folder.join(format!("{name}.pgm")), bytes).ok();
}
