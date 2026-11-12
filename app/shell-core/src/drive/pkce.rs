//! proof key for code exchange (rfc 7636): the app makes a random secret
//! (the verifier), sends only its hash (the challenge) when the browser
//! asks the person to sign in, and proves it started the sign-in by
//! sending the secret when it collects the token. a program that saw the
//! redirect cannot use the code without the secret.

use sha2::{Digest, Sha256};

const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url without padding, as oauth uses it.
pub fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for slot in 0..=chunk.len() {
            out.push(URL_SAFE[((n >> (18 - 6 * slot)) & 63) as usize] as char);
        }
    }
    out
}

/// a fresh random verifier: 32 random bytes, 43 characters.
pub fn new_verifier() -> Result<String, String> {
    Ok(base64url(&random_bytes::<32>()?))
}

/// a random value to tie the browser's answer to this sign-in.
pub fn new_state() -> Result<String, String> {
    Ok(base64url(&random_bytes::<16>()?))
}

fn random_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("No random numbers are available ({error})."))?;
    Ok(bytes)
}

/// the challenge sent with the sign-in: base64url(sha256(verifier)).
pub fn challenge(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rfc_7636_example_gives_its_challenge() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifiers_are_long_enough_url_safe_and_different() {
        let a = new_verifier().unwrap();
        let b = new_verifier().unwrap();
        assert_eq!(a.len(), 43);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(a, b);
    }

    #[test]
    fn base64url_has_no_padding() {
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }
}
