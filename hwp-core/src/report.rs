use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WarningKind {
    SectionMissing,
    SectionUnreadable,
    RecordTruncated,
    StreamSizeMismatch,
    CountMismatch,
    DocInfoUnreadable,
    MissingBinData,
    LinkedPicture,
    PictureWithoutRef,
    InvalidLineSegFlags,
    NestingTooDeep,
    ExtensionMismatch,
    UnknownControl,
    UnknownNumberFormat,
    UnsupportedObject,
    LoneSurrogate,
    InvalidXml,
    DistributionDecrypted,
    StreamChecksumMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    pub kind: WarningKind,
    pub section: Option<u32>,
    pub detail: String,
}

impl Warning {
    pub fn new(kind: WarningKind, section: Option<u32>, detail: impl Into<String>) -> Self {
        Self {
            kind,
            section,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fatal {
    NotHwp,
    Hwp3Unsupported,
    HwpmlUnsupported,
    PasswordProtected,
    WrongPassword,
    /// An encryption scheme this parser does not support. Kept as a unit variant for callers
    /// that matched it before detailed messages were added.
    UnsupportedEncryption,
    /// An unsupported encryption scheme, with the version and supported range.
    UnsupportedEncryptionDetail(String),
    /// A container or protected stream failed an integrity check. This is damage, not a
    /// password mismatch.
    CorruptDocument(String),
    NoReadableSection,
    Internal(String),
}

impl Fatal {
    pub fn kind(&self) -> &'static str {
        match self {
            Fatal::NotHwp => "NotHwp",
            Fatal::Hwp3Unsupported => "Hwp3Unsupported",
            Fatal::HwpmlUnsupported => "HwpmlUnsupported",
            Fatal::PasswordProtected => "PasswordProtected",
            Fatal::WrongPassword => "WrongPassword",
            Fatal::UnsupportedEncryption | Fatal::UnsupportedEncryptionDetail(_) => {
                "UnsupportedEncryption"
            }
            Fatal::CorruptDocument(_) => "CorruptDocument",
            Fatal::NoReadableSection => "NoReadableSection",
            Fatal::Internal(_) => "Internal",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Fatal::UnsupportedEncryptionDetail(detail)
            | Fatal::CorruptDocument(detail)
            | Fatal::Internal(detail) => detail,
            _ => "",
        }
    }
}

impl Serialize for Fatal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Fatal", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("detail", self.detail())?;
        state.end()
    }
}

impl std::fmt::Display for Fatal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fatal::UnsupportedEncryptionDetail(detail)
            | Fatal::CorruptDocument(detail)
            | Fatal::Internal(detail) => {
                write!(formatter, "{}: {detail}", self.kind())
            }
            _ => formatter.write_str(self.kind()),
        }
    }
}

impl std::error::Error for Fatal {}

#[cfg(test)]
mod tests {
    use super::Fatal;

    #[test]
    fn unsupported_encryption_unit_variant_stays_source_compatible() {
        let plain = Fatal::UnsupportedEncryption;
        assert_eq!(plain.kind(), "UnsupportedEncryption");
        assert_eq!(plain.detail(), "");

        let detailed = Fatal::UnsupportedEncryptionDetail("EncryptVersion 3".into());
        assert_eq!(detailed.kind(), "UnsupportedEncryption");
        assert_eq!(detailed.detail(), "EncryptVersion 3");
        assert_eq!(
            detailed.to_string(),
            "UnsupportedEncryption: EncryptVersion 3"
        );
        let json = serde_json::to_value(&detailed).expect("serialize");
        assert_eq!(json["kind"], "UnsupportedEncryption");
        assert_eq!(json["detail"], "EncryptVersion 3");
    }

    #[test]
    fn corruption_is_a_distinct_fatal_kind() {
        let corrupt = Fatal::CorruptDocument("DocInfo is truncated".into());
        assert_eq!(corrupt.kind(), "CorruptDocument");
        assert_eq!(corrupt.detail(), "DocInfo is truncated");
        assert_ne!(corrupt.kind(), Fatal::WrongPassword.kind());
        let json = serde_json::to_value(&corrupt).expect("serialize");
        assert_eq!(json["kind"], "CorruptDocument");
    }
}
