// Copyright (c) Michael Grier
//
// Open a hand-authored .jsonlog fixture via `BinlogIndex::open_json` and
// assert that the resulting index reports the expected length, per-event
// meta, and decoded payload.

use std::io::Cursor;

use munin_msbuild::{index::BinlogIndex, reader::BinlogEvent};

const FAKE_JSONLOG: &str = include_str!("data/fake.jsonlog");

#[test]
fn open_json_minimal_fixture() {
    let index =
        BinlogIndex::open_json(Cursor::new(FAKE_JSONLOG.as_bytes())).expect("open_json fixture");

    assert_eq!(index.header().file_format_version, 18);
    assert_eq!(index.len(), 1);

    let meta = index.meta(0).expect("meta(0)");
    assert_eq!(
        format!("{:?}", meta.record_kind),
        "BuildFinished",
        "record kind name"
    );

    let event = index.get(0).expect("get(0)").expect("decoded present");
    match event {
        BinlogEvent::BuildFinished(ev) => {
            assert!(ev.succeeded, "succeeded should be true");
            assert_eq!(ev.fields.thread_id, 0);
            assert!(ev.fields.message.is_none());
        }
        other => panic!("expected BuildFinished, got {other:?}"),
    }
}
