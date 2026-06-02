// Copyright (c) Michael Grier

//! Shared test helpers for `munin-binlog-mcp` integration tests.

use std::{fs::File, io::Cursor, path::PathBuf};

use munin_msbuild::{BinlogIndex, jsonlog::dump_index};

/// Load a named jsonlog test fixture and return its decoded
/// [`BinlogIndex`].
///
/// "Fixtures" are produced at test time by round-tripping a real
/// `.binlog` through the jsonlog encoder — so they exercise the
/// jsonlog read/write path without requiring multi-megabyte
/// `.jsonlog` files to be committed.
///
/// Currently recognized names:
/// - `"hello"` — round-trip of `hello.binlog`.
pub fn open_jsonlog_fixture(name: &str) -> BinlogIndex {
    let binlog_path = match name {
        "hello" => {
            let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            p.push("..");
            p.push("munin-msbuild");
            p.push("tests");
            p.push("data");
            p.push("hello.binlog");
            p
        }
        other => panic!("unknown jsonlog fixture: {other:?}"),
    };

    let f =
        File::open(&binlog_path).unwrap_or_else(|e| panic!("open {}: {e}", binlog_path.display()));
    let index = BinlogIndex::open(f).expect("open binlog");

    let mut bytes = Vec::new();
    dump_index(&index, &mut bytes).expect("dump_index");

    BinlogIndex::open_json(Cursor::new(&bytes)).expect("open_json round-trip")
}
