//! signing a document: where the signature goes, the signature
//! dictionary with its /ByteRange and /Contents placeholders, and filling
//! them in once the rest of the file is written.

use std::time::SystemTime;

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::cms::{add_unsigned_attribute, build_signed_data};
use super::dss::{add_validation_data, ValidationData};
use super::incremental::{write_update, HandWritten};
use super::keys::{open_pkcs12, Signer};
use super::look::draw_visible_signature;
use super::mdp::{
    check_signing_allowed, lock_fields, lock_of_field, mark_certified, transform_references,
    CertificationLevel, FieldLock,
};
use super::oids;
use super::placeholder::{entry, fill_placeholders, signature_body};
use super::timestamp::{fetch_token, signature_value_of, TimestampAuthority};
use super::workflow::check_turn;
use crate::add::{add_field, ensure_form, unique_name, NewField, NewFieldKind};
use crate::audit::{append_audit_entry, AuditEvent};
use crate::error::{FormError, FormResult};
use crate::geometry::VisibleRect;
use crate::objects::{check_document, name, page_ids, push_to_array, text_string};
use crate::read::{kind_of, Alignment, FieldKind};
use crate::tree::find_field;

pub(crate) use super::dates::{pdf_date, readable_date, utc_parts};

/// where the signature goes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SignaturePlacement {
    /// an empty signature field that is already in the form
    ExistingField { field: String },
    /// a new signature field drawn at this place (points from the visible top left)
    NewField {
        page: u32,
        rect: VisibleRect,
        #[serde(default)]
        name: Option<String>,
    },
    /// a signature with nothing to see on the page
    Invisible {
        #[serde(default)]
        name: Option<String>,
    },
}

/// what a visible signature shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureAppearance {
    /// a picture of a handwritten or typed signature, as png
    #[serde(default)]
    pub image_png: Option<Vec<u8>>,
    /// lines of text: who signed, when, and the reason and place if given
    #[serde(default = "yes")]
    pub show_details: bool,
}

fn yes() -> bool {
    true
}

impl Default for SignatureAppearance {
    fn default() -> Self {
        Self {
            image_png: None,
            show_details: true,
        }
    }
}

/// everything needed to sign.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignRequest {
    /// the certificate file (.p12 or .pfx) and its password
    pub pkcs12: Vec<u8>,
    pub password: String,
    pub placement: SignaturePlacement,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    /// the signer's name as shown; the certificate's name when not given
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub contact_info: Option<String>,
    #[serde(default)]
    pub appearance: SignatureAppearance,
}

/// room for the signature, in bytes, beyond the certificates it carries.
const SIGNATURE_OVERHEAD: usize = 4096;

/// room for a time stamp token: the token's own signature, its time stamp
/// information and the authority's certificate chain. public authorities'
/// tokens measure 2 to 7 kb; this leaves room for a long chain.
pub(crate) const TIMESTAMP_ROOM: usize = 16_384;

/// what else a signature does besides signing. all of it is optional.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningOptions {
    /// a time stamp authority to vouch for the signing time. only a hash
    /// of the signature is sent (see `TIMESTAMP_DISCLOSURE`)
    #[serde(default)]
    pub timestamp: Option<TimestampAuthority>,
    /// certify the document (only its first signature may): which later
    /// changes the author allows
    #[serde(default)]
    pub certify: Option<CertificationLevel>,
    /// fields this signature locks. a signature field that already has a
    /// lock (/Lock, see `set_signature_lock`) uses its own instead
    #[serde(default)]
    pub lock: Option<FieldLock>,
    /// certificates and revocation answers the caller already has, written
    /// into a document security store after signing (nothing is fetched)
    #[serde(default)]
    pub validation: Option<ValidationData>,
    /// refuse to sign a field while an earlier signature field in the
    /// form's signing order is still empty
    #[serde(default)]
    pub follow_signing_order: bool,
    /// the name the person set in Letters: when given, the signature is
    /// recorded in the document's audit trail (see `AUDIT_TRAIL_NOTE`)
    #[serde(default)]
    pub audit_actor: Option<String>,
}

/// signs a document and returns the signed file.
///
/// the original bytes are kept exactly; the signature and its field are
/// added after them as an incremental update, so earlier signatures stay
/// valid.
pub fn sign(doc_bytes: &[u8], request: &SignRequest) -> FormResult<Vec<u8>> {
    sign_with_options(doc_bytes, request, &SigningOptions::default())
}

