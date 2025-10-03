use std::env::{var, VarError::*};
use std::ffi::OsString;
use std::fmt;
use std::path::{is_separator, PathBuf};

const CREDENTIALS_DIRECTORY: &str = "CREDENTIALS_DIRECTORY";

/// Takes a systemd credential ID. If the environment variable
/// `CREDENTIALS_DIRECTORY` is set and valid Unicode, and the file corresponding
/// to the credential exists, returns the path of the file corresponding to the
/// credential.
///
/// Absence of the environment variable and inexistence of the file are handled
/// gracefully returning `Ok(None)`.
pub fn path(id: &str) -> Result<Option<PathBuf>, PathError> {
    use PathError::*;

    if id.contains(is_separator) {
        return Err(InvalidCredentialId { id: id.to_owned() });
    }

    let credential = match var(CREDENTIALS_DIRECTORY) {
        Err(NotUnicode(os)) => return Err(EnvVarNotUnicode { os }),
        Err(NotPresent) => return Ok(None),
        Ok(env) => PathBuf::from(env).join(id),
    };

    Ok(credential.exists().then_some(credential))
}

/// The error returned by [`path`].
#[derive(Debug)]
pub enum PathError {
    InvalidCredentialId { id: String },
    EnvVarNotUnicode { os: OsString },
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use PathError::*;
        match self {
		InvalidCredentialId { id } => write!(f, "The systemd credential ID '{id}' is invalid."),
		EnvVarNotUnicode { os } => write!(f, "The value of environment variable '{CREDENTIALS_DIRECTORY}' is not valid Unicode (it lossily translates to '{}').", os.to_string_lossy()),
	}
    }
}
