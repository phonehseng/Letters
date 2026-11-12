//! digital signatures with a certificate kept on this computer.
//!
//! a digital signature proves two things: that whoever had the private key
//! of a certificate signed the document, and that the signed bytes haven't
//! changed since. it does not by itself prove who that person is: that
//! takes trusting whoever issued the certificate, which letters does not
//! check (see `verify`).
//!
//! format: the signature is written the way iso 32000 describes, as a
//! signature field whose /V is a /Sig dictionary with /Filter
//! /Adobe.PPKLite and /SubFilter /adbe.pkcs7.detached, holding a cms
//! SignedData over the file's bytes (all except the signature itself, as
//! named by /ByteRange). why adbe.pkcs7.detached and not
//! ETSI.CAdES.detached: the signed attributes include the signing time,
//! which pades baseline signatures (ETSI.CAdES.detached) are not allowed
//! to carry, and without a time stamp server (which needs the network)
//! letters can't reach the pades levels above the basic one anyway.
//! adbe.pkcs7.detached is accepted by every pdf reader. the attributes
//! still include ess signing-certificate-v2, as cades asks, so the
//! signature names the exact certificate used.
//!
//! every signature is saved as an incremental update, so signatures made
//! before it stay valid.
//!
//! out of scope, because they need a server: time stamps from a time
//! stamp authority, long term validation (lt / lta), and checking
//! certificates against revocation lists or trust lists.

mod algorithms;
mod certificate;
mod cms;
mod dates;
mod doc_timestamp;
mod document;
mod dss;
mod incremental;
mod keys;
mod look;
mod mdp;
mod oids;
mod permissions;
mod placeholder;
mod report_text;
mod revisions;
mod sign;
mod timestamp;
mod tsp;
mod verify;
mod workflow;

pub use certificate::{
    create_self_signed_certificate, CertificateRequest, CreatedCertificate, KeyKind,
};
pub use doc_timestamp::add_document_timestamp;
pub use document::{
    verify_document, DocumentVerification, SignatureDetails, SignatureKind, TimeSource,
};
pub use dss::{add_validation_data, ValidationData};
pub use incremental::save_incremental;
pub(crate) use incremental::write_update;
pub use mdp::{set_signature_lock, signature_lock, CertificationLevel, FieldLock};
pub use permissions::{ChangeVerdict, SignatureChanges};
pub(crate) use revisions::AUDIT_TRAIL_FILE;
pub use revisions::{revisions, Change, ChangeKind, RevisionReport};
pub(crate) use sign::readable_date;
pub use sign::{
    sign, sign_with_options, SignRequest, SignatureAppearance, SignaturePlacement, SigningOptions,
};
pub use timestamp::{TimestampAuthority, TimestampInfo, CLOCK_TIME_NOTE, TIMESTAMP_DISCLOSURE};
pub(crate) use verify::revision_ends;
pub use verify::{verify, CertificateSummary, SignatureReport, TrustStatus, TRUST_NOTE};
pub use workflow::{set_signing_order, signing_workflow, WorkflowStep};
