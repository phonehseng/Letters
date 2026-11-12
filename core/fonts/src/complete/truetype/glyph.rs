//! turning outlines into TrueType glyph data, and the glyf, loca and hmtx
//! tables that list every glyph.

use super::*;

/// one glyph ready for the glyf table.
pub(crate) struct EncodedGlyph {
    pub(crate) bytes: Vec<u8>,
    pub(crate) bounds: (i16, i16, i16, i16),
    pub(crate) points: u16,
    pub(crate) contours: u16,
    pub(crate) advance: u16,
    pub(crate) lsb: i16,
}

impl EncodedGlyph {
    pub(crate) fn empty(advance: u16) -> EncodedGlyph {
        EncodedGlyph {
            bytes: Vec::new(),
            bounds: (0, 0, 0, 0),
            points: 0,
            contours: 0,
            advance,
            lsb: 0,
        }
    }
}

pub(crate) fn to_units(value: f64, units: f64) -> i32 {
    (value * units).round().clamp(-32_000.0, 32_000.0) as i32
}

/// turns an outline into TrueType glyph data: clean, extremes added,
/// TrueType direction, coordinates rounded to font units.
pub(crate) fn encode_glyph(shape: &Shape, advance: f64, units: f64) -> EncodedGlyph {
    let advance_units = to_units(advance, units).clamp(0, 65_535) as u16;
    let prepared = shape.cleaned().with_truetype_direction().with_extrema();
    let mut contours: Vec<Vec<(i32, i32, bool)>> = Vec::new();
    for contour in &prepared.contours {
        let mut points: Vec<(i32, i32, bool)> = Vec::new();
        for node in &contour.nodes {
            let point = (to_units(node.x, units), to_units(node.y, units), node.on);
            if let Some(last) = points.last_mut() {
                if last.0 == point.0 && last.1 == point.1 {
                    last.2 |= point.2;
                    continue;
                }
            }
            points.push(point);
        }
        while points.len() > 1
            && points[0].0 == points[points.len() - 1].0
            && points[0].1 == points[points.len() - 1].1
        {
            let last = points.pop().expect("checked above");
            points[0].2 |= last.2;
        }
        if points.len() >= 3 {
            contours.push(points);
        }
    }
    if contours.is_empty() {
        return EncodedGlyph::empty(advance_units);
    }
    let all = contours.iter().flatten();
    let x_min = all.clone().map(|p| p.0).min().unwrap_or(0);
    let x_max = all.clone().map(|p| p.0).max().unwrap_or(0);
    let y_min = all.clone().map(|p| p.1).min().unwrap_or(0);
    let y_max = all.map(|p| p.1).max().unwrap_or(0);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(contours.len() as i16).to_be_bytes());
    for value in [x_min, y_min, x_max, y_max] {
        bytes.extend_from_slice(&(value as i16).to_be_bytes());
    }
    let mut end = 0usize;
    for contour in &contours {
        end += contour.len();
        bytes.extend_from_slice(&((end - 1) as u16).to_be_bytes());
    }
    // no instructions: new glyphs are unhinted
    bytes.extend_from_slice(&0u16.to_be_bytes());
    let points: Vec<(i32, i32, bool)> = contours.iter().flatten().copied().collect();
    encode_points(&points, &mut bytes);
    EncodedGlyph {
        bytes,
        bounds: (x_min as i16, y_min as i16, x_max as i16, y_max as i16),
        points: points.len() as u16,
        contours: contours.len() as u16,
        advance: advance_units,
        lsb: x_min as i16,
    }
}

/// the flags, then x and y as differences, each as short as allowed.
pub(crate) fn encode_points(points: &[(i32, i32, bool)], out: &mut Vec<u8>) {
    const ON_CURVE: u8 = 0x01;
    const X_SHORT: u8 = 0x02;
    const Y_SHORT: u8 = 0x04;
    const X_SAME_OR_POSITIVE: u8 = 0x10;
    const Y_SAME_OR_POSITIVE: u8 = 0x20;
    let mut flags = Vec::with_capacity(points.len());
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let (mut last_x, mut last_y) = (0i32, 0i32);
    for (x, y, on) in points {
        let (dx, dy) = (x - last_x, y - last_y);
        (last_x, last_y) = (*x, *y);
        let mut flag = if *on { ON_CURVE } else { 0 };
        encode_delta(dx, X_SHORT, X_SAME_OR_POSITIVE, &mut flag, &mut xs);
        encode_delta(dy, Y_SHORT, Y_SAME_OR_POSITIVE, &mut flag, &mut ys);
        flags.push(flag);
    }
    out.extend_from_slice(&flags);
    out.extend_from_slice(&xs);
    out.extend_from_slice(&ys);
}

pub(crate) fn encode_delta(
    delta: i32,
    short: u8,
    same_or_positive: u8,
    flag: &mut u8,
    out: &mut Vec<u8>,
) {
    if delta == 0 {
        *flag |= same_or_positive;
    } else if delta.abs() <= 255 {
        *flag |= short;
        if delta > 0 {
            *flag |= same_or_positive;
        }
        out.push(delta.unsigned_abs() as u8);
    } else {
        out.extend_from_slice(&(delta.clamp(-32_768, 32_767) as i16).to_be_bytes());
    }
}

/// the glyf table (each glyph padded to 4 bytes) and its long loca.
pub(crate) fn glyf_and_loca(glyphs: &[Vec<u8>]) -> (Vec<u8>, Vec<u8>) {
    let mut glyf = Vec::new();
    let mut loca = Vec::with_capacity((glyphs.len() + 1) * 4);
    for bytes in glyphs {
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        glyf.extend_from_slice(bytes);
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
    }
    loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
    (glyf, loca)
}

/// one full entry per glyph: simplest, and always right.
pub(crate) fn hmtx_table(advances: &[u16], lsbs: &[i16]) -> Vec<u8> {
    let mut table = Vec::with_capacity(advances.len() * 4);
    for (advance, lsb) in advances.iter().zip(lsbs) {
        table.extend_from_slice(&advance.to_be_bytes());
        table.extend_from_slice(&lsb.to_be_bytes());
    }
    table
}
