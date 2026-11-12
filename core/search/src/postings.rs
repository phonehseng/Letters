//! posting lists: for each word, the passages (pages or paragraphs) it is in.
//!
//! a list is stored as bytes: the gap from the previous passage number,
//! then how often the word occurs there, both as variable length numbers
//! (seven bits a byte). passages are added in increasing order, so gaps
//! are small and most entries take two bytes. this is what keeps the
//! index of a large library in tens of megabytes rather than hundreds.

/// appends `value` as a variable length number.
pub fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// reads a variable length number, moving `at` past it. none at the end
/// of the bytes or on a damaged number.
pub fn read_varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0;
    while *at < bytes.len() {
        let byte = bytes[*at];
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
    None
}

/// the passages one word occurs in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PostingList {
    bytes: Vec<u8>,
    /// the last passage added, which the next gap is counted from
    last: Option<u32>,
    /// how many passages hold the word
    count: u32,
}

/// one entry: a passage and how many times the word is in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Posting {
    pub unit: u32,
    pub frequency: u32,
}

impl PostingList {
    /// adds a passage. passages must come in increasing order; a passage
    /// that is not after the last one is ignored rather than corrupting
    /// the list.
    pub fn push(&mut self, unit: u32, frequency: u32) {
        let gap = match self.last {
            Some(last) if unit <= last => return,
            Some(last) => unit - last,
            None => unit,
        };
        write_varint(&mut self.bytes, u64::from(gap));
        write_varint(&mut self.bytes, u64::from(frequency.max(1)));
        self.last = Some(unit);
        self.count += 1;
    }

    pub fn len(&self) -> usize {
        self.count as usize
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn byte_size(&self) -> usize {
        self.bytes.len()
    }

    pub fn iter(&self) -> PostingIter<'_> {
        PostingIter {
            bytes: &self.bytes,
            at: 0,
            unit: 0,
            first: true,
        }
    }

    /// the stored form, for writing the index to disk.
    pub fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, u64::from(self.count));
        write_varint(out, u64::from(self.last.map_or(0, |l| l + 1)));
        write_varint(out, self.bytes.len() as u64);
        out.extend_from_slice(&self.bytes);
    }

    pub fn decode(bytes: &[u8], at: &mut usize) -> Option<PostingList> {
        let count = read_varint(bytes, at)? as u32;
        let last = read_varint(bytes, at)? as u32;
        let length = read_varint(bytes, at)? as usize;
        let end = at.checked_add(length).filter(|end| *end <= bytes.len())?;
        let list = PostingList {
            bytes: bytes[*at..end].to_vec(),
            last: last.checked_sub(1),
            count,
        };
        *at = end;
        Some(list)
    }

    /// a copy holding only the passages `keep` says yes to, numbered anew
    /// by `renumber`. used when the index is compacted.
    pub fn filtered(&self, mut renumber: impl FnMut(u32) -> Option<u32>) -> PostingList {
        let mut list = PostingList::default();
        for posting in self.iter() {
            if let Some(unit) = renumber(posting.unit) {
                list.push(unit, posting.frequency);
            }
        }
        list
    }
}

pub struct PostingIter<'a> {
    bytes: &'a [u8],
    at: usize,
    unit: u32,
    first: bool,
}

impl Iterator for PostingIter<'_> {
    type Item = Posting;

    fn next(&mut self) -> Option<Posting> {
        let gap = read_varint(self.bytes, &mut self.at)? as u32;
        let frequency = read_varint(self.bytes, &mut self.at)? as u32;
        self.unit = if self.first { gap } else { self.unit + gap };
        self.first = false;
        Some(Posting {
            unit: self.unit,
            frequency,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_round_trip() {
        let mut bytes = Vec::new();
        for value in [0u64, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX] {
            bytes.clear();
            write_varint(&mut bytes, value);
            let mut at = 0;
            assert_eq!(read_varint(&bytes, &mut at), Some(value));
            assert_eq!(at, bytes.len());
        }
        assert_eq!(read_varint(&[0x80], &mut 0), None);
    }

    #[test]
    fn lists_keep_order_and_frequencies_in_two_bytes_an_entry() {
        let mut list = PostingList::default();
        for unit in [0u32, 3, 4, 100, 101] {
            list.push(unit, 2);
        }
        list.push(50, 1); // out of order: ignored
        let units: Vec<u32> = list.iter().map(|p| p.unit).collect();
        assert_eq!(units, vec![0, 3, 4, 100, 101]);
        assert_eq!(list.len(), 5);
        assert_eq!(list.byte_size(), 10);
    }

    #[test]
    fn lists_round_trip_through_bytes_and_can_be_filtered() {
        let mut list = PostingList::default();
        list.push(5, 1);
        list.push(9, 3);
        list.push(20, 1);
        let mut bytes = Vec::new();
        list.encode(&mut bytes);
        let mut at = 0;
        let decoded = PostingList::decode(&bytes, &mut at).unwrap();
        assert_eq!(decoded, list);
        // the list can still grow after being read back
        let mut grown = decoded.clone();
        grown.push(21, 1);
        assert_eq!(grown.len(), 4);
        let kept = list.filtered(|unit| (unit != 9).then_some(unit / 5));
        let units: Vec<u32> = kept.iter().map(|p| p.unit).collect();
        assert_eq!(units, vec![1, 4]);
    }
}
