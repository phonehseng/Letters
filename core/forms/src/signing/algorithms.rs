//! checking a signature value against a public key, for the algorithms
//! letters reads: rsa with pkcs#1 v1.5 or pss padding, and ecdsa on p-256.

use p256::ecdsa::signature::hazmat::PrehashVerifier;
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::{Pkcs1v15Sign, PublicKey as RsaPublicKey};
use sha2::{Sha256, Sha384, Sha512};
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};

use super::cms::digest;
use super::keys::PssParameters;
use super::oids;

/// checks a signature over `message` with a public key, for rsa (pkcs#1
/// v1.5 and pss) and ecdsa p-256.
pub(crate) fn verify_signature(
    key: &SubjectPublicKeyInfoOwned,
    algorithm: &AlgorithmIdentifierOwned,
    digest_algorithm: &der::asn1::ObjectIdentifier,
    message: &[u8],
    signature: &[u8],
) -> Result<(), String> {
    let wrong = || "The signature doesn’t match the signer’s certificate.".to_owned();
    let unsupported = || format!("The signature algorithm {} isn’t supported.", algorithm.oid);
    let key_bytes = key.subject_public_key.raw_bytes();
    match algorithm.oid {
        oids::RSA_ENCRYPTION
        | oids::SHA256_WITH_RSA
        | oids::SHA384_WITH_RSA
        | oids::SHA512_WITH_RSA => {
            let public = RsaPublicKey::from_pkcs1_der(key_bytes)
                .map_err(|_| "The certificate’s RSA key can’t be read.".to_owned())?;
            let hash_oid = match algorithm.oid {
                oids::SHA256_WITH_RSA => oids::SHA256,
                oids::SHA384_WITH_RSA => oids::SHA384,
                oids::SHA512_WITH_RSA => oids::SHA512,
                _ => *digest_algorithm,
            };
            let hashed = digest(&hash_oid, &[message]).ok_or_else(unsupported)?;
            let scheme = match hash_oid {
                oids::SHA256 => Pkcs1v15Sign::new::<Sha256>(),
                oids::SHA384 => Pkcs1v15Sign::new::<Sha384>(),
                _ => Pkcs1v15Sign::new::<Sha512>(),
            };
            public
                .verify(scheme, &hashed, signature)
                .map_err(|_| wrong())
        }
        oids::RSASSA_PSS => {
            let public = RsaPublicKey::from_pkcs1_der(key_bytes)
                .map_err(|_| "The certificate’s RSA key can’t be read.".to_owned())?;
            let parameters = algorithm
                .parameters
                .as_ref()
                .and_then(|any| any.decode_as::<PssParameters>().ok())
                .ok_or_else(unsupported)?;
            let hash_oid = parameters
                .hash
                .map(|hash| hash.oid)
                .ok_or_else(unsupported)?;
            let salt = parameters.salt_length.unwrap_or(20) as usize;
            let hashed = digest(&hash_oid, &[message]).ok_or_else(unsupported)?;
            let result = match hash_oid {
                oids::SHA256 => public.verify(
                    rsa::pss::Pss::<Sha256>::new_with_salt(salt),
                    &hashed,
                    signature,
                ),
                oids::SHA384 => public.verify(
                    rsa::pss::Pss::<Sha384>::new_with_salt(salt),
                    &hashed,
                    signature,
                ),
                _ => public.verify(
                    rsa::pss::Pss::<Sha512>::new_with_salt(salt),
                    &hashed,
                    signature,
                ),
            };
            result.map_err(|_| wrong())
        }
        oids::ECDSA_WITH_SHA256 | oids::ECDSA_WITH_SHA384 | oids::ECDSA_WITH_SHA512 => {
            let curve = key
                .algorithm
                .parameters
                .as_ref()
                .and_then(|any| any.decode_as::<der::asn1::ObjectIdentifier>().ok());
            if curve != Some(oids::P256) {
                return Err("Only ECDSA keys on the P-256 curve are supported.".into());
            }
            let public = p256::ecdsa::VerifyingKey::from_sec1_bytes(key_bytes)
                .map_err(|_| "The certificate’s ECDSA key can’t be read.".to_owned())?;
            let hash_oid = match algorithm.oid {
                oids::ECDSA_WITH_SHA384 => oids::SHA384,
                oids::ECDSA_WITH_SHA512 => oids::SHA512,
                _ => oids::SHA256,
            };
            let hashed = digest(&hash_oid, &[message]).ok_or_else(unsupported)?;
            let parsed = p256::ecdsa::Signature::from_der(signature).map_err(|_| wrong())?;
            public.verify_prehash(&hashed, &parsed).map_err(|_| wrong())
        }
        _ => Err(unsupported()),
    }
}
