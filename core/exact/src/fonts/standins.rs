//! stand-ins for fonts the pdf does not embed, or may not be embedded
//! (recipe 3.8).
//!
//! the standard 14 names map to metric-compatible open fonts (sil ofl 1.1,
//! licenses next to the files in `fonts/`): helvetica and arial to arimo,
//! times to tinos, courier to cousine. anything else gets tinos when the
//! font says it is serif, arimo otherwise. the stand-in's outlines are used
//! with the pdf's widths as advances, so positions stay exact while shapes
//! differ.

static ARIMO_REGULAR: &[u8] = include_bytes!("../../fonts/Arimo-Regular.ttf");
static ARIMO_BOLD: &[u8] = include_bytes!("../../fonts/Arimo-Bold.ttf");
static TINOS_REGULAR: &[u8] = include_bytes!("../../fonts/Tinos-Regular.ttf");
static TINOS_BOLD: &[u8] = include_bytes!("../../fonts/Tinos-Bold.ttf");
static COUSINE_REGULAR: &[u8] = include_bytes!("../../fonts/Cousine-Regular.ttf");

/// every bundled stand-in, regular and bold: (program, file name).
pub fn bundled() -> [(&'static [u8], &'static str); 5] {
    [
        (ARIMO_REGULAR, "Arimo-Regular.ttf"),
        (ARIMO_BOLD, "Arimo-Bold.ttf"),
        (TINOS_REGULAR, "Tinos-Regular.ttf"),
        (TINOS_BOLD, "Tinos-Bold.ttf"),
        (COUSINE_REGULAR, "Cousine-Regular.ttf"),
    ]
}

/// a chosen stand-in: its program, its name and how close it is.
pub struct StandIn {
    pub bytes: &'static [u8],
    pub file: &'static str,
    pub note: String,
}

#[derive(Clone, Copy)]
enum Role {
    Sans,
    Serif,
    Mono,
}

#[derive(Clone, Copy)]
enum Weight {
    Regular,
    Bold,
}

/// the known names: role and weight. italics and obliques have no open
/// stand-in here, so they get the upright face.
fn known(base_name: &str) -> Option<(Role, Weight)> {
    use Role::*;
    use Weight::*;
    Some(match base_name {
        "Helvetica" | "Helvetica-Oblique" | "Arial" | "ArialMT" => (Sans, Regular),
        "Helvetica-Bold" | "Helvetica-BoldOblique" | "Arial-BoldMT" => (Sans, Bold),
        "Times-Roman" | "Times-Italic" | "TimesNewRomanPSMT" => (Serif, Regular),
        "Times-Bold" | "Times-BoldItalic" => (Serif, Bold),
        "Courier" | "Courier-Bold" | "Courier-Oblique" | "Courier-BoldOblique" => (Mono, Regular),
        _ => return None,
    })
}

/// the stand-in for a font that can't be embedded.
pub fn stand_in_for(base_name: &str, serif: bool) -> StandIn {
    let mut note = String::new();
    let (role, weight) = match known(base_name) {
        Some(choice) => {
            note.push_str("metric-compatible stand-in");
            choice
        }
        None => {
            note.push_str("generic stand-in, shapes differ");
            (
                if serif || base_name.starts_with("Times") {
                    Role::Serif
                } else {
                    Role::Sans
                },
                Weight::Regular,
            )
        }
    };
    let styled = base_name.ends_with("Oblique") || base_name.ends_with("Italic");
    if styled || (base_name.starts_with("Courier") && base_name != "Courier") {
        note.push_str(", style not available (upright or regular used)");
    }
    let (bytes, file) = match (role, weight) {
        (Role::Sans, Weight::Regular) => (ARIMO_REGULAR, "Arimo-Regular.ttf"),
        (Role::Sans, Weight::Bold) => (ARIMO_BOLD, "Arimo-Bold.ttf"),
        (Role::Serif, Weight::Regular) => (TINOS_REGULAR, "Tinos-Regular.ttf"),
        (Role::Serif, Weight::Bold) => (TINOS_BOLD, "Tinos-Bold.ttf"),
        (Role::Mono, _) => (COUSINE_REGULAR, "Cousine-Regular.ttf"),
    };
    StandIn { bytes, file, note }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_names_get_metric_compatible_fonts() {
        assert_eq!(stand_in_for("Helvetica-Bold", false).file, "Arimo-Bold.ttf");
        assert_eq!(stand_in_for("Times-Italic", true).file, "Tinos-Regular.ttf");
        assert!(stand_in_for("Times-Italic", true)
            .note
            .contains("style not available"));
        assert_eq!(stand_in_for("Courier", false).file, "Cousine-Regular.ttf");
        assert_eq!(stand_in_for("Minion Pro", true).file, "Tinos-Regular.ttf");
        assert_eq!(stand_in_for("Lato", false).file, "Arimo-Regular.ttf");
    }

    #[test]
    fn the_bundled_fonts_are_readable() {
        for bytes in [
            ARIMO_REGULAR,
            ARIMO_BOLD,
            TINOS_REGULAR,
            TINOS_BOLD,
            COUSINE_REGULAR,
        ] {
            let face = ttf_parser::Face::parse(bytes, 0).unwrap();
            assert!(face.glyph_index('A').is_some());
        }
    }
}
