// Copyright (c) Michael Grier

//! Unit tests for the jsonlog dumper (binlog → jsonlog).

use std::io::{Cursor, Write};

use flate2::{write::GzEncoder, Compression};

use super::{dump_index, dump_index_pretty};
use crate::{
    index::BinlogIndex,
    jsonlog::schema::{JsonlogEventBody, JsonlogFile, MUNIN_JSONLOG_VERSION},
    record_kind::BinaryLogRecordKind,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn enc7(value: i32) -> Vec<u8> {
    let mut v = value as u32;
    let mut buf = Vec::new();
    loop {
        let mut b = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        buf.push(b);
        if v == 0 {
            break;
        }
    }
    buf
}

fn gzip(decompressed: &[u8]) -> Vec<u8> {
    let mut e = GzEncoder::new(Vec::new(), Compression::fast());
    e.write_all(decompressed).unwrap();
    e.finish().unwrap()
}

fn header(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&18i32.to_le_bytes());
    buf.extend_from_slice(&18i32.to_le_bytes());
}

fn record(buf: &mut Vec<u8>, kind: i32, payload: &[u8]) {
    buf.extend_from_slice(&enc7(kind));
    buf.extend_from_slice(&enc7(payload.len() as i32));
    buf.extend_from_slice(payload);
}

fn eof(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&enc7(BinaryLogRecordKind::EndOfFile as i32));
}

/// Minimal valid event payload: flags=MESSAGE(0x0004), message=null,
/// (and optional kind-specific trailer added by the caller).
fn flags_message_null() -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&enc7(0x0004));
    p.extend_from_slice(&enc7(0));
    p
}

fn open_index(bytes: Vec<u8>) -> BinlogIndex {
    BinlogIndex::open(Cursor::new(bytes)).expect("open binlog")
}

fn dump_to_jsonlog_file(index: &BinlogIndex) -> JsonlogFile {
    let mut out = Vec::new();
    dump_index(index, &mut out).expect("dump");
    serde_json::from_slice(&out).expect("parse jsonlog")
}

// ---------------------------------------------------------------------------
// Cases
// ---------------------------------------------------------------------------

#[test]
fn case_01_header_only() {
    let mut d = Vec::new();
    header(&mut d);
    eof(&mut d);
    let index = open_index(gzip(&d));
    let file = dump_to_jsonlog_file(&index);
    assert_eq!(file.munin_jsonlog_version, MUNIN_JSONLOG_VERSION);
    assert_eq!(file.header.file_format_version, 18);
    assert_eq!(file.header.min_reader_version, 18);
    assert!(file.events.is_empty());
    assert!(file.strings.is_empty());
    assert!(file.name_value_lists.is_empty());
    assert!(file.archives.is_empty());
}

#[test]
fn case_02_build_started() {
    let mut d = Vec::new();
    header(&mut d);
    let mut p = flags_message_null();
    p.extend_from_slice(&enc7(0)); // environment NVL = null
    record(&mut d, BinaryLogRecordKind::BuildStarted as i32, &p);
    eof(&mut d);
    let file = dump_to_jsonlog_file(&open_index(gzip(&d)));
    assert_eq!(file.events.len(), 1);
    assert_eq!(file.events[0].kind, "BuildStarted");
    assert!(matches!(file.events[0].body, JsonlogEventBody::Decoded(_)));
}

#[test]
fn case_03_build_finished() {
    let mut d = Vec::new();
    header(&mut d);
    let mut p = flags_message_null();
    p.push(1u8); // succeeded
    record(&mut d, BinaryLogRecordKind::BuildFinished as i32, &p);
    eof(&mut d);
    let file = dump_to_jsonlog_file(&open_index(gzip(&d)));
    assert_eq!(file.events.len(), 1);
    assert_eq!(file.events[0].kind, "BuildFinished");
    assert!(matches!(file.events[0].body, JsonlogEventBody::Decoded(_)));
}

#[test]
fn case_04_message() {
    let mut d = Vec::new();
    header(&mut d);
    let mut p = flags_message_null();
    p.extend_from_slice(&enc7(0)); // subcategory
    p.extend_from_slice(&enc7(0)); // code
    p.extend_from_slice(&enc7(0)); // file
    p.extend_from_slice(&enc7(0)); // project file
    p.extend_from_slice(&[0u8; 4 * 4]); // 4 i32 line/column
    p.extend_from_slice(&enc7(0)); // importance
    record(&mut d, BinaryLogRecordKind::Message as i32, &p);
    eof(&mut d);
    let file = dump_to_jsonlog_file(&open_index(gzip(&d)));
    assert_eq!(file.events.len(), 1);
    assert_eq!(file.events[0].kind, "Message");
}

#[test]
fn case_05_two_events_preserves_order() {
    let mut d = Vec::new();
    header(&mut d);
    let mut p1 = flags_message_null();
    p1.extend_from_slice(&enc7(0));
    record(&mut d, BinaryLogRecordKind::BuildStarted as i32, &p1);
    let mut p2 = flags_message_null();
    p2.push(1u8);
    record(&mut d, BinaryLogRecordKind::BuildFinished as i32, &p2);
    eof(&mut d);
    let file = dump_to_jsonlog_file(&open_index(gzip(&d)));
    assert_eq!(file.events.len(), 2);
    assert_eq!(file.events[0].kind, "BuildStarted");
    assert_eq!(file.events[1].kind, "BuildFinished");
    assert!(file.events[1].byte_offset > file.events[0].byte_offset);
}

