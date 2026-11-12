//! a fake time stamp authority (rfc 3161) that runs inside the tests on
//! 127.0.0.1, so no test contacts the internet. it signs tokens itself with
//! a p-256 key and a certificate made here, or hands each request to
//! `openssl ts -reply` for tokens made by an independent implementation.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::time::Duration;

use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{
    CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos,
};
use der::asn1::{GeneralizedTime, Int, ObjectIdentifier, OctetString, SetOfVec};
use der::{Any, Decode, Encode};
use sha2::{Digest, Sha256};
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};
use x509_cert::attr::Attribute;
use x509_cert::builder::profile::BuilderProfile;
use x509_cert::builder::{Builder, CertificateBuilder};
use x509_cert::ext::pkix::{BasicConstraints, ExtendedKeyUsage, KeyUsage, KeyUsages};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::Validity;
use x509_cert::Certificate;

const SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const ECDSA_WITH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const TST_INFO: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.1.4");
const SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
const MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const TIME_STAMPING: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.8");
const SIGNING_CERTIFICATE_V2: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.47");
const POLICY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.4.1.99999.1");

#[derive(Clone, Debug, der::Sequence)]
struct MessageImprint {
    hash_algorithm: AlgorithmIdentifierOwned,
    hashed_message: OctetString,
}

#[derive(Clone, Debug, der::Sequence)]
struct TimeStampReq {
    version: u8,
    message_imprint: MessageImprint,
    #[asn1(optional = "true")]
    req_policy: Option<ObjectIdentifier>,
    #[asn1(optional = "true")]
    nonce: Option<Int>,
    #[asn1(optional = "true")]
    cert_req: Option<bool>,
}

#[derive(Clone, Debug, der::Sequence)]
struct TstInfo {
    version: u8,
    policy: ObjectIdentifier,
    message_imprint: MessageImprint,
    serial_number: Int,
    gen_time: GeneralizedTime,
    #[asn1(optional = "true")]
    nonce: Option<Int>,
}

/// how the fake authority answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Behaviour {
    /// a correct token
    Honest,
    /// a token over a different hash
    WrongImprint,
    /// a token with a different nonce than asked
    WrongNonce,
    /// status 2, rejection
    Refuse,
    /// a token without the ess signing certificate attribute rfc 3161 requires
    NoSigningCertificate,
    /// asks for this name and password (http basic auth)
    Password(String, String),
    /// hands the request to `openssl ts -reply`
    OpenSsl(PathBuf),
}

/// a running fake authority.
pub struct FakeTsa {
    pub url: String,
    /// the authority's certificate, pem, for `openssl ts -verify -CAfile`
    pub certificate_pem: String,
    pub certificate_der: Vec<u8>,
}

/// the authority's key and certificate, made fresh for each fake.
struct Authority {
    key: p256::ecdsa::SigningKey,
    certificate: Certificate,
    folder: PathBuf,
}

impl FakeTsa {
    /// starts the fake on a free port; it answers until the test ends.
    pub fn start(behaviour: Behaviour, folder: &Path) -> FakeTsa {
        let authority = make_authority(folder);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/tsr", listener.local_addr().unwrap());
        let certificate_der = authority.certificate.to_der().unwrap();
        let certificate_pem = pem("CERTIFICATE", &certificate_der);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = answer(stream, &authority, &behaviour);
            }
        });
        FakeTsa {
            url,
            certificate_pem,
            certificate_der,
        }
    }
}

/// a profile for a self-signed certificate marked only for time stamping
/// (rfc 3161 section 2.3: the extended key usage must be critical).
struct TsaProfile {
    subject: Name,
}

impl BuilderProfile for TsaProfile {
    fn get_issuer(&self, subject: &Name) -> Name {
        subject.clone()
    }

    fn get_subject(&self) -> Name {
        self.subject.clone()
    }

    fn build_extensions(
        &self,
        _: spki::SubjectPublicKeyInfoRef<'_>,
        _: spki::SubjectPublicKeyInfoRef<'_>,
        _: &x509_cert::certificate::TbsCertificate,
    ) -> x509_cert::builder::Result<Vec<x509_cert::ext::Extension>> {
        let extension = |oid: &str, critical: bool, value: Vec<u8>| x509_cert::ext::Extension {
            extn_id: ObjectIdentifier::new_unwrap(oid),
            critical,
            extn_value: OctetString::new(value).unwrap(),
        };
        let basic = BasicConstraints {
            ca: false,
            path_len_constraint: None,
        };
        let usage = KeyUsage(KeyUsages::DigitalSignature | KeyUsages::NonRepudiation);
        let extended = ExtendedKeyUsage(vec![TIME_STAMPING]);
        Ok(vec![
            extension("2.5.29.19", true, basic.to_der()?),
            extension("2.5.29.15", true, usage.to_der()?),
            extension("2.5.29.37", true, extended.to_der()?),
        ])
    }
}

