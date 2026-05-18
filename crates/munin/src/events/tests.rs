// Copyright (c) Michael Grier

use std::io::Cursor;

use super::*;
use crate::{
    field_flags::BuildEventArgsFieldFlags,
    nvl_table::{NameValueListTable, NameValuePair},
    string_table::StringTable,
};

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

/// Encode a bool (1 byte).
fn encode_bool(val: bool) -> Vec<u8> {
    vec![if val { 1 } else { 0 }]
}

/// Encode an i64 as 8 little-endian bytes.
fn encode_i64_le(val: i64) -> Vec<u8> {
    val.to_le_bytes().to_vec()
}

/// Encode a .NET-style length-prefixed string.
fn encode_dotnet_string(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut buf = encode_7bit(bytes.len() as i32);
    buf.extend_from_slice(bytes);
    buf
}

/// Build a flags-only preamble for BuildEventArgsFields (no optional fields).
fn encode_empty_fields() -> Vec<u8> {
    encode_7bit(BuildEventArgsFieldFlags::NONE.bits() as i32)
}

/// Build a message-only preamble (flags + message index).
fn encode_message_fields(msg_index: i32) -> Vec<u8> {
    let mut data = encode_7bit(BuildEventArgsFieldFlags::MESSAGE.bits() as i32);
    data.extend(encode_7bit(msg_index));
    data
}

/// Build fields with importance flag set and importance value.
fn encode_importance_fields(importance: i32, version: i32) -> Vec<u8> {
    if version >= 13 {
        let flags = BuildEventArgsFieldFlags::IMPORTANCE;
        let mut data = encode_7bit(flags.bits() as i32);
        data.extend(encode_7bit(importance));
        data
    } else {
        // Pre-13: importance is written unconditionally when read_importance=true,
        // no flag needed.
        let mut data = encode_7bit(BuildEventArgsFieldFlags::NONE.bits() as i32);
        data.extend(encode_7bit(importance));
        data
    }
}

/// Builds a string table and NVL table with some pre-populated entries.
/// Returns (StringTable, NvlTable) where:
///   string index 10 = "key1", 11 = "val1", 12 = "key2", 13 = "val2"
///   string index 14 = "project.csproj", 15 = "MyTarget", 16 = "MyTask"
///   string index 17 = "task.dll", 18 = "Build", 19 = "MyProperty"
///   string index 20 = "MyValue", 21 = "item.cs", 22 = "SubCat"
///   string index 23 = "CS0001", 24 = "file.cs", 25 = "proj.csproj"
///   NVL index 10 = [(key1_idx=10, val1_idx=11), (key2_idx=12, val2_idx=13)]
fn make_tables() -> (StringTable, NameValueListTable) {
    let mut strings = StringTable::new();
    strings.add("key1".to_string()); // 10
    strings.add("val1".to_string()); // 11
    strings.add("key2".to_string()); // 12
    strings.add("val2".to_string()); // 13
    strings.add("project.csproj".to_string()); // 14
    strings.add("MyTarget".to_string()); // 15
    strings.add("MyTask".to_string()); // 16
    strings.add("task.dll".to_string()); // 17
    strings.add("Build".to_string()); // 18
    strings.add("MyProperty".to_string()); // 19
    strings.add("MyValue".to_string()); // 20
    strings.add("item.cs".to_string()); // 21
    strings.add("SubCat".to_string()); // 22
    strings.add("CS0001".to_string()); // 23
    strings.add("file.cs".to_string()); // 24
    strings.add("proj.csproj".to_string()); // 25

    let mut nvl = NameValueListTable::new();
    nvl.add(vec![
        NameValuePair {
            key_index: 10,
            value_index: 11,
        },
        NameValuePair {
            key_index: 12,
            value_index: 13,
        },
    ]);

    (strings, nvl)
}

// ===========================================================================
// MN-11: Build/Project lifecycle events
// ===========================================================================

// --- BuildStarted ---

#[test]
fn build_started_minimal() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0)); // null environment dict (index 0)
    let mut cursor = Cursor::new(data);
    let event = read_build_started(&mut cursor, &strings, &nvl, 18).unwrap();
    assert!(event.environment.is_none());
}

#[test]
fn build_started_with_environment() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(10)); // NVL index 10
    let mut cursor = Cursor::new(data);
    let event = read_build_started(&mut cursor, &strings, &nvl, 18).unwrap();
    let env = event.environment.unwrap();
    assert_eq!(env.len(), 2);
    assert_eq!(env[0], ("key1".to_string(), "val1".to_string()));
    assert_eq!(env[1], ("key2".to_string(), "val2".to_string()));
}

#[test]
fn build_started_with_message() {
    let (strings, nvl) = make_tables();
    let mut data = encode_message_fields(14); // "project.csproj"
    data.extend(encode_7bit(0)); // null environment
    let mut cursor = Cursor::new(data);
    let event = read_build_started(&mut cursor, &strings, &nvl, 18).unwrap();
    assert_eq!(event.fields.message.as_deref(), Some("project.csproj"));
    assert!(event.environment.is_none());
}

// --- BuildFinished ---

#[test]
fn build_finished_succeeded() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_bool(true));
    let mut cursor = Cursor::new(data);
    let event = read_build_finished(&mut cursor, &strings, 18).unwrap();
    assert!(event.succeeded);
}

#[test]
fn build_finished_failed() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_bool(false));
    let mut cursor = Cursor::new(data);
    let event = read_build_finished(&mut cursor, &strings, 18).unwrap();
    assert!(!event.succeeded);
}

#[test]
fn build_finished_with_message() {
    let (strings, _) = make_tables();
    let mut data = encode_message_fields(18); // "Build"
    data.extend(encode_bool(true));
    let mut cursor = Cursor::new(data);
    let event = read_build_finished(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_deref(), Some("Build"));
    assert!(event.succeeded);
}

// --- ProjectStarted ---

#[test]
fn project_started_minimal_v18() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_bool(false)); // no parent context
    data.extend(encode_7bit(14)); // project_file dedup index
    data.extend(encode_7bit(42)); // project_id
    data.extend(encode_7bit(15)); // target_names dedup
    data.extend(encode_7bit(0)); // tools_version null
                                 // v18: global_properties always written
    data.extend(encode_7bit(0)); // null nvl
                                 // property_list
    data.extend(encode_7bit(0)); // null nvl
                                 // item_list (v12+ terminator)
    data.extend(encode_7bit(0)); // empty item type = end
    let mut cursor = Cursor::new(data);
    let event = read_project_started(&mut cursor, &strings, &nvl, 18).unwrap();
    assert_eq!(event.project_file.as_deref(), Some("project.csproj"));
    assert_eq!(event.project_id, 42);
    assert_eq!(event.target_names.as_dedup(), Some("MyTarget"));
    assert!(event.parent_context.is_none());
    assert!(event.global_properties.is_none());
    assert!(event.property_list.is_none());
    assert!(event.item_list.is_none());
}

#[test]
fn project_started_with_parent_context() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_bool(true)); // has parent context
                                    // BuildEventContext (6 fields for v18 — matches context.rs)
    data.extend(encode_7bit(1)); // node_id
    data.extend(encode_7bit(2)); // project_context_id
    data.extend(encode_7bit(3)); // target_id
    data.extend(encode_7bit(4)); // task_id
    data.extend(encode_7bit(5)); // submission_id
    data.extend(encode_7bit(6)); // project_instance_id
    data.extend(encode_7bit(7)); // evaluation_id
    data.extend(encode_7bit(14)); // project_file
    data.extend(encode_7bit(1)); // project_id
    data.extend(encode_7bit(0)); // target_names null
    data.extend(encode_7bit(0)); // tools_version null
    data.extend(encode_7bit(0)); // global_properties null
    data.extend(encode_7bit(0)); // property_list null
    data.extend(encode_7bit(0)); // item_list terminator
    let mut cursor = Cursor::new(data);
    let event = read_project_started(&mut cursor, &strings, &nvl, 18).unwrap();
    let ctx = event.parent_context.unwrap();
    assert_eq!(ctx.node_id, 1);
    assert_eq!(ctx.project_context_id, 2);
    assert_eq!(ctx.target_id, 3);
    assert_eq!(ctx.task_id, 4);
    assert_eq!(ctx.evaluation_id, 7);
}

