// Copyright (c) Michael Grier

//! JL-4.5: CLI round-trip integration test.
//!
//! Invokes the `munin-jsonlog` CLI on the real `hello.binlog`
//! fixture: `dump` → temp jsonlog file → `pack` → temp binlog file.
//! Then opens both binlogs through [`BinlogIndex::open`] and asserts
//! event-kind equivalence.

use std::{fs::File, io::BufReader, path::PathBuf};

use assert_cmd::Command;
use munin_msbuild::BinlogIndex;
use tempfile::tempdir;

fn hello_binlog_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("munin-msbuild");
    p.push("tests");
    p.push("data");
    p.push("hello.binlog");
    p
}

fn open_index(path: &PathBuf) -> BinlogIndex {
    let f = File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    BinlogIndex::open(BufReader::new(f))
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
}

#[test]
fn dump_then_pack_roundtrips_hello_binlog() {
    let tmp = tempdir().expect("tempdir");
    let jsonlog_path = tmp.path().join("hello.jsonlog");
    let repacked_path = tmp.path().join("hello.repacked.binlog");
    let original = hello_binlog_path();

    Command::cargo_bin("munin-jsonlog")
        .expect("locate munin-jsonlog binary")
        .arg("dump")
        .arg(&original)
        .arg("-o")
        .arg(&jsonlog_path)
        .assert()
        .success();

    Command::cargo_bin("munin-jsonlog")
        .expect("locate munin-jsonlog binary")
        .arg("pack")
        .arg(&jsonlog_path)
        .arg("-o")
        .arg(&repacked_path)
        .assert()
        .success();

    let src = open_index(&original);
    let rt = open_index(&repacked_path);

    assert_eq!(
        src.len(),
        rt.len(),
        "event count must match after dump+pack round-trip",
    );

    let src_kinds: Vec<_> = src.iter_meta().map(|(_, m)| m.record_kind).collect();
    let rt_kinds: Vec<_> = rt.iter_meta().map(|(_, m)| m.record_kind).collect();
    assert_eq!(
        src_kinds, rt_kinds,
        "event-kind sequence must match after dump+pack round-trip",
    );
}
