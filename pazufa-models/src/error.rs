pub type Result<RV> = std::result::Result<RV, Error>;

#[derive(Debug)]
pub enum Error {
    AuthError {
        error: String,
    },
    /// Something went wrong in track validation
    TrackValidation {
        error: String,
    },
}

impl std::error::Error for Error {
    fn cause(&self) -> Option<&dyn std::error::Error> {
        None
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TrackValidation { error } => write!(f, "Track Validation Failed: {error}"),
            Self::AuthError { error } => write!(f, "Error in Authentication: {error}"),
        }
    }
}

impl From<argon2::password_hash::Error> for Error {
    fn from(value: argon2::password_hash::Error) -> Self {
        Self::AuthError {
            error: format!("Argon2 Error while Hashing password: {value}"),
        }
    }
}
impl From<base64::DecodeError> for Error {
    fn from(value: base64::DecodeError) -> Self {
        Self::AuthError {
            error: format!("Error deserializing Password: Invalid Base 64 representation: {value}"),
        }
    }
}

impl From<track_validation::TrackValidationError> for Error {
    fn from(value: track_validation::TrackValidationError) -> Self {
        Self::TrackValidation {
            error: value.to_string(),
        }
    }
}