#[test]
fn project_started_with_global_properties() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_bool(false)); // no parent context
    data.extend(encode_7bit(14)); // project_file
    data.extend(encode_7bit(1)); // project_id
    data.extend(encode_7bit(0)); // target_names
    data.extend(encode_7bit(0)); // tools_version
    data.extend(encode_7bit(10)); // global_properties = NVL entry 10
    data.extend(encode_7bit(0)); // property_list null
    data.extend(encode_7bit(0)); // item_list terminator
    let mut cursor = Cursor::new(data);
    let event = read_project_started(&mut cursor, &strings, &nvl, 18).unwrap();
    let gp = event.global_properties.unwrap();
    assert_eq!(gp.len(), 2);
    assert_eq!(gp[0].0, "key1");
    assert_eq!(gp[0].1, "val1");
}

// --- ProjectFinished ---

#[test]
fn project_finished_succeeded() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(14)); // project_file
    data.extend(encode_bool(true));
    let mut cursor = Cursor::new(data);
    let event = read_project_finished(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.project_file.as_deref(), Some("project.csproj"));
    assert!(event.succeeded);
}

#[test]
fn project_finished_failed() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0)); // null project file
    data.extend(encode_bool(false));
    let mut cursor = Cursor::new(data);
    let event = read_project_finished(&mut cursor, &strings, 18).unwrap();
    assert!(event.project_file.is_none());
    assert!(!event.succeeded);
}

// ===========================================================================
// MN-12: Target lifecycle events
// ===========================================================================

// --- TargetStarted ---

#[test]
fn target_started_minimal() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(15)); // target_name "MyTarget"
    data.extend(encode_7bit(14)); // project_file "project.csproj"
    data.extend(encode_7bit(17)); // target_file "task.dll"
    data.extend(encode_7bit(0)); // parent_target null
    data.extend(encode_7bit(3)); // build_reason
    let mut cursor = Cursor::new(data);
    let event = read_target_started(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.target_name.as_deref(), Some("MyTarget"));
    assert_eq!(event.project_file.as_deref(), Some("project.csproj"));
    assert_eq!(event.target_file.as_deref(), Some("task.dll"));
    assert!(event.parent_target.is_none());
    assert_eq!(event.build_reason, 3);
}

#[test]
fn target_started_v3_no_build_reason() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    // v3 < 10: read_optional_string reads bool + raw .NET string
    data.extend(encode_bool(true));
    data.extend(encode_dotnet_string("Target1")); // target_name
    data.extend(encode_bool(false)); // project_file absent
    data.extend(encode_bool(false)); // target_file absent
    data.extend(encode_bool(false)); // parent_target absent
                                     // No build_reason for version <= 3
    let mut cursor = Cursor::new(data);
    let event = read_target_started(&mut cursor, &strings, 3).unwrap();
    assert_eq!(event.target_name.as_deref(), Some("Target1"));
    assert_eq!(event.build_reason, 0);
}

#[test]
fn target_started_with_parent_target() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(15)); // target_name
    data.extend(encode_7bit(14)); // project_file
    data.extend(encode_7bit(0)); // target_file
    data.extend(encode_7bit(18)); // parent_target "Build"
    data.extend(encode_7bit(1)); // build_reason
    let mut cursor = Cursor::new(data);
    let event = read_target_started(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.parent_target.as_deref(), Some("Build"));
}

// --- TargetFinished ---

#[test]
fn target_finished_succeeded_no_outputs() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_bool(true));
    data.extend(encode_7bit(14)); // project_file
    data.extend(encode_7bit(17)); // target_file
    data.extend(encode_7bit(15)); // target_name
    data.extend(encode_7bit(0)); // task_item_list count=0
    let mut cursor = Cursor::new(data);
    let event = read_target_finished(&mut cursor, &strings, &nvl, 18).unwrap();
    assert!(event.succeeded);
    assert_eq!(event.target_name.as_deref(), Some("MyTarget"));
    assert!(event.target_outputs.is_none());
}

#[test]
fn target_finished_with_outputs() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_bool(true));
    data.extend(encode_7bit(14)); // project_file
    data.extend(encode_7bit(0)); // target_file null
    data.extend(encode_7bit(15)); // target_name
                                  // task_item_list: 1 item
    data.extend(encode_7bit(1)); // count
    data.extend(encode_7bit(21)); // item_spec "item.cs"
    data.extend(encode_7bit(0)); // null metadata dict
    let mut cursor = Cursor::new(data);
    let event = read_target_finished(&mut cursor, &strings, &nvl, 18).unwrap();
    let outputs = event.target_outputs.unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].item_spec.as_deref(), Some("item.cs"));
}

#[test]
fn target_finished_failed() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_bool(false)); // succeeded
    data.extend(encode_7bit(0)); // project_file
    data.extend(encode_7bit(0)); // target_file
    data.extend(encode_7bit(0)); // target_name
    data.extend(encode_7bit(0)); // empty task_item_list
    let mut cursor = Cursor::new(data);
    let event = read_target_finished(&mut cursor, &strings, &nvl, 18).unwrap();
    assert!(!event.succeeded);
}

// --- TargetSkipped ---

#[test]
fn target_skipped_v18_with_condition() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(2, 18); // read_importance=true
    data.extend(encode_7bit(17)); // target_file
    data.extend(encode_7bit(15)); // target_name
    data.extend(encode_7bit(0)); // parent_target null
                                 // v13+ fields
    data.extend(encode_7bit(19)); // condition "MyProperty"
    data.extend(encode_7bit(20)); // evaluated_condition "MyValue"
    data.extend(encode_bool(false)); // originally_succeeded
    data.extend(encode_7bit(5)); // build_reason
                                 // v14+ fields
    data.extend(encode_7bit(1)); // skip_reason
    data.extend(encode_bool(false)); // no original context
    let mut cursor = Cursor::new(data);
    let event = read_target_skipped(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.target_file.as_deref(), Some("task.dll"));
    assert_eq!(event.target_name.as_dedup(), Some("MyTarget"));
    assert_eq!(event.condition.as_deref(), Some("MyProperty"));
    assert_eq!(event.evaluated_condition.as_deref(), Some("MyValue"));
    assert_eq!(event.skip_reason, 1);
    assert_eq!(event.build_reason, 5);
    assert!(event.original_build_event_context.is_none());
}

#[test]
fn target_skipped_v14_with_original_context() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(0)); // target_file
    data.extend(encode_7bit(15)); // target_name
    data.extend(encode_7bit(0)); // parent_target
                                 // v13+ fields
    data.extend(encode_7bit(0)); // condition null
    data.extend(encode_7bit(0)); // evaluated_condition null
    data.extend(encode_bool(true)); // originally_succeeded
    data.extend(encode_7bit(0)); // build_reason
                                 // v14+ fields
    data.extend(encode_7bit(2)); // skip_reason
    data.extend(encode_bool(true)); // has original context
                                    // BuildEventContext
    data.extend(encode_7bit(10)); // node_id
    data.extend(encode_7bit(20)); // project_context_id
    data.extend(encode_7bit(30)); // target_id
    data.extend(encode_7bit(40)); // task_id
    data.extend(encode_7bit(50)); // submission_id
    data.extend(encode_7bit(60)); // project_instance_id
    data.extend(encode_7bit(70)); // evaluation_id
    let mut cursor = Cursor::new(data);
    let event = read_target_skipped(&mut cursor, &strings, 18).unwrap();
    let ctx = event.original_build_event_context.unwrap();
    assert_eq!(ctx.node_id, 10);
    assert_eq!(ctx.project_context_id, 20);
    assert_eq!(event.skip_reason, 2);
}

