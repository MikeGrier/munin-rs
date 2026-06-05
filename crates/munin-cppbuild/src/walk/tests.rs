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

#[test]
fn to_global_properties_marks_all_as_command_line() {
    use crate::schema::PropertySource;

    let index = build_index(vec![project_started_event(
        1,
        r"C:\src\X.vcxproj",
        &[
            ("Configuration", "Debug"),
            ("Platform", "x64"),
            ("CustomProp", "value"),
        ],
        &[],
    )]);

    let projects = walk_projects(&index).expect("walk");
    let globals = projects[0].to_global_properties();
    assert_eq!(globals.len(), 3);
    for g in &globals {
        assert_eq!(g.source, PropertySource::CommandLine);
    }
    assert_eq!(globals[0].name, "Configuration");
    assert_eq!(globals[0].value, "Debug");
    assert_eq!(globals[2].name, "CustomProp");
    assert_eq!(globals[2].value, "value");
}

#[test]
fn to_global_properties_preserves_msbuild_order() {
    let index = build_index(vec![project_started_event(
        1,
        r"C:\src\X.vcxproj",
        &[("Z", "1"), ("A", "2"), ("M", "3")],
        &[],
    )]);

    let projects = walk_projects(&index).expect("walk");
    let globals = projects[0].to_global_properties();
    let names: Vec<&str> = globals.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, vec!["Z", "A", "M"]);
}

#[test]
fn to_global_properties_is_empty_when_no_globals() {
    let index = build_index(vec![project_started_event(
        1,
        r"C:\src\X.vcxproj",
        &[],
        &[("Configuration", "Debug")],
    )]);

    let projects = walk_projects(&index).expect("walk");
    assert!(projects[0].to_global_properties().is_empty());
}

// ── walk_cl_tasks ──────────────────────────────────────────────────

