use std::{fmt};
use std::io::Error;
use zip::result::ZipError;

pub type ResolverResult<T> = Result<T, ResolverError>;

#[derive(Debug)]
pub enum ResolverError {
    /// This file is probably not a zip archive
    InvalidZip(ZipError),

    /// The requested file could not be found in the archive
    FileNotFound,

    /// The requested file could not be read from the archive
    FileReadFailed(Error),

    /// Not ready (not loaded or prepared)
    NotReady
}

impl fmt::Display for ResolverError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ResolverError::InvalidZip(err) => write!(fmt, "invalid resolver Zip: {}", err),
            ResolverError::FileNotFound => write!(fmt, "specified file not found in resolver archive"),
            ResolverError::FileReadFailed(err) => write!(fmt, "specified file read filed: {}", err),
            ResolverError::NotReady => write!(fmt, "the resolver zip isn't ready, did you load?"),
        }
    }
}