#[test]
fn target_skipped_v12_no_skip_reason_field() {
    // v12 < 13, so condition/evaluatedCondition/originallySucceeded not written
    // v12 < 14, so skip_reason not overwritten
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 12);
    data.extend(encode_7bit(15)); // target_file
    data.extend(encode_7bit(0)); // target_name
    data.extend(encode_7bit(0)); // parent_target
                                 // No v13+ fields
    data.extend(encode_7bit(0)); // build_reason
                                 // No v14+ fields
    let mut cursor = Cursor::new(data);
    let event = read_target_skipped(&mut cursor, &strings, 12).unwrap();
    assert_eq!(event.skip_reason, 0);
    assert_eq!(event.build_reason, 0);
    assert!(event.condition.is_none());
}

// ===========================================================================
// MN-13: Task lifecycle events
// ===========================================================================

// --- TaskStarted ---

#[test]
fn task_started_minimal() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(16)); // task_name "MyTask"
    data.extend(encode_7bit(14)); // project_file
    data.extend(encode_7bit(17)); // task_file "task.dll"
                                  // v20+: task_assembly_location
    data.extend(encode_7bit(17)); // task_assembly_location
    let mut cursor = Cursor::new(data);
    let event = read_task_started(&mut cursor, &strings, 20).unwrap();
    assert_eq!(event.task_name.as_dedup(), Some("MyTask"));
    assert_eq!(event.project_file.as_deref(), Some("project.csproj"));
    assert_eq!(event.task_file.as_dedup(), Some("task.dll"));
    assert_eq!(event.task_assembly_location.as_dedup(), Some("task.dll"));
}

#[test]
fn task_started_v19_no_assembly_location() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(16)); // task_name
    data.extend(encode_7bit(0)); // project_file null
    data.extend(encode_7bit(0)); // task_file null
    let mut cursor = Cursor::new(data);
    let event = read_task_started(&mut cursor, &strings, 19).unwrap();
    assert!(event.task_assembly_location.is_none());
}

#[test]
fn task_started_all_null_fields() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0)); // task_name null
    data.extend(encode_7bit(0)); // project_file null
    data.extend(encode_7bit(0)); // task_file null
    let mut cursor = Cursor::new(data);
    let event = read_task_started(&mut cursor, &strings, 18).unwrap();
    assert!(event.task_name.is_none());
    assert!(event.project_file.is_none());
    assert!(event.task_file.is_none());
}

// --- TaskFinished ---

#[test]
fn task_finished_succeeded() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_bool(true));
    data.extend(encode_7bit(16)); // task_name
    data.extend(encode_7bit(14)); // project_file
    data.extend(encode_7bit(17)); // task_file
    let mut cursor = Cursor::new(data);
    let event = read_task_finished(&mut cursor, &strings, 18).unwrap();
    assert!(event.succeeded);
    assert_eq!(event.task_name.as_dedup(), Some("MyTask"));
}

#[test]
fn task_finished_failed() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_bool(false));
    data.extend(encode_7bit(0)); // task_name null
    data.extend(encode_7bit(0)); // project_file
    data.extend(encode_7bit(0)); // task_file
    let mut cursor = Cursor::new(data);
    let event = read_task_finished(&mut cursor, &strings, 18).unwrap();
    assert!(!event.succeeded);
}

// --- TaskCommandLine ---

#[test]
fn task_command_line_minimal() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(18)); // command_line "Build"
    data.extend(encode_7bit(16)); // task_name "MyTask"
    let mut cursor = Cursor::new(data);
    let event = read_task_command_line(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.command_line.as_dedup(), Some("Build"));
    assert_eq!(event.task_name.as_dedup(), Some("MyTask"));
}

#[test]
fn task_command_line_null_command() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(0)); // command_line null
    data.extend(encode_7bit(0)); // task_name null
    let mut cursor = Cursor::new(data);
    let event = read_task_command_line(&mut cursor, &strings, 18).unwrap();
    assert!(event.command_line.is_none());
    assert!(event.task_name.is_none());
}

// --- TaskParameter ---

#[test]
fn task_parameter_minimal_v21() {
    let (strings, nvl) = make_tables();
    let mut data = encode_importance_fields(0, 21);
    data.extend(encode_7bit(1)); // kind
    data.extend(encode_7bit(15)); // item_type "MyTarget"
    data.extend(encode_7bit(0)); // items count=0
    data.extend(encode_7bit(19)); // parameter_name "MyProperty"
    data.extend(encode_7bit(0)); // property_name null
    let mut cursor = Cursor::new(data);
    let event = read_task_parameter(&mut cursor, &strings, &nvl, 21).unwrap();
    assert_eq!(event.kind, 1);
    assert_eq!(event.item_type.as_dedup(), Some("MyTarget"));
    assert!(event.items.is_none());
    assert_eq!(event.parameter_name.as_dedup(), Some("MyProperty"));
    assert!(event.property_name.is_none());
}

#[test]
fn task_parameter_with_items() {
    let (strings, nvl) = make_tables();
    let mut data = encode_importance_fields(0, 21);
    data.extend(encode_7bit(2)); // kind
    data.extend(encode_7bit(0)); // item_type null
                                 // items: 1 item
    data.extend(encode_7bit(1)); // count
    data.extend(encode_7bit(21)); // item_spec "item.cs"
    data.extend(encode_7bit(0)); // metadata null
    data.extend(encode_7bit(0)); // parameter_name null
    data.extend(encode_7bit(0)); // property_name null
    let mut cursor = Cursor::new(data);
    let event = read_task_parameter(&mut cursor, &strings, &nvl, 21).unwrap();
    let items = event.items.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_spec.as_dedup(), Some("item.cs"));
}

#[test]
fn task_parameter_v20_no_names() {
    let (strings, nvl) = make_tables();
    let mut data = encode_importance_fields(0, 20);
    data.extend(encode_7bit(0)); // kind
    data.extend(encode_7bit(0)); // item_type null
    data.extend(encode_7bit(0)); // items count=0
                                 // v20: no parameter_name/property_name
    let mut cursor = Cursor::new(data);
    let event = read_task_parameter(&mut cursor, &strings, &nvl, 20).unwrap();
    assert!(event.parameter_name.is_none());
    assert!(event.property_name.is_none());
}

// ===========================================================================
// MN-14: Diagnostic events (Error, Warning, Message, CriticalBuildMessage)
// ===========================================================================

/// Build the 8 diagnostic location fields for v18 (dedup string indices).
#[allow(clippy::too_many_arguments)]
fn encode_diagnostic_location_v18(
    subcategory: i32,
    code: i32,
    file: i32,
    project_file: i32,
    line: i32,
    col: i32,
    end_line: i32,
    end_col: i32,
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend(encode_7bit(subcategory));
    data.extend(encode_7bit(code));
    data.extend(encode_7bit(file));
    data.extend(encode_7bit(project_file));
    data.extend(encode_7bit(line));
    data.extend(encode_7bit(col));
    data.extend(encode_7bit(end_line));
    data.extend(encode_7bit(end_col));
    data
}

// --- BuildError ---

#[test]
fn build_error_with_location() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_diagnostic_location_v18(
        22, 23, 24, 25, 10, 5, 10, 50,
    ));
    let mut cursor = Cursor::new(data);
    let event = read_build_error(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.location.subcategory.as_deref(), Some("SubCat"));
    assert_eq!(event.location.code.as_deref(), Some("CS0001"));
    assert_eq!(event.location.file.as_deref(), Some("file.cs"));
    assert_eq!(event.location.project_file.as_deref(), Some("proj.csproj"));
    assert_eq!(event.location.line_number, 10);
    assert_eq!(event.location.column_number, 5);
    assert_eq!(event.location.end_line_number, 10);
    assert_eq!(event.location.end_column_number, 50);
}

