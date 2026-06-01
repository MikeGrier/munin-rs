// Copyright (c) Michael Grier

use std::io::Cursor;

use super::*;
use crate::record_kind::BinaryLogRecordKind;

// ---------------------------------------------------------------------------
// Helper: build a minimal in-memory binlog for testing
// ---------------------------------------------------------------------------

/// Encode an i32 as a 7-bit variable-length integer.
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

/// Encode an i32 as little-endian 4 bytes.
fn encode_i32_le(value: i32) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

/// Build a minimal GZip-compressed binlog with a BuildStarted and BuildFinished
/// event (the simplest possible valid binlog).
fn build_test_binlog() -> Vec<u8> {
    use std::io::Write;

    use flate2::{write::GzEncoder, Compression};

    let mut decompressed = Vec::new();

    // Header: version=18, min_reader_version=18
    decompressed.extend_from_slice(&encode_i32_le(18));
    decompressed.extend_from_slice(&encode_i32_le(18));

    // BuildStarted event (kind=1).
    // Payload: flags (just MESSAGE), message string index (0 = null).
    let payload = {
        let mut p = Vec::new();
        // flags = MESSAGE (0x0004)
        p.extend_from_slice(&encode_7bit(0x0004));
        // message dedup string index = 0 (null)
        p.extend_from_slice(&encode_7bit(0));
        // environment: NVL table index = 0 (null, no environment)
        p.extend_from_slice(&encode_7bit(0));
        p
    };
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::BuildStarted as i32));
    decompressed.extend_from_slice(&encode_7bit(payload.len() as i32));
    decompressed.extend_from_slice(&payload);

    // BuildFinished event (kind=2).
    let payload2 = {
        let mut p = Vec::new();
        // flags = MESSAGE (0x0004)
        p.extend_from_slice(&encode_7bit(0x0004));
        // message dedup string index = 0 (null)
        p.extend_from_slice(&encode_7bit(0));
        // succeeded = true
        p.push(1u8);
        p
    };
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::BuildFinished as i32));
    decompressed.extend_from_slice(&encode_7bit(payload2.len() as i32));
    decompressed.extend_from_slice(&payload2);

    // EndOfFile sentinel (kind=0).
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::EndOfFile as i32));

    // GZip compress.
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&decompressed).unwrap();
    encoder.finish().unwrap()
}

/// Build a binlog with a BuildEventContext on the BuildFinished event.
fn build_binlog_with_context() -> Vec<u8> {
    use std::io::Write;

    use flate2::{write::GzEncoder, Compression};

    let mut decompressed = Vec::new();

    // Header: version=18, min_reader_version=18
    decompressed.extend_from_slice(&encode_i32_le(18));
    decompressed.extend_from_slice(&encode_i32_le(18));

    // BuildStarted event (kind=1) — no context.
    let payload = {
        let mut p = Vec::new();
        p.extend_from_slice(&encode_7bit(0x0004)); // flags = MESSAGE
        p.extend_from_slice(&encode_7bit(0)); // message = null
        p.extend_from_slice(&encode_7bit(0)); // environment NVL index = 0 (null)
        p
    };
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::BuildStarted as i32));
    decompressed.extend_from_slice(&encode_7bit(payload.len() as i32));
    decompressed.extend_from_slice(&payload);

    // BuildFinished event (kind=2) — WITH context.
    let payload2 = {
        let mut p = Vec::new();
        // flags = MESSAGE | BUILD_EVENT_CONTEXT = 0x0004 | 0x0001 = 0x0005
        p.extend_from_slice(&encode_7bit(0x0005));
        // message dedup string index = 0 (null)
        p.extend_from_slice(&encode_7bit(0));
        // BuildEventContext: 7 fields (all 7-bit ints)
        p.extend_from_slice(&encode_7bit(1)); // node_id
        p.extend_from_slice(&encode_7bit(42)); // project_context_id
        p.extend_from_slice(&encode_7bit(7)); // target_id
        p.extend_from_slice(&encode_7bit(3)); // task_id
        p.extend_from_slice(&encode_7bit(0)); // submission_id
        p.extend_from_slice(&encode_7bit(0)); // project_instance_id
        p.extend_from_slice(&encode_7bit(0)); // evaluation_id (version > 1 → version 18 > 1)
                                              // succeeded = true
        p.push(1u8);
        p
    };
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::BuildFinished as i32));
    decompressed.extend_from_slice(&encode_7bit(payload2.len() as i32));
    decompressed.extend_from_slice(&payload2);

    // EndOfFile
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::EndOfFile as i32));

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&decompressed).unwrap();
    encoder.finish().unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn open_minimal_binlog() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    assert_eq!(
        index.len(),
        2,
        "expected 2 events (BuildStarted + BuildFinished)"
    );
    assert!(!index.is_empty());
}