#[test]
fn case_06_string_table_dumped() {
    let mut d = Vec::new();
    header(&mut d);
    record(&mut d, BinaryLogRecordKind::String as i32, b"hello");
    record(&mut d, BinaryLogRecordKind::String as i32, b"world");
    eof(&mut d);
    let file = dump_to_jsonlog_file(&open_index(gzip(&d)));
    assert_eq!(file.strings, vec!["hello".to_string(), "world".to_string()]);
}

#[test]
fn case_07_nvl_table_dumped() {
    let mut d = Vec::new();
    header(&mut d);
    record(&mut d, BinaryLogRecordKind::String as i32, b"k");
    record(&mut d, BinaryLogRecordKind::String as i32, b"v");
    // NVL: one pair (key_index=10, value_index=11)
    let mut nvl = Vec::new();
    nvl.extend_from_slice(&enc7(1)); // count
    nvl.extend_from_slice(&enc7(10));
    nvl.extend_from_slice(&enc7(11));
    record(&mut d, BinaryLogRecordKind::NameValueList as i32, &nvl);
    eof(&mut d);
    let file = dump_to_jsonlog_file(&open_index(gzip(&d)));
    assert_eq!(file.name_value_lists.len(), 1);
    assert_eq!(file.name_value_lists[0], vec![[10u32, 11u32]]);
}

#[test]
fn case_08_archive_blob_base64() {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let mut d = Vec::new();
    header(&mut d);
    let archive_bytes: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01];
    record(
        &mut d,
        BinaryLogRecordKind::ProjectImportArchive as i32,
        archive_bytes,
    );
    eof(&mut d);
    let file = dump_to_jsonlog_file(&open_index(gzip(&d)));
    assert_eq!(file.archives.len(), 1);
    assert_eq!(file.archives[0].data_b64, B64.encode(archive_bytes));
}

#[test]
fn case_09_unknown_kind_falls_back_to_payload_b64() {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

    // Manually construct an IndexEntry-shaped state by parsing a real binlog
    // and then dumping; for the fallback case we use a payload that fails
    // decode by registering an unknown kind. Since BinlogIndex::open rejects
    // unknown kinds silently (auxiliary skip), forcing a fallback path
    // through the public API requires a kind that *is* known to
    // BinaryLogRecordKind but whose payload bytes won't fully decode.
    //
    // We give BuildFinished a payload that's too short (no `succeeded`
    // byte). The reader will return Err during dispatch_event; the dumper
    // should fall back to payload_b64.
    let mut d = Vec::new();
    header(&mut d);
    let truncated = flags_message_null(); // missing succeeded byte
    record(
        &mut d,
        BinaryLogRecordKind::BuildFinished as i32,
        &truncated,
    );
    eof(&mut d);
    let file = dump_to_jsonlog_file(&open_index(gzip(&d)));
    assert_eq!(file.events.len(), 1);
    match &file.events[0].body {
        JsonlogEventBody::PayloadB64(s) => {
            assert_eq!(*s, B64.encode(&truncated));
        }
        other => panic!("expected payload_b64 fallback, got {other:?}"),
    }
}

#[test]
fn case_10_pretty_form_is_indented() {
    let mut d = Vec::new();
    header(&mut d);
    let mut p = flags_message_null();
    p.extend_from_slice(&enc7(0));
    record(&mut d, BinaryLogRecordKind::BuildStarted as i32, &p);
    eof(&mut d);
    let index = open_index(gzip(&d));

    let mut compact = Vec::new();
    dump_index(&index, &mut compact).unwrap();
    let mut pretty = Vec::new();
    dump_index_pretty(&index, &mut pretty).unwrap();

    let compact_s = String::from_utf8(compact).unwrap();
    let pretty_s = String::from_utf8(pretty).unwrap();
    // Pretty form contains newlines; compact form does not.
    assert!(!compact_s.contains('\n'));
    assert!(pretty_s.contains('\n'));
    // Both parse back to equivalent JSON values.
    let a: serde_json::Value = serde_json::from_str(&compact_s).unwrap();
    let b: serde_json::Value = serde_json::from_str(&pretty_s).unwrap();
    assert_eq!(a, b);
}

#[test]
fn case_11_event_byte_offset_matches_meta() {
    let mut d = Vec::new();
    header(&mut d);
    let mut p1 = flags_message_null();
    p1.extend_from_slice(&enc7(0));
    record(&mut d, BinaryLogRecordKind::BuildStarted as i32, &p1);
    let mut p2 = flags_message_null();
    p2.push(1u8);
    record(&mut d, BinaryLogRecordKind::BuildFinished as i32, &p2);
    eof(&mut d);
    let index = open_index(gzip(&d));
    let file = dump_to_jsonlog_file(&index);
    for (i, ev) in file.events.iter().enumerate() {
        assert_eq!(ev.byte_offset, index.meta(i).unwrap().byte_offset);
    }
}
