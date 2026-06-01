// Copyright (c) Michael Grier

//! Binlog header reader and GZip decompression setup.
//!
//! A `.binlog` file is a GZip-compressed stream whose decompressed content
//! begins with the format version and minimum reader version (two little-endian
//! `i32` values), followed by serialized build event records.

use std::io::{BufReader, Read};

use flate2::read::GzDecoder;

use crate::{error::MuninError, primitives::read_i32_le};

/// The minimum binlog format version we support. Version 18 introduced
/// forward-compatible reading with per-record length prefixes.
pub const MIN_SUPPORTED_VERSION: i32 = 18;

/// The maximum binlog format version we have full support for.
pub const MAX_KNOWN_VERSION: i32 = 25;

/// Parsed binlog file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinlogHeader {
    /// The file format version written by the BinaryLogger.
    pub file_format_version: i32,

    /// The minimum reader version needed to interpret this file.
    pub min_reader_version: i32,
}

/// Open a binlog stream: decompress, read the header, and return a reader
/// positioned at the start of the record stream.
///
/// The caller provides any `Read` — typically a `File` or `Cursor<Vec<u8>>`.
/// The returned reader decompresses on the fly; all subsequent reads
/// produce decompressed record bytes.
///
/// The `.binlog` format is entirely GZip-compressed; the version header is
/// the first 8 bytes of the decompressed stream.
pub fn open_binlog<R: Read>(
    reader: R,
) -> Result<(BinlogHeader, BufReader<GzDecoder<R>>), MuninError> {
    let gz = GzDecoder::new(reader);
    let mut buffered = BufReader::with_capacity(32768, gz);

    let file_format_version = read_i32_le(&mut buffered)?;

    if file_format_version < MIN_SUPPORTED_VERSION {
        return Err(MuninError::UnsupportedVersion {
            file_version: file_format_version,
            min_reader_version: 0,
        });
    }

    // Version 18+ writes a second i32: the minimum reader version.
    let min_reader_version = read_i32_le(&mut buffered)?;

    let header = BinlogHeader {
        file_format_version,
        min_reader_version,
    };

    Ok((header, buffered))
}

#[cfg(test)]
mod tests;
