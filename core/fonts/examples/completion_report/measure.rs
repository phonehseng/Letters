//! measuring a completion against the true glyphs, and judging how
//! visible the seam is.

use super::*;

/// the measurements of one completion against the truth.
#[derive(Default)]
pub(crate) struct Measured {
    pub(crate) glyphs: usize,
    pub(crate) mean_distance: f64,
    pub(crate) max_distance: f64,
    pub(crate) advance_error: f64,
    pub(crate) ssim: f64,
    pub(crate) differing: f64,
    pub(crate) shape_ssim: f64,
    pub(crate) shape_differing: f64,
    pub(crate) strategies: BTreeMap<Strategy, usize>,
}

pub(crate) fn measure(
    completed: &CompletedFont,
    subset: &SubsetFont,
    truth: &GlyphSet,
    text: &str,
    only: Option<&[char]>,
    dump_name: Option<&str>,
) -> Measured {
    let mut measured = Measured::default();
    for provenance in &completed.glyphs {
        *measured.strategies.entry(provenance.strategy).or_default() += 1;
    }
    let (mut total, mut advance_total) = (0.0, 0.0);
    for (c, glyph) in &completed.new_glyphs {
        if only.is_some_and(|only| !only.contains(c)) {
            continue;
        }
        let Some(true_glyph) = truth.get(*c) else {
            continue;
        };
        let distance = shape_distance(&Samples::of(&true_glyph.shape), &Samples::of(&glyph.shape));
        if std::env::var_os("LETTERS_VERBOSE").is_some() {
            let (t, g) = (true_glyph.shape.bounds(), glyph.shape.bounds());
            eprintln!(
                "    {c}: mean {:.2} max {:.1} advance {:+.1} true box {:?} made box {:?}",
                distance.mean * 1000.0,
                distance.max * 1000.0,
                (glyph.advance - true_glyph.advance) * 1000.0,
                t.map(|b| [
                    (b.x_min * 1000.0) as i32,
                    (b.y_min * 1000.0) as i32,
                    (b.x_max * 1000.0) as i32,
                    (b.y_max * 1000.0) as i32
                ]),
                g.map(|b| [
                    (b.x_min * 1000.0) as i32,
                    (b.y_min * 1000.0) as i32,
                    (b.x_max * 1000.0) as i32,
                    (b.y_max * 1000.0) as i32
                ]),
            );
        }
        total += distance.mean;
        measured.max_distance = measured.max_distance.max(distance.max);
        advance_total += (glyph.advance - true_glyph.advance).abs();
        measured.glyphs += 1;
    }
    let n = measured.glyphs.max(1) as f64;
    measured.mean_distance = total / n;
    measured.advance_error = advance_total / n;

    let metrics = &truth.metrics;
    let margin = 6.0;
    let baseline = margin + metrics.ascender * PIXELS_PER_EM;
    let height =
        ((metrics.ascender - metrics.descender) * PIXELS_PER_EM + 2.0 * margin).ceil() as usize;
    let true_glyph = |c: char| truth.get(c).cloned();
    let made_glyph = |c: char| {
        subset
            .glyph(c)
            .cloned()
            .or_else(|| completed.new_glyphs.get(&c).cloned())
    };
    // the made shapes set at the true advances: shapes alone, no drift
    let made_at_true_advance = |c: char| {
        let mut glyph = made_glyph(c)?;
        glyph.advance = truth.get(c)?.advance;
        Some(glyph)
    };
    let (true_line, true_width) = line_segments(text, &true_glyph, baseline, margin);
    let (made_line, made_width) = line_segments(text, &made_glyph, baseline, margin);
    let (shapes_line, _) = line_segments(text, &made_at_true_advance, baseline, margin);
    let width = true_width.max(made_width).ceil() as usize + 2;
    let a = fill(&true_line, width, height);
    let b = fill(&made_line, width, height);
    let shapes = fill(&shapes_line, width, height);
    measured.ssim = ssim(&a, &b);
    measured.differing = differing_pixels(&a, &b);
    measured.shape_ssim = ssim(&a, &shapes);
    measured.shape_differing = differing_pixels(&a, &shapes);
    if let (Some(folder), Some(name)) = (std::env::var_os("LETTERS_DUMP"), dump_name) {
        let folder = PathBuf::from(folder);
        dump(&folder, &format!("{name}-true"), &a);
        dump(&folder, &format!("{name}-made"), &b);
    }
    measured
}

pub(crate) const SEAM_WORDS: [&str; 4] = [
    "invisible",
    "very hard to see",
    "visible on close reading",
    "plainly visible",
];

/// how visible the new letters' shapes are, 0 (invisible) to 3, from the
/// sentence drawn with the made shapes at the true advances.
pub(crate) fn shape_class(measured: &Measured) -> usize {
    let (ssim, differing) = (measured.shape_ssim, measured.shape_differing);
    if ssim >= 0.995 && differing <= 1.0 {
        0
    } else if ssim >= 0.97 && differing <= 8.0 {
        1
    } else if ssim >= 0.93 && differing <= 18.0 {
        2
    } else {
        3
    }
}

/// how visible the spacing is, 0 to 3, from the mean advance error: a
/// thousandth of the em is a fifth of a pixel at 11 pt and 300 dpi, and
/// letter spacing off by 1% of the em starts to show in a line.
pub(crate) fn spacing_class(measured: &Measured) -> usize {
    match measured.advance_error * 1000.0 {
        error if error <= 1.0 => 0,
        error if error <= 4.0 => 1,
        error if error <= 10.0 => 2,
        _ => 3,
    }
}

/// how visible the seam is, in words: the worse of shapes and spacing.
pub(crate) fn seam(measured: &Measured) -> &'static str {
    if measured.glyphs == 0 {
        return "nothing added";
    }
    SEAM_WORDS[shape_class(measured).max(spacing_class(measured))]
}

pub(crate) fn strategy_mix(measured: &Measured) -> String {
    measured
        .strategies
        .iter()
        .map(|(strategy, count)| format!("{count} {}", strategy.label()))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn used_donors(completed: &CompletedFont) -> String {
    let mut names: Vec<String> = completed
        .glyphs
        .iter()
        .filter_map(|g| g.donor.as_ref())
        .map(|d| {
            if d.style.eq_ignore_ascii_case("regular") {
                d.family.clone()
            } else {
                format!("{} {}", d.family, d.style)
            }
        })
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        "none".into()
    } else {
        names.join(", ")
    }
}
