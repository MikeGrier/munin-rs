// Copyright (c) Michael Grier

//! Error types for the munin binlog reader.

use std::{fmt, io};

/// Errors that can occur when reading or decoding a binlog file.
#[derive(Debug)]
pub enum MuninError {
    /// An I/O error occurred while reading the stream.
    Io(io::Error),

    /// The file does not appear to be a valid binlog (bad magic, truncated header, etc.).
    InvalidFormat(String),

    /// The binlog's format version is not supported by this reader.
    UnsupportedVersion {
        file_version: i32,
        min_reader_version: i32,
    },

    /// A record references a string or name-value-list index that has not been seen.
    InvalidIndex { kind: &'static str, index: i32 },

    /// A 7-bit encoded integer exceeded the maximum allowed byte count.
    OverlongVarInt,

    /// A string record contains invalid UTF-8.
    InvalidUtf8,
}

impl fmt::Display for MuninError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MuninError::Io(e) => write!(f, "I/O error: {e}"),
            MuninError::InvalidFormat(msg) => write!(f, "invalid binlog format: {msg}"),
            MuninError::UnsupportedVersion {
                file_version,
                min_reader_version,
            } => write!(
                f,
                "unsupported binlog version {file_version} \
                 (minimum reader version {min_reader_version}, we support >= 18)"
            ),
            MuninError::InvalidIndex { kind, index } => {
                write!(f, "invalid {kind} index: {index}")
            }
            MuninError::OverlongVarInt => write!(f, "7-bit encoded integer is too long"),
            MuninError::InvalidUtf8 => write!(f, "string record contains invalid UTF-8"),
        }
    }
}

impl std::error::Error for MuninError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MuninError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for MuninError {
    fn from(e: io::Error) -> Self {
        MuninError::Io(e)
    }
}
