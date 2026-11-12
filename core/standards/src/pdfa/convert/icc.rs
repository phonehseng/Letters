//! an srgb colour profile made here, so Letters ships no third-party
//! profile and there is no licence to track. it is an icc version 2
//! display profile (icc.1:2001-04) with the srgb primaries adapted to the
//! d50 white of the profile connection space and the srgb tone curve
//! (iec 61966-2-1) as a 1024 point table.

/// the description written into the profile.
const DESCRIPTION: &str = "sRGB IEC61966-2.1 (Letters)";
const COPYRIGHT: &str = "No copyright, use freely";
/// points of the tone curve table: enough that a curve read back by
/// interpolation is within a fraction of a level of 8-bit srgb.
const CURVE_POINTS: usize = 1024;

/// the srgb red, green and blue colorants and the media white point, as
/// xyz under d50 (the bradford adapted values every srgb profile carries).
const RED: [f64; 3] = [0.436_074_7, 0.222_504_5, 0.013_932_2];
const GREEN: [f64; 3] = [0.385_064_9, 0.716_878_6, 0.097_104_5];
const BLUE: [f64; 3] = [0.143_080_4, 0.060_616_9, 0.714_173_3];
const D50_WHITE: [f64; 3] = [0.964_2, 1.0, 0.824_9];

/// the profile's bytes.
pub fn srgb_profile() -> Vec<u8> {
    let curve = curve_tag();
    let tags: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"desc", description_tag(DESCRIPTION)),
        (*b"cprt", text_tag(COPYRIGHT)),
        (*b"wtpt", xyz_tag(D50_WHITE)),
        (*b"rXYZ", xyz_tag(RED)),
        (*b"gXYZ", xyz_tag(GREEN)),
        (*b"bXYZ", xyz_tag(BLUE)),
        (*b"rTRC", curve.clone()),
        (*b"gTRC", curve.clone()),
        (*b"bTRC", curve),
    ];
    assemble(&tags)
}

/// the 128 byte header, the tag table, then the tags, each on a four byte
/// boundary (icc.1 section 6).
fn assemble(tags: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let table_size = 4 + tags.len() * 12;
    let mut offset = 128 + table_size;
    let mut table = (tags.len() as u32).to_be_bytes().to_vec();
    let mut data = Vec::new();
    for (signature, bytes) in tags {
        table.extend_from_slice(signature);
        table.extend_from_slice(&(offset as u32).to_be_bytes());
        table.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        let mut padded = bytes.clone();
        while padded.len() % 4 != 0 {
            padded.push(0);
        }
        offset += padded.len();
        data.extend(padded);
    }
    let total = 128 + table.len() + data.len();
    let mut profile = header(total as u32);
    profile.extend(table);
    profile.extend(data);
    profile
}

fn header(size: u32) -> Vec<u8> {
    let mut header = vec![0u8; 128];
    header[0..4].copy_from_slice(&size.to_be_bytes());
    // version 2.1.0
    header[8] = 0x02;
    header[9] = 0x10;
    header[12..16].copy_from_slice(b"mntr");
    header[16..20].copy_from_slice(b"RGB ");
    header[20..24].copy_from_slice(b"XYZ ");
    // the date fields (24..36) are left at zero: a fixed date keeps the
    // profile the same byte for byte every time
    header[36..40].copy_from_slice(b"acsp");
    // rendering intent 0 (perceptual) at 64..68, then the pcs illuminant
    header[68..80].copy_from_slice(&xyz_numbers(D50_WHITE));
    header
}

/// s15Fixed16Number: a signed 16.16 fixed point number (icc.1 5.3.4).
fn s15_fixed16(value: f64) -> [u8; 4] {
    ((value * 65_536.0).round() as i32).to_be_bytes()
}

fn xyz_numbers(xyz: [f64; 3]) -> Vec<u8> {
    xyz.iter().flat_map(|value| s15_fixed16(*value)).collect()
}

fn xyz_tag(xyz: [f64; 3]) -> Vec<u8> {
    let mut tag = b"XYZ \0\0\0\0".to_vec();
    tag.extend(xyz_numbers(xyz));
    tag
}

fn text_tag(text: &str) -> Vec<u8> {
    let mut tag = b"text\0\0\0\0".to_vec();
    tag.extend_from_slice(text.as_bytes());
    tag.push(0);
    tag
}

/// textDescriptionType (icc.1:2001 6.5.17): ascii, then empty unicode
/// and scriptcode parts.
fn description_tag(text: &str) -> Vec<u8> {
    let mut tag = b"desc\0\0\0\0".to_vec();
    tag.extend_from_slice(&((text.len() + 1) as u32).to_be_bytes());
    tag.extend_from_slice(text.as_bytes());
    tag.push(0);
    // unicode language code and count (none)
    tag.extend_from_slice(&[0u8; 8]);
    // scriptcode code, count and its 67 byte field
    tag.extend_from_slice(&[0u8; 3]);
    tag.extend_from_slice(&[0u8; 67]);
    tag
}

/// the srgb tone curve: linear near black, a 2.4 power above
/// (iec 61966-2-1 5.2), as 16-bit samples.
fn curve_tag() -> Vec<u8> {
    let mut tag = b"curv\0\0\0\0".to_vec();
    tag.extend_from_slice(&(CURVE_POINTS as u32).to_be_bytes());
    for index in 0..CURVE_POINTS {
        let encoded = index as f64 / (CURVE_POINTS - 1) as f64;
        let linear = if encoded <= 0.040_45 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        };
        tag.extend_from_slice(&((linear * 65_535.0).round() as u16).to_be_bytes());
    }
    tag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_is_well_formed() {
        let profile = srgb_profile();
        let size = u32::from_be_bytes([profile[0], profile[1], profile[2], profile[3]]) as usize;
        assert_eq!(size, profile.len());
        assert_eq!(&profile[36..40], b"acsp");
        assert_eq!(&profile[16..20], b"RGB ");
        let count =
            u32::from_be_bytes([profile[128], profile[129], profile[130], profile[131]]) as usize;
        assert_eq!(count, 9);
        for tag in 0..count {
            let at = 132 + tag * 12;
            let offset = u32::from_be_bytes(profile[at + 4..at + 8].try_into().unwrap()) as usize;
            let length = u32::from_be_bytes(profile[at + 8..at + 12].try_into().unwrap()) as usize;
            assert!(offset.is_multiple_of(4) && offset + length <= profile.len());
        }
        assert_eq!(s15_fixed16(1.0), [0, 1, 0, 0]);
    }
}
