//! excel's built-in number formats: xlsx files refer to these by number
//! alone (numFmtId 0-49) without writing the code. the codes are those
//! of the en-US locale, as listed in ecma-376 part 1, 18.8.30.

/// (id, code) for every built-in format with a fixed code.
const BUILT_IN: &[(u32, &str)] = &[
    (0, "General"),
    (1, "0"),
    (2, "0.00"),
    (3, "#,##0"),
    (4, "#,##0.00"),
    (5, "\"$\"#,##0_);\\(\"$\"#,##0\\)"),
    (6, "\"$\"#,##0_);[Red]\\(\"$\"#,##0\\)"),
    (7, "\"$\"#,##0.00_);\\(\"$\"#,##0.00\\)"),
    (8, "\"$\"#,##0.00_);[Red]\\(\"$\"#,##0.00\\)"),
    (9, "0%"),
    (10, "0.00%"),
    (11, "0.00E+00"),
    (12, "# ?/?"),
    (13, "# ??/??"),
    (14, "m/d/yyyy"),
    (15, "d-mmm-yy"),
    (16, "d-mmm"),
    (17, "mmm-yy"),
    (18, "h:mm AM/PM"),
    (19, "h:mm:ss AM/PM"),
    (20, "h:mm"),
    (21, "h:mm:ss"),
    (22, "m/d/yyyy h:mm"),
    (37, "#,##0 ;(#,##0)"),
    (38, "#,##0 ;[Red](#,##0)"),
    (39, "#,##0.00;(#,##0.00)"),
    (40, "#,##0.00;[Red](#,##0.00)"),
    (41, "_(* #,##0_);_(* \\(#,##0\\);_(* \"-\"_);_(@_)"),
    (
        42,
        "_(\"$\"* #,##0_);_(\"$\"* \\(#,##0\\);_(\"$\"* \"-\"_);_(@_)",
    ),
    (43, "_(* #,##0.00_);_(* \\(#,##0.00\\);_(* \"-\"??_);_(@_)"),
    (
        44,
        "_(\"$\"* #,##0.00_);_(\"$\"* \\(#,##0.00\\);_(\"$\"* \"-\"??_);_(@_)",
    ),
    (45, "mm:ss"),
    (46, "[h]:mm:ss"),
    (47, "mmss.0"),
    (48, "##0.0E+0"),
    (49, "@"),
];

/// the code of a built-in format.
pub fn builtin_code(id: u32) -> Option<&'static str> {
    BUILT_IN
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, code)| *code)
}

/// the built-in id of a code, so files stay as small as excel's own.
pub fn builtin_id(code: &str) -> Option<u32> {
    BUILT_IN
        .iter()
        .find(|(_, known)| *known == code)
        .map(|(id, _)| *id)
}

/// the first id excel uses for formats a file defines itself
pub const FIRST_CUSTOM_ID: u32 = 164;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_codes_match_both_ways() {
        assert_eq!(builtin_code(14), Some("m/d/yyyy"));
        assert_eq!(builtin_id("0.00%"), Some(10));
        assert_eq!(builtin_code(100), None);
    }
}
