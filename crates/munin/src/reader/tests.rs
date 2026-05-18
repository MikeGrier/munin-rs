// Copyright (c) Michael Grier

use std::io::{Cursor, Write};

use flate2::{write::GzEncoder, Compression};

use super::*;
use crate::record_kind::BinaryLogRecordKind;

// ---------------------------------------------------------------------------
// Encoding helpers (mirrors events/tests.rs, extended for full records)
// ---------------------------------------------------------------------------

/// Encode a single i32 as a 7-bit variable-length integer.
fn encode_7bit(value: i32) -> Vec<u8> {
    let mut v = value as u32;
    let mut buf = Vec::new();
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if v == 0 {
            break;
        }
    }
    buf
}

/// Encode a bool (1 byte).
fn encode_bool(val: bool) -> Vec<u8> {
    vec![if val { 1 } else { 0 }]
}

/// Build a flags-only preamble for BuildEventArgsFields (no optional fields).
fn encode_empty_fields() -> Vec<u8> {
    encode_7bit(0) // NONE flags
}

/// Encode a complete record: kind (varint) + length (varint) + payload.
fn encode_record(kind: BinaryLogRecordKind, payload: &[u8]) -> Vec<u8> {
    let mut buf = encode_7bit(kind as i32);
    buf.extend(encode_7bit(payload.len() as i32));
    buf.extend_from_slice(payload);
    buf
}

/// Build a synthetic binlog: header + gzip-compressed body.
fn make_binlog(version: i32, min_reader: i32, body: &[u8]) -> Vec<u8> {
    let mut uncompressed = Vec::new();
    uncompressed.extend_from_slice(&version.to_le_bytes());
    uncompressed.extend_from_slice(&min_reader.to_le_bytes());
    uncompressed.extend_from_slice(body);

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&uncompressed).unwrap();
    encoder.finish().unwrap()
}