fn project_finished_event(project_context_id: i32) -> JsonlogEvent {
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
            "thread_id": 0, "importance": 0,
            "line_number": 0, "column_number": 0,
            "end_line_number": 0, "end_column_number": 0,
        },
        "project_file": "p.vcxproj",
        "succeeded": true,
    });
    JsonlogEvent {
        kind: "ProjectFinished".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

fn task_started_event(task_name: &str, project_context_id: i32, task_id: i32) -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits() as i32,
            "build_event_context": {
                "node_id": 1,
                "project_context_id": project_context_id,
                "target_id": 1,
                "task_id": task_id,
                "submission_id": 0,
                "project_instance_id": project_context_id,
                "evaluation_id": -1,
            },
            "thread_id": 0, "importance": 0,
            "line_number": 0, "column_number": 0,
            "end_line_number": 0, "end_column_number": 0,
        },
        "task_name": task_name,
        "project_file": "p.vcxproj",
        "task_file": null,
        "task_assembly_location": null,
    });
    JsonlogEvent {
        kind: "TaskStarted".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

fn task_finished_event(task_name: &str, project_context_id: i32, task_id: i32) -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits() as i32,
            "build_event_context": {
                "node_id": 1,
                "project_context_id": project_context_id,
                "target_id": 1,
                "task_id": task_id,
                "submission_id": 0,
                "project_instance_id": project_context_id,
                "evaluation_id": -1,
            },
            "thread_id": 0, "importance": 0,
            "line_number": 0, "column_number": 0,
            "end_line_number": 0, "end_column_number": 0,
        },
        "succeeded": true,
        "task_name": task_name,
        "project_file": "p.vcxproj",
        "task_file": null,
    });
    JsonlogEvent {
        kind: "TaskFinished".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

fn task_command_line_event(command: &str, task_id: i32) -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": (BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits()
                | BuildEventArgsFieldFlags::MESSAGE.bits()) as i32,
            "message": command,
            "build_event_context": {
                "node_id": 1,
                "project_context_id": 10,
                "target_id": 1,
                "task_id": task_id,
                "submission_id": 0,
                "project_instance_id": 10,
                "evaluation_id": -1,
            },
            "thread_id": 0, "importance": 1,
            "line_number": 0, "column_number": 0,
            "end_line_number": 0, "end_column_number": 0,
        },
        "command_line": command,
        "task_name": "CL",
    });
    JsonlogEvent {
        kind: "TaskCommandLine".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

fn message_event(text: &str, task_id: i32) -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": (BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits()
                | BuildEventArgsFieldFlags::MESSAGE.bits()) as i32,
            "message": text,
            "build_event_context": {
                "node_id": 1,
                "project_context_id": 10,
                "target_id": 1,
                "task_id": task_id,
                "submission_id": 0,
                "project_instance_id": 10,
                "evaluation_id": -1,
            },
            "thread_id": 0, "importance": 1,
            "line_number": 0, "column_number": 0,
            "end_line_number": 0, "end_column_number": 0,
        },
    });
    JsonlogEvent {
        kind: "Message".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

#[test]
fn no_cl_tasks_yields_empty() {
    let index = build_index(vec![]);
    let cls = walk_cl_tasks(&index).expect("walk");
    assert!(cls.is_empty());
}

#[test]
fn single_cl_task_captures_command_line_and_messages() {
    let index = build_index(vec![
        project_started_event(
            10,
            r"C:\src\a.vcxproj",
            &[("Configuration", "Debug"), ("Platform", "x64")],
            &[],
        ),
        task_started_event("CL", 10, 5),
        task_command_line_event(r"CL.exe /c a.cpp /showIncludes", 5),
        message_event(r"Note: including file: C:\sdk\stdio.h", 5),
        message_event(r"Note: including file:  C:\sdk\stddef.h", 5),
        task_finished_event("CL", 10, 5),
        project_finished_event(10),
    ]);

    let cls = walk_cl_tasks(&index).expect("walk");
    assert_eq!(cls.len(), 1);
    assert_eq!(cls[0].project_context_id, 10);
    assert_eq!(
        cls[0].command_line.as_deref(),
        Some(r"CL.exe /c a.cpp /showIncludes")
    );
    assert_eq!(cls[0].messages.len(), 2);
    assert!(cls[0].messages[0].contains("stdio.h"));
    assert!(cls[0].messages[1].contains("stddef.h"));
}

#[test]
fn non_cl_tasks_are_ignored() {
    let index = build_index(vec![
        project_started_event(10, "p.vcxproj", &[], &[]),
        task_started_event("Csc", 10, 5),
        task_command_line_event(r"csc.exe /target:exe", 5),
        message_event("not a CL message", 5),
        task_finished_event("Csc", 10, 5),
        project_finished_event(10),
    ]);

    let cls = walk_cl_tasks(&index).expect("walk");
    assert!(cls.is_empty());
}

#[test]
fn message_outside_cl_bracket_is_dropped() {
    let index = build_index(vec![
        project_started_event(10, "p.vcxproj", &[], &[]),
        message_event("before any task", 0),
        task_started_event("CL", 10, 5),
        message_event("inside", 5),
        task_finished_event("CL", 10, 5),
        message_event("after task", 0),
        project_finished_event(10),
    ]);

    let cls = walk_cl_tasks(&index).expect("walk");
    assert_eq!(cls.len(), 1);
    assert_eq!(cls[0].messages, vec!["inside".to_string()]);
}

#[test]
fn multiple_cl_tasks_in_one_project() {
    let index = build_index(vec![
        project_started_event(10, "p.vcxproj", &[], &[]),
        task_started_event("CL", 10, 5),
        task_command_line_event("cl a.cpp", 5),
        message_event("Note: including file: a.h", 5),
        task_finished_event("CL", 10, 5),
        task_started_event("CL", 10, 6),
        task_command_line_event("cl b.cpp", 6),
        message_event("Note: including file: b.h", 6),
        task_finished_event("CL", 10, 6),
        project_finished_event(10),
    ]);

    let cls = walk_cl_tasks(&index).expect("walk");
    assert_eq!(cls.len(), 2);
    assert_eq!(cls[0].command_line.as_deref(), Some("cl a.cpp"));
    assert_eq!(cls[0].messages, vec!["Note: including file: a.h"]);
    assert_eq!(cls[1].command_line.as_deref(), Some("cl b.cpp"));
    assert_eq!(cls[1].messages, vec!["Note: including file: b.h"]);
}

#[test]
fn cl_tasks_across_two_projects_get_distinct_context_ids() {
    let index = build_index(vec![
        project_started_event(10, "a.vcxproj", &[], &[]),
        task_started_event("CL", 10, 5),
        task_finished_event("CL", 10, 5),
        project_finished_event(10),
        project_started_event(11, "b.vcxproj", &[], &[]),
        task_started_event("CL", 11, 7),
        task_finished_event("CL", 11, 7),
        project_finished_event(11),
    ]);

    let cls = walk_cl_tasks(&index).expect("walk");
    assert_eq!(cls.len(), 2);
    assert_eq!(cls[0].project_context_id, 10);
    assert_eq!(cls[1].project_context_id, 11);
}

#[test]
fn cl_task_with_no_command_line_event_is_still_emitted() {
    let index = build_index(vec![
        project_started_event(10, "p.vcxproj", &[], &[]),
        task_started_event("CL", 10, 5),
        message_event("Note: including file: a.h", 5),
        task_finished_event("CL", 10, 5),
        project_finished_event(10),
    ]);

    let cls = walk_cl_tasks(&index).expect("walk");
    assert_eq!(cls.len(), 1);
    assert_eq!(cls[0].command_line, None);
    assert_eq!(cls[0].messages.len(), 1);
}

#[test]
fn first_task_command_line_event_wins() {
    let index = build_index(vec![
        project_started_event(10, "p.vcxproj", &[], &[]),
        task_started_event("CL", 10, 5),
        task_command_line_event("first", 5),
        task_command_line_event("second", 5),
        task_finished_event("CL", 10, 5),
        project_finished_event(10),
    ]);

    let cls = walk_cl_tasks(&index).expect("walk");
    assert_eq!(cls[0].command_line.as_deref(), Some("first"));
}
