//! the names of preset shape outlines in both formats: drawingml's
//! `prstGeom` names (which the model uses) and opendocument's
//! `draw:enhanced-geometry draw:type` names.
//!
//! libreoffice also writes "ooxml-<drawingml name>" for shapes that came
//! from word; those map straight back.

/// drawingml name, opendocument name. only outlines both formats draw the
/// same way are listed; the others fall back to the nearest one (see
/// `nearest_odf_type`) and the compatibility report says so.
const PRESETS: &[(&str, &str)] = &[
    ("rect", "rectangle"),
    ("roundRect", "round-rectangle"),
    ("ellipse", "ellipse"),
    ("triangle", "isosceles-triangle"),
    ("rtTriangle", "right-triangle"),
    ("diamond", "diamond"),
    ("parallelogram", "parallelogram"),
    ("trapezoid", "trapezoid"),
    ("pentagon", "pentagon"),
    ("hexagon", "hexagon"),
    ("octagon", "octagon"),
    ("star4", "star4"),
    ("star5", "star5"),
    ("star8", "star8"),
    ("star12", "star12"),
    ("star24", "star24"),
    ("cloud", "cloud"),
    ("heart", "heart"),
    ("smileyFace", "smiley"),
    ("sun", "sun"),
    ("moon", "moon"),
    ("lightningBolt", "lightning"),
    ("can", "can"),
    ("cube", "cube"),
    ("plus", "cross"),
    ("donut", "ring"),
    ("noSmoking", "forbidden"),
    ("blockArc", "block-arc"),
    ("foldedCorner", "paper"),
    ("frame", "frame"),
    ("rightArrow", "right-arrow"),
    ("leftArrow", "left-arrow"),
    ("upArrow", "up-arrow"),
    ("downArrow", "down-arrow"),
    ("leftRightArrow", "left-right-arrow"),
    ("upDownArrow", "up-down-arrow"),
    ("quadArrow", "quad-arrow"),
    ("notchedRightArrow", "notched-right-arrow"),
    ("chevron", "chevron"),
    ("homePlate", "pentagon-right"),
    ("circularArrow", "circular-arrow"),
    ("wedgeRectCallout", "rectangular-callout"),
    ("wedgeRoundRectCallout", "round-rectangular-callout"),
    ("wedgeEllipseCallout", "round-callout"),
    ("cloudCallout", "cloud-callout"),
    ("borderCallout1", "line-callout-1"),
    ("borderCallout2", "line-callout-2"),
    ("borderCallout3", "line-callout-3"),
    ("flowChartProcess", "flowchart-process"),
    ("flowChartAlternateProcess", "flowchart-alternate-process"),
    ("flowChartDecision", "flowchart-decision"),
    ("flowChartTerminator", "flowchart-terminator"),
    ("flowChartDocument", "flowchart-document"),
    ("flowChartInputOutput", "flowchart-data"),
    ("flowChartConnector", "flowchart-connector"),
];

/// the opendocument name for a drawingml preset, when it has one.
pub fn odf_type(preset: &str) -> Option<&'static str> {
    PRESETS
        .iter()
        .find(|(drawingml, _)| *drawingml == preset)
        .map(|(_, odf)| *odf)
}

/// the drawingml preset for an opendocument shape type.
pub fn preset_for_odf_type(kind: &str) -> Option<String> {
    if let Some(name) = kind.strip_prefix("ooxml-") {
        return (name != "non-primitive").then(|| name.to_string());
    }
    PRESETS
        .iter()
        .find(|(_, odf)| *odf == kind)
        .map(|(drawingml, _)| drawingml.to_string())
}

/// the opendocument type to draw a preset with, and whether it is exact.
/// presets libreoffice has no own name for are written as libreoffice
/// writes word's own shapes, "ooxml-<name>", which only libreoffice reads.
pub fn nearest_odf_type(preset: &str) -> (String, bool) {
    match odf_type(preset) {
        Some(kind) => (kind.to_string(), true),
        None => (format!("ooxml-{preset}"), false),
    }
}

/// a rounded rectangle's corner: drawingml gives it in 100000ths of the
/// shorter side, opendocument in 21600ths of it.
pub fn round_rect_adjust_to_odf(adjust: i64) -> i64 {
    (adjust * 21600 + 50000) / 100_000
}

pub fn round_rect_adjust_from_odf(modifier: i64) -> i64 {
    (modifier * 100_000 + 10800) / 21600
}

/// drawingml's default corner of a rounded rectangle (a sixth of the
/// shorter side).
pub const ROUND_RECT_DEFAULT: i64 = 16667;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_names_map_both_ways() {
        assert_eq!(odf_type("roundRect"), Some("round-rectangle"));
        assert_eq!(
            preset_for_odf_type("round-rectangle").as_deref(),
            Some("roundRect")
        );
        assert_eq!(
            preset_for_odf_type("ooxml-wedgeRectCallout").as_deref(),
            Some("wedgeRectCallout")
        );
        assert_eq!(preset_for_odf_type("ooxml-non-primitive"), None);
        assert_eq!(
            nearest_odf_type("plaque"),
            ("ooxml-plaque".to_string(), false)
        );
        for (drawingml, odf) in PRESETS {
            assert_eq!(preset_for_odf_type(odf).as_deref(), Some(*drawingml));
        }
    }

    #[test]
    fn rounded_corners_convert_between_units() {
        assert_eq!(round_rect_adjust_to_odf(ROUND_RECT_DEFAULT), 3600);
        assert_eq!(round_rect_adjust_from_odf(3600), ROUND_RECT_DEFAULT);
    }
}