#[test]
fn header_is_correct() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    assert_eq!(index.header().file_format_version, 18);
    assert_eq!(index.header().min_reader_version, 18);
}

#[test]
fn meta_record_kinds() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    assert_eq!(
        index.meta(0).unwrap().record_kind,
        BinaryLogRecordKind::BuildStarted
    );
    assert_eq!(
        index.meta(1).unwrap().record_kind,
        BinaryLogRecordKind::BuildFinished
    );
    assert!(index.meta(2).is_none());
}

#[test]
fn meta_byte_offsets_increase() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    let off0 = index.meta(0).unwrap().byte_offset;
    let off1 = index.meta(1).unwrap().byte_offset;
    assert!(off1 > off0, "second event offset must be after first");
}

#[test]
fn meta_payload_len_nonzero() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    assert!(index.meta(0).unwrap().payload_len > 0);
    assert!(index.meta(1).unwrap().payload_len > 0);
}

#[test]
fn get_deserializes_build_started() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    let event = index.get(0).expect("deserialize failed").unwrap();
    assert!(matches!(event, BinlogEvent::BuildStarted(_)));
}

#[test]
fn get_deserializes_build_finished() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    let event = index.get(1).expect("deserialize failed").unwrap();
    assert!(matches!(event, BinlogEvent::BuildFinished(_)));
}

#[test]
fn get_out_of_range_returns_none() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    assert!(index.get(999).expect("should not error").is_none());
}

#[test]
fn get_all_returns_all_events() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    let events = index.get_all().expect("get_all failed");
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], BinlogEvent::BuildStarted(_)));
    assert!(matches!(events[1], BinlogEvent::BuildFinished(_)));
}

#[test]
fn indices_by_kind_filters_correctly() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    let started = index.indices_by_kind(BinaryLogRecordKind::BuildStarted);
    assert_eq!(started, vec![0]);
    let finished = index.indices_by_kind(BinaryLogRecordKind::BuildFinished);
    assert_eq!(finished, vec![1]);
    let messages = index.indices_by_kind(BinaryLogRecordKind::Message);
    assert!(messages.is_empty());
}

#[test]
fn extract_context_from_payload() {
    let data = build_binlog_with_context();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");

    // BuildStarted (index 0) has no context.
    assert!(index.meta(0).unwrap().context.is_none());

    // BuildFinished (index 1) has context with project_context_id=42.
    let ctx = index.meta(1).unwrap().context.unwrap();
    assert_eq!(ctx.project_context_id, 42);
    assert_eq!(ctx.target_id, 7);
    assert_eq!(ctx.task_id, 3);
}

#[test]
fn indices_by_project_context() {
    let data = build_binlog_with_context();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    let matching = index.indices_by_project_context(42);
    assert_eq!(matching, vec![1]);
    let none = index.indices_by_project_context(999);
    assert!(none.is_empty());
}

#[test]
fn indices_by_target_id() {
    let data = build_binlog_with_context();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    let matching = index.indices_by_target_id(7);
    assert_eq!(matching, vec![1]);
}