/// signs like `sign`, with the extras in `options`.
pub fn sign_with_options(
    doc_bytes: &[u8],
    request: &SignRequest,
    options: &SigningOptions,
) -> FormResult<Vec<u8>> {
    let signer = open_pkcs12(&request.pkcs12, &request.password)?;
    let before = Document::load_mem(doc_bytes)?;
    check_document(&before)?;
    check_signing_allowed(&before, options.certify)?;
    if let (true, SignaturePlacement::ExistingField { field }) =
        (options.follow_signing_order, &request.placement)
    {
        check_turn(&before, field)?;
    }
    let signing_time = SystemTime::now();
    let signer_name = request
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| common_name(&signer))
        .unwrap_or_else(|| "Unknown".to_owned());

    let mut after = before.clone();
    let (field_id, widget_id) = prepare_field(&mut after, &request.placement)?;
    let signature_id = after.new_object_id();
    after
        .get_dictionary_mut(field_id)?
        .set("V", Object::Reference(signature_id));
    mark_form_signed(&mut after)?;
    let lock = lock_of_field(&after, field_id).or_else(|| options.lock.clone());
    if let Some(lock) = &lock {
        lock_fields(&mut after, lock)?;
    }
    if options.certify.is_some() {
        mark_certified(&mut after, signature_id)?;
    }
    draw_visible_signature(&mut after, widget_id, request, &signer_name, signing_time)?;

    let mut reserved = signature_room(&signer);
    if options.timestamp.is_some() {
        reserved += TIMESTAMP_ROOM;
    }
    let text_entries = [
        ("Name", signer_name.clone()),
        ("M", pdf_date(signing_time)),
        ("Reason", request.reason.clone().unwrap_or_default()),
        ("Location", request.location.clone().unwrap_or_default()),
        (
            "ContactInfo",
            request.contact_info.clone().unwrap_or_default(),
        ),
    ];
    let kind = "/Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached";
    let references = transform_references(options.certify, lock.as_ref());
    let extra = if references.is_empty() {
        Vec::new()
    } else {
        entry("Reference", &Object::Array(references))?
    };
    let body = signature_body(kind, reserved, &text_entries, &extra)?;
    let written = HandWritten {
        id: signature_id,
        body,
    };
    let update = write_update(doc_bytes, &before, &after, &[written])?;
    let start = update.offsets[&signature_id];
    let signed = fill_placeholders(update.bytes, start, reserved, |covered| {
        make_signature(&signer, covered, signing_time, options.timestamp.as_ref())
    })?;
    let signed = match &options.validation {
        Some(data) => add_validation_data(&signed, data)?,
        None => signed,
    };
    let Some(actor) = &options.audit_actor else {
        return Ok(signed);
    };
    let field = field_name(&after, field_id);
    let event = signed_event(
        &signer,
        request,
        field,
        &signer_name,
        signing_time,
        options.timestamp.is_some(),
    );
    append_audit_entry(&signed, actor, &event)
}

fn field_name(doc: &Document, field_id: ObjectId) -> String {
    crate::tree::collect_fields(doc)
        .into_iter()
        .find(|field| field.id == field_id)
        .map(|field| field.name)
        .unwrap_or_default()
}

/// the audit entry for a signature just made. a time stamp, when asked
/// for, has been checked by now (signing fails otherwise).
fn signed_event(
    signer: &Signer,
    request: &SignRequest,
    field: String,
    signer_name: &str,
    signing_time: SystemTime,
    time_stamped: bool,
) -> AuditEvent {
    let fingerprint: String = Sha256::digest(&signer.certificate_der)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let time_source = if time_stamped {
        "a time stamp authority"
    } else {
        "the signer’s computer clock"
    };
    AuditEvent::Signed {
        field,
        signer: signer_name.to_owned(),
        certificate_subject: signer.certificate.tbs_certificate().subject().to_string(),
        certificate_fingerprint: fingerprint,
        signing_time: readable_date(signing_time),
        time_source: time_source.to_owned(),
        reason: request
            .reason
            .clone()
            .filter(|text| !text.trim().is_empty()),
        location: request
            .location
            .clone()
            .filter(|text| !text.trim().is_empty()),
    }
}

