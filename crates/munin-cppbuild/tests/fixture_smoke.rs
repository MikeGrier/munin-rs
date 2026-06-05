// Copyright (c) Michael Grier

//! Smoke test: the synthetic fixture binlog round-trips through
//! `munin_msbuild::BinlogIndex::open`. CPP-2.5 builds the full M2
//! pipeline on top of the same fixture.

mod common;

use std::io::Cursor;

use munin_msbuild::{BinaryLogRecordKind, BinlogIndex};

use crate::common::synthetic_vcxproj_binlog;

#[test]
fn fixture_binlog_opens_with_expected_event_kinds() {
    let bytes = synthetic_vcxproj_binlog();
    let index = BinlogIndex::open(Cursor::new(&bytes)).expect("fixture binlog should open");

    let project_started_count = index
        .indices_by_kind(BinaryLogRecordKind::ProjectStarted)
        .len();
    let project_finished_count = index
        .indices_by_kind(BinaryLogRecordKind::ProjectFinished)
        .len();
    let build_started_count = index
        .indices_by_kind(BinaryLogRecordKind::BuildStarted)
        .len();
    let build_finished_count = index
        .indices_by_kind(BinaryLogRecordKind::BuildFinished)
        .len();

    assert_eq!(project_started_count, 2, "two project invocations");
    assert_eq!(project_finished_count, 2);
    assert_eq!(build_started_count, 1);
    assert_eq!(build_finished_count, 1);
}

#[test]
fn fixture_binlog_is_under_10kb() {
    let bytes = synthetic_vcxproj_binlog();
    assert!(
        bytes.len() < 10_000,
        "fixture binlog must stay under 10 KB: {} bytes",
        bytes.len()
    );
}