#[test]
fn indices_by_task_id() {
    let data = build_binlog_with_context();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    let matching = index.indices_by_task_id(3);
    assert_eq!(matching, vec![1]);
}

#[test]
fn query_combined_filters() {
    let data = build_binlog_with_context();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");

    // Match by kind only.
    let r = index.query(Some(BinaryLogRecordKind::BuildFinished), None, None, None);
    assert_eq!(r, vec![1]);

    // Match by kind + project context.
    let r = index.query(
        Some(BinaryLogRecordKind::BuildFinished),
        Some(42),
        None,
        None,
    );
    assert_eq!(r, vec![1]);

    // Mismatched kind — should be empty.
    let r = index.query(Some(BinaryLogRecordKind::Message), Some(42), None, None);
    assert!(r.is_empty());

    // All criteria match.
    let r = index.query(
        Some(BinaryLogRecordKind::BuildFinished),
        Some(42),
        Some(7),
        Some(3),
    );
    assert_eq!(r, vec![1]);

    // One criterion mismatches.
    let r = index.query(
        Some(BinaryLogRecordKind::BuildFinished),
        Some(42),
        Some(7),
        Some(999),
    );
    assert!(r.is_empty());

    // No filters — returns all.
    let r = index.query(None, None, None, None);
    assert_eq!(r, vec![0, 1]);
}

#[test]
fn iter_meta_yields_all() {
    let data = build_test_binlog();
    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    let metas: Vec<_> = index.iter_meta().collect();
    assert_eq!(metas.len(), 2);
    assert_eq!(metas[0].0, 0);
    assert_eq!(metas[1].0, 1);
}

#[test]
fn empty_binlog_produces_empty_index() {
    use std::io::Write;

    use flate2::{write::GzEncoder, Compression};

    let mut decompressed = Vec::new();
    decompressed.extend_from_slice(&encode_i32_le(18));
    decompressed.extend_from_slice(&encode_i32_le(18));
    // Immediately EndOfFile.
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::EndOfFile as i32));

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&decompressed).unwrap();
    let data = encoder.finish().unwrap();

    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    assert!(index.is_empty());
    assert_eq!(index.len(), 0);
    assert!(index.get(0).expect("should not error").is_none());
}

#[test]
fn string_table_populated_during_indexing() {
    use std::io::Write;

    use flate2::{write::GzEncoder, Compression};

    let mut decompressed = Vec::new();
    decompressed.extend_from_slice(&encode_i32_le(18));
    decompressed.extend_from_slice(&encode_i32_le(18));

    // A String record with payload "hello".
    let string_payload = b"hello";
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::String as i32));
    decompressed.extend_from_slice(&encode_7bit(string_payload.len() as i32));
    decompressed.extend_from_slice(string_payload);

    // BuildStarted event referencing that string (index 10 = first user string).
    let payload = {
        let mut p = Vec::new();
        p.extend_from_slice(&encode_7bit(0x0004)); // flags = MESSAGE
        p.extend_from_slice(&encode_7bit(10)); // message = string index 10 ("hello")
        p.extend_from_slice(&encode_7bit(0)); // environment NVL index = 0 (null)
        p
    };
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::BuildStarted as i32));
    decompressed.extend_from_slice(&encode_7bit(payload.len() as i32));
    decompressed.extend_from_slice(&payload);

    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::EndOfFile as i32));

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&decompressed).unwrap();
    let data = encoder.finish().unwrap();

    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    assert_eq!(index.strings().len(), 1);
    assert_eq!(index.strings().get(10).unwrap(), Some("hello"));

    // The deserialized event should have the resolved message.
    let event = index.get(0).unwrap().unwrap();
    if let BinlogEvent::BuildStarted(ref e) = event {
        assert_eq!(e.fields.message.as_deref(), Some("hello"));
    } else {
        panic!("expected BuildStarted");
    }
}

