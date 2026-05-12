// Copyright (c) Michael Grier

use std::io::Cursor;

use super::*;

/// Helper: encode a single i32 as a 7-bit variable-length integer.
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

/// Helper: build a byte stream from a slice of i32 values, each 7-bit encoded.
fn encode_fields(fields: &[i32]) -> Vec<u8> {
    let mut buf = Vec::new();
    for &f in fields {
        buf.extend_from_slice(&encode_7bit(f));
    }
    buf
}

#[test]
fn read_context_version_2_plus() {
    // 7 fields: node=1, projCtx=2, target=3, task=4, submission=5, projInst=6, eval=7
    let data = encode_fields(&[1, 2, 3, 4, 5, 6, 7]);
    let mut cursor = Cursor::new(data);
    let ctx = read_build_event_context(&mut cursor, 18).unwrap();
    assert_eq!(ctx.node_id, 1);
    assert_eq!(ctx.project_context_id, 2);
    assert_eq!(ctx.target_id, 3);
    assert_eq!(ctx.task_id, 4);
    assert_eq!(ctx.submission_id, 5);
    assert_eq!(ctx.project_instance_id, 6);
    assert_eq!(ctx.evaluation_id, 7);
}

#[test]
fn read_context_version_1_no_eval_id() {
    // Only 6 fields when version <= 1
    let data = encode_fields(&[10, 20, 30, 40, 50, 60]);
    let mut cursor = Cursor::new(data);
    let ctx = read_build_event_context(&mut cursor, 1).unwrap();
    assert_eq!(ctx.node_id, 10);
    assert_eq!(ctx.project_context_id, 20);
    assert_eq!(ctx.target_id, 30);
    assert_eq!(ctx.task_id, 40);
    assert_eq!(ctx.submission_id, 50);
    assert_eq!(ctx.project_instance_id, 60);
    assert_eq!(ctx.evaluation_id, -1); // INVALID_EVALUATION_ID
}

#[test]
fn read_context_all_zeros() {
    let data = encode_fields(&[0, 0, 0, 0, 0, 0, 0]);
    let mut cursor = Cursor::new(data);
    let ctx = read_build_event_context(&mut cursor, 25).unwrap();
    assert_eq!(ctx.node_id, 0);
    assert_eq!(ctx.project_context_id, 0);
    assert_eq!(ctx.target_id, 0);
    assert_eq!(ctx.task_id, 0);
    assert_eq!(ctx.submission_id, 0);
    assert_eq!(ctx.project_instance_id, 0);
    assert_eq!(ctx.evaluation_id, 0);
}

#[test]
fn read_context_negative_values() {
    // -1 in 7-bit encoding: all 1-bits across 5 bytes
    let data = encode_fields(&[-1, -1, -1, -1, -1, -1, -1]);
    let mut cursor = Cursor::new(data);
    let ctx = read_build_event_context(&mut cursor, 18).unwrap();
    assert_eq!(ctx.node_id, -1);
    assert_eq!(ctx.project_context_id, -1);
    assert_eq!(ctx.target_id, -1);
    assert_eq!(ctx.task_id, -1);
    assert_eq!(ctx.submission_id, -1);
    assert_eq!(ctx.project_instance_id, -1);
    assert_eq!(ctx.evaluation_id, -1);
}

#[test]
fn read_context_large_values() {
    let data = encode_fields(&[
        1_000_000, 2_000_000, 3_000_000, 4_000_000, 5_000_000, 6_000_000, 7_000_000,
    ]);
    let mut cursor = Cursor::new(data);
    let ctx = read_build_event_context(&mut cursor, 18).unwrap();
    assert_eq!(ctx.node_id, 1_000_000);
    assert_eq!(ctx.project_context_id, 2_000_000);
    assert_eq!(ctx.target_id, 3_000_000);
    assert_eq!(ctx.task_id, 4_000_000);
    assert_eq!(ctx.submission_id, 5_000_000);
    assert_eq!(ctx.project_instance_id, 6_000_000);
    assert_eq!(ctx.evaluation_id, 7_000_000);
}

#[test]
fn read_context_truncated_stream_errors() {
    // Only enough bytes for 3 fields
    let data = encode_fields(&[1, 2, 3]);
    let mut cursor = Cursor::new(data);
    let result = read_build_event_context(&mut cursor, 18);
    assert!(result.is_err());
}

#[test]
fn debug_format() {
    let ctx = BuildEventContext {
        node_id: 1,
        project_context_id: 2,
        target_id: 3,
        task_id: 4,
        submission_id: 5,
        project_instance_id: 6,
        evaluation_id: 7,
    };
    let debug = format!("{ctx:?}");
    assert!(debug.contains("node_id: 1"));
    assert!(debug.contains("evaluation_id: 7"));
}

#[test]
fn equality() {
    let a = BuildEventContext {
        node_id: 1,
        project_context_id: 2,
        target_id: 3,
        task_id: 4,
        submission_id: 5,
        project_instance_id: 6,
        evaluation_id: 7,
    };
    let b = a;
    assert_eq!(a, b);
}

#[test]
fn inequality_on_any_field() {
    let base = BuildEventContext {
        node_id: 1,
        project_context_id: 2,
        target_id: 3,
        task_id: 4,
        submission_id: 5,
        project_instance_id: 6,
        evaluation_id: 7,
    };
    // Differ in evaluation_id
    let different = BuildEventContext {
        evaluation_id: 99,
        ..base
    };
    assert_ne!(base, different);
}

#[test]
fn version_boundary_at_2() {
    // version=2 should read evaluation_id
    let data = encode_fields(&[1, 2, 3, 4, 5, 6, 42]);
    let mut cursor = Cursor::new(data);
    let ctx = read_build_event_context(&mut cursor, 2).unwrap();
    assert_eq!(ctx.evaluation_id, 42);
}
