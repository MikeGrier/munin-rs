// Copyright (c) Michael Grier

use std::io::Cursor;

use super::*;
use crate::{field_flags::BuildEventArgsFieldFlags, string_table::StringTable};

/// Encode a single i32 as a 7-bit variable-length integer.
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

/// Build a stream that starts with flags, then has the listed field payloads.
fn build_stream(flags: BuildEventArgsFieldFlags, payloads: &[Vec<u8>]) -> Vec<u8> {
    let mut data = encode_7bit(flags.bits() as i32);
    for p in payloads {
        data.extend_from_slice(p);
    }
    data
}

#[test]
fn no_flags_set_produces_defaults() {
    let strings = StringTable::new();
    let data = build_stream(BuildEventArgsFieldFlags::NONE, &[]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.flags, BuildEventArgsFieldFlags::NONE);
    assert!(fields.message.is_none());
    assert!(fields.build_event_context.is_none());
    assert_eq!(fields.thread_id, 0);
    assert!(fields.help_keyword.is_none());
    assert!(fields.sender_name.is_none());
    assert!(fields.timestamp.is_none());
    assert!(fields.extended.is_none());
    assert!(fields.subcategory.is_none());
    assert!(fields.code.is_none());
    assert!(fields.file.is_none());
    assert!(fields.project_file.is_none());
    assert_eq!(fields.line_number, 0);
    assert_eq!(fields.column_number, 0);
    assert_eq!(fields.end_line_number, 0);
    assert_eq!(fields.end_column_number, 0);
    assert!(fields.arguments.is_none());
    assert_eq!(fields.importance, 0);
}

#[test]
fn message_flag_reads_dedup_string() {
    let mut strings = StringTable::new();
    let idx = strings.add("hello world".to_string()); // index 10

    let data = build_stream(BuildEventArgsFieldFlags::MESSAGE, &[encode_7bit(idx)]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.message.as_deref(), Some("hello world"));
}

#[test]
fn null_message_index() {
    let strings = StringTable::new();
    let data = build_stream(
        BuildEventArgsFieldFlags::MESSAGE,
        &[encode_7bit(0)], // null
    );
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert!(fields.message.is_none());
}

#[test]
fn empty_message_index() {
    let strings = StringTable::new();
    let data = build_stream(
        BuildEventArgsFieldFlags::MESSAGE,
        &[encode_7bit(1)], // empty
    );
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.message.as_deref(), Some(""));
}

#[test]
fn build_event_context_flag() {
    let strings = StringTable::new();
    // 7 fields for context: 1,2,3,4,5,6,7
    let mut ctx_bytes = Vec::new();
    for i in 1..=7 {
        ctx_bytes.extend_from_slice(&encode_7bit(i));
    }
    let data = build_stream(BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT, &[ctx_bytes]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    let ctx = fields.build_event_context.unwrap();
    assert_eq!(ctx.node_id, 1);
    assert_eq!(ctx.evaluation_id, 7);
}

#[test]
fn thread_id_flag() {
    let strings = StringTable::new();
    let data = build_stream(BuildEventArgsFieldFlags::THREAD_ID, &[encode_7bit(42)]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.thread_id, 42);
}

#[test]
fn help_keyword_flag() {
    let mut strings = StringTable::new();
    let idx = strings.add("MSB1234".to_string());
    let data = build_stream(BuildEventArgsFieldFlags::HELP_KEYWORD, &[encode_7bit(idx)]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.help_keyword.as_deref(), Some("MSB1234"));
}

#[test]
fn sender_name_flag() {
    let mut strings = StringTable::new();
    let idx = strings.add("MSBuild".to_string());
    let data = build_stream(BuildEventArgsFieldFlags::SENDER_NAME, &[encode_7bit(idx)]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.sender_name.as_deref(), Some("MSBuild"));
}

#[test]
fn timestamp_flag() {
    let strings = StringTable::new();
    // DateTime: 8 bytes i64 ticks + 7-bit i32 kind
    let ticks: i64 = 637_500_000_000_000_000;
    let mut payload = Vec::new();
    payload.extend_from_slice(&ticks.to_le_bytes());
    payload.extend_from_slice(&encode_7bit(1)); // Utc
    let data = build_stream(BuildEventArgsFieldFlags::TIMESTAMP, &[payload]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    let ts = fields.timestamp.unwrap();
    assert_eq!(ts.ticks, ticks);
    assert_eq!(ts.kind, 1);
}

#[test]
fn line_and_column_flags() {
    let strings = StringTable::new();
    let flags = BuildEventArgsFieldFlags::LINE_NUMBER
        | BuildEventArgsFieldFlags::COLUMN_NUMBER
        | BuildEventArgsFieldFlags::END_LINE_NUMBER
        | BuildEventArgsFieldFlags::END_COLUMN_NUMBER;
    let data = build_stream(
        flags,
        &[
            encode_7bit(10),
            encode_7bit(20),
            encode_7bit(30),
            encode_7bit(40),
        ],
    );
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.line_number, 10);
    assert_eq!(fields.column_number, 20);
    assert_eq!(fields.end_line_number, 30);
    assert_eq!(fields.end_column_number, 40);
}

#[test]
fn arguments_flag() {
    let mut strings = StringTable::new();
    let idx_a = strings.add("arg1".to_string()); // 10
    let idx_b = strings.add("arg2".to_string()); // 11

    let mut payload = encode_7bit(2); // count=2
    payload.extend_from_slice(&encode_7bit(idx_a));
    payload.extend_from_slice(&encode_7bit(idx_b));

    let data = build_stream(BuildEventArgsFieldFlags::ARGUMENTS, &[payload]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    let args = fields.arguments.unwrap();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0].as_deref(), Some("arg1"));
    assert_eq!(args[1].as_deref(), Some("arg2"));
}