fn make_authority(folder: &Path) -> Authority {
    use p256::elliptic_curve::Generate;
    let key = p256::ecdsa::SigningKey::generate_from_rng(&mut rand::rng());
    let public = SubjectPublicKeyInfoOwned::from_key(key.verifying_key()).unwrap();
    let profile = TsaProfile {
        subject: Name::from_str("CN=Letters Test Time Stamps,O=Test").unwrap(),
    };
    let validity = Validity::from_now(Duration::from_secs(86_400)).unwrap();
    let serial = SerialNumber::generate(&mut rand::rng());
    let builder = CertificateBuilder::new(profile, serial, validity, public).unwrap();
    let certificate = builder.build::<_, p256::ecdsa::DerSignature>(&key).unwrap();
    Authority {
        key,
        certificate,
        folder: folder.to_owned(),
    }
}

/// reads one http request and writes one answer, as rfc 3161 section 3.4.
fn answer(
    mut stream: std::net::TcpStream,
    authority: &Authority,
    behaviour: &Behaviour,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let (headers, body) = read_request(&mut stream)?;
    if let Behaviour::Password(user, password) = behaviour {
        let expected = format!(
            "authorization: basic {}",
            base64(format!("{user}:{password}").as_bytes())
        );
        if !headers
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase())
        {
            let refusal =
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            return stream.write_all(refusal.as_bytes());
        }
    }
    let reply = match behaviour {
        Behaviour::OpenSsl(openssl) => openssl_reply(openssl, &authority.folder, &body),
        other => our_reply(authority, other, &body),
    };
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/timestamp-reply\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&reply)
}

fn read_request(stream: &mut std::net::TcpStream) -> std::io::Result<(String, Vec<u8>)> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..count]);
        if let Some(end) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&data[..end]).into_owned();
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            while data.len() < end + 4 + length {
                let count = stream.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                data.extend_from_slice(&buffer[..count]);
            }
            return Ok((headers, data[end + 4..].to_vec()));
        }
    }
    Ok((String::from_utf8_lossy(&data).into_owned(), Vec::new()))
}

/// a TimeStampResp made here: status, then the token.
fn our_reply(authority: &Authority, behaviour: &Behaviour, request_der: &[u8]) -> Vec<u8> {
    let request = TimeStampReq::from_der(request_der).unwrap();
    if *behaviour == Behaviour::Refuse {
        // PKIStatusInfo { status 2 (rejection), statusString "not today" }
        let status = der_sequence(&[
            2u8.to_der().unwrap(),
            der_sequence(&[der::asn1::Utf8StringRef::new("not today")
                .unwrap()
                .to_der()
                .unwrap()]),
        ]);
        return der_sequence(&[status]);
    }
    let mut imprint = request.message_imprint.clone();
    if *behaviour == Behaviour::WrongImprint {
        imprint.hashed_message = OctetString::new(vec![0u8; 32]).unwrap();
    }
    let nonce = match behaviour {
        Behaviour::WrongNonce => Some(Int::new(&[0x42]).unwrap()),
        _ => request.nonce.clone(),
    };
    let info = TstInfo {
        version: 1,
        policy: POLICY,
        message_imprint: imprint,
        serial_number: Int::new(&[0x01, 0x23]).unwrap(),
        gen_time: GeneralizedTime::from_system_time(std::time::SystemTime::now()).unwrap(),
        nonce,
    };
    let names_certificate = *behaviour != Behaviour::NoSigningCertificate;
    let token = sign_token(authority, &info.to_der().unwrap(), names_certificate);
    let status = der_sequence(&[0u8.to_der().unwrap()]);
    der_sequence(&[status, token])
}