/// /SigFlags 3: 1 = the document has signatures, 2 = only append to it
/// from now on (iso 32000 table 225).
pub(super) fn mark_form_signed(doc: &mut Document) -> FormResult<()> {
    let form = ensure_form(doc)?;
    doc.get_dictionary_mut(form)?
        .set("SigFlags", Object::Integer(3));
    Ok(())
}

/// the cms signature over the covered bytes, with a signature time stamp
/// (rfc 3161 appendix a) when an authority is given.
fn make_signature(
    signer: &Signer,
    covered: [&[u8]; 2],
    signing_time: SystemTime,
    authority: Option<&TimestampAuthority>,
) -> FormResult<Vec<u8>> {
    let mut hasher = Sha256::new();
    hasher.update(covered[0]);
    hasher.update(covered[1]);
    let digest = hasher.finalize();
    let signature = build_signed_data(signer, &digest, signing_time)?;
    let Some(authority) = authority else {
        return Ok(signature);
    };
    let value = signature_value_of(&signature)
        .ok_or_else(|| FormError::SigningFailed("the new signature can’t be read back".into()))?;
    let token = fetch_token(authority, &[&value])?;
    add_unsigned_attribute(&signature, oids::SIGNATURE_TIMESTAMP_TOKEN, &token)
}

fn common_name(signer: &Signer) -> Option<String> {
    super::keys::common_name(&signer.certificate.tbs_certificate().subject().to_string())
}

/// enough room for the certificates the signature carries, plus the
/// signature and its attributes.
fn signature_room(signer: &Signer) -> usize {
    let certificates: usize =
        signer.certificate_der.len() + signer.chain_der.iter().map(Vec::len).sum::<usize>();
    certificates + signer.key.signature_size() + SIGNATURE_OVERHEAD
}

/// finds or makes the signature field and returns its field and widget ids.
pub(super) fn prepare_field(
    doc: &mut Document,
    placement: &SignaturePlacement,
) -> FormResult<(ObjectId, ObjectId)> {
    match placement {
        SignaturePlacement::ExistingField { field } => {
            let node = find_field(doc, field)?;
            if kind_of(doc, &node) != Some(FieldKind::Signature) {
                return Err(FormError::NotASignatureField(field.clone()));
            }
            if node.inherited(doc, b"V").is_some() {
                return Err(FormError::AlreadySigned(field.clone()));
            }
            Ok((node.id, node.widgets[0]))
        }
        SignaturePlacement::NewField { page, rect, name } => {
            let spec = NewField {
                name: name.clone().unwrap_or_else(|| "Signature".to_owned()),
                page: *page,
                rect: *rect,
                kind: NewFieldKind::Signature,
                tooltip: None,
                required: false,
                read_only: false,
                font_size: 0.0,
                alignment: Alignment::Left,
            };
            let made = add_field(doc, &spec)?;
            let node = find_field(doc, &made)?;
            Ok((node.id, node.widgets[0]))
        }
        SignaturePlacement::Invisible { name } => add_invisible_field(doc, name.as_deref()),
    }
}

/// an invisible signature still needs a field and a widget; the widget has
/// no size, sits on the first page and is marked as printable and locked.
pub(super) fn add_invisible_field(
    doc: &mut Document,
    wanted: Option<&str>,
) -> FormResult<(ObjectId, ObjectId)> {
    let form = ensure_form(doc)?;
    let field_name = unique_name(doc, wanted.unwrap_or("Signature"));
    let first_page = *page_ids(doc).first().ok_or(FormError::UnknownPage(1))?;
    let mut dict = Dictionary::new();
    dict.set("FT", name("Sig"));
    dict.set("T", text_string(&field_name));
    dict.set("Type", name("Annot"));
    dict.set("Subtype", name("Widget"));
    dict.set(
        "Rect",
        Object::Array(vec![0.into(), 0.into(), 0.into(), 0.into()]),
    );
    dict.set("P", Object::Reference(first_page));
    // 4 = print, 128 = locked
    dict.set("F", Object::Integer(132));
    let id = doc.add_object(Object::Dictionary(dict));
    push_to_array(doc, form, b"Fields", Object::Reference(id))?;
    push_to_array(doc, first_page, b"Annots", Object::Reference(id))?;
    Ok((id, id))
}
