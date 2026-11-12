//! finds, adds, fills and flattens form fields, and signs documents locally.
//!
//! every operation works on a `lopdf::Document` in memory (signing and
//! checking signatures work on the file's bytes, because a signature
//! covers exact bytes). pages are counted from 0. positions are in points
//! from the top left of the page as the reader sees it, y going down,
//! after the page's turn (/Rotate) and cut (/CropBox) are applied.
//!
//! nothing here uses the network: no servers, no time stamps fetched from
//! elsewhere, no tracking.

mod add;
mod appearance;
mod date;
mod detect;
mod edit;
mod error;
mod fill;
mod flatten;
mod fonts;
mod geometry;
mod image;
mod objects;
mod read;
mod signing;
mod stamp;
mod text;
mod tree;

pub use add::{add_field, NewField, NewFieldKind, RadioButton};
pub use detect::{detect_fields, DetectedField, DetectedKind};
pub use edit::{remove_field, update_field, FieldChanges};
pub use error::{FormError, FormResult};
pub use fill::{set_value, FieldValue};
pub use flatten::{flatten, redraw_fields, FlattenOptions, FlattenReport};
pub use geometry::VisibleRect;
pub use read::{fields, Alignment, ChoiceOption, FieldInfo, FieldKind, WidgetInfo};
pub use signing::{
    create_self_signed_certificate, sign, verify, CertificateRequest, CertificateSummary,
    CreatedCertificate, KeyKind, SignRequest, SignatureAppearance, SignaturePlacement,
    SignatureReport, TrustStatus, TRUST_NOTE,
};
pub use stamp::{place_signature_image, SignatureImage};