#[test]
fn build_error_all_nulls() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_diagnostic_location_v18(0, 0, 0, 0, 0, 0, 0, 0));
    let mut cursor = Cursor::new(data);
    let event = read_build_error(&mut cursor, &strings, 18).unwrap();
    assert!(event.location.subcategory.is_none());
    assert!(event.location.code.is_none());
    assert!(event.location.file.is_none());
    assert!(event.location.project_file.is_none());
    assert_eq!(event.location.line_number, 0);
    assert_eq!(event.location.column_number, 0);
}

#[test]
fn build_error_with_message_and_location() {
    let (strings, _) = make_tables();
    let mut data = encode_message_fields(18); // "Build"
    data.extend(encode_diagnostic_location_v18(0, 23, 24, 0, 1, 1, 1, 1));
    let mut cursor = Cursor::new(data);
    let event = read_build_error(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_deref(), Some("Build"));
    assert_eq!(event.location.code.as_deref(), Some("CS0001"));
}

// --- BuildWarning ---

#[test]
fn build_warning_with_location() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_diagnostic_location_v18(0, 23, 24, 25, 5, 1, 5, 80));
    let mut cursor = Cursor::new(data);
    let event = read_build_warning(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.location.code.as_dedup(), Some("CS0001"));
    assert_eq!(event.location.line_number, 5);
    assert_eq!(event.location.end_column_number, 80);
}

#[test]
fn build_warning_all_nulls() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_diagnostic_location_v18(0, 0, 0, 0, 0, 0, 0, 0));
    let mut cursor = Cursor::new(data);
    let event = read_build_warning(&mut cursor, &strings, 18).unwrap();
    assert!(event.location.code.is_none());
}

// --- BuildMessage ---

#[test]
fn build_message_reads_importance() {
    let (strings, _) = make_tables();
    let data = encode_importance_fields(2, 18);
    let mut cursor = Cursor::new(data);
    let event = read_build_message(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.importance, 2);
}

#[test]
fn build_message_minimal() {
    let (strings, _) = make_tables();
    let data = encode_importance_fields(0, 18);
    let mut cursor = Cursor::new(data);
    let event = read_build_message(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.importance, 0);
}

// --- CriticalBuildMessage ---

#[test]
fn critical_build_message_reads_importance() {
    let (strings, _) = make_tables();
    let data = encode_importance_fields(1, 18);
    let mut cursor = Cursor::new(data);
    let event = read_critical_build_message(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.importance, 1);
}

// ===========================================================================
// MN-15: Evaluation events
// ===========================================================================

// --- ProjectEvaluationStarted ---

#[test]
fn project_evaluation_started_minimal() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(14)); // project_file
    let mut cursor = Cursor::new(data);
    let event = read_project_evaluation_started(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.project_file.as_dedup(), Some("project.csproj"));
}

#[test]
fn project_evaluation_started_null_project() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0));
    let mut cursor = Cursor::new(data);
    let event = read_project_evaluation_started(&mut cursor, &strings, 18).unwrap();
    assert!(event.project_file.is_none());
}

// --- ProjectEvaluationFinished ---

#[test]
fn project_evaluation_finished_v18_no_profile() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(14)); // project_file
                                  // v12+: global_properties always in v18
    data.extend(encode_7bit(0)); // null
                                 // property_list
    data.extend(encode_7bit(0)); // null
                                 // item_list terminator
    data.extend(encode_7bit(0)); // empty type = end
                                 // v5+: has_profile_data
    data.extend(encode_bool(false));
    let mut cursor = Cursor::new(data);
    let event = read_project_evaluation_finished(&mut cursor, &strings, &nvl, 18).unwrap();
    assert_eq!(event.project_file.as_dedup(), Some("project.csproj"));
    assert!(event.global_properties.is_none());
    assert!(event.property_list.is_none());
    assert!(event.item_list.is_none());
    assert!(!event.has_profile_data);
    assert!(event.profile_data.is_none());
}

#[test]
fn project_evaluation_finished_v18_with_properties() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(14)); // project_file
                                  // global_properties = NVL 10
    data.extend(encode_7bit(10));
    // property_list = NVL 10
    data.extend(encode_7bit(10));
    // item_list terminator
    data.extend(encode_7bit(0));
    // no profile data
    data.extend(encode_bool(false));
    let mut cursor = Cursor::new(data);
    let event = read_project_evaluation_finished(&mut cursor, &strings, &nvl, 18).unwrap();
    let gp = event.global_properties.unwrap();
    assert_eq!(gp.len(), 2);
    let pl = event.property_list.unwrap();
    assert_eq!(pl.len(), 2);
}

#[test]
fn project_evaluation_finished_with_profile_data() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(14)); // project_file
    data.extend(encode_7bit(0)); // global_properties null
    data.extend(encode_7bit(0)); // property_list null
    data.extend(encode_7bit(0)); // item_list terminator
    data.extend(encode_bool(true)); // has_profile_data

    // 1 profile entry
    data.extend(encode_7bit(1)); // count

    // EvaluationLocation
    data.extend(encode_7bit(15)); // element_name "MyTarget"
    data.extend(encode_7bit(0)); // description null
    data.extend(encode_7bit(0)); // evaluation_description null
    data.extend(encode_7bit(14)); // file "project.csproj"
    data.extend(encode_7bit(2)); // kind
    data.extend(encode_7bit(3)); // evaluation_pass
    data.extend(encode_bool(true)); // has line
    data.extend(encode_7bit(42)); // line
                                  // v6+: id and parent_id
    data.extend(encode_i64_le(100)); // id
    data.extend(encode_bool(true)); // has parent_id
    data.extend(encode_i64_le(50)); // parent_id

    // ProfiledLocation
    data.extend(encode_7bit(10)); // number_of_hits
    data.extend(encode_i64_le(1000)); // exclusive_time_ticks
    data.extend(encode_i64_le(2000)); // inclusive_time_ticks

    let mut cursor = Cursor::new(data);
    let event = read_project_evaluation_finished(&mut cursor, &strings, &nvl, 18).unwrap();
    assert!(event.has_profile_data);
    let pd = event.profile_data.unwrap();
    assert_eq!(pd.len(), 1);
    assert_eq!(pd[0].location.element_name.as_dedup(), Some("MyTarget"));
    assert_eq!(pd[0].location.file.as_dedup(), Some("project.csproj"));
    assert_eq!(pd[0].location.kind, 2);
    assert_eq!(pd[0].location.evaluation_pass, 3);
    assert_eq!(pd[0].location.line, Some(42));
    assert_eq!(pd[0].location.id, 100);
    assert_eq!(pd[0].location.parent_id, Some(50));
    assert_eq!(pd[0].profiled_location.number_of_hits, 10);
    assert_eq!(pd[0].profiled_location.exclusive_time_ticks, 1000);
    assert_eq!(pd[0].profiled_location.inclusive_time_ticks, 2000);
}

#[test]
fn project_evaluation_finished_v4_no_profile() {
    // v4 < 5, so no profile data; v4 < 12, so no properties/items
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0)); // project_file null
                                 // No v12+ properties, no v5+ profile
    let mut cursor = Cursor::new(data);
    let event = read_project_evaluation_finished(&mut cursor, &strings, &nvl, 4).unwrap();
    assert!(event.global_properties.is_none());
    assert!(!event.has_profile_data);
}

#[test]
fn project_evaluation_finished_v5_with_empty_profile() {
    // v5 has profile data but no v12+ properties
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0)); // project_file null
    data.extend(encode_bool(false)); // has_profile_data = false
    let mut cursor = Cursor::new(data);
    let event = read_project_evaluation_finished(&mut cursor, &strings, &nvl, 5).unwrap();
    assert!(!event.has_profile_data);
}

