//! the pdf/a conformance levels Letters checks, and what differs between
//! them. part 1 (iso 19005-1) is based on pdf 1.4; parts 2 and 3 (iso
//! 19005-2 and -3) on iso 32000-1 and share their clause numbers.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfaLevel {
    /// pdf/a-1b: the look is preserved
    #[serde(rename = "a1b")]
    A1b,
    /// pdf/a-2b: as 1b, on newer pdf, transparency and layers allowed
    #[serde(rename = "a2b")]
    A2b,
    /// pdf/a-2u: as 2b, and all text can be read out as unicode
    #[serde(rename = "a2u")]
    A2u,
    /// pdf/a-3b: as 2b, and any kind of file may be attached
    #[serde(rename = "a3b")]
    A3b,
}

impl PdfaLevel {
    /// the part of iso 19005: 1, 2 or 3.
    pub fn part(self) -> u8 {
        match self {
            PdfaLevel::A1b => 1,
            PdfaLevel::A2b | PdfaLevel::A2u => 2,
            PdfaLevel::A3b => 3,
        }
    }

    /// the conformance letter written in pdfaid:conformance.
    pub fn conformance(self) -> &'static str {
        match self {
            PdfaLevel::A2u => "U",
            _ => "B",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PdfaLevel::A1b => "PDF/A-1b",
            PdfaLevel::A2b => "PDF/A-2b",
            PdfaLevel::A2u => "PDF/A-2u",
            PdfaLevel::A3b => "PDF/A-3b",
        }
    }

    pub fn is_part_one(self) -> bool {
        self.part() == 1
    }

    /// part 1 forbids transparency (19005-1 6.4); later parts allow it.
    pub fn allows_transparency(self) -> bool {
        !self.is_part_one()
    }

    /// 2u asks for unicode for every font (19005-2 6.2.11.7).
    pub fn needs_unicode(self) -> bool {
        self == PdfaLevel::A2u
    }

    /// the clause reference for this level: `one` in part 1, `later` in
    /// parts 2 and 3 (which share numbering).
    pub fn clause(self, one: &str, later: &str) -> String {
        match self.part() {
            1 => format!("ISO 19005-1 {one}"),
            part => format!("ISO 19005-{part} {later}"),
        }
    }
}