#[test]
fn unknown_record_kind_skipped() {
    use std::io::Write;

    use flate2::{write::GzEncoder, Compression};

    let mut decompressed = Vec::new();
    decompressed.extend_from_slice(&encode_i32_le(18));
    decompressed.extend_from_slice(&encode_i32_le(18));

    // An unknown record kind (200) with some payload.
    decompressed.extend_from_slice(&encode_7bit(200));
    let unknown_payload = vec![0u8; 10];
    decompressed.extend_from_slice(&encode_7bit(unknown_payload.len() as i32));
    decompressed.extend_from_slice(&unknown_payload);

    // BuildStarted event.
    let payload = {
        let mut p = Vec::new();
        p.extend_from_slice(&encode_7bit(0x0004));
        p.extend_from_slice(&encode_7bit(0));
        p.extend_from_slice(&encode_7bit(0)); // environment NVL index = 0 (null)
        p
    };
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::BuildStarted as i32));
    decompressed.extend_from_slice(&encode_7bit(payload.len() as i32));
    decompressed.extend_from_slice(&payload);

    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::EndOfFile as i32));

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&decompressed).unwrap();
    let data = encoder.finish().unwrap();

    let index = BinlogIndex::open(Cursor::new(data)).expect("failed to open");
    // Only the BuildStarted should be indexed; the unknown record is skipped.
    assert_eq!(index.len(), 1);
    assert_eq!(
        index.meta(0).unwrap().record_kind,
        BinaryLogRecordKind::BuildStarted
    );
}

#[test]
fn rejects_negative_record_length() {
    use std::io::Write;

    use flate2::{write::GzEncoder, Compression};

    // Build a binlog where the first real record has a negative (-1) length.
    // -1 encoded as a 7-bit i32 = [0xFF, 0xFF, 0xFF, 0xFF, 0x0F]
    let mut decompressed = Vec::new();
    decompressed.extend_from_slice(&encode_i32_le(18)); // version
    decompressed.extend_from_slice(&encode_i32_le(18)); // min_reader_version
                                                        // BuildStarted kind byte.
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::BuildStarted as i32));
    // Negative length: -1
    decompressed.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&decompressed).unwrap();
    let data = encoder.finish().unwrap();

    let err = BinlogIndex::open(Cursor::new(data)).unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}

#[test]
fn rejects_oversized_record_length() {
    use std::io::Write;

    use flate2::{write::GzEncoder, Compression};

    // MAX_BINLOG_FIELD_LEN + 1 = 0x1000_0001 = 268,435,457; encoded as 7-bit i32:
    // [0x81, 0x80, 0x80, 0x80, 0x01]
    let mut decompressed = Vec::new();
    decompressed.extend_from_slice(&encode_i32_le(18));
    decompressed.extend_from_slice(&encode_i32_le(18));
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::BuildStarted as i32));
    decompressed.extend_from_slice(&[0x81, 0x80, 0x80, 0x80, 0x01]); // MAX+1 = 0x10000001

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&decompressed).unwrap();
    let data = encoder.finish().unwrap();

    let err = BinlogIndex::open(Cursor::new(data)).unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}

#[test]
fn rejects_negative_nvl_count_in_index() {
    use std::io::Write;

    use flate2::{write::GzEncoder, Compression};

    // A NameValueList record whose count field is -1. The index builder should
    // reject this before allocating the pairs vector.
    // -1 encoded as 7-bit i32 = [0xFF, 0xFF, 0xFF, 0xFF, 0x0F]
    let payload: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, 0x0F];
    let mut decompressed = Vec::new();
    decompressed.extend_from_slice(&encode_i32_le(18));
    decompressed.extend_from_slice(&encode_i32_le(18));
    decompressed.extend_from_slice(&encode_7bit(BinaryLogRecordKind::NameValueList as i32));
    decompressed.extend_from_slice(&encode_7bit(payload.len() as i32));
    decompressed.extend_from_slice(payload);

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&decompressed).unwrap();
    let data = encoder.finish().unwrap();

    let err = BinlogIndex::open(Cursor::new(data)).unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}