// ===========================================================================
// Shared helpers: DiagnosticLocation
// ===========================================================================

#[test]
fn diagnostic_location_default() {
    let loc = DiagnosticLocation::default();
    assert!(loc.subcategory.is_none());
    assert!(loc.code.is_none());
    assert!(loc.file.is_none());
    assert!(loc.project_file.is_none());
    assert_eq!(loc.line_number, 0);
    assert_eq!(loc.column_number, 0);
    assert_eq!(loc.end_line_number, 0);
    assert_eq!(loc.end_column_number, 0);
}

// ===========================================================================
// Edge cases and additional coverage
// ===========================================================================

#[test]
fn build_started_legacy_v9() {
    // v9 < 10: uses legacy inline dict
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    // Legacy inline dict: count=1, key="a", value="b"
    data.extend(encode_7bit(1));
    data.extend(encode_dotnet_string("a"));
    data.extend(encode_dotnet_string("b"));
    let mut cursor = Cursor::new(data);
    let event = read_build_started(&mut cursor, &strings, &nvl, 9).unwrap();
    let env = event.environment.unwrap();
    assert_eq!(env.len(), 1);
    assert_eq!(env[0], ("a".to_string(), "b".to_string()));
}

#[test]
fn project_finished_with_message_and_project_file() {
    let (strings, _) = make_tables();
    let mut data = encode_message_fields(18); // "Build"
    data.extend(encode_7bit(14)); // project_file "project.csproj"
    data.extend(encode_bool(true));
    let mut cursor = Cursor::new(data);
    let event = read_project_finished(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("Build"));
    assert_eq!(event.project_file.as_dedup(), Some("project.csproj"));
    assert!(event.succeeded);
}

/// Helper trait for cleaner test assertions on `Option<String>`.
trait AsDedup {
    fn as_dedup(&self) -> Option<&str>;
}
impl AsDedup for Option<String> {
    fn as_dedup(&self) -> Option<&str> {
        self.as_deref()
    }
}

// ===========================================================================
// MN-16: Property event types
// ===========================================================================

// --- PropertyReassignment ---

#[test]
fn property_reassignment_all_fields() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(19)); // property_name "MyProperty"
    data.extend(encode_7bit(20)); // previous_value "MyValue"
    data.extend(encode_7bit(18)); // new_value "Build"
    data.extend(encode_7bit(24)); // location "file.cs"
    let mut cursor = Cursor::new(data);
    let event = read_property_reassignment(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.property_name.as_dedup(), Some("MyProperty"));
    assert_eq!(event.previous_value.as_dedup(), Some("MyValue"));
    assert_eq!(event.new_value.as_dedup(), Some("Build"));
    assert_eq!(event.location.as_dedup(), Some("file.cs"));
}

#[test]
fn property_reassignment_all_null() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(0)); // property_name null
    data.extend(encode_7bit(0)); // previous_value null
    data.extend(encode_7bit(0)); // new_value null
    data.extend(encode_7bit(0)); // location null
    let mut cursor = Cursor::new(data);
    let event = read_property_reassignment(&mut cursor, &strings, 18).unwrap();
    assert!(event.property_name.is_none());
    assert!(event.previous_value.is_none());
    assert!(event.new_value.is_none());
    assert!(event.location.is_none());
}

#[test]
fn property_reassignment_with_message() {
    let (strings, _) = make_tables();
    let flags = BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE;
    let mut data = encode_7bit(flags.bits() as i32);
    data.extend(encode_7bit(14)); // message "project.csproj"
    data.extend(encode_7bit(2)); // importance
    data.extend(encode_7bit(19)); // property_name
    data.extend(encode_7bit(20)); // previous_value
    data.extend(encode_7bit(18)); // new_value
    data.extend(encode_7bit(0)); // location null
    let mut cursor = Cursor::new(data);
    let event = read_property_reassignment(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("project.csproj"));
    assert_eq!(event.property_name.as_dedup(), Some("MyProperty"));
}

#[test]
fn property_reassignment_partial_nulls() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(19)); // property_name "MyProperty"
    data.extend(encode_7bit(0)); // previous_value null
    data.extend(encode_7bit(20)); // new_value "MyValue"
    data.extend(encode_7bit(0)); // location null
    let mut cursor = Cursor::new(data);
    let event = read_property_reassignment(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.property_name.as_dedup(), Some("MyProperty"));
    assert!(event.previous_value.is_none());
    assert_eq!(event.new_value.as_dedup(), Some("MyValue"));
    assert!(event.location.is_none());
}

// --- UninitializedPropertyRead ---

#[test]
fn uninitialized_property_read_with_name() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(19)); // property_name "MyProperty"
    let mut cursor = Cursor::new(data);
    let event = read_uninitialized_property_read(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.property_name.as_dedup(), Some("MyProperty"));
}

#[test]
fn uninitialized_property_read_null_name() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(0)); // property_name null
    let mut cursor = Cursor::new(data);
    let event = read_uninitialized_property_read(&mut cursor, &strings, 18).unwrap();
    assert!(event.property_name.is_none());
}

#[test]
fn uninitialized_property_read_with_message() {
    let (strings, _) = make_tables();
    let flags = BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE;
    let mut data = encode_7bit(flags.bits() as i32);
    data.extend(encode_7bit(18)); // message "Build"
    data.extend(encode_7bit(1)); // importance
    data.extend(encode_7bit(19)); // property_name "MyProperty"
    let mut cursor = Cursor::new(data);
    let event = read_uninitialized_property_read(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("Build"));
    assert_eq!(event.fields.importance, 1);
    assert_eq!(event.property_name.as_dedup(), Some("MyProperty"));
}

// --- PropertyInitialValueSet ---

#[test]
fn property_initial_value_set_all_fields() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(19)); // property_name "MyProperty"
    data.extend(encode_7bit(20)); // property_value "MyValue"
    data.extend(encode_7bit(24)); // property_source "file.cs"
    let mut cursor = Cursor::new(data);
    let event = read_property_initial_value_set(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.property_name.as_dedup(), Some("MyProperty"));
    assert_eq!(event.property_value.as_dedup(), Some("MyValue"));
    assert_eq!(event.property_source.as_dedup(), Some("file.cs"));
}

#[test]
fn property_initial_value_set_all_null() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(0)); // property_name null
    data.extend(encode_7bit(0)); // property_value null
    data.extend(encode_7bit(0)); // property_source null
    let mut cursor = Cursor::new(data);
    let event = read_property_initial_value_set(&mut cursor, &strings, 18).unwrap();
    assert!(event.property_name.is_none());
    assert!(event.property_value.is_none());
    assert!(event.property_source.is_none());
}

#[test]
fn property_initial_value_set_partial_fields() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(19)); // property_name "MyProperty"
    data.extend(encode_7bit(0)); // property_value null
    data.extend(encode_7bit(24)); // property_source "file.cs"
    let mut cursor = Cursor::new(data);
    let event = read_property_initial_value_set(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.property_name.as_dedup(), Some("MyProperty"));
    assert!(event.property_value.is_none());
    assert_eq!(event.property_source.as_dedup(), Some("file.cs"));
}

#[test]
fn property_initial_value_set_with_message() {
    let (strings, _) = make_tables();
    let flags = BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE;
    let mut data = encode_7bit(flags.bits() as i32);
    data.extend(encode_7bit(14)); // message "project.csproj"
    data.extend(encode_7bit(0)); // importance
    data.extend(encode_7bit(19)); // property_name
    data.extend(encode_7bit(20)); // property_value
    data.extend(encode_7bit(18)); // property_source "Build"
    let mut cursor = Cursor::new(data);
    let event = read_property_initial_value_set(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("project.csproj"));
    assert_eq!(event.property_name.as_dedup(), Some("MyProperty"));
    assert_eq!(event.property_source.as_dedup(), Some("Build"));
}

