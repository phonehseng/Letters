//! the asn.1 structures of the time-stamp protocol (rfc 3161): the request
//! Letters sends, the response a time stamp authority gives back, and the
//! signed time-stamp info (TSTInfo) inside the token.

use der::asn1::{Int, ObjectIdentifier, OctetString};
use der::{Any, Decode, Encode};
use spki::AlgorithmIdentifierOwned;

/// MessageImprint: which hash, and the hash itself (rfc 3161 section 2.4.1).
#[derive(Clone, Debug, PartialEq, Eq, der::Sequence)]
pub struct MessageImprint {
    pub hash_algorithm: AlgorithmIdentifierOwned,
    pub hashed_message: OctetString,
}

/// TimeStampReq (rfc 3161 section 2.4.1). version is always 1.
#[derive(Clone, Debug, PartialEq, Eq, der::Sequence)]
pub struct TimeStampReq {
    pub version: u8,
    pub message_imprint: MessageImprint,
    #[asn1(optional = "true")]
    pub req_policy: Option<ObjectIdentifier>,
    /// a random number the answer must repeat, so an old answer can't be replayed
    #[asn1(optional = "true")]
    pub nonce: Option<Int>,
    /// true asks the authority to put its certificate in the token
    #[asn1(optional = "true")]
    pub cert_req: Option<bool>,
}

/// PKIStatusInfo (rfc 3161 section 2.4.2). status 0 is granted and 1 is
/// granted with modifications; anything else is a refusal.
#[derive(Clone, Debug, PartialEq, Eq, der::Sequence)]
pub struct PkiStatusInfo {
    pub status: u32,
    #[asn1(optional = "true")]
    pub status_string: Option<Vec<Any>>,
    #[asn1(optional = "true")]
    pub fail_info: Option<der::asn1::BitString>,
}

/// the parts of TimeStampResp Letters reads: the status and the token,
/// kept as its encoded ContentInfo.
pub struct TimeStampResp {
    pub status: u32,
    pub status_text: Option<String>,
    pub token: Option<Vec<u8>>,
}

/// Accuracy (rfc 3161 section 2.4.2): how far the time may be off.
#[derive(Clone, Debug, PartialEq, Eq, der::Sequence)]
pub struct Accuracy {
    #[asn1(optional = "true")]
    pub seconds: Option<Int>,
    #[asn1(context_specific = "0", tag_mode = "IMPLICIT", optional = "true")]
    pub millis: Option<Int>,
    #[asn1(context_specific = "1", tag_mode = "IMPLICIT", optional = "true")]
    pub micros: Option<Int>,
}

/// TSTInfo (rfc 3161 section 2.4.2). the time is kept as raw text, since
/// authorities may add fractions of a second that a strict reader refuses.
#[derive(Clone, Debug, PartialEq, Eq, der::Sequence)]
pub struct TstInfo {
    pub version: u8,
    pub policy: ObjectIdentifier,
    pub message_imprint: MessageImprint,
    pub serial_number: Int,
    pub gen_time: Any,
    #[asn1(optional = "true")]
    pub accuracy: Option<Accuracy>,
    #[asn1(optional = "true")]
    pub ordering: Option<bool>,
    #[asn1(optional = "true")]
    pub nonce: Option<Int>,
    #[asn1(context_specific = "0", tag_mode = "EXPLICIT", optional = "true")]
    pub tsa: Option<Any>,
}

/// TimeStampResp is read by hand: its token is a whole ContentInfo whose
/// exact bytes are kept, so the signature over them can be checked.
pub fn parse_response(bytes: &[u8]) -> Result<TimeStampResp, String> {
    let outer = Any::from_der(bytes).map_err(|error| error.to_string())?;
    let mut reader = der::SliceReader::new(outer.value()).map_err(|error| error.to_string())?;
    let status_info =
        PkiStatusInfo::decode(&mut reader).map_err(|error: der::Error| error.to_string())?;
    let token = if der::Reader::is_finished(&reader) {
        None
    } else {
        let token = Any::decode(&mut reader).map_err(|error: der::Error| error.to_string())?;
        Some(token.to_der().map_err(|error| error.to_string())?)
    };
    let status_text = status_info.status_string.as_ref().map(|texts| {
        texts
            .iter()
            .map(|text| String::from_utf8_lossy(text.value()).into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    });
    Ok(TimeStampResp {
        status: status_info.status,
        status_text,
        token,
    })
}

/// reads a GeneralizedTime's text, "20261112093000Z" or with a fraction
/// "20261112093000.123Z", into seconds since 1970 (fractions dropped).
pub fn generalized_time_seconds(time: &Any) -> Option<i64> {
    let text = std::str::from_utf8(time.value()).ok()?;
    let digits: String = text.chars().take(14).collect();
    if digits.len() != 14 || !digits.chars().all(|c| c.is_ascii_digit()) || !text.ends_with('Z') {
        return None;
    }
    let part = |range: std::ops::Range<usize>| digits[range].parse::<i64>().ok();
    let (year, month, day) = (part(0..4)?, part(4..6)?, part(6..8)?);
    let (hour, minute, second) = (part(8..10)?, part(10..12)?, part(12..14)?);
    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

/// days since 1970-01-01 for a calendar date (howard hinnant's algorithm,
/// the inverse of `utc_parts` in sign.rs).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generalized_times_with_and_without_fractions() {
        let plain = Any::new(der::Tag::GeneralizedTime, b"20261112093000Z".to_vec()).unwrap();
        let fraction = Any::new(der::Tag::GeneralizedTime, b"20261112093000.25Z".to_vec()).unwrap();
        assert_eq!(generalized_time_seconds(&plain), Some(1_794_475_800));
        assert_eq!(generalized_time_seconds(&fraction), Some(1_794_475_800));
        let epoch = Any::new(der::Tag::GeneralizedTime, b"19700101000000Z".to_vec()).unwrap();
        assert_eq!(generalized_time_seconds(&epoch), Some(0));
    }

    #[test]
    fn requests_encode_and_decode() {
        let request = TimeStampReq {
            version: 1,
            message_imprint: MessageImprint {
                hash_algorithm: AlgorithmIdentifierOwned {
                    oid: super::super::oids::SHA256,
                    parameters: None,
                },
                hashed_message: OctetString::new(vec![7u8; 32]).unwrap(),
            },
            req_policy: None,
            nonce: Some(Int::new(&[1, 2, 3]).unwrap()),
            cert_req: Some(true),
        };
        let der = request.to_der().unwrap();
        assert_eq!(TimeStampReq::from_der(&der).unwrap(), request);
    }
}
