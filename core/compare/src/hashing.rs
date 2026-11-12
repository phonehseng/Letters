//! a small, fast hash for fingerprints.
//!
//! the comparison hashes words (to match pages) and page drawings (to skip
//! pages that can't differ). it needs the same answer on every run and
//! every machine, which the standard library's hasher does not promise, so
//! this is the well known FNV-1a hash, which is a few lines long.

const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Fnv64(u64);

impl Fnv64 {
    pub fn new() -> Self {
        Fnv64(OFFSET_BASIS)
    }

    pub fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    pub fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

/// the hash of one string.
pub(crate) fn hash_str(text: &str) -> u64 {
    let mut hash = Fnv64::new();
    hash.write(text.as_bytes());
    hash.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_match_the_published_fnv_1a() {
        assert_eq!(hash_str(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(hash_str("a"), 0xaf63_dc4c_8601_ec8c);
    }
}
