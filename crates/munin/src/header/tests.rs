// Copyright (c) Michael Grier

use std::io::{Cursor, Read, Write};

use flate2::{write::GzEncoder, Compression};

use super::*;
use crate::error::MuninError;

/// Build a minimal synthetic binlog: entire stream is GZip-compressed,
/// with the version header inside the compressed data.
fn make_binlog(version: i32, min_reader: i32, body: &[u8]) -> Vec<u8> {
    let mut uncompressed = Vec::new();
    uncompressed.extend_from_slice(&version.to_le_bytes());
    uncompressed.extend_from_slice(&min_reader.to_le_bytes());
    uncompressed.extend_from_slice(body);

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&uncompressed).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn open_valid_v25_header() {
    let data = make_binlog(25, 18, &[0x00]); // EndOfFile record
    let (header, _reader) = open_binlog(Cursor::new(data)).unwrap();
    assert_eq!(header.file_format_version, 25);
    assert_eq!(header.min_reader_version, 18);
}

#[test]
fn open_valid_v18_header() {
    let data = make_binlog(18, 18, &[0x00]);
    let (header, _reader) = open_binlog(Cursor::new(data)).unwrap();
    assert_eq!(header.file_format_version, 18);
    assert_eq!(header.min_reader_version, 18);
}

#[test]
fn open_valid_v22_header() {
    let data = make_binlog(22, 18, &[0x00]);
    let (header, _reader) = open_binlog(Cursor::new(data)).unwrap();
    assert_eq!(header.file_format_version, 22);
    assert_eq!(header.min_reader_version, 18);
}

#[test]
fn reject_version_below_18() {
    let data = make_binlog(17, 17, &[0x00]);
    let err = open_binlog(Cursor::new(data)).unwrap_err();
    assert!(matches!(
        err,
        MuninError::UnsupportedVersion {
            file_version: 17,
            ..
        }
    ));
}

#[test]
fn reject_version_1() {
    let data = make_binlog(1, 0, &[0x00]);
    let err = open_binlog(Cursor::new(data)).unwrap_err();
    assert!(matches!(
        err,
        MuninError::UnsupportedVersion {
            file_version: 1,
            ..
        }
    ));
}

#[test]
fn reject_version_0() {
    let data = make_binlog(0, 0, &[0x00]);
    let err = open_binlog(Cursor::new(data)).unwrap_err();
    assert!(matches!(
        err,
        MuninError::UnsupportedVersion {
            file_version: 0,
            ..
        }
    ));
}

#[test]
fn decompress_body_after_open() {
    let body = b"MSBuild test payload";
    let data = make_binlog(25, 18, body);
    let (_header, mut reader) = open_binlog(Cursor::new(data)).unwrap();

    let mut buf = vec![0u8; body.len()];
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, body);
}

#[test]
fn empty_body_reads_eof() {
    let data = make_binlog(25, 18, &[]);
    let (_header, mut reader) = open_binlog(Cursor::new(data)).unwrap();

    let mut buf = [0u8; 1];
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn truncated_header_4_bytes() {
    // Only 4 bytes — missing the min_reader_version.
    let data = 25i32.to_le_bytes().to_vec();
    let err = open_binlog(Cursor::new(data)).unwrap_err();
    assert!(matches!(err, MuninError::Io(_)));
}

#[test]
fn truncated_header_0_bytes() {
    let data: Vec<u8> = Vec::new();
    let err = open_binlog(Cursor::new(data)).unwrap_err();
    assert!(matches!(err, MuninError::Io(_)));
}

#[test]
fn future_version_accepted_if_above_min() {
    // A hypothetical version 30 should still open (we read forward-compatibly).
    let data = make_binlog(30, 18, &[0x00]);
    let (header, _reader) = open_binlog(Cursor::new(data)).unwrap();
    assert_eq!(header.file_format_version, 30);
    assert_eq!(header.min_reader_version, 18);
}

#[test]
fn header_struct_debug_format() {
    let h = BinlogHeader {
        file_format_version: 25,
        min_reader_version: 18,
    };
    let dbg = format!("{h:?}");
    assert!(dbg.contains("25"));
    assert!(dbg.contains("18"));
}
