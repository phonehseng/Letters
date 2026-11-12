//! finding fonts that look alike or take up the same room.
//!
//! "metric compatible" fonts give every character the same width, so a
//! document set in one breaks its lines in exactly the same places in the
//! other (Arimo for Arial, Carlito for Calibri). those come first. the
//! rest are ranked by how they look: x-height, cap height, stroke weight,
//! contrast, width and the panose classification.

use crate::facts::ASCII_SAMPLE;
use crate::index::{Family, FontIndex};
use crate::model::Measurements;
use crate::substitute;

/// how alike two sets of widths are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidthComparison {
    /// characters both fonts have
    pub shared: usize,
    /// the average difference, thousandths of the em
    pub mean: f32,
    /// the largest difference, thousandths of the em
    pub max: u16,
}

pub fn compare_widths(a: &Measurements, b: &Measurements) -> WidthComparison {
    let mut shared = 0usize;
    let mut total = 0u32;
    let mut max = 0u16;
    for (first, second) in a.ascii_widths.iter().zip(&b.ascii_widths) {
        if *first == 0 || *second == 0 {
            continue;
        }
        let difference = first.abs_diff(*second);
        shared += 1;
        total += u32::from(difference);
        max = max.max(difference);
    }
    WidthComparison {
        shared,
        mean: if shared == 0 {
            f32::MAX
        } else {
            total as f32 / shared as f32
        },
        max,
    }
}

/// the same widths for nearly every printable ascii character. one per
/// mille of rounding is allowed.
pub fn metric_compatible(a: &Measurements, b: &Measurements) -> bool {
    let comparison = compare_widths(a, b);
    comparison.shared >= ASCII_SAMPLE.count() - 5 && comparison.mean <= 1.0 && comparison.max <= 4
}

fn average_letter_width(measurements: &Measurements) -> f32 {
    let letters: Vec<f32> = ASCII_SAMPLE
        .zip(&measurements.ascii_widths)
        .filter(|(c, width)| c.is_ascii_lowercase() && **width > 0)
        .map(|(_, width)| f32::from(*width))
        .collect();
    if letters.is_empty() {
        500.0
    } else {
        letters.iter().sum::<f32>() / letters.len() as f32
    }
}

/// how different two fonts look; 0 is identical, 1 is a clear but
/// modest difference.
pub fn look_distance(a: &Measurements, b: &Measurements) -> f32 {
    let x_height = f32::from(a.x_height.abs_diff(b.x_height)) / 40.0;
    let cap_height = f32::from(a.cap_height.abs_diff(b.cap_height)) / 60.0;
    let stem = (a.stem - b.stem).abs() / 25.0;
    let contrast = (a.contrast.max(0.1) / b.contrast.max(0.1)).ln().abs() * 2.0;
    let width = (average_letter_width(a) - average_letter_width(b)).abs() / 40.0;
    // panose's serif style and proportion say a lot when both have them
    let panose = if a.panose[0] != 0 && b.panose[0] != 0 {
        f32::from(u8::from(a.panose[1] != b.panose[1]) + u8::from(a.panose[3] != b.panose[3])) * 0.5
    } else {
        0.0
    };
    x_height + cap_height + stem + contrast + width + panose
}

/// one look-alike.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Similar {
    pub family: String,
    pub metric_compatible: bool,
    /// smaller is more alike
    pub distance: f32,
}

/// families that look like `name`, most alike first, at most `limit`.
pub fn similar_to(index: &FontIndex, name: &str, limit: usize) -> Vec<Similar> {
    let target = index.family(name);
    let target_measurements = target
        .and_then(|family| family.regular())
        .map(|face| &face.facts.measurements);
    let target_category = target.map(Family::category);
    let known = substitute::substitute_for(name);
    let mut found: Vec<Similar> = index
        .families
        .iter()
        .filter(|family| Some(family.key.as_str()) != target.map(|target| target.key.as_str()))
        .filter_map(|family| {
            let from_table =
                known.is_some_and(|known| crate::index::family_key(known.with) == family.key);
            let measured = family.regular().map(|face| &face.facts.measurements);
            let (metric, distance) = match (target_measurements, measured) {
                (Some(a), Some(b)) => (metric_compatible(a, b), look_distance(a, b)),
                _ => (false, 8.0),
            };
            let metric = metric || from_table && known.is_some_and(|known| known.metric_compatible);
            let category_penalty = match target_category {
                Some(category) if category != family.category() => 3.0,
                _ => 0.0,
            };
            let distance = if from_table {
                distance.min(0.5)
            } else {
                distance + category_penalty
            };
            // without measurements on both sides only the table and the
            // category can speak; keep those to the same category
            if target_measurements.is_none()
                && !from_table
                && (category_penalty > 0.0 || target_category.is_none())
            {
                return None;
            }
            Some(Similar {
                family: family.name.clone(),
                metric_compatible: metric,
                distance,
            })
        })
        .collect();
    found.sort_by(|a, b| {
        b.metric_compatible
            .cmp(&a.metric_compatible)
            .then(a.distance.total_cmp(&b.distance))
            .then_with(|| a.family.cmp(&b.family))
    });
    found.truncate(limit);
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::Catalogue;
    use crate::index::tests::record;
    use crate::model::Source;

    #[test]
    fn same_widths_are_metric_compatible() {
        let a = record("A", "Regular", 400, false, Source::System)
            .facts
            .measurements;
        let mut b = a.clone();
        assert!(metric_compatible(&a, &b));
        b.ascii_widths[40] += 30;
        assert!(!metric_compatible(&a, &b));
        let mut c = a.clone();
        c.ascii_widths[10] += 1;
        assert!(metric_compatible(&a, &c));
    }

    #[test]
    fn the_metric_twin_comes_first() {
        let mut arial = record("Arial", "Regular", 400, false, Source::System);
        arial.facts.measurements.ascii_widths = (0..95).map(|i| 300 + i * 3).collect();
        let mut arimo = record("Arimo", "Regular", 400, false, Source::Bundled);
        arimo.facts.measurements.ascii_widths = arial.facts.measurements.ascii_widths.clone();
        arimo.facts.measurements.stem = 140.0;
        let lookalike = record("Lookalike", "Regular", 400, false, Source::System);
        let index = FontIndex::build(vec![arial, arimo, lookalike], Catalogue::shipped());
        let found = similar_to(&index, "Arial", 5);
        assert_eq!(found[0].family, "Arimo");
        assert!(found[0].metric_compatible);
        assert!(!found[1].metric_compatible);
    }

    #[test]
    fn the_table_speaks_for_fonts_not_on_this_computer() {
        let index = FontIndex::build(Vec::new(), Catalogue::shipped());
        let found = similar_to(&index, "Calibri", 10);
        assert_eq!(found[0].family, "Carlito");
        assert!(found[0].metric_compatible);
    }
}