// ===========================================================================
// MN-17: Environment/Response event types
// ===========================================================================

// --- EnvironmentVariableRead ---

#[test]
fn environment_variable_read_v22_all_fields() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 22);
    data.extend(encode_7bit(19)); // environment_variable_name "MyProperty"
    data.extend(encode_7bit(42)); // line
    data.extend(encode_7bit(10)); // column
    data.extend(encode_7bit(24)); // file_name "file.cs"
    let mut cursor = Cursor::new(data);
    let event = read_environment_variable_read(&mut cursor, &strings, 22).unwrap();
    assert_eq!(
        event.environment_variable_name.as_dedup(),
        Some("MyProperty")
    );
    assert_eq!(event.line, 42);
    assert_eq!(event.column, 10);
    assert_eq!(event.file_name.as_dedup(), Some("file.cs"));
}

#[test]
fn environment_variable_read_v22_null_fields() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 22);
    data.extend(encode_7bit(0)); // environment_variable_name null
    data.extend(encode_7bit(0)); // line
    data.extend(encode_7bit(0)); // column
    data.extend(encode_7bit(0)); // file_name null
    let mut cursor = Cursor::new(data);
    let event = read_environment_variable_read(&mut cursor, &strings, 22).unwrap();
    assert!(event.environment_variable_name.is_none());
    assert_eq!(event.line, 0);
    assert_eq!(event.column, 0);
    assert!(event.file_name.is_none());
}

#[test]
fn environment_variable_read_v21_no_location() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 21);
    data.extend(encode_7bit(19)); // environment_variable_name "MyProperty"
                                  // No line/column/fileName for version < 22
    let mut cursor = Cursor::new(data);
    let event = read_environment_variable_read(&mut cursor, &strings, 21).unwrap();
    assert_eq!(
        event.environment_variable_name.as_dedup(),
        Some("MyProperty")
    );
    assert_eq!(event.line, 0);
    assert_eq!(event.column, 0);
    assert!(event.file_name.is_none());
}

#[test]
fn environment_variable_read_v18_no_location() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_7bit(18)); // environment_variable_name "Build"
    let mut cursor = Cursor::new(data);
    let event = read_environment_variable_read(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.environment_variable_name.as_dedup(), Some("Build"));
    assert_eq!(event.line, 0);
}

#[test]
fn environment_variable_read_with_message() {
    let (strings, _) = make_tables();
    let flags = BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE;
    let mut data = encode_7bit(flags.bits() as i32);
    data.extend(encode_7bit(14)); // message "project.csproj"
    data.extend(encode_7bit(1)); // importance
    data.extend(encode_7bit(19)); // name "MyProperty"
    data.extend(encode_7bit(5)); // line
    data.extend(encode_7bit(10)); // column
    data.extend(encode_7bit(24)); // file_name "file.cs"
    let mut cursor = Cursor::new(data);
    let event = read_environment_variable_read(&mut cursor, &strings, 22).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("project.csproj"));
    assert_eq!(event.fields.importance, 1);
    assert_eq!(
        event.environment_variable_name.as_dedup(),
        Some("MyProperty")
    );
    assert_eq!(event.line, 5);
}

// --- ResponseFileUsed ---

#[test]
fn response_file_used_with_path() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(24)); // response_file_path "file.cs"
    let mut cursor = Cursor::new(data);
    let event = read_response_file_used(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.response_file_path.as_dedup(), Some("file.cs"));
}

#[test]
fn response_file_used_null_path() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0)); // response_file_path null
    let mut cursor = Cursor::new(data);
    let event = read_response_file_used(&mut cursor, &strings, 18).unwrap();
    assert!(event.response_file_path.is_none());
}

#[test]
fn response_file_used_with_message() {
    let (strings, _) = make_tables();
    let mut data = encode_message_fields(14); // "project.csproj"
    data.extend(encode_7bit(24)); // response_file_path "file.cs"
    let mut cursor = Cursor::new(data);
    let event = read_response_file_used(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("project.csproj"));
    assert_eq!(event.response_file_path.as_dedup(), Some("file.cs"));
}

#[test]
fn response_file_used_different_strings() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(14)); // response_file_path "project.csproj"
    let mut cursor = Cursor::new(data);
    let event = read_response_file_used(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.response_file_path.as_dedup(), Some("project.csproj"));
}

// ===========================================================================
// MN-18: Assembly/Import event types
// ===========================================================================

/// Encode a GUID as 16 raw bytes.
fn encode_guid(bytes: [u8; 16]) -> Vec<u8> {
    bytes.to_vec()
}

// --- AssemblyLoad ---

#[test]
fn assembly_load_all_fields() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(2)); // context
    data.extend(encode_7bit(16)); // loading_initiator "MyTask"
    data.extend(encode_7bit(17)); // assembly_name "task.dll"
    data.extend(encode_7bit(24)); // assembly_path "file.cs"
    let guid = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10,
    ];
    data.extend(encode_guid(guid));
    data.extend(encode_7bit(18)); // app_domain_name "Build"
    let mut cursor = Cursor::new(data);
    let event = read_assembly_load(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.context, 2);
    assert_eq!(event.loading_initiator.as_dedup(), Some("MyTask"));
    assert_eq!(event.assembly_name.as_dedup(), Some("task.dll"));
    assert_eq!(event.assembly_path.as_dedup(), Some("file.cs"));
    assert_eq!(event.mvid, guid);
    assert_eq!(event.app_domain_name.as_dedup(), Some("Build"));
}

#[test]
fn assembly_load_null_strings() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0)); // context
    data.extend(encode_7bit(0)); // loading_initiator null
    data.extend(encode_7bit(0)); // assembly_name null
    data.extend(encode_7bit(0)); // assembly_path null
    data.extend(encode_guid([0u8; 16]));
    data.extend(encode_7bit(0)); // app_domain_name null
    let mut cursor = Cursor::new(data);
    let event = read_assembly_load(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.context, 0);
    assert!(event.loading_initiator.is_none());
    assert!(event.assembly_name.is_none());
    assert!(event.assembly_path.is_none());
    assert_eq!(event.mvid, [0u8; 16]);
    assert!(event.app_domain_name.is_none());
}

#[test]
fn assembly_load_partial_fields() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(1)); // context
    data.extend(encode_7bit(0)); // loading_initiator null
    data.extend(encode_7bit(17)); // assembly_name "task.dll"
    data.extend(encode_7bit(0)); // assembly_path null
    data.extend(encode_guid([0xAB; 16]));
    data.extend(encode_7bit(0)); // app_domain_name null
    let mut cursor = Cursor::new(data);
    let event = read_assembly_load(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.context, 1);
    assert!(event.loading_initiator.is_none());
    assert_eq!(event.assembly_name.as_dedup(), Some("task.dll"));
    assert_eq!(event.mvid, [0xAB; 16]);
}

#[test]
fn assembly_load_with_message() {
    let (strings, _) = make_tables();
    let mut data = encode_message_fields(14); // "project.csproj"
    data.extend(encode_7bit(3)); // context
    data.extend(encode_7bit(16)); // loading_initiator "MyTask"
    data.extend(encode_7bit(17)); // assembly_name "task.dll"
    data.extend(encode_7bit(0)); // assembly_path null
    data.extend(encode_guid([0u8; 16]));
    data.extend(encode_7bit(0)); // app_domain_name null
    let mut cursor = Cursor::new(data);
    let event = read_assembly_load(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("project.csproj"));
    assert_eq!(event.context, 3);
    assert_eq!(event.loading_initiator.as_dedup(), Some("MyTask"));
}

// --- ProjectImported ---

