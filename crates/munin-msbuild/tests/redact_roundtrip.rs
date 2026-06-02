// Copyright (c) Michael Grier

//! Integration test: open a real `.binlog`, run a `Redactor`, dump to
//! jsonlog, re-open via `open_json`, and verify the redaction landed
//! and the index still round-trips back through `write_binlog` + `open`.

use std::{fs::File, io::Cursor};

use munin_msbuild::{BinlogIndex, jsonlog::dump_index, redact::Redactor};

const HELLO_BINLOG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/hello.binlog");

fn open_hello() -> BinlogIndex {
    let f = File::open(HELLO_BINLOG).expect("missing test fixture: hello.binlog");
    BinlogIndex::open(f).expect("open binlog")
}

#[test]
fn redact_token_removes_string_and_round_trips() {
    // Baseline: confirm the unredacted index contains the literal
    // "hello" somewhere in its string table; otherwise the assertion
    // below is vacuous.
    let baseline = open_hello();
    let baseline_strings: Vec<String> = baseline.strings().entries().to_vec();
    assert!(
        baseline_strings.iter().any(|s| s.contains("HelloBinlog")),
        "fixture invariant: hello.binlog should contain the literal 'HelloBinlog' \
         somewhere in its string table",
    );

    // Apply redaction.
    let mut index = open_hello();
    Redactor::new()
        .with_token("HelloBinlog")
        .with_common_patterns()
        .with_autodetect_username()
        .apply(&mut index);

    // (a) The token must not appear in any string.
    for s in index.strings().entries() {
        assert!(
            !s.contains("HelloBinlog"),
            "redacted string still contains 'HelloBinlog': {s:?}",
        );
    }

    // (b) At least one string changed vs. the unredacted index.
    let after: Vec<String> = index.strings().entries().to_vec();
    assert_eq!(after.len(), baseline_strings.len(), "string table length");
    assert!(
        after
            .iter()
            .zip(baseline_strings.iter())
            .any(|(a, b)| a != b),
        "expected at least one string to differ after redaction",
    );

    // (c) The redacted index must round-trip through jsonlog.
    let mut json_bytes = Vec::new();
    dump_index(&index, &mut json_bytes).expect("dump_index");
    let reopened = BinlogIndex::open_json(Cursor::new(&json_bytes)).expect("open_json");
    assert_eq!(reopened.len(), index.len());
    for s in reopened.strings().entries() {
        assert!(!s.contains("HelloBinlog"));
    }

    // ...and back through write_binlog / open.
    let mut binlog_bytes = Vec::new();
    reopened
        .write_binlog(&mut binlog_bytes)
        .expect("write_binlog");
    let reopened_bin = BinlogIndex::open(Cursor::new(&binlog_bytes)).expect("open binlog");
    assert_eq!(reopened_bin.len(), index.len());
    for s in reopened_bin.strings().entries() {
        assert!(
            !s.contains("HelloBinlog"),
            "redacted string still contains 'HelloBinlog' after write_binlog round-trip: {s:?}",
        );
    }
}
