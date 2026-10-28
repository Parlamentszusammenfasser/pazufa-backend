use std::fmt::Display;

pub type Result<RV> = std::result::Result<RV, Error>;

#[derive(Debug, Clone)]
pub enum Error {
    /// The object was deserialized, but a validation invariant failed to
    /// hold up
    ValidationError { field: String, message: String },
    /// The object could not be serialized
    SerializationError { error: String },
    /// The object has been tried to deserialize from an invalid representation
    DeserializationError { error: String },
}

impl From<argon2::password_hash::Error> for Error {
    fn from(value: argon2::password_hash::Error) -> Self {
        Self::DeserializationError {
            error: format!("Argon2 Error while Hashing password: {value}"),
        }
    }
}
impl From<base64::DecodeError> for Error {
    fn from(value: base64::DecodeError) -> Self {
        Self::DeserializationError {
            error: format!("Error deserializing Password: Invalid Base 64 representation: {value}"),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self, f)
    }
}
