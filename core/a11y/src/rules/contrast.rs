//! colour contrast of text against what is behind it.
//!
//! the text colour comes from the page content; the background has to be
//! seen, so the page area under the text is drawn by the renderer the
//! caller plugs in. the background is the most common colour in that
//! area that is clearly not the text colour: around and between letters
//! there is far more background than anything else.

use std::collections::{BTreeMap, HashMap};

use crate::colour::{contrast_ratio, nearest_passing_colour, required_ratio, Rgb};
use crate::geometry::Rect;
use crate::report::{ContentRef, ContrastMeasurement, FixKind, Location};
use crate::rules::{Collector, Context, RegionRenderer, RgbaImage};

/// pixels closer than this to the text colour count as text, not background.
const TEXT_COLOUR_DISTANCE: i32 = 48;

/// one group of failing text: same page, colours and requirement.
struct FailingGroup {
    lowest: f64,
    required: f64,
    text_colour: Rgb,
    background: Rgb,
    rect: Rect,
    runs: Vec<ContentRef>,
    count: usize,
    sample: String,
}

pub(crate) fn check(ctx: &Context, renderer: Option<&mut dyn RegionRenderer>, out: &mut Collector) {
    let Some(renderer) = renderer else {
        out.not_checked(
            "low-contrast",
            "Contrast needs a picture of the pages, and none was available for this check.",
        );
        return;
    };
    let mut measured = 0;
    let mut unmeasured = 0;
    for (page, content) in ctx.contents() {
        let (page_width, page_height) = ctx.pages[page as usize].frame.visible_size();
        let page_area = Rect::new(0.0, 0.0, page_width, page_height);
        let mut groups: BTreeMap<(Rgb, Rgb, u64), FailingGroup> = BTreeMap::new();
        let candidates = content
            .texts
            .iter()
            .filter(|run| !run.invisible && !run.text.trim().is_empty())
            .take(ctx.options.contrast_runs_per_page);
        for run in candidates {
            let (Some(text_colour), Some(area)) = (run.colour, run.rect.intersection(&page_area))
            else {
                unmeasured += 1;
                continue;
            };
            let Some(image) = renderer.render_region(page, area, ctx.options.contrast_scale) else {
                unmeasured += 1;
                continue;
            };
            let Some(background) = background_colour(&image, text_colour) else {
                unmeasured += 1;
                continue;
            };
            measured += 1;
            let ratio = contrast_ratio(text_colour, background);
            let required = required_ratio(run.font_size, run.bold);
            if ratio >= required {
                continue;
            }
            let key = (text_colour, background, (required * 10.0) as u64);
            let group = groups.entry(key).or_insert_with(|| FailingGroup {
                lowest: ratio,
                required,
                text_colour,
                background,
                rect: run.rect,
                runs: Vec::new(),
                count: 0,
                sample: run.text.trim().chars().take(40).collect(),
            });
            group.lowest = group.lowest.min(ratio);
            group.rect = group.rect.union(&run.rect);
            group.count += 1;
            // text drawn inside a reusable form can't be recoloured on its own
            if !run.in_form && !group.runs.iter().any(|r| r.op_index == run.op_index) {
                group.runs.push(ContentRef {
                    page,
                    op_index: run.op_index,
                });
            }
        }
        for group in groups.into_values() {
            report_group(out, page, group);
        }
    }
    if measured == 0 {
        let reason = if unmeasured == 0 {
            "The document has no visible text to measure."
        } else {
            "The colours of the text or the page behind it couldn’t be measured."
        };
        out.not_checked("low-contrast", reason);
    }
}

fn report_group(out: &mut Collector, page: u32, group: FailingGroup) {
    let suggested = nearest_passing_colour(group.text_colour, group.background, group.required);
    let details = format!(
        "On page {}, {} in {} on {} has a contrast of {:.2} to 1; it needs {} to 1. For example: “{}”.",
        page + 1,
        if group.count == 1 { "1 piece of text".to_owned() } else { format!("{} pieces of text", group.count) },
        group.text_colour.hex(),
        group.background.hex(),
        group.lowest,
        if group.required < 4.0 { "3" } else { "4.5" },
        group.sample
    );
    let fix = (!group.runs.is_empty()).then(|| FixKind::ChangeTextColour {
        runs: group.runs.clone(),
        suggested,
    });
    let finding = out.report(
        "low-contrast",
        details,
        Location::page(page, Some(group.rect)),
        fix,
    );
    finding.contrast = Some(ContrastMeasurement {
        ratio: group.lowest,
        required: group.required,
        text_colour: group.text_colour,
        background_colour: group.background,
        suggested_colour: suggested,
    });
}

/// the colour behind the text in a drawn area: the most common colour
/// among pixels that are clearly not the text colour. when every pixel is
/// close to the text colour, the text is as good as invisible and that
/// colour is the answer.
pub(crate) fn background_colour(image: &RgbaImage, text: Rgb) -> Option<Rgb> {
    let mut far: HashMap<(u8, u8, u8), (usize, [u64; 3])> = HashMap::new();
    let mut near: HashMap<(u8, u8, u8), (usize, [u64; 3])> = HashMap::new();
    for y in 0..image.height {
        for x in 0..image.width {
            let [r, g, b] = image.pixel(x, y)?;
            let distance_squared = [(r, text.r), (g, text.g), (b, text.b)]
                .iter()
                .map(|&(a, b)| (a as i32 - b as i32).pow(2))
                .sum::<i32>();
            let bucket = (r >> 3, g >> 3, b >> 3);
            let counts = if distance_squared > TEXT_COLOUR_DISTANCE * TEXT_COLOUR_DISTANCE {
                &mut far
            } else {
                &mut near
            };
            let entry = counts.entry(bucket).or_insert((0, [0; 3]));
            entry.0 += 1;
            entry.1[0] += r as u64;
            entry.1[1] += g as u64;
            entry.1[2] += b as u64;
        }
    }
    let counts = if far.is_empty() { near } else { far };
    let (count, sums) = counts.into_values().max_by_key(|(count, _)| *count)?;
    let average = |sum: u64| (sum / count as u64) as u8;
    Some(Rgb::new(
        average(sums[0]),
        average(sums[1]),
        average(sums[2]),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(pixels: &[[u8; 3]]) -> RgbaImage {
        RgbaImage {
            width: pixels.len() as u32,
            height: 1,
            pixels: pixels
                .iter()
                .flat_map(|&[r, g, b]| [r, g, b, 255])
                .collect(),
        }
    }

    #[test]
    fn the_background_is_the_common_colour_that_is_not_the_text() {
        let black = [0, 0, 0];
        let white = [255, 255, 255];
        let pixels = [
            black,
            black,
            black,
            black,
            white,
            white,
            white,
            [128, 128, 128],
        ];
        assert_eq!(
            background_colour(&image(&pixels), Rgb::BLACK),
            Some(Rgb::WHITE)
        );
    }

    #[test]
    fn text_the_same_colour_as_its_background_measures_one_to_one() {
        let grey = [200, 200, 200];
        let background = background_colour(&image(&[grey, grey]), Rgb::new(200, 200, 200)).unwrap();
        assert!((contrast_ratio(background, Rgb::new(200, 200, 200)) - 1.0).abs() < 0.01);
    }
}
