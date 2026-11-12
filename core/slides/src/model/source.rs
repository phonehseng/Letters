//! what an element remembers of the file it came from.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceFormat {
    Pptx,
    Odp,
}

/// the xml an element was read from and a fingerprint of the model as
/// read. while the model still has that fingerprint, writers put the xml
/// back byte for byte; once it changes, they rebuild only the pieces that
/// changed and copy the rest (see `crate::preserve`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub format: SourceFormat,
    pub xml: String,
    pub fingerprint: String,
    /// fingerprints of the separately rebuildable pieces ("xfrm",
    /// "geometry", "text"...), so a moved shape keeps its text xml and a
    /// retyped shape keeps its geometry xml
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pieces: Vec<(String, String)>,
    /// for a whole part (a slide, a layout): the package part it was read
    /// from, whose relationship ids its xml uses. a duplicated slide keeps
    /// it, so its pictures still find their relationships
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
}

impl Source {
    pub fn piece(&self, name: &str) -> Option<&str> {
        self.pieces
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// a child element (or an attribute, when `xml` starts without `<`) the
/// model does not read, kept as written. `after` is the name of the
/// modelled element it followed, so it goes back to the same place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawChild {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    pub xml: String,
}

impl RawChild {
    pub fn element(after: Option<&str>, xml: &str) -> RawChild {
        RawChild {
            after: after.map(str::to_string),
            xml: xml.to_string(),
        }
    }
}

/// a stable fingerprint of any model value: fnv-1a over its json (the
/// same scheme letters-office uses for its drawings).
pub fn fingerprint_of<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_vec(value).unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in json {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}
