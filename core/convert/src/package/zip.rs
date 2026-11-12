//! a small zip writer: the container of xlsx and pptx files.
//!
//! office files are zip archives of xml parts. each part is compressed
//! with deflate in memory, then written with its local header; the
//! central directory that lists them goes at the end. only what office
//! programs need is written: no zip64 (parts stay far below 4 gb), no
//! encryption, a fixed timestamp.

use std::io::Write;

use flate2::write::DeflateEncoder;
use flate2::{Compression, Crc};

use crate::error::write_failed;
use crate::ConvertError;

/// 1 january 2020, midnight, in ms-dos form (a zip needs some date)
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = ((2020 - 1980) << 9) | (1 << 5) | 1;

struct Entry {
    name: String,
    crc: u32,
    compressed: u32,
    size: u32,
    offset: u32,
}

/// writes a zip archive part by part.
pub(crate) struct ZipWriter<W: Write> {
    out: W,
    written: u64,
    entries: Vec<Entry>,
}

impl<W: Write> ZipWriter<W> {
    pub fn new(out: W) -> Self {
        Self {
            out,
            written: 0,
            entries: Vec::new(),
        }
    }

    /// adds one file, compressed.
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<(), ConvertError> {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).map_err(write_failed)?;
        let compressed = encoder.finish().map_err(write_failed)?;
        let mut crc = Crc::new();
        crc.update(data);
        let too_big =
            |_| ConvertError::WriteFailed("(a part of the file is larger than 4 GB)".into());
        let entry = Entry {
            name: name.to_string(),
            crc: crc.sum(),
            compressed: u32::try_from(compressed.len()).map_err(too_big)?,
            size: u32::try_from(data.len()).map_err(too_big)?,
            offset: u32::try_from(self.written).map_err(too_big)?,
        };
        let mut header = Vec::with_capacity(30 + name.len());
        header.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        header.extend_from_slice(&20u16.to_le_bytes()); // version needed: 2.0
        header.extend_from_slice(&(1u16 << 11).to_le_bytes()); // names are utf-8
        header.extend_from_slice(&8u16.to_le_bytes()); // deflate
        header.extend_from_slice(&DOS_TIME.to_le_bytes());
        header.extend_from_slice(&DOS_DATE.to_le_bytes());
        header.extend_from_slice(&entry.crc.to_le_bytes());
        header.extend_from_slice(&entry.compressed.to_le_bytes());
        header.extend_from_slice(&entry.size.to_le_bytes());
        header.extend_from_slice(&(name.len() as u16).to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes()); // no extra field
        header.extend_from_slice(name.as_bytes());
        self.write(&header)?;
        self.write(&compressed)?;
        self.entries.push(entry);
        Ok(())
    }

    /// writes the central directory and returns the output.
    pub fn finish(mut self) -> Result<W, ConvertError> {
        let start = self.written;
        let entries = std::mem::take(&mut self.entries);
        for entry in &entries {
            let mut record = Vec::with_capacity(46 + entry.name.len());
            record.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            record.extend_from_slice(&20u16.to_le_bytes()); // made by: 2.0
            record.extend_from_slice(&20u16.to_le_bytes()); // needed: 2.0
            record.extend_from_slice(&(1u16 << 11).to_le_bytes());
            record.extend_from_slice(&8u16.to_le_bytes());
            record.extend_from_slice(&DOS_TIME.to_le_bytes());
            record.extend_from_slice(&DOS_DATE.to_le_bytes());
            record.extend_from_slice(&entry.crc.to_le_bytes());
            record.extend_from_slice(&entry.compressed.to_le_bytes());
            record.extend_from_slice(&entry.size.to_le_bytes());
            record.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            record.extend_from_slice(&[0; 12]); // extra, comment, disk, attributes
            record.extend_from_slice(&entry.offset.to_le_bytes());
            record.extend_from_slice(entry.name.as_bytes());
            self.write(&record)?;
        }
        let size = self.written - start;
        let mut end = Vec::with_capacity(22);
        end.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        end.extend_from_slice(&[0; 4]); // this disk, disk with the directory
        end.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        end.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        end.extend_from_slice(&(size as u32).to_le_bytes());
        end.extend_from_slice(&(start as u32).to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes()); // no comment
        self.write(&end)?;
        self.out.flush().map_err(write_failed)?;
        Ok(self.out)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), ConvertError> {
        self.out.write_all(bytes).map_err(write_failed)?;
        self.written += bytes.len() as u64;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn parts_can_be_read_back() {
        let mut zip = ZipWriter::new(Vec::new());
        zip.add("a.xml", b"<a>hello hello hello</a>").unwrap();
        zip.add("dir/b.txt", b"").unwrap();
        let bytes = zip.finish().unwrap();
        // the end record says two entries, and the first part inflates back
        let end = bytes.len() - 22;
        assert_eq!(&bytes[end..end + 4], &[0x50, 0x4b, 0x05, 0x06]);
        assert_eq!(u16::from_le_bytes([bytes[end + 10], bytes[end + 11]]), 2);
        let name_length = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
        let compressed = u32::from_le_bytes(bytes[18..22].try_into().unwrap()) as usize;
        let data = &bytes[30 + name_length..30 + name_length + compressed];
        let mut text = String::new();
        flate2::read::DeflateDecoder::new(data)
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "<a>hello hello hello</a>");
    }
}