#[test]
fn importance_flag_v18() {
    let strings = StringTable::new();
    let data = build_stream(
        BuildEventArgsFieldFlags::IMPORTANCE,
        &[encode_7bit(2)], // MessageImportance.Normal = 2
    );
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.importance, 2);
}

#[test]
fn importance_unconditional_v12() {
    // Format version < 13: importance is always written when read_importance=true,
    // regardless of flags.
    let strings = StringTable::new();
    let data = build_stream(
        BuildEventArgsFieldFlags::NONE,
        &[encode_7bit(1)], // MessageImportance.High = 1
    );
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 12, true).unwrap();
    assert_eq!(fields.importance, 1);
}

#[test]
fn importance_not_read_when_not_requested_v12() {
    // Format version < 13, read_importance=false: importance should NOT be read.
    let strings = StringTable::new();
    let data = build_stream(BuildEventArgsFieldFlags::NONE, &[]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 12, false).unwrap();
    assert_eq!(fields.importance, 0);
}

#[test]
fn file_and_project_file_flags() {
    let mut strings = StringTable::new();
    let f_idx = strings.add("foo.cs".to_string()); // 10
    let p_idx = strings.add("bar.csproj".to_string()); // 11

    let data = build_stream(
        BuildEventArgsFieldFlags::FILE | BuildEventArgsFieldFlags::PROJECT_FILE,
        &[encode_7bit(f_idx), encode_7bit(p_idx)],
    );
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.file.as_deref(), Some("foo.cs"));
    assert_eq!(fields.project_file.as_deref(), Some("bar.csproj"));
}

#[test]
fn subcategory_and_code_flags() {
    let mut strings = StringTable::new();
    let sub_idx = strings.add("compiler".to_string()); // 10
    let code_idx = strings.add("CS0001".to_string()); // 11

    let data = build_stream(
        BuildEventArgsFieldFlags::SUBCATEGORY | BuildEventArgsFieldFlags::CODE,
        &[encode_7bit(sub_idx), encode_7bit(code_idx)],
    );
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.subcategory.as_deref(), Some("compiler"));
    assert_eq!(fields.code.as_deref(), Some("CS0001"));
}

#[test]
fn multiple_flags_combined() {
    let mut strings = StringTable::new();
    let msg_idx = strings.add("Build succeeded".to_string()); // 10
    let sender_idx = strings.add("MSBuild".to_string()); // 11

    let flags = BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::SENDER_NAME;

    let data = build_stream(flags, &[encode_7bit(msg_idx), encode_7bit(sender_idx)]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    assert_eq!(fields.message.as_deref(), Some("Build succeeded"));
    assert_eq!(fields.sender_name.as_deref(), Some("MSBuild"));
}

#[test]
fn extended_fields() {
    let mut strings = StringTable::new();
    let type_idx = strings.add("CustomType".to_string()); // 10
    let data_idx = strings.add("{\"key\":1}".to_string()); // 11

    // Extended: type_index, metadata_nvl_index (7-bit int), data_index
    let mut ext_payload = encode_7bit(type_idx);
    ext_payload.extend_from_slice(&encode_7bit(0)); // no metadata (null nvl index)
    ext_payload.extend_from_slice(&encode_7bit(data_idx));

    let data = build_stream(BuildEventArgsFieldFlags::EXTENDED, &[ext_payload]);
    let mut cursor = Cursor::new(data);
    let fields = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap();
    let ext = fields.extended.unwrap();
    assert_eq!(ext.extended_type.as_deref(), Some("CustomType"));
    assert_eq!(ext.extended_metadata_index, 0);
    assert_eq!(ext.extended_data.as_deref(), Some("{\"key\":1}"));
}

// ---------------------------------------------------------------------------
// #14: Regression — negative ARGUMENTS count
// ---------------------------------------------------------------------------

#[test]
fn rejects_negative_arguments_count() {
    // ARGUMENTS flag (0x4000) is set but the count field is -1.
    // read_build_event_args_fields should return InvalidFormat before
    // allocating the arguments vector.
    let strings = StringTable::new();
    let mut data = encode_7bit(BuildEventArgsFieldFlags::ARGUMENTS.bits() as i32);
    data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]); // count = -1
    let mut cursor = Cursor::new(data);
    let err = read_build_event_args_fields(&mut cursor, &strings, 18, false).unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}
