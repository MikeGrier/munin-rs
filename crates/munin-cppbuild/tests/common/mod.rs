// Copyright (c) Michael Grier

//! Shared fixtures for `munin-cppbuild` integration tests.
//!
//! See `DESIGN-NOTES.md` §D-CPP-FIXTURE1 for the rationale behind
//! constructing fixtures programmatically rather than checking in
//! binary `.binlog` artifacts.

#![allow(dead_code)]

use munin_msbuild::{
    BinlogIndex,
    field_flags::BuildEventArgsFieldFlags,
    jsonlog::schema::{
        JsonlogEvent, JsonlogEventBody, JsonlogFile, JsonlogHeader, MUNIN_JSONLOG_VERSION,
    },
};

/// Path used for the synthetic `.vcxproj` in fixtures. Absolute and
/// Windows-shaped so path-root resolution behaves realistically.
pub const FIXTURE_PROJECT_PATH: &str = r"C:\src\product\app\app.vcxproj";

/// Build a single decoded `BuildStarted` jsonlog event.
fn build_started_event() -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits() as i32,
            "build_event_context": {
                "node_id": 1,
                "project_context_id": 0,
                "target_id": -1,
                "task_id": -1,
                "submission_id": 0,
                "project_instance_id": 0,
                "evaluation_id": -1,
            },
            "thread_id": 0,
            "importance": 0,
            "line_number": 0,
            "column_number": 0,
            "end_line_number": 0,
            "end_column_number": 0,
        },
        "environment": [],
    });
    JsonlogEvent {
        kind: "BuildStarted".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

/// Build a `ProjectStarted` event for the synthetic `.vcxproj` with a
/// given context id and globals.
fn project_started_event(
    project_context_id: i32,
    project_file: &str,
    globals: &[(&str, &str)],
) -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits() as i32,
            "build_event_context": {
                "node_id": 1,
                "project_context_id": project_context_id,
                "target_id": -1,
                "task_id": -1,
                "submission_id": 0,
                "project_instance_id": project_context_id,
                "evaluation_id": -1,
            },
            "thread_id": 0,
            "importance": 0,
            "line_number": 0,
            "column_number": 0,
            "end_line_number": 0,
            "end_column_number": 0,
        },
        "parent_context": null,
        "project_file": project_file,
        "project_id": project_context_id,
        "target_names": "Build",
        "tools_version": "Current",
        "global_properties": globals
            .iter()
            .map(|(k, v)| [k, v])
            .collect::<Vec<_>>(),
        "property_list": [],
        "item_list": [],
    });
    JsonlogEvent {
        kind: "ProjectStarted".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

/// Build a `ProjectFinished` event for the given project context.
fn project_finished_event(project_context_id: i32, project_file: &str) -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits() as i32,
            "build_event_context": {
                "node_id": 1,
                "project_context_id": project_context_id,
                "target_id": -1,
                "task_id": -1,
                "submission_id": 0,
                "project_instance_id": project_context_id,
                "evaluation_id": -1,
            },
            "thread_id": 0,
            "importance": 0,
            "line_number": 0,
            "column_number": 0,
            "end_line_number": 0,
            "end_column_number": 0,
        },
        "project_file": project_file,
        "succeeded": true,
    });
    JsonlogEvent {
        kind: "ProjectFinished".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

/// Build a `BuildFinished` event.
fn build_finished_event() -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": 0,
            "build_event_context": null,
            "thread_id": 0,
            "importance": 0,
            "line_number": 0,
            "column_number": 0,
            "end_line_number": 0,
            "end_column_number": 0,
        },
        "succeeded": true,
    });
    JsonlogEvent {
        kind: "BuildFinished".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

/// Construct a synthetic binlog representing one `.vcxproj` built
/// twice (Debug|x64 followed by Release|x64) with no targets / tasks
/// beyond project bracketing.
///
/// Returns the raw gzip-compressed `.binlog` bytes, suitable for
/// passing to [`munin_msbuild::BinlogIndex::open`].
pub fn synthetic_vcxproj_binlog() -> Vec<u8> {
    let events = vec![
        build_started_event(),
        project_started_event(
            10,
            FIXTURE_PROJECT_PATH,
            &[("Configuration", "Debug"), ("Platform", "x64")],
        ),
        project_finished_event(10, FIXTURE_PROJECT_PATH),
        project_started_event(
            11,
            FIXTURE_PROJECT_PATH,
            &[("Configuration", "Release"), ("Platform", "x64")],
        ),
        project_finished_event(11, FIXTURE_PROJECT_PATH),
        build_finished_event(),
    ];

    let file = JsonlogFile {
        munin_jsonlog_version: MUNIN_JSONLOG_VERSION,
        header: JsonlogHeader {
            file_format_version: 18,
            min_reader_version: 18,
        },
        strings: Vec::new(),
        name_value_lists: Vec::new(),
        archives: Vec::new(),
        events,
    };

    let index = BinlogIndex::from_jsonlog(file).expect("fixture jsonlog should parse");
    let mut bytes = Vec::new();
    index
        .write_binlog(&mut bytes)
        .expect("fixture binlog should write");
    bytes
}