/// Build a String record payload — raw UTF-8 bytes (no dotnet length prefix).
fn string_record_payload(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

/// Build a NameValueList record payload: count + (key_idx, val_idx) pairs.
fn nvl_record_payload(pairs: &[(i32, i32)]) -> Vec<u8> {
    let mut buf = encode_7bit(pairs.len() as i32);
    for &(k, v) in pairs {
        buf.extend(encode_7bit(k));
        buf.extend(encode_7bit(v));
    }
    buf
}

/// Encode a BuildFinished payload: empty fields + succeeded bool.
fn build_finished_payload(succeeded: bool) -> Vec<u8> {
    let mut buf = encode_empty_fields();
    buf.extend(encode_bool(succeeded));
    buf
}

/// Encode a BuildCanceled payload: just empty fields.
fn build_canceled_payload() -> Vec<u8> {
    encode_empty_fields()
}

/// Encode a BuildStarted payload: empty fields + null environment dict (index 0).
fn build_started_payload() -> Vec<u8> {
    let mut buf = encode_empty_fields();
    // environment: NVL index 0 = null
    buf.extend(encode_7bit(0));
    buf
}

/// Encode a ProjectFinished payload: empty fields + null project_file + succeeded.
fn project_finished_payload(succeeded: bool) -> Vec<u8> {
    let mut buf = encode_empty_fields();
    buf.extend(encode_7bit(0)); // project_file = null
    buf.extend(encode_bool(succeeded));
    buf
}

/// Encode a BuildMessage payload: empty fields with importance.
fn build_message_payload() -> Vec<u8> {
    // NONE flags
    // importance written unconditionally for message events in pre-v13
    // but we're at v18+. The is_message_event=true path still reads importance
    // when the flag is not set AND version < 13. For v18+ we need the flag.
    // Actually for v18+ (>= 13), importance is flag-driven. If no IMPORTANCE
    // flag, it's not read. So empty fields with is_message_event=true is fine
    // for v18+ as long as we don't set the IMPORTANCE flag.
    encode_7bit(0)
}

// ---------------------------------------------------------------------------
// EndOfFile
// ---------------------------------------------------------------------------

/// EndOfFile record is just the kind byte (0), no length prefix.
fn eof_record() -> Vec<u8> {
    encode_7bit(BinaryLogRecordKind::EndOfFile as i32)
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
fn open_and_immediate_eof() {
    let body = eof_record();
    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert_eq!(reader.header().file_format_version, 25);
    assert!(reader.next_event().unwrap().is_none());
}

#[test]
fn read_single_build_finished_event() {
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let event = reader.next_event().unwrap().unwrap();
    match &event {
        BinlogEvent::BuildFinished(e) => assert!(e.succeeded),
        other => panic!("expected BuildFinished, got {:?}", other.record_kind()),
    }
    assert_eq!(event.record_kind(), BinaryLogRecordKind::BuildFinished);

    assert!(reader.next_event().unwrap().is_none());
}

#[test]
fn read_build_started_and_finished() {
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::BuildStarted,
        &build_started_payload(),
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(false),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let e1 = reader.next_event().unwrap().unwrap();
    assert!(matches!(e1, BinlogEvent::BuildStarted(_)));

    let e2 = reader.next_event().unwrap().unwrap();
    match &e2 {
        BinlogEvent::BuildFinished(e) => assert!(!e.succeeded),
        other => panic!("expected BuildFinished, got {:?}", other.record_kind()),
    }

    assert!(reader.next_event().unwrap().is_none());
}

#[test]
fn string_records_populate_string_table() {
    let mut body = Vec::new();
    // Add two string records
    body.extend(encode_record(
        BinaryLogRecordKind::String,
        &string_record_payload("hello"),
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::String,
        &string_record_payload("world"),
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    // String records are ingested silently; first event is BuildFinished.
    let event = reader.next_event().unwrap().unwrap();
    assert!(matches!(event, BinlogEvent::BuildFinished(_)));

    assert_eq!(reader.strings().len(), 2);
    assert_eq!(reader.strings().get(10).unwrap(), Some("hello"));
    assert_eq!(reader.strings().get(11).unwrap(), Some("world"));
}

#[test]
fn nvl_records_populate_nvl_table() {
    let mut body = Vec::new();
    // Two string records first: index 10 = "key", index 11 = "value"
    body.extend(encode_record(
        BinaryLogRecordKind::String,
        &string_record_payload("key"),
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::String,
        &string_record_payload("value"),
    ));
    // NVL record: one pair (key=10, value=11)
    body.extend(encode_record(
        BinaryLogRecordKind::NameValueList,
        &nvl_record_payload(&[(10, 11)]),
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let event = reader.next_event().unwrap().unwrap();
    assert!(matches!(event, BinlogEvent::BuildFinished(_)));

    assert_eq!(reader.nvl_table().len(), 1);
    let resolved = reader.nvl_table().resolve(10, reader.strings()).unwrap();
    let pairs = resolved.unwrap();
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0], ("key".to_string(), "value".to_string()));
}

#[test]
fn archive_records_are_stored() {
    let archive_data = b"PK\x03\x04fake-zip-data-12345";
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        archive_data,
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let event = reader.next_event().unwrap().unwrap();
    assert!(matches!(event, BinlogEvent::BuildFinished(_)));

    assert_eq!(reader.archives().len(), 1);
    assert_eq!(reader.archives()[0], archive_data);
}

#[test]
fn multiple_archives_accumulated() {
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        b"archive1",
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        b"archive2",
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        b"archive3",
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    assert_eq!(reader.archives().len(), 3);
    assert_eq!(reader.archives()[0], b"archive1");
    assert_eq!(reader.archives()[1], b"archive2");
    assert_eq!(reader.archives()[2], b"archive3");
}

#[test]
fn unknown_record_kind_skipped() {
    let mut body = Vec::new();
    // Unknown kind = 99, payload = some garbage
    body.extend(encode_7bit(99)); // kind
    body.extend(encode_7bit(5)); // length = 5
    body.extend(&[0xDE, 0xAD, 0xBE, 0xEF, 0x42]); // payload
                                                  // Real event follows
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    // Unknown record skipped, next event is BuildFinished
    let event = reader.next_event().unwrap().unwrap();
    assert!(matches!(event, BinlogEvent::BuildFinished(_)));
    assert!(reader.next_event().unwrap().is_none());
}

#[test]
fn multiple_unknown_records_skipped() {
    let mut body = Vec::new();
    // Three unknown kinds
    for kind_val in [50, 60, 70] {
        body.extend(encode_7bit(kind_val));
        body.extend(encode_7bit(3)); // 3-byte payload
        body.extend(&[0, 0, 0]);
    }
    body.extend(encode_record(
        BinaryLogRecordKind::BuildCanceled,
        &build_canceled_payload(),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let event = reader.next_event().unwrap().unwrap();
    assert!(matches!(event, BinlogEvent::BuildCanceled(_)));
    assert!(reader.next_event().unwrap().is_none());
}

#[test]
fn forward_compat_extra_payload_bytes_discarded() {
    // A BuildFinished record with extra trailing bytes in the payload.
    let mut payload = build_finished_payload(true);
    payload.extend(&[0xFF, 0xFF, 0xFF]); // extra bytes from a "future" version

    let mut body = Vec::new();
    body.extend(encode_record(BinaryLogRecordKind::BuildFinished, &payload));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    // The reader should succeed, ignoring the extra trailing bytes.
    let event = reader.next_event().unwrap().unwrap();
    match &event {
        BinlogEvent::BuildFinished(e) => assert!(e.succeeded),
        other => panic!("expected BuildFinished, got {:?}", other.record_kind()),
    }
    assert!(reader.next_event().unwrap().is_none());
}

#[test]
fn read_all_events_collects_everything() {
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::BuildStarted,
        &build_started_payload(),
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let events = reader.read_all_events().unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], BinlogEvent::BuildStarted(_)));
    assert!(matches!(events[1], BinlogEvent::BuildFinished(_)));
}

#[test]
fn read_all_events_empty_log() {
    let body = eof_record();
    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let events = reader.read_all_events().unwrap();
    assert!(events.is_empty());
}

#[test]
fn next_event_after_eof_returns_none() {
    let body = eof_record();
    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    // Calling again still returns None.
    assert!(reader.next_event().unwrap().is_none());
    assert!(reader.next_event().unwrap().is_none());
}

#[test]
fn interleaved_string_and_event_records() {
    let mut body = Vec::new();

    // String "hello" → index 10
    body.extend(encode_record(
        BinaryLogRecordKind::String,
        &string_record_payload("hello"),
    ));

    // BuildCanceled
    body.extend(encode_record(
        BinaryLogRecordKind::BuildCanceled,
        &build_canceled_payload(),
    ));

    // String "world" → index 11
    body.extend(encode_record(
        BinaryLogRecordKind::String,
        &string_record_payload("world"),
    ));

    // BuildFinished
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));

    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let e1 = reader.next_event().unwrap().unwrap();
    assert!(matches!(e1, BinlogEvent::BuildCanceled(_)));
    assert_eq!(reader.strings().len(), 1);

    let e2 = reader.next_event().unwrap().unwrap();
    assert!(matches!(e2, BinlogEvent::BuildFinished(_)));
    assert_eq!(reader.strings().len(), 2);
}

#[test]
fn version_18_minimum_supported() {
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));
    body.extend(eof_record());

    let data = make_binlog(18, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();
    assert_eq!(reader.header().file_format_version, 18);

    let event = reader.next_event().unwrap().unwrap();
    assert!(matches!(event, BinlogEvent::BuildFinished(_)));
}

