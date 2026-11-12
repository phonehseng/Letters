//! word's obfuscated font embedding (ecma-376 part 1, 17.8.1; recipe 10).
//!
//! an embedded font is stored as an `.odttf` part: the first 32 bytes of
//! the font are xored with a 16-byte key, which is the font's guid read
//! from its last hex pair to its first.

/// a guid in the form word writes in `w:fontKey`.
pub fn font_key_from(seed: u64) -> String {
    // two rounds of a 64-bit mix give 128 stable bits from one seed
    let first = mix(seed);
    let second = mix(first ^ 0x9E37_79B9_7F4A_7C15);
    let hex = format!("{first:016X}{second:016X}");
    format!(
        "{{{}-{}-{}-{}-{}}}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

/// the key bytes of a guid: its 16 bytes from the last hex pair to the first.
fn key_bytes(font_key: &str) -> [u8; 16] {
    let digits: String = font_key.chars().filter(char::is_ascii_hexdigit).collect();
    let mut key = [0u8; 16];
    for (index, slot) in key.iter_mut().enumerate() {
        let pair = &digits[2 * index..2 * index + 2];
        *slot = u8::from_str_radix(pair, 16).unwrap_or(0);
    }
    key.reverse();
    key
}

/// xors the first 32 bytes with the key; doing it twice gives the font back.
pub fn obfuscate(font: &[u8], font_key: &str) -> Vec<u8> {
    let key = key_bytes(font_key);
    let mut out = font.to_vec();
    for (index, byte) in out.iter_mut().take(32).enumerate() {
        *byte ^= key[index % 16];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_the_guid_read_backwards() {
        // the prototype's case (test_units.py): guid bytes 00..0f, so the key starts with 0x0f
        let key = "{00010203-0405-0607-0809-0A0B0C0D0E0F}";
        let data = vec![0u8; 64];
        let obfuscated = obfuscate(&data, key);
        assert_eq!(obfuscated[0], 0x0F);
        assert_eq!(obfuscated[15], 0x00);
        assert_eq!(obfuscated[16], 0x0F); // the 16-byte key repeats over 32 bytes
        assert_eq!(&obfuscated[32..], &data[32..]); // only the first 32 bytes change
    }

    #[test]
    fn obfuscation_is_its_own_inverse() {
        let key = font_key_from(42);
        let data: Vec<u8> = (0..100u32).map(|i| (i * 37 % 256) as u8).collect();
        assert_eq!(obfuscate(&obfuscate(&data, &key), &key), data);
    }

    #[test]
    fn keys_look_like_guids() {
        let key = font_key_from(1);
        assert_eq!(key.len(), 38);
        assert!(key.starts_with('{') && key.ends_with('}'));
        assert_ne!(key, font_key_from(2));
    }
}
