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
mod audit;
mod build;
mod data;
mod date;
mod detect;
mod edit;
mod error;
mod fill;
mod flatten;
mod fonts;
mod geometry;
mod image;
pub mod logic;
mod objects;
mod read;
mod signing;
mod stamp;
mod text;
mod tree;

pub use add::{add_field, NewField, NewFieldKind, RadioButton};
pub use audit::{
    add_audit_page, append_audit_entry, audit_trail, AuditEntry, AuditEvent, AuditTrailReport,
    AuditVerdict, CheckedEntry, AUDIT_TRAIL_NOTE,
};
pub use build::{
    add_button, duplicate_across_pages, group_fields, reset_form, set_default_value,
    set_export_value, set_field_appearance, set_tab_order, tab_kind, tab_order, BorderStyle,
    ButtonAction, Color, FieldAppearance, FieldFont, NewButton, TabOrder, NO_SUBMIT_NOTE,
};
pub use data::{
    export_data, import_data, merge_form_data, DataFormat, FilledCopy, ImportReport, MergedData,
    SkippedField, MERGED_FILE_COLUMN,
};
pub use detect::{detect_fields, DetectedField, DetectedKind};
pub use edit::{remove_field, update_field, FieldChanges};
pub use error::{FormError, FormResult};
pub use fill::{set_value, FieldValue};
pub use flatten::{flatten, redraw_fields, FlattenOptions, FlattenReport, FLATTEN_EXPLANATION};
pub use geometry::VisibleRect;
pub use logic::{
    fill_field, form_logic, recalculate, set_calculation_order, set_field_logic, Aggregate,
    Calculation, CalculationIssue, ChangedField, FieldFormat, FieldLogic, FieldLogicChanges,
    FillReport, FormLogic, NegativeStyle, ScriptTrigger, SeparatorStyle, SpecialFormat,
    UnsupportedScript, Validation, CONDITIONAL_VISIBILITY_NOTE, CUSTOM_SCRIPT_NOTE,
};
pub use read::{fields, Alignment, ChoiceOption, FieldInfo, FieldKind, WidgetInfo};
pub use signing::{
    add_document_timestamp, sign_with_options, verify_document, DocumentVerification,
    SignatureDetails, SignatureKind, SigningOptions, TimeSource, TimestampAuthority, TimestampInfo,
    CLOCK_TIME_NOTE, TIMESTAMP_DISCLOSURE,
};
pub use signing::{
    add_validation_data, revisions, save_incremental, set_signature_lock, set_signing_order,
    signature_lock, signing_workflow, CertificationLevel, Change, ChangeKind, ChangeVerdict,
    FieldLock, RevisionReport, SignatureChanges, ValidationData, WorkflowStep,
};
pub use signing::{
    create_self_signed_certificate, sign, verify, CertificateRequest, CertificateSummary,
    CreatedCertificate, KeyKind, SignRequest, SignatureAppearance, SignaturePlacement,
    SignatureReport, TrustStatus, TRUST_NOTE,
};
pub use stamp::{place_signature_image, SignatureImage};