#[test]
fn reject_version_below_18() {
    let data = make_binlog(17, 17, &[0]);
    let err = BinlogReader::open(Cursor::new(data)).unwrap_err();
    assert!(matches!(
        err,
        MuninError::UnsupportedVersion {
            file_version: 17,
            ..
        }
    ));
}

#[test]
fn mixed_auxiliary_and_events() {
    let mut body = Vec::new();

    // Strings: 10="k1", 11="v1"
    body.extend(encode_record(
        BinaryLogRecordKind::String,
        &string_record_payload("k1"),
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::String,
        &string_record_payload("v1"),
    ));

    // NVL: [(10, 11)]
    body.extend(encode_record(
        BinaryLogRecordKind::NameValueList,
        &nvl_record_payload(&[(10, 11)]),
    ));

    // Archive
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        b"archive-data",
    ));

    // Unknown kind
    body.extend(encode_7bit(99));
    body.extend(encode_7bit(2));
    body.extend(&[0, 0]);

    // Event
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));

    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let event = reader.next_event().unwrap().unwrap();
    assert!(matches!(event, BinlogEvent::BuildFinished(_)));

    // Verify all auxiliary data was accumulated
    assert_eq!(reader.strings().len(), 2);
    assert_eq!(reader.nvl_table().len(), 1);
    assert_eq!(reader.archives().len(), 1);
}

#[test]
fn build_message_event_dispatch() {
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::Message,
        &build_message_payload(),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let event = reader.next_event().unwrap().unwrap();
    assert_eq!(event.record_kind(), BinaryLogRecordKind::Message);
    assert!(matches!(event, BinlogEvent::Message(_)));
}

