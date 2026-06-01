// Copyright (c) Michael Grier
//
// Full roundtrip: binlog → jsonlog → binlog → reopen, then assert that
// every per-event meta and decoded payload matches the original index.

use std::io::Cursor;

use munin_msbuild::{index::BinlogIndex, jsonlog};

const HELLO_BINLOG: &[u8] = include_bytes!("data/hello.binlog");

#[test]
fn binlog_to_jsonlog_to_binlog_roundtrip() {
    // Original index.
    let original = BinlogIndex::open(Cursor::new(HELLO_BINLOG)).expect("open original");

    // Dump to jsonlog.
    let mut jsonlog_bytes = Vec::new();
    jsonlog::dump_index(&original, &mut jsonlog_bytes).expect("dump jsonlog");

    // Reconstruct an index from the jsonlog (decoded path).
    let from_json = BinlogIndex::open_json(Cursor::new(&jsonlog_bytes)).expect("open_json");

    // Pack the index back into a fresh binlog stream.
    let mut binlog_bytes = Vec::new();
    from_json
        .write_binlog(&mut binlog_bytes)
        .expect("write_binlog");

    // Reopen the freshly written binlog through the standard reader.
    let roundtripped = BinlogIndex::open(Cursor::new(&binlog_bytes)).expect("reopen roundtripped");

    assert_eq!(original.len(), roundtripped.len(), "event count must match");
    assert_eq!(
        original.header().file_format_version,
        roundtripped.header().file_format_version,
        "format version must match"
    );

    for i in 0..original.len() {
        let orig_meta = original.meta(i).unwrap();
        let rt_meta = roundtripped.meta(i).unwrap();
        assert_eq!(
            orig_meta.record_kind, rt_meta.record_kind,
            "record kind at index {i}"
        );
        assert_eq!(
            orig_meta.context, rt_meta.context,
            "build event context at index {i}"
        );

        let orig_event = original.get(i).expect("orig get").expect("orig present");
        let rt_event = roundtripped.get(i).expect("rt get").expect("rt present");
        let orig_dbg = format!("{:?}", orig_event);
        let rt_dbg = format!("{:?}", rt_event);
        assert_eq!(orig_dbg, rt_dbg, "decoded event at index {i} differs");
    }
}
