// Copyright (c) Michael Grier

//! Integration test: dump a real `.binlog` file as `.jsonlog` and verify
//! every event lands as either a `decoded` payload that matches the
//! index's decoded event or a `payload_b64` that matches the index's
//! stored payload bytes.

use std::fs::File;

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use munin_msbuild::{
    BinlogIndex,
    jsonlog::{JsonlogEventBody, JsonlogFile, MUNIN_JSONLOG_VERSION, dump_index},
};

const HELLO_BINLOG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/hello.binlog");

#[test]
fn dump_hello_binlog_to_jsonlog() {
    let file = File::open(HELLO_BINLOG).expect("missing test fixture: hello.binlog");
    let index = BinlogIndex::open(file).expect("open binlog");

    let mut buf = Vec::new();
    dump_index(&index, &mut buf).expect("dump_index");

    let jsonlog: JsonlogFile = serde_json::from_slice(&buf).expect("parse jsonlog");

    assert_eq!(jsonlog.munin_jsonlog_version, MUNIN_JSONLOG_VERSION);
    assert_eq!(
        jsonlog.header.file_format_version,
        index.header().file_format_version
    );
    assert_eq!(jsonlog.events.len(), index.len());
    assert_eq!(jsonlog.strings.len(), index.strings().entries().len());
    assert_eq!(
        jsonlog.name_value_lists.len(),
        index.nvl_table().entries().len()
    );
    assert_eq!(jsonlog.archives.len(), index.archives().len());

    let mut decoded_count = 0usize;
    let mut fallback_count = 0usize;
    for (i, ev) in jsonlog.events.iter().enumerate() {
        let meta = index.meta(i).expect("meta in range");
        assert_eq!(ev.byte_offset, meta.byte_offset);
        assert_eq!(ev.kind, format!("{:?}", meta.record_kind));
        match &ev.body {
            JsonlogEventBody::Decoded(_) => {
                // Re-decoding the index's payload must succeed.
                index.get(i).expect("decode").expect("event present");
                decoded_count += 1;
            }
            JsonlogEventBody::PayloadB64(s) => {
                let expected = index.payload_bytes(i).expect("payload bytes");
                let actual = B64.decode(s).expect("base64 decode");
                assert_eq!(actual, expected, "payload_b64 mismatch at event {i}");
                fallback_count += 1;
            }
        }
    }

    // hello.binlog is a successful build so we expect at least some events
    // to be decoded. Fallbacks are allowed but should not be the whole file.
    assert!(decoded_count > 0, "no events were decoded");
    assert_eq!(decoded_count + fallback_count, index.len());
}
