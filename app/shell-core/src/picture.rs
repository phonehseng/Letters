//! how big a picture is, read from the start of its file.
//!
//! the picture tool keeps a picture's proportions while it is placed and
//! resized, so it needs the width and height before anything is drawn.
//! png and jpeg (the two kinds the annotation engine takes) both say
//! their size near the start of the file, so only the header is read.

use std::fs;
use std::io::Read;
use std::path::Path;

/// the size of a picture in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PictureSize {
    pub width: u32,
    pub height: u32,
}

/// how much of a file is read to find its size. a jpeg may hold a large
/// preview or colour profile before its size, so this is generous.
const HEADER_BYTES: u64 = 512 * 1024;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// the size of the png or jpeg at `path`, or a sentence saying why not.
pub fn picture_size_of_file(path: &Path) -> Result<PictureSize, String> {
    let mut header = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(HEADER_BYTES).read_to_end(&mut header))
        .map_err(|error| format!("The picture can’t be read. Choose it again. ({error})"))?;
    picture_size(&header)
        .ok_or_else(|| "This picture can’t be used. Choose a PNG or JPEG picture.".to_owned())
}

/// the size written in a png or jpeg header, or none for anything else.
pub fn picture_size(bytes: &[u8]) -> Option<PictureSize> {
    png_size(bytes).or_else(|| jpeg_size(bytes))
}

/// a png starts with its signature and then the IHDR chunk, whose first
/// eight bytes are the width and height.
fn png_size(bytes: &[u8]) -> Option<PictureSize> {
    if bytes.get(..8)? != PNG_SIGNATURE || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    nonzero(width, height)
}

/// a jpeg is a list of marked segments. the "start of frame" segment
/// holds the height and then the width.
fn jpeg_size(bytes: &[u8]) -> Option<PictureSize> {
    if bytes.get(..2)? != [0xff, 0xd8] {
        return None;
    }
    let mut position = 2;
    loop {
        // segments start with 0xff, sometimes padded with more of them
        while *bytes.get(position)? == 0xff && *bytes.get(position + 1)? == 0xff {
            position += 1;
        }
        if *bytes.get(position)? != 0xff {
            return None;
        }
        let marker = *bytes.get(position + 1)?;
        let length = u16::from_be_bytes([*bytes.get(position + 2)?, *bytes.get(position + 3)?]);
        if is_start_of_frame(marker) {
            let height = u16::from_be_bytes([*bytes.get(position + 5)?, *bytes.get(position + 6)?]);
            let width = u16::from_be_bytes([*bytes.get(position + 7)?, *bytes.get(position + 8)?]);
            return nonzero(width.into(), height.into());
        }
        if length < 2 {
            return None;
        }
        position += 2 + length as usize;
    }
}

/// the frame markers c0 to cf, except c4 (huffman tables), c8 (reserved)
/// and cc (arithmetic coding), which share the range but hold no size.
fn is_start_of_frame(marker: u8) -> bool {
    (0xc0..=0xcf).contains(&marker) && ![0xc4, 0xc8, 0xcc].contains(&marker)
}

fn nonzero(width: u32, height: u32) -> Option<PictureSize> {
    (width > 0 && height > 0).then_some(PictureSize { width, height })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    /// a jpeg start: an APP0 segment, a table segment, then the frame.
    fn jpeg_header(width: u16, height: u16, frame_marker: u8) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8];
        bytes.extend_from_slice(&[0xff, 0xe0, 0x00, 0x10]);
        bytes.extend_from_slice(&[0; 14]);
        bytes.extend_from_slice(&[0xff, 0xc4, 0x00, 0x04, 0x00, 0x00]);
        bytes.extend_from_slice(&[0xff, frame_marker, 0x00, 0x11, 0x08]);
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&[3; 10]);
        bytes
    }

    #[test]
    fn reads_the_size_of_a_png() {
        assert_eq!(
            picture_size(&png_header(640, 480)),
            Some(PictureSize {
                width: 640,
                height: 480
            })
        );
    }

    #[test]
    fn reads_the_size_of_a_jpeg_past_other_segments() {
        assert_eq!(
            picture_size(&jpeg_header(32, 16, 0xc0)),
            Some(PictureSize {
                width: 32,
                height: 16
            })
        );
        // a progressive jpeg uses another frame marker
        assert_eq!(picture_size(&jpeg_header(5, 7, 0xc2)).unwrap().width, 5);
    }

    #[test]
    fn other_files_have_no_size() {
        assert_eq!(picture_size(b"%PDF-1.7"), None);
        assert_eq!(picture_size(&[]), None);
        assert_eq!(picture_size(&png_header(640, 480)[..20]), None);
        assert_eq!(picture_size(&jpeg_header(32, 16, 0xc0)[..30]), None);
        assert_eq!(picture_size(&png_header(0, 480)), None);
    }

    #[test]
    fn a_missing_file_says_to_choose_it_again() {
        let error = picture_size_of_file(Path::new("no such picture.png")).unwrap_err();
        assert!(error.contains("Choose it again"), "{error}");
    }
}
