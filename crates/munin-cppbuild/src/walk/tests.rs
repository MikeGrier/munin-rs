// Copyright (c) Michael Grier

//! Unit tests for [`walk_projects`].

use super::*;

use munin_msbuild::{
    BinlogIndex,
    field_flags::BuildEventArgsFieldFlags,
    jsonlog::schema::{
        JsonlogEvent, JsonlogEventBody, JsonlogFile, JsonlogHeader, MUNIN_JSONLOG_VERSION,
    },
};

/// Build a single decoded `ProjectStarted` jsonlog event with the given
/// project_context_id, project_file, and global_properties.
fn project_started_event(
    project_context_id: i32,
    project_file: &str,
    global_properties: &[(&str, &str)],
    property_list: &[(&str, &str)],
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
        "target_names": "",
        "tools_version": "Current",
        "global_properties": global_properties
            .iter()
            .map(|(k, v)| [k, v])
            .collect::<Vec<_>>(),
        "property_list": property_list
            .iter()
            .map(|(k, v)| [k, v])
            .collect::<Vec<_>>(),
        "item_list": [],
    });

    JsonlogEvent {
        kind: "ProjectStarted".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

fn build_index(events: Vec<JsonlogEvent>) -> BinlogIndex {
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
    BinlogIndex::from_jsonlog(file).expect("synthetic jsonlog should parse")
}

#[test]
fn walks_zero_projects() {
    let index = build_index(Vec::new());
    let projects = walk_projects(&index).expect("walk");
    assert!(projects.is_empty());
}

#[test]
fn walks_one_project_with_globals() {
    let index = build_index(vec![project_started_event(
        7,
        r"C:\src\Hello.vcxproj",
        &[("Configuration", "Debug"), ("Platform", "x64")],
        &[],
    )]);

    let projects = walk_projects(&index).expect("walk");
    assert_eq!(projects.len(), 1);
    let p = &projects[0];
    assert_eq!(p.project_context_id, 7);
    assert_eq!(p.project_file.as_deref(), Some(r"C:\src\Hello.vcxproj"));
    assert_eq!(p.configuration(), Some("Debug"));
    assert_eq!(p.platform(), Some("x64"));
}

#[test]
fn walks_two_projects_preserves_order() {
    let index = build_index(vec![
        project_started_event(
            10,
            r"C:\src\A.vcxproj",
            &[("Configuration", "Debug"), ("Platform", "x64")],
            &[],
        ),
        project_started_event(
            11,
            r"C:\src\A.vcxproj",
            &[("Configuration", "Release"), ("Platform", "x64")],
            &[],
        ),
    ]);

    let projects = walk_projects(&index).expect("walk");
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].project_context_id, 10);
    assert_eq!(projects[0].configuration(), Some("Debug"));
    assert_eq!(projects[1].project_context_id, 11);
    assert_eq!(projects[1].configuration(), Some("Release"));
}

#[test]
fn configuration_falls_back_to_property_list() {
    let index = build_index(vec![project_started_event(
        3,
        r"C:\src\B.vcxproj",
        &[],
        &[("Configuration", "Debug"), ("Platform", "Win32")],
    )]);

    let projects = walk_projects(&index).expect("walk");
    assert_eq!(projects[0].configuration(), Some("Debug"));
    assert_eq!(projects[0].platform(), Some("Win32"));
}

#[test]
fn lookup_is_case_insensitive() {
    let index = build_index(vec![project_started_event(
        1,
        r"C:\src\C.vcxproj",
        &[("CONFIGURATION", "Release"), ("plaTFORM", "ARM64")],
        &[],
    )]);

    let projects = walk_projects(&index).expect("walk");
    assert_eq!(projects[0].configuration(), Some("Release"));
    assert_eq!(projects[0].platform(), Some("ARM64"));
}

#[test]
fn missing_context_yields_zero_id() {
    // Build a ProjectStarted with flags=0 (no context).
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
        "parent_context": null,
        "project_file": "X.vcxproj",
        "project_id": 0,
        "target_names": "",
        "tools_version": "",
        "global_properties": [],
        "property_list": [],
        "item_list": [],
    });
    let index = build_index(vec![JsonlogEvent {
        kind: "ProjectStarted".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }]);

    let projects = walk_projects(&index).expect("walk");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].project_context_id, 0);
}

#[test]
fn missing_globals_are_empty_not_panicking() {
    let body = serde_json::json!({
        "fields": {
            "flags": BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits() as i32,
            "build_event_context": {
                "node_id": 1, "project_context_id": 2, "target_id": -1,
                "task_id": -1, "submission_id": 0,
                "project_instance_id": 2, "evaluation_id": -1,
            },
            "thread_id": 0,
            "importance": 0,
            "line_number": 0,
            "column_number": 0,
            "end_line_number": 0,
            "end_column_number": 0,
        },
        "parent_context": null,
        "project_file": "Y.vcxproj",
        "project_id": 2,
        "target_names": "",
        "tools_version": "",
        "global_properties": null,
        "property_list": null,
        "item_list": [],
    });
    let index = build_index(vec![JsonlogEvent {
        kind: "ProjectStarted".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }]);

    let projects = walk_projects(&index).expect("walk");
    assert!(projects[0].global_properties.is_empty());
    assert!(projects[0].property_list.is_empty());
    assert_eq!(projects[0].configuration(), None);
    assert_eq!(projects[0].platform(), None);
}