#[test]
fn project_imported_v18_all_fields() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_bool(false)); // import_ignored
    data.extend(encode_7bit(14)); // imported_project_file "project.csproj"
    data.extend(encode_7bit(24)); // unexpanded_project "file.cs"
    let mut cursor = Cursor::new(data);
    let event = read_project_imported(&mut cursor, &strings, 18).unwrap();
    assert!(!event.import_ignored);
    assert_eq!(
        event.imported_project_file.as_dedup(),
        Some("project.csproj")
    );
    assert_eq!(event.unexpanded_project.as_dedup(), Some("file.cs"));
}

#[test]
fn project_imported_v18_ignored() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_bool(true)); // import_ignored
    data.extend(encode_7bit(0)); // imported_project_file null
    data.extend(encode_7bit(0)); // unexpanded_project null
    let mut cursor = Cursor::new(data);
    let event = read_project_imported(&mut cursor, &strings, 18).unwrap();
    assert!(event.import_ignored);
    assert!(event.imported_project_file.is_none());
    assert!(event.unexpanded_project.is_none());
}

#[test]
fn project_imported_v18_null_strings() {
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 18);
    data.extend(encode_bool(false)); // import_ignored
    data.extend(encode_7bit(0)); // imported_project_file null
    data.extend(encode_7bit(0)); // unexpanded_project null
    let mut cursor = Cursor::new(data);
    let event = read_project_imported(&mut cursor, &strings, 18).unwrap();
    assert!(!event.import_ignored);
    assert!(event.imported_project_file.is_none());
    assert!(event.unexpanded_project.is_none());
}

#[test]
fn project_imported_v2_no_import_ignored() {
    // v2 <= 2: no importIgnored field
    let (strings, _) = make_tables();
    let mut data = encode_importance_fields(0, 2);
    // v2 < 10: optional strings use bool prefix + raw .NET string
    data.extend(encode_bool(true)); // imported_project_file present
    data.extend(encode_dotnet_string("imported.proj"));
    data.extend(encode_bool(false)); // unexpanded_project absent
    let mut cursor = Cursor::new(data);
    let event = read_project_imported(&mut cursor, &strings, 2).unwrap();
    assert!(!event.import_ignored);
    assert_eq!(
        event.imported_project_file.as_dedup(),
        Some("imported.proj")
    );
    assert!(event.unexpanded_project.is_none());
}

#[test]
fn project_imported_with_message() {
    let (strings, _) = make_tables();
    let flags = BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::IMPORTANCE;
    let mut data = encode_7bit(flags.bits() as i32);
    data.extend(encode_7bit(18)); // message "Build"
    data.extend(encode_7bit(0)); // importance
    data.extend(encode_bool(false)); // import_ignored
    data.extend(encode_7bit(14)); // imported_project_file "project.csproj"
    data.extend(encode_7bit(0)); // unexpanded_project null
    let mut cursor = Cursor::new(data);
    let event = read_project_imported(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("Build"));
    assert_eq!(
        event.imported_project_file.as_dedup(),
        Some("project.csproj")
    );
}

// ===========================================================================
// MN-19: BuildCheck event types
// ===========================================================================

// BuildCheckMessage/Warning/Error reuse the existing readers (same binary format).
// The type aliases confirm they share the same struct layout.

#[test]
fn build_check_message_uses_build_message_reader() {
    let (strings, _) = make_tables();
    let data = encode_importance_fields(2, 18);
    let mut cursor = Cursor::new(data);
    let event: BuildCheckMessageEvent = read_build_message(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.importance, 2);
}

#[test]
fn build_check_warning_uses_build_warning_reader() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_diagnostic_location_v18(0, 23, 24, 0, 1, 1, 1, 1));
    let mut cursor = Cursor::new(data);
    let event: BuildCheckWarningEvent = read_build_warning(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.location.code.as_dedup(), Some("CS0001"));
}

#[test]
fn build_check_error_uses_build_error_reader() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_diagnostic_location_v18(
        22, 23, 24, 25, 10, 5, 10, 50,
    ));
    let mut cursor = Cursor::new(data);
    let event: BuildCheckErrorEvent = read_build_error(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.location.subcategory.as_dedup(), Some("SubCat"));
    assert_eq!(event.location.code.as_dedup(), Some("CS0001"));
}

// --- BuildCheckTracing ---

#[test]
fn build_check_tracing_with_data() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(10)); // NVL index 10
    let mut cursor = Cursor::new(data);
    let event = read_build_check_tracing(&mut cursor, &strings, &nvl, 18).unwrap();
    let td = event.tracing_data.unwrap();
    assert_eq!(td.len(), 2);
    assert_eq!(td[0], ("key1".to_string(), "val1".to_string()));
    assert_eq!(td[1], ("key2".to_string(), "val2".to_string()));
}

#[test]
fn build_check_tracing_null_data() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0)); // null NVL
    let mut cursor = Cursor::new(data);
    let event = read_build_check_tracing(&mut cursor, &strings, &nvl, 18).unwrap();
    assert!(event.tracing_data.is_none());
}

#[test]
fn build_check_tracing_with_message() {
    let (strings, nvl) = make_tables();
    let mut data = encode_message_fields(14); // "project.csproj"
    data.extend(encode_7bit(10)); // NVL index 10
    let mut cursor = Cursor::new(data);
    let event = read_build_check_tracing(&mut cursor, &strings, &nvl, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("project.csproj"));
    let td = event.tracing_data.unwrap();
    assert_eq!(td.len(), 2);
}

// --- BuildCheckAcquisition ---

#[test]
fn build_check_acquisition_basic() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_dotnet_string("C:\\rules\\check.dll"));
    data.extend(encode_dotnet_string("C:\\project\\project.csproj"));
    let mut cursor = Cursor::new(data);
    let event = read_build_check_acquisition(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.acquisition_path, "C:\\rules\\check.dll");
    assert_eq!(event.project_path, "C:\\project\\project.csproj");
}

#[test]
fn build_check_acquisition_empty_strings() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_dotnet_string(""));
    data.extend(encode_dotnet_string(""));
    let mut cursor = Cursor::new(data);
    let event = read_build_check_acquisition(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.acquisition_path, "");
    assert_eq!(event.project_path, "");
}

#[test]
fn build_check_acquisition_with_message() {
    let (strings, _) = make_tables();
    let mut data = encode_message_fields(18); // "Build"
    data.extend(encode_dotnet_string("rules.dll"));
    data.extend(encode_dotnet_string("proj.csproj"));
    let mut cursor = Cursor::new(data);
    let event = read_build_check_acquisition(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("Build"));
    assert_eq!(event.acquisition_path, "rules.dll");
    assert_eq!(event.project_path, "proj.csproj");
}

#[test]
fn build_check_acquisition_unicode_paths() {
    let (strings, _) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_dotnet_string("C:\\règles\\vérification.dll"));
    data.extend(encode_dotnet_string("C:\\projet\\项目.csproj"));
    let mut cursor = Cursor::new(data);
    let event = read_build_check_acquisition(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.acquisition_path, "C:\\règles\\vérification.dll");
    assert_eq!(event.project_path, "C:\\projet\\项目.csproj");
}

// ===========================================================================
// MN-20: Remaining event types (BuildSubmissionStarted, BuildCanceled)
// ===========================================================================

// --- BuildSubmissionStarted ---

