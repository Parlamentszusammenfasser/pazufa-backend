use snafu::prelude::*;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, PazufaError>;
macro_rules! error_from {
    ($from:ty, $to:ident, $suberr:ident, $variant:ident) => {
        impl From<$from> for PazufaError {
            fn from(source: $from) -> Self {
                Self::$to {
                    source: Box::new($suberr::$variant { source }),
                }
            }
        }
    };
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum DataValidationError {
    #[snafu(display("{source}"))]
    PazufaModel { source: pazufa_models::error::Error },

    #[snafu(display("Unauthorized because: {reason}"))]
    Unauthorized { reason: String },

    #[snafu(display("Deliberate Conflict: {reason}"))]
    Conflict { reason: String },

    #[snafu(display("Password Hashing Issue"))]
    PasswordHashing {
        source: argon2::password_hash::Error,
    },
    #[snafu(display("Format error decoding this base64-expression"))]
    Base64Decode { source: base64::DecodeError },

    #[snafu(display("Track Validation Failed"))]
    TrackValidation {
        source: track_validation::TrackValidationError,
    },

    #[snafu(display("The Vorgang could not be validated: {reason}"))]
    MalformedVorgang { reason: String },

    #[snafu(display("Required field missing: {field}"))]
    MissingField { field: String },

    #[snafu(display("Invalid format for field {field}: {message}"))]
    InvalidFormat { field: String, message: String },

    #[snafu(display("{msg}"))]
    InvalidEnumValue { msg: String },

    #[snafu(display("Incomplete Data supplied: Expected to find `{input}` in DB but didn't"))]
    IncompleteDataSupplied { input: String },

    #[snafu(display("Duplicate API ID of a {object}: {id}"))]
    DuplicateApiId { id: Uuid, object: String },

    #[snafu(display("Ambiguous match found: {message}"))]
    AmbiguousMatch { message: String },

    #[snafu(display("Multiple merge candidates found: {candidates:?}"))]
    MultipleMergeCandidates { candidates: Vec<Uuid> },

    #[snafu(display("UUID parsing error: {source}"))]
    UuidParse { source: uuid::Error },

    #[snafu(display(""))]
    QueryParametersNotSatisfied,
}
error_from!(
    track_validation::TrackValidationError,
    Validation,
    DataValidationError,
    TrackValidation
);

error_from!(
    pazufa_models::error::Error,
    Validation,
    DataValidationError,
    PazufaModel
);
error_from!(uuid::Error, Validation, DataValidationError, UuidParse);
error_from!(
    argon2::password_hash::Error,
    Validation,
    DataValidationError,
    PasswordHashing
);
error_from!(
    base64::DecodeError,
    Validation,
    DataValidationError,
    Base64Decode
);

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum DatabaseError {
    #[snafu(display("SQLX Database Operation Failed: {source}"))]
    Sqlx { source: sqlx::Error },
    #[snafu(display("Database Migration Failed: {source}"))]
    Migration { source: sqlx::migrate::MigrateError },

    #[snafu(display("{source}"))]
    Unknown {
        source: Box<dyn std::error::Error + Sync + Send>,
    },
}

error_from!(sqlx::Error, Database, DatabaseError, Sqlx);
error_from!(
    sqlx::migrate::MigrateError,
    Database,
    DatabaseError,
    Migration
);
error_from!(
    Box<dyn std::error::Error + Sync + Send>,
    Database,
    DatabaseError,
    Unknown
);

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum InfrastructureError {
    #[snafu(display("Infrastructure Error: {reason}"))]
    General { reason: String },

    #[snafu(display("Server error: {source}"))]
    Server { source: axum::Error },

    #[snafu(display("Hardware error: {source}"))]
    Hardware { source: std::io::Error },

    #[snafu(display("Mail delivery error: {source}"))]
    Mail {
        source: lettre::transport::smtp::Error,
    },

    #[snafu(display("Environment variable error: {source}"))]
    Environment { source: std::env::VarError },

    #[snafu(display("Configuration error: {message}"))]
    Configuration {
        message: String,
        config: Box<crate::Configuration>,
    },
}

error_from!(axum::Error, Infrastructure, InfrastructureError, Server);
error_from!(
    std::io::Error,
    Infrastructure,
    InfrastructureError,
    Hardware
);
error_from!(
    lettre::transport::smtp::Error,
    Infrastructure,
    InfrastructureError,
    Mail
);
error_from!(
    std::env::VarError,
    Infrastructure,
    InfrastructureError,
    Environment
);

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum PazufaError {
    #[snafu(display("Validation error: {source}"))]
    Validation { source: Box<DataValidationError> },

    #[snafu(display("Database error: {source}"))]
    Database { source: Box<DatabaseError> },

    #[snafu(display("Infrastructure error: {source}"))]
    Infrastructure { source: Box<InfrastructureError> },

    #[snafu(display("HTTP header conversion error: {source}"))]
    HeaderConversion {
        source: Box<axum::http::header::ToStrError>,
    },
    #[snafu(display("{message}"))]
    Other { message: String },
}
impl From<DataValidationError> for PazufaError {
    fn from(source: DataValidationError) -> Self {
        Self::Validation {
            source: Box::new(source),
        }
    }
}
impl From<DatabaseError> for PazufaError {
    fn from(source: DatabaseError) -> Self {
        Self::Database {
            source: Box::new(source),
        }
    }
}
impl From<InfrastructureError> for PazufaError {
    fn from(source: InfrastructureError) -> Self {
        Self::Infrastructure {
            source: Box::new(source),
        }
    }
}
impl From<axum::http::header::ToStrError> for PazufaError {
    fn from(source: axum::http::header::ToStrError) -> Self {
        Self::HeaderConversion {
            source: Box::new(source),
        }
    }
}

// Helper methods for creating errors
impl PazufaError {
    pub fn other<T: Into<String>>(message: T) -> Self {
        PazufaError::Other {
            message: message.into(),
        }
    }
}
