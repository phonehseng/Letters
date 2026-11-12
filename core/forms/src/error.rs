use thiserror::Error;

/// everything that can go wrong with forms and signatures.
///
/// the ui shows these messages as they are, so they are written for the
/// person using the app: a plain sentence, no blame, and where possible
/// what to do next.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum FormError {
    #[error("This PDF is locked with a password. Unlock it first, then fill or sign it.")]
    Encrypted,

    #[error("This form was made with XFA, an older Adobe form format that Letters can’t fill. Open it in Adobe Acrobat or ask for a regular PDF form.")]
    XfaUnsupported,

    #[error("This PDF has no form fields.")]
    NoForm,

    #[error("There is no field called “{0}” in this form.")]
    UnknownField(String),

    #[error("Page {0} doesn’t exist in this document.")]
    UnknownPage(u32),

    #[error("The field “{0}” is read-only, so it can’t be changed.")]
    ReadOnly(String),

    #[error("The field “{field}” is required, so it can’t be left empty.")]
    Required { field: String },

    #[error("The field “{field}” takes at most {max} characters.")]
    TooLong { field: String, max: u32 },

    #[error("“{value}” isn’t one of the choices for “{field}”.")]
    NotAnOption { field: String, value: String },

    #[error("The field “{field}” can’t take that kind of answer. {hint}")]
    WrongValueKind { field: String, hint: String },

    #[error("“{0}” isn’t a date Letters understands. Type it like 2026-11-11.")]
    BadDate(String),

    #[error("Letters can’t write “{0}” into a form field yet. Only letters used in Western European languages are supported for now.")]
    UnsupportedCharacters(String),

    #[error("There is already a field called “{0}”. Choose another name.")]
    NameTaken(String),

    #[error("A field name can’t be empty or contain a full stop.")]
    BadName,

    #[error("This field needs a size on the page that is bigger than zero.")]
    EmptyRect,

    #[error("A list of choices needs at least one choice.")]
    NoOptions,

    #[error("Letters can’t read this signature image. Use a PNG picture. ({0})")]
    BadImage(String),

    #[error("This file can’t be read as a PDF. ({0})")]
    BadPdf(String),

    #[error(
        "Letters can’t open this certificate file. Check that it is a .p12 or .pfx file. ({0})"
    )]
    BadCertificate(String),

    #[error("The password for this certificate file isn’t right.")]
    WrongCertificatePassword,

    #[error("This certificate file has no private key in it, so it can’t be used to sign.")]
    NoPrivateKey,

    #[error("Letters can sign with RSA and P-256 keys only. This certificate uses a different kind of key.")]
    UnsupportedKey,

    #[error(
        "The field “{0}” is already signed. Choose an empty signature field or add a new one."
    )]
    AlreadySigned(String),

    #[error("The field “{0}” isn’t a signature field.")]
    NotASignatureField(String),

    #[error("The document couldn’t be signed. ({0})")]
    SigningFailed(String),

    #[error("The certificate couldn’t be made. ({0})")]
    CertificateFailed(String),
}

/// a shorter way to write results in this crate.
pub type FormResult<T> = Result<T, FormError>;

impl From<lopdf::Error> for FormError {
    fn from(error: lopdf::Error) -> Self {
        FormError::BadPdf(error.to_string())
    }
}