#[test]
fn build_check_message_vs_message_distinct() {
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::Message,
        &build_message_payload(),
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::BuildCheckMessage,
        &build_message_payload(),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let e1 = reader.next_event().unwrap().unwrap();
    assert_eq!(e1.record_kind(), BinaryLogRecordKind::Message);
    assert!(matches!(e1, BinlogEvent::Message(_)));

    let e2 = reader.next_event().unwrap().unwrap();
    assert_eq!(e2.record_kind(), BinaryLogRecordKind::BuildCheckMessage);
    assert!(matches!(e2, BinlogEvent::BuildCheckMessage(_)));
}

#[test]
fn project_finished_event_dispatch() {
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectFinished,
        &project_finished_payload(false),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let event = reader.next_event().unwrap().unwrap();
    match &event {
        BinlogEvent::ProjectFinished(e) => {
            assert!(!e.succeeded);
            assert!(e.project_file.is_none());
        }
        other => panic!("expected ProjectFinished, got {:?}", other.record_kind()),
    }
}

#[test]
fn zero_length_payload_record() {
    // An event with a zero-length payload. This would cause the event reader
    // to fail, but it exercises the framing logic.
    let mut body = Vec::new();
    body.extend(encode_7bit(BinaryLogRecordKind::BuildCanceled as i32));
    body.extend(encode_7bit(0)); // zero-length payload
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    // BuildCanceled needs at least a flags field; zero-length payload
    // should cause an I/O error (unexpected EOF on the Cursor).
    let result = reader.next_event();
    assert!(result.is_err());
}

#[test]
fn many_string_records() {
    let mut body = Vec::new();
    for i in 0..100 {
        body.extend(encode_record(
            BinaryLogRecordKind::String,
            &string_record_payload(&format!("string_{i}")),
        ));
    }
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let event = reader.next_event().unwrap().unwrap();
    assert!(matches!(event, BinlogEvent::BuildFinished(_)));

    assert_eq!(reader.strings().len(), 100);
    for i in 0..100 {
        let idx = 10 + i;
        assert_eq!(
            reader.strings().get(idx).unwrap(),
            Some(format!("string_{i}").as_str())
        );
    }
}

#[test]
fn sequence_of_all_lifecycle_events() {
    // Simulate a minimal build lifecycle:
    // BuildStarted → BuildFinished
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::BuildStarted,
        &build_started_payload(),
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let events = reader.read_all_events().unwrap();
    assert_eq!(events.len(), 2);

    let kinds: Vec<_> = events.iter().map(|e| e.record_kind()).collect();
    assert_eq!(
        kinds,
        vec![
            BinaryLogRecordKind::BuildStarted,
            BinaryLogRecordKind::BuildFinished,
        ]
    );
}

#[test]
fn empty_nvl_record() {
    let mut body = Vec::new();
    // NVL record with zero pairs
    body.extend(encode_record(
        BinaryLogRecordKind::NameValueList,
        &nvl_record_payload(&[]),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    assert_eq!(reader.nvl_table().len(), 1);
}

#[test]
fn string_with_unicode_content() {
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::String,
        &string_record_payload("日本語テスト"),
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::String,
        &string_record_payload("🦀 Rust"),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    assert_eq!(reader.strings().get(10).unwrap(), Some("日本語テスト"));
    assert_eq!(reader.strings().get(11).unwrap(), Some("🦀 Rust"));
}

#[test]
fn large_unknown_record_skipped() {
    let mut body = Vec::new();
    // Unknown kind = 200, large payload (1024 bytes)
    let big_payload = vec![0xAB; 1024];
    body.extend(encode_7bit(200));
    body.extend(encode_7bit(big_payload.len() as i32));
    body.extend(&big_payload);

    body.extend(encode_record(
        BinaryLogRecordKind::BuildFinished,
        &build_finished_payload(true),
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    let event = reader.next_event().unwrap().unwrap();
    assert!(matches!(event, BinlogEvent::BuildFinished(_)));
}

#[test]
fn version_22_header() {
    let body = eof_record();
    let data = make_binlog(22, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert_eq!(reader.header().file_format_version, 22);
    assert_eq!(reader.header().min_reader_version, 18);
    assert!(reader.next_event().unwrap().is_none());
}

// ---------------------------------------------------------------------------
// Archive extraction tests
// ---------------------------------------------------------------------------

/// Create a zip archive in memory with the given (path, contents) entries.
fn make_zip(entries: &[(&str, &str)]) -> Vec<u8> {
    use zip::{write::SimpleFileOptions, ZipWriter};

    let buf = Vec::new();
    let mut writer = ZipWriter::new(Cursor::new(buf));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for &(name, content) in entries {
        writer.start_file(name, options).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }

    writer.finish().unwrap().into_inner()
}

#[test]
fn extract_single_file_archive() {
    let zip_data = make_zip(&[("project.csproj", "<Project />")]);
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        &zip_data,
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    let entries = reader.extract_archives().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "project.csproj");
    assert_eq!(entries[0].contents, "<Project />");
}

#[test]
fn extract_multiple_files_archive() {
    let zip_data = make_zip(&[
        ("dir/a.targets", "targets-content"),
        ("dir/b.props", "props-content"),
        ("c.csproj", "csproj-content"),
    ]);
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        &zip_data,
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    let entries = reader.extract_archives().unwrap();
    assert_eq!(entries.len(), 3);

    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"dir/a.targets"));
    assert!(paths.contains(&"dir/b.props"));
    assert!(paths.contains(&"c.csproj"));
}

#[test]
fn extract_multiple_archives() {
    let zip1 = make_zip(&[("file1.xml", "content1")]);
    let zip2 = make_zip(&[("file2.xml", "content2")]);
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        &zip1,
    ));
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        &zip2,
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    let entries = reader.extract_archives().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].contents, "content1");
    assert_eq!(entries[1].contents, "content2");
}

