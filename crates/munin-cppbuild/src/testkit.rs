// Copyright (c) Michael Grier

//! Synthetic `.binlog` fixture builders for use by integration tests
//! in this crate and in downstream consumers (such as
//! `munin-jsonlog-cli`).
//!
//! See `DESIGN-NOTES.md` §D-CPP-FIXTURE1 for the rationale behind
//! constructing fixtures programmatically rather than checking in
//! binary `.binlog` artifacts.
//!
//! Not intended for production use; only the synthesis helpers needed
//! by tests are exposed.

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
        "project_file": FIXTURE_PROJECT_PATH,
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
        "project_file": FIXTURE_PROJECT_PATH,
        "task_file": null,
    });
    JsonlogEvent {
        kind: "TaskFinished".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

fn task_command_line_event(command: &str, project_context_id: i32, task_id: i32) -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": (BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits()
                | BuildEventArgsFieldFlags::MESSAGE.bits()) as i32,
            "message": command,
            "build_event_context": {
                "node_id": 1,
                "project_context_id": project_context_id,
                "target_id": 1,
                "task_id": task_id,
                "submission_id": 0,
                "project_instance_id": project_context_id,
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

fn message_event(text: &str, project_context_id: i32, task_id: i32) -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": (BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits()
                | BuildEventArgsFieldFlags::MESSAGE.bits()) as i32,
            "message": text,
            "build_event_context": {
                "node_id": 1,
                "project_context_id": project_context_id,
                "target_id": 1,
                "task_id": task_id,
                "submission_id": 0,
                "project_instance_id": project_context_id,
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

/// Construct a synthetic binlog that brackets a single `.vcxproj`
/// invocation containing a single CL task. The task records the
/// supplied `command_line` and `messages` (one `Message` event per
/// entry, in order).
pub fn synthetic_cl_task_binlog(command_line: &str, messages: &[&str]) -> Vec<u8> {
    let project_id: i32 = 10;
    let task_id: i32 = 5;

    let mut events = vec![
        build_started_event(),
        project_started_event(
            project_id,
            FIXTURE_PROJECT_PATH,
            &[("Configuration", "Debug"), ("Platform", "x64")],
        ),
        task_started_event("CL", project_id, task_id),
        task_command_line_event(command_line, project_id, task_id),
    ];
    for msg in messages {
        events.push(message_event(msg, project_id, task_id));
    }
    events.push(task_finished_event("CL", project_id, task_id));
    events.push(project_finished_event(project_id, FIXTURE_PROJECT_PATH));
    events.push(build_finished_event());

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

fn link_command_line_event(command: &str, project_context_id: i32, task_id: i32) -> JsonlogEvent {
    let body = serde_json::json!({
        "fields": {
            "flags": (BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits()
                | BuildEventArgsFieldFlags::MESSAGE.bits()) as i32,
            "message": command,
            "build_event_context": {
                "node_id": 1,
                "project_context_id": project_context_id,
                "target_id": 1,
                "task_id": task_id,
                "submission_id": 0,
                "project_instance_id": project_context_id,
                "evaluation_id": -1,
            },
            "thread_id": 0, "importance": 1,
            "line_number": 0, "column_number": 0,
            "end_line_number": 0, "end_column_number": 0,
        },
        "command_line": command,
        "task_name": "Link",
    });
    JsonlogEvent {
        kind: "TaskCommandLine".to_string(),
        byte_offset: 0,
        body: JsonlogEventBody::Decoded(body),
    }
}

/// Construct a synthetic binlog that brackets a single `.vcxproj`
/// invocation containing one CL task followed by one Link task.
/// Either task's `messages` may be empty.
pub fn synthetic_cl_link_binlog(
    cl_command_line: &str,
    cl_messages: &[&str],
    link_command_line: &str,
    link_messages: &[&str],
) -> Vec<u8> {
    let project_id: i32 = 10;
    let cl_task_id: i32 = 5;
    let link_task_id: i32 = 6;

    let mut events = vec![
        build_started_event(),
        project_started_event(
            project_id,
            FIXTURE_PROJECT_PATH,
            &[("Configuration", "Debug"), ("Platform", "x64")],
        ),
        task_started_event("CL", project_id, cl_task_id),
        task_command_line_event(cl_command_line, project_id, cl_task_id),
    ];
    for msg in cl_messages {
        events.push(message_event(msg, project_id, cl_task_id));
    }
    events.push(task_finished_event("CL", project_id, cl_task_id));

    events.push(task_started_event("Link", project_id, link_task_id));
    events.push(link_command_line_event(
        link_command_line,
        project_id,
        link_task_id,
    ));
    for msg in link_messages {
        events.push(message_event(msg, project_id, link_task_id));
    }
    events.push(task_finished_event("Link", project_id, link_task_id));

    events.push(project_finished_event(project_id, FIXTURE_PROJECT_PATH));
    events.push(build_finished_event());

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
