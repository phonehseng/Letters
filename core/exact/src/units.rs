//! the units word and pdf use, and the two rounding rules the recipe needs.
//!
//! | name       | size              | where                               |
//! |------------|-------------------|-------------------------------------|
//! | pt         | 1/72 in           | pdf user space                      |
//! | twip       | 1/20 pt           | `w:framePr`, `w:spacing`, `w:pgSz`  |
//! | px         | 1/600 in = 0.12 pt| word's layout grid                  |
//! | u          | 1/3000 in         | word's pdf writer print grid        |
//! | half point | 0.5 pt            | `w:sz`                              |
//! | emu        | 1/12700 pt        | drawingml                           |

pub const TWIPS_PER_PT: f64 = 20.0;
pub const PT_PER_PIXEL: f64 = 0.12;
pub const EMU_PER_PT: f64 = 12700.0;

/// rounds half to even, the way python's `round` does.
///
/// the prototype's measured plans were made with python, and a few of its
/// rules land exactly on halves (an advance of 6 units over 12 units per
/// pixel, for example), so the port must break those ties the same way.
pub fn round_half_even(value: f64) -> f64 {
    value.round_ties_even()
}

/// rounds half up, the way fonttools' `otRound` does when it scales fonts.
pub fn round_half_up(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// points to twips, rounded half to even like the prototype.
pub fn pt_to_twips(points: f64) -> i64 {
    round_half_even(points * TWIPS_PER_PT) as i64
}

/// points to english metric units for drawingml.
pub fn pt_to_emu(points: f64) -> i64 {
    round_half_even(points * EMU_PER_PT) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_go_to_even_or_up() {
        assert_eq!(round_half_even(6.5), 6.0);
        assert_eq!(round_half_even(7.5), 8.0);
        assert_eq!(round_half_even(-2.5), -2.0);
        assert_eq!(round_half_up(6.5), 7.0);
        assert_eq!(round_half_up(-2.5), -2.0);
    }

    #[test]
    fn twips_and_emu() {
        // the prototype's own cases (test_units.py)
        assert_eq!(pt_to_twips(72.37), 1447);
        assert_eq!(pt_to_emu(1.0), 12700);
    }
}