#[test]
fn extract_empty_archive() {
    // A valid zip with no entries.
    let zip_data = make_zip(&[]);
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        &zip_data,
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    let entries = reader.extract_archives().unwrap();
    assert!(entries.is_empty());
}

#[test]
fn extract_no_archives() {
    let body = eof_record();
    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    let entries = reader.extract_archives().unwrap();
    assert!(entries.is_empty());
}

#[test]
fn extract_archive_with_unicode_content() {
    let zip_data = make_zip(&[("unicode.txt", "日本語テスト 🦀")]);
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        &zip_data,
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    let entries = reader.extract_archives().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].contents, "日本語テスト 🦀");
}

#[test]
fn extract_archive_with_empty_file() {
    let zip_data = make_zip(&[("empty.txt", "")]);
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        &zip_data,
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    let entries = reader.extract_archives().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "empty.txt");
    assert_eq!(entries[0].contents, "");
}

#[test]
fn extract_archive_with_nested_paths() {
    let zip_data = make_zip(&[
        ("a/b/c/deep.xml", "<deep />"),
        ("a/b/shallow.xml", "<shallow />"),
    ]);
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        &zip_data,
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    let entries = reader.extract_archives().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, "a/b/c/deep.xml");
    assert_eq!(entries[1].path, "a/b/shallow.xml");
}

#[test]
fn extract_large_file_in_archive() {
    let large_content = "x".repeat(10_000);
    let zip_data = make_zip(&[("large.txt", &large_content)]);
    let mut body = Vec::new();
    body.extend(encode_record(
        BinaryLogRecordKind::ProjectImportArchive,
        &zip_data,
    ));
    body.extend(eof_record());

    let data = make_binlog(25, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();

    assert!(reader.next_event().unwrap().is_none());
    let entries = reader.extract_archives().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].contents.len(), 10_000);
}

// ---------------------------------------------------------------------------
// #14: Regression tests — negative record length / negative NVL count
// ---------------------------------------------------------------------------

#[test]
fn rejects_negative_record_length_in_reader() {
    // A record whose 7-bit length field is -1. The reader should reject this
    // with InvalidFormat before attempting any allocation.
    let mut body = Vec::new();
    body.extend(encode_7bit(BinaryLogRecordKind::BuildStarted as i32));
    body.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]); // length = -1

    let data = make_binlog(18, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();
    let err = reader.next_event().unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}

#[test]
fn rejects_negative_nvl_count_in_reader() {
    // A NameValueList record whose count field is -1. The reader should reject
    // this before allocating the pairs vector.
    let payload: Vec<u8> = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]; // count = -1
    let mut body = Vec::new();
    body.extend(encode_record(BinaryLogRecordKind::NameValueList, &payload));

    let data = make_binlog(18, 18, &body);
    let mut reader = BinlogReader::open(Cursor::new(data)).unwrap();
    let err = reader.next_event().unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}
