use borsh::io;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum StorageError {
    Database(sled::Error),
    Serialization(io::Error),
    Integrity(&'static str),
    MissingStorageVersion,
    UnsupportedStorageVersion { expected: u8, found: u8 },
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Database(error) => write!(f, "storage database error: {error}"),
            StorageError::Serialization(error) => write!(f, "storage serialization error: {error}"),
            StorageError::Integrity(message) => write!(f, "storage integrity error: {message}"),
            StorageError::MissingStorageVersion => {
                f.write_str("storage version is missing from existing database")
            }
            StorageError::UnsupportedStorageVersion { expected, found } => write!(
                f,
                "unsupported storage version: expected {expected}, found {found}"
            ),
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StorageError::Database(error) => Some(error),
            StorageError::Serialization(error) => Some(error),
            StorageError::Integrity(_) => None,
            StorageError::MissingStorageVersion => None,
            StorageError::UnsupportedStorageVersion { .. } => None,
        }
    }
}

impl From<sled::Error> for StorageError {
    fn from(error: sled::Error) -> Self {
        StorageError::Database(error)
    }
}

impl From<io::Error> for StorageError {
    fn from(error: io::Error) -> Self {
        StorageError::Serialization(error)
    }
}