#[test]
fn build_submission_started_all_fields() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(10)); // global_properties NVL index 10
                                  // entry_projects_full_path: 2 strings
    data.extend(encode_7bit(2));
    data.extend(encode_7bit(14)); // "project.csproj"
    data.extend(encode_7bit(25)); // "proj.csproj"
                                  // target_names: 1 string
    data.extend(encode_7bit(1));
    data.extend(encode_7bit(18)); // "Build"
    data.extend(encode_7bit(7)); // flags
    data.extend(encode_7bit(42)); // submission_id
    let mut cursor = Cursor::new(data);
    let event = read_build_submission_started(&mut cursor, &strings, &nvl, 18).unwrap();
    let gp = event.global_properties.unwrap();
    assert_eq!(gp.len(), 2);
    assert_eq!(gp[0], ("key1".to_string(), "val1".to_string()));
    let epp = event.entry_projects_full_path.unwrap();
    assert_eq!(epp.len(), 2);
    assert_eq!(epp[0], "project.csproj");
    assert_eq!(epp[1], "proj.csproj");
    let tn = event.target_names.unwrap();
    assert_eq!(tn.len(), 1);
    assert_eq!(tn[0], "Build");
    assert_eq!(event.flags, 7);
    assert_eq!(event.submission_id, 42);
}

#[test]
fn build_submission_started_null_properties() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0)); // global_properties null
    data.extend(encode_7bit(0)); // entry_projects_full_path count=0
    data.extend(encode_7bit(0)); // target_names count=0
    data.extend(encode_7bit(0)); // flags
    data.extend(encode_7bit(1)); // submission_id
    let mut cursor = Cursor::new(data);
    let event = read_build_submission_started(&mut cursor, &strings, &nvl, 18).unwrap();
    assert!(event.global_properties.is_none());
    assert!(event.entry_projects_full_path.is_none());
    assert!(event.target_names.is_none());
    assert_eq!(event.flags, 0);
    assert_eq!(event.submission_id, 1);
}

#[test]
fn build_submission_started_empty_string_lists() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(10)); // global_properties NVL index 10
    data.extend(encode_7bit(0)); // entry_projects empty
    data.extend(encode_7bit(0)); // target_names empty
    data.extend(encode_7bit(3)); // flags
    data.extend(encode_7bit(5)); // submission_id
    let mut cursor = Cursor::new(data);
    let event = read_build_submission_started(&mut cursor, &strings, &nvl, 18).unwrap();
    assert!(event.global_properties.is_some());
    assert!(event.entry_projects_full_path.is_none());
    assert!(event.target_names.is_none());
    assert_eq!(event.flags, 3);
    assert_eq!(event.submission_id, 5);
}

#[test]
fn build_submission_started_with_message() {
    let (strings, nvl) = make_tables();
    let mut data = encode_message_fields(14); // "project.csproj"
    data.extend(encode_7bit(0)); // global_properties null
    data.extend(encode_7bit(1)); // entry_projects: 1
    data.extend(encode_7bit(14)); // "project.csproj"
    data.extend(encode_7bit(0)); // target_names empty
    data.extend(encode_7bit(0)); // flags
    data.extend(encode_7bit(10)); // submission_id
    let mut cursor = Cursor::new(data);
    let event = read_build_submission_started(&mut cursor, &strings, &nvl, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("project.csproj"));
    let epp = event.entry_projects_full_path.unwrap();
    assert_eq!(epp.len(), 1);
    assert_eq!(epp[0], "project.csproj");
    assert_eq!(event.submission_id, 10);
}

#[test]
fn build_submission_started_multiple_targets() {
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields();
    data.extend(encode_7bit(0)); // global_properties null
    data.extend(encode_7bit(0)); // entry_projects empty
                                 // target_names: 3 strings
    data.extend(encode_7bit(3));
    data.extend(encode_7bit(18)); // "Build"
    data.extend(encode_7bit(15)); // "MyTarget"
    data.extend(encode_7bit(16)); // "MyTask"
    data.extend(encode_7bit(0)); // flags
    data.extend(encode_7bit(0)); // submission_id
    let mut cursor = Cursor::new(data);
    let event = read_build_submission_started(&mut cursor, &strings, &nvl, 18).unwrap();
    let tn = event.target_names.unwrap();
    assert_eq!(tn.len(), 3);
    assert_eq!(tn[0], "Build");
    assert_eq!(tn[1], "MyTarget");
    assert_eq!(tn[2], "MyTask");
}

// --- BuildCanceled ---

#[test]
fn build_canceled_minimal() {
    let (strings, _) = make_tables();
    let data = encode_empty_fields();
    let mut cursor = Cursor::new(data);
    let event = read_build_canceled(&mut cursor, &strings, 18).unwrap();
    assert!(event.fields.message.is_none());
}

#[test]
fn build_canceled_with_message() {
    let (strings, _) = make_tables();
    let data = encode_message_fields(18); // "Build"
    let mut cursor = Cursor::new(data);
    let event = read_build_canceled(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("Build"));
}

#[test]
fn build_canceled_with_various_messages() {
    let (strings, _) = make_tables();
    let data = encode_message_fields(14); // "project.csproj"
    let mut cursor = Cursor::new(data);
    let event = read_build_canceled(&mut cursor, &strings, 18).unwrap();
    assert_eq!(event.fields.message.as_dedup(), Some("project.csproj"));
}

// ===========================================================================
// MN-19: Regression tests — negative collection counts
// ===========================================================================

#[test]
fn rejects_negative_profile_entry_count() {
    // Version 5: reads has_profile_data bool then the profile entry count.
    // A negative count should be rejected with InvalidFormat.
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields(); // flags NONE
    data.extend(encode_7bit(0)); // project_file = null
    data.extend(encode_bool(true)); // has_profile_data = true
    data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]); // count = -1
    let mut cursor = Cursor::new(data);
    let err = read_project_evaluation_finished(&mut cursor, &strings, &nvl, 5).unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}

#[test]
fn rejects_negative_string_list_count() {
    // read_string_list immediately reads the count; -1 should be rejected.
    let (strings, _) = make_tables();
    let data: Vec<u8> = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]; // count = -1
    let mut cursor = Cursor::new(data);
    let err = super::read_string_list(&mut cursor, &strings).unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}

#[test]
fn rejects_negative_task_item_count() {
    // read_task_item_list immediately reads the count; -1 should be rejected.
    let (strings, nvl) = make_tables();
    let data: Vec<u8> = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]; // count = -1
    let mut cursor = Cursor::new(data);
    let err = super::read_task_item_list(&mut cursor, &strings, &nvl, 18).unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}

#[test]
fn rejects_negative_item_count_pre_v10() {
    // read_project_items with version < 10 reads a flat item count; -1 should be rejected.
    let (strings, nvl) = make_tables();
    let data: Vec<u8> = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]; // count = -1
    let mut cursor = Cursor::new(data);
    let err = super::read_project_items(&mut cursor, &strings, &nvl, 9).unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}

#[test]
fn rejects_negative_item_group_count_v10_11() {
    // read_project_items with version 10–11 reads a group count; -1 should be rejected.
    let (strings, nvl) = make_tables();
    let data: Vec<u8> = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]; // count = -1
    let mut cursor = Cursor::new(data);
    let err = super::read_project_items(&mut cursor, &strings, &nvl, 10).unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}

#[test]
fn rejects_negative_target_output_count() {
    // Regression: read_target_finished calls read_task_item_list for outputs.
    // A negative output count should be rejected with InvalidFormat.
    // Payload for version 18:
    //   [0x00] flags NONE, [0x01] succeeded=true, [0x00][0x00][0x00] three null optional strings,
    //   [0xFF…0x0F] task-item count = -1.
    let (strings, nvl) = make_tables();
    let mut data = encode_empty_fields(); // flags NONE
    data.extend(encode_bool(true)); // succeeded
    data.extend(encode_7bit(0)); // project_file null (v18: dedup index)
    data.extend(encode_7bit(0)); // target_file null
    data.extend(encode_7bit(0)); // target_name null
    data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]); // target_outputs count = -1
    let mut cursor = Cursor::new(data);
    let err = read_target_finished(&mut cursor, &strings, &nvl, 18).unwrap_err();
    assert!(
        matches!(err, crate::error::MuninError::InvalidFormat(_)),
        "expected InvalidFormat, got {err:?}"
    );
}