/// the token: a cms signed data over the TSTInfo, signed by the authority.
fn sign_token(authority: &Authority, tst_info: &[u8], names_certificate: bool) -> Vec<u8> {
    let attribute = |oid: ObjectIdentifier, value: Any| {
        let mut values = SetOfVec::new();
        values.insert(value).unwrap();
        Attribute { oid, values }
    };
    let digest = Sha256::digest(tst_info).to_vec();
    let mut list = vec![
        attribute(CONTENT_TYPE, Any::encode_from(&TST_INFO).unwrap()),
        attribute(
            MESSAGE_DIGEST,
            Any::encode_from(&OctetString::new(digest).unwrap()).unwrap(),
        ),
    ];
    if names_certificate {
        // SigningCertificateV2 { certs: [ ESSCertIDv2 { certHash } ] }, sha-256 by default
        let hash = Sha256::digest(authority.certificate.to_der().unwrap()).to_vec();
        let id = der_sequence(&[OctetString::new(hash).unwrap().to_der().unwrap()]);
        let value = der_sequence(&[der_sequence(&[id])]);
        list.push(attribute(
            SIGNING_CERTIFICATE_V2,
            Any::from_der(&value).unwrap(),
        ));
    }
    let attributes = SetOfVec::try_from(list).unwrap();
    use p256::ecdsa::signature::Signer as _;
    let signature: p256::ecdsa::DerSignature = authority.key.sign(&attributes.to_der().unwrap());
    let tbs = authority.certificate.tbs_certificate();
    let signer = SignerInfo {
        version: CmsVersion::V1,
        sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: tbs.issuer().clone(),
            serial_number: tbs.serial_number().clone(),
        }),
        digest_alg: AlgorithmIdentifierOwned {
            oid: SHA256,
            parameters: None,
        },
        signed_attrs: Some(attributes),
        signature_algorithm: AlgorithmIdentifierOwned {
            oid: ECDSA_WITH_SHA256,
            parameters: None,
        },
        signature: OctetString::new(signature.to_bytes().to_vec()).unwrap(),
        unsigned_attrs: None,
    };
    let mut digests = SetOfVec::new();
    digests
        .insert(AlgorithmIdentifierOwned {
            oid: SHA256,
            parameters: None,
        })
        .unwrap();
    let mut signers = SetOfVec::new();
    signers.insert(signer).unwrap();
    let certificates = SetOfVec::try_from(vec![CertificateChoices::Certificate(
        authority.certificate.clone(),
    )])
    .unwrap();
    let signed = SignedData {
        version: CmsVersion::V3,
        digest_algorithms: digests,
        encap_content_info: EncapsulatedContentInfo {
            econtent_type: TST_INFO,
            econtent: Some(Any::new(der::Tag::OctetString, tst_info.to_vec()).unwrap()),
        },
        certificates: Some(CertificateSet(certificates)),
        crls: None,
        signer_infos: SignerInfos(signers),
    };
    ContentInfo {
        content_type: SIGNED_DATA,
        content: Any::encode_from(&signed).unwrap(),
    }
    .to_der()
    .unwrap()
}

/// a der SEQUENCE around already encoded parts.
fn der_sequence(parts: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = parts.concat();
    Any::new(der::Tag::Sequence, body)
        .unwrap()
        .to_der()
        .unwrap()
}

/// lets openssl answer: `openssl ts -reply` with a configuration made here.
fn openssl_reply(openssl: &Path, folder: &Path, request: &[u8]) -> Vec<u8> {
    let query = folder.join("request.tsq");
    let reply = folder.join("reply.tsr");
    std::fs::write(&query, request).unwrap();
    let config = folder.join("tsa.cnf");
    let output = Command::new(openssl)
        .args(["ts", "-reply", "-config"])
        .arg(&config)
        .arg("-queryfile")
        .arg(&query)
        .arg("-out")
        .arg(&reply)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "openssl ts -reply failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read(reply).unwrap()
}

/// writes the files `openssl ts -reply` needs: the key, the certificate,
/// a serial number file and a configuration. returns the authority's
/// certificate pem path. call before starting an OpenSsl fake.
pub fn prepare_openssl_authority(openssl: &Path, folder: &Path) -> PathBuf {
    let key = folder.join("tsa.key");
    let certificate = folder.join("tsa.pem");
    let run = |args: &[&str]| {
        let output = Command::new(openssl).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&[
        "ecparam",
        "-name",
        "prime256v1",
        "-genkey",
        "-noout",
        "-out",
        key.to_str().unwrap(),
    ]);
    run(&[
        "req",
        "-new",
        "-x509",
        "-key",
        key.to_str().unwrap(),
        "-out",
        certificate.to_str().unwrap(),
        "-days",
        "2",
        "-subj",
        "/CN=OpenSSL Test TSA",
        "-addext",
        "extendedKeyUsage=critical,timeStamping",
        "-addext",
        "basicConstraints=critical,CA:false",
        "-addext",
        "keyUsage=critical,digitalSignature",
    ]);
    std::fs::write(folder.join("serial"), "01\n").unwrap();
    let slash = |path: &Path| path.to_str().unwrap().replace('\\', "/");
    let config = format!(
        "[ tsa ]\ndefault_tsa = tsa_config\n[ tsa_config ]\nserial = {}\nsigner_cert = {}\ncerts = {}\nsigner_key = {}\nsigner_digest = sha256\ndefault_policy = 1.2.3.4.1\ndigests = sha256\naccuracy = secs:1\nordering = no\ntsa_name = no\ness_cert_id_chain = no\ness_cert_id_alg = sha256\n",
        slash(&folder.join("serial")),
        slash(&certificate),
        slash(&certificate),
        slash(&key)
    );
    std::fs::write(folder.join("tsa.cnf"), config).unwrap();
    certificate
}

fn pem(label: &str, der: &[u8]) -> String {
    let encoded = base64(der);
    let lines: Vec<&str> = encoded
        .as_bytes()
        .chunks(64)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect();
    format!(
        "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
        lines.join("\n")
    )
}

pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let joined = u32::from_be_bytes([
            0,
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ]);
        for position in 0..4 {
            if position <= chunk.len() {
                out.push(ALPHABET[((joined >> (18 - 6 * position)) & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
