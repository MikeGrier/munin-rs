// Copyright (c) Michael Grier

use std::path::PathBuf;

use serde_json::json;

use super::*;

/// Path to the hello.binlog test fixture (successful build, no errors/warnings).
fn hello_binlog_path() -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(".."); // crates/
    p.push("munin");
    p.push("tests");
    p.push("data");
    p.push("hello.binlog");
    p.to_string_lossy().into_owned()
}

/// Open the hello.binlog fixture and return its session handle.
fn open_hello(sessions: &mut SessionMap) -> SessionHandle {
    let path = hello_binlog_path();
    let args = json!({ "file": path });
    let result = call_binlog_open(&args, sessions).expect("binlog_open should succeed");
    // Parse the handle from "Session: N\r\n..."
    let first_line = result.lines().next().unwrap();
    let handle_str = first_line.strip_prefix("Session: ").unwrap();
    handle_str.parse::<SessionHandle>().unwrap()
}

// ── binlog_open tests ─────────────────────────────────────────────────────────

#[test]
fn open_returns_session_handle_and_summary() {
    let mut sessions = SessionMap::new();
    let path = hello_binlog_path();
    let args = json!({ "file": path });
    let result = call_binlog_open(&args, &mut sessions).expect("binlog_open should succeed");

    assert!(result.contains("Session: "));
    assert!(result.contains("Format version:"));
    assert!(result.contains("Total events:"));
    assert!(result.contains("Errors:"));
    assert!(result.contains("Warnings:"));
}

#[test]
fn open_nonexistent_file_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "file": "nonexistent.binlog" });
    let result = call_binlog_open(&args, &mut sessions);
    assert!(result.is_err());
}

#[test]
fn open_missing_file_param_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({});
    let result = call_binlog_open(&args, &mut sessions);
    assert!(result.is_err());
}

#[test]
fn open_assigns_unique_handles() {
    let mut sessions = SessionMap::new();
    let h1 = open_hello(&mut sessions);
    let h2 = open_hello(&mut sessions);
    assert_ne!(h1, h2, "each open should produce a unique handle");
}

// ── binlog_close tests ────────────────────────────────────────────────────────

#[test]
fn close_valid_session_succeeds() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_close(&args, &mut sessions).expect("close should succeed");
    assert!(result.contains("closed"));
}

#[test]
fn close_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999 });
    let result = call_binlog_close(&args, &mut sessions);
    assert!(result.is_err());
}

#[test]
fn close_same_handle_twice_returns_error() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    call_binlog_close(&args, &mut sessions).expect("first close should succeed");
    let result = call_binlog_close(&args, &mut sessions);
    assert!(result.is_err(), "second close should fail");
}

// ── binlog_summary tests ─────────────────────────────────────────────────────

#[test]
fn summary_reports_build_succeeded() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_summary(&args, &mut sessions).expect("binlog_summary should succeed");

    assert!(
        result.contains("Build result: succeeded"),
        "hello.binlog is a successful build"
    );
}

#[test]
fn summary_reports_zero_errors_and_warnings() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_summary(&args, &mut sessions).expect("binlog_summary should succeed");

    assert!(result.contains("Errors: 0"), "hello.binlog has no errors");
    assert!(
        result.contains("Warnings: 0"),
        "hello.binlog has no warnings"
    );
}

#[test]
fn summary_includes_file_path() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_summary(&args, &mut sessions).expect("binlog_summary should succeed");

    assert!(result.contains("File:"), "summary should include file path");
}

#[test]
fn summary_includes_format_version() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_summary(&args, &mut sessions).expect("binlog_summary should succeed");

    assert!(
        result.contains("Format version:"),
        "summary should include format version"
    );
}

#[test]
fn summary_includes_total_events() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_summary(&args, &mut sessions).expect("binlog_summary should succeed");

    assert!(
        result.contains("Total events:"),
        "summary should include total event count"
    );
}

#[test]
fn summary_includes_projects_section() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_summary(&args, &mut sessions).expect("binlog_summary should succeed");

    assert!(
        result.contains("Projects ("),
        "summary should list projects"
    );
}

#[test]
fn summary_includes_duration() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_summary(&args, &mut sessions).expect("binlog_summary should succeed");

    assert!(
        result.contains("Duration:"),
        "summary should include build duration"
    );
}

#[test]
fn summary_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999 });
    let result = call_binlog_summary(&args, &mut sessions);
    assert!(result.is_err());
}

// ── binlog_errors tests ───────────────────────────────────────────────────────

#[test]
fn errors_on_clean_build_returns_no_errors() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_errors(&args, &mut sessions).expect("binlog_errors should succeed");

    assert_eq!(result, "No errors found.");
}

#[test]
fn errors_with_limit_on_clean_build() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "limit": 5 });
    let result = call_binlog_errors(&args, &mut sessions).expect("binlog_errors should succeed");

    assert_eq!(result, "No errors found.");
}

#[test]
fn errors_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999 });
    let result = call_binlog_errors(&args, &mut sessions);
    assert!(result.is_err());
}

// ── binlog_warnings tests ─────────────────────────────────────────────────────

#[test]
fn warnings_on_clean_build_returns_no_warnings() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result =
        call_binlog_warnings(&args, &mut sessions).expect("binlog_warnings should succeed");

    assert_eq!(result, "No warnings found.");
}

#[test]
fn warnings_with_code_filter_on_clean_build() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "code": "CS0168" });
    let result =
        call_binlog_warnings(&args, &mut sessions).expect("binlog_warnings should succeed");

    assert_eq!(result, "No warnings found.");
}

#[test]
fn warnings_with_project_filter_on_clean_build() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "project": "hello" });
    let result =
        call_binlog_warnings(&args, &mut sessions).expect("binlog_warnings should succeed");

    assert_eq!(result, "No warnings found.");
}

#[test]
fn warnings_with_limit_on_clean_build() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "limit": 3 });
    let result =
        call_binlog_warnings(&args, &mut sessions).expect("binlog_warnings should succeed");

    assert_eq!(result, "No warnings found.");
}

#[test]
fn warnings_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999 });
    let result = call_binlog_warnings(&args, &mut sessions);
    assert!(result.is_err());
}

// ── tool dispatch tests ───────────────────────────────────────────────────────

#[test]
fn call_routes_to_binlog_summary() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call("binlog_summary", &args, &mut sessions);
    assert!(result.is_ok());
}

#[test]
fn call_routes_to_binlog_errors() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call("binlog_errors", &args, &mut sessions);
    assert!(result.is_ok());
}

#[test]
fn call_routes_to_binlog_warnings() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call("binlog_warnings", &args, &mut sessions);
    assert!(result.is_ok());
}

#[test]
fn call_unknown_tool_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({});
    let result = call("nonexistent_tool", &args, &mut sessions);
    assert!(result.is_err());
}

// ── tool list tests ───────────────────────────────────────────────────────────

#[test]
fn list_includes_all_fourteen_tools() {
    let tools = list();
    let arr = tools.as_array().expect("list() should return an array");

    let names: Vec<&str> = arr
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(names.contains(&"binlog_open"), "missing binlog_open");
    assert!(names.contains(&"binlog_close"), "missing binlog_close");
    assert!(names.contains(&"binlog_summary"), "missing binlog_summary");
    assert!(names.contains(&"binlog_errors"), "missing binlog_errors");
    assert!(
        names.contains(&"binlog_warnings"),
        "missing binlog_warnings"
    );
    assert!(
        names.contains(&"binlog_project_tree"),
        "missing binlog_project_tree"
    );
    assert!(names.contains(&"binlog_events"), "missing binlog_events");
    assert!(
        names.contains(&"binlog_event_detail"),
        "missing binlog_event_detail"
    );
    assert!(
        names.contains(&"binlog_properties"),
        "missing binlog_properties"
    );
    assert!(names.contains(&"binlog_items"), "missing binlog_items");
    assert!(
        names.contains(&"binlog_error_context"),
        "missing binlog_error_context"
    );
    assert!(
        names.contains(&"binlog_task_timeline"),
        "missing binlog_task_timeline"
    );
    assert!(
        names.contains(&"binlog_feedback"),
        "missing binlog_feedback"
    );
    assert!(names.contains(&"binlog_setup"), "missing binlog_setup");
    assert_eq!(names.len(), 14, "should have exactly 14 tools");
}

#[test]
fn list_tools_have_descriptions() {
    let tools = list();
    let arr = tools.as_array().unwrap();
    for tool in arr {
        let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        assert!(
            tool.get("description")
                .and_then(|d| d.as_str())
                .is_some_and(|d| !d.is_empty()),
            "tool '{name}' should have a non-empty description"
        );
    }
}

#[test]
fn list_tools_have_input_schemas() {
    let tools = list();
    let arr = tools.as_array().unwrap();
    for tool in arr {
        let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        assert!(
            tool.get("inputSchema").is_some(),
            "tool '{name}' should have an inputSchema"
        );
    }
}

// ── binlog_project_tree tests ─────────────────────────────────────────────────

#[test]
fn project_tree_shows_projects() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result =
        call_binlog_project_tree(&args, &mut sessions).expect("project_tree should succeed");

    assert!(
        result.contains("Project tree"),
        "should show project tree header"
    );
    assert!(
        result.contains("Project:"),
        "should list at least one project"
    );
}

#[test]
fn project_tree_shows_targets() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result =
        call_binlog_project_tree(&args, &mut sessions).expect("project_tree should succeed");

    assert!(
        result.contains("Target:"),
        "should list targets within projects"
    );
}

#[test]
fn project_tree_shows_tasks() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result =
        call_binlog_project_tree(&args, &mut sessions).expect("project_tree should succeed");

    assert!(result.contains("Task:"), "should list tasks within targets");
}

#[test]
fn project_tree_shows_project_id() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result =
        call_binlog_project_tree(&args, &mut sessions).expect("project_tree should succeed");

    assert!(result.contains("Project ID:"), "should show project IDs");
}

#[test]
fn project_tree_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999 });
    let result = call_binlog_project_tree(&args, &mut sessions);
    assert!(result.is_err());
}

// ── binlog_events tests ───────────────────────────────────────────────────────

#[test]
fn events_no_filter_returns_all() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_events(&args, &mut sessions).expect("binlog_events should succeed");

    assert!(result.contains("Events:"), "should show event count header");
    assert!(result.contains("[0]"), "should include event at index 0");
}

#[test]
fn events_filter_by_kind() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "kind": "ProjectStarted" });
    let result = call_binlog_events(&args, &mut sessions).expect("binlog_events should succeed");

    assert!(
        result.contains("ProjectStarted"),
        "filtered events should be ProjectStarted"
    );
    for line in result.lines().skip(2) {
        if line.starts_with('[') {
            assert!(
                line.contains("ProjectStarted"),
                "all events should be ProjectStarted, got: {line}"
            );
        }
    }
}

#[test]
fn events_filter_by_kind_case_insensitive() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "kind": "projectstarted" });
    let result = call_binlog_events(&args, &mut sessions).expect("binlog_events should succeed");

    assert!(
        result.contains("ProjectStarted"),
        "case-insensitive kind filter should work"
    );
}

#[test]
fn events_with_limit() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "limit": 3 });
    let result = call_binlog_events(&args, &mut sessions).expect("binlog_events should succeed");

    let event_lines: Vec<&str> = result.lines().filter(|l| l.starts_with('[')).collect();
    assert_eq!(
        event_lines.len(),
        3,
        "should return exactly 3 events when limit=3"
    );
}

#[test]
fn events_text_filter_nonmatching() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "text": "ZZZZNONEXISTENT_TEXT_ZZZZ" });
    let result = call_binlog_events(&args, &mut sessions).expect("binlog_events should succeed");

    assert_eq!(result, "No events matched the filter.");
}

#[test]
fn events_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999 });
    let result = call_binlog_events(&args, &mut sessions);
    assert!(result.is_err());
}

// ── binlog_event_detail tests ─────────────────────────────────────────────────

#[test]
fn event_detail_index_zero() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "index": 0 });
    let result =
        call_binlog_event_detail(&args, &mut sessions).expect("event_detail should succeed");

    assert!(result.contains("Event index: 0"));
    assert!(result.contains("Kind:"));
    assert!(result.contains("Type:"));
}

#[test]
fn event_detail_shows_context() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "kind": "ProjectStarted" });
    let events_result =
        call_binlog_events(&args, &mut sessions).expect("binlog_events should succeed");

    let first_event_line = events_result
        .lines()
        .find(|l| l.starts_with('['))
        .expect("should have at least one event");
    let idx_str = &first_event_line[1..first_event_line.find(']').unwrap()];
    let idx: u64 = idx_str.parse().unwrap();

    let args = json!({ "session": handle, "index": idx });
    let result =
        call_binlog_event_detail(&args, &mut sessions).expect("event_detail should succeed");

    assert!(
        result.contains("Type: ProjectStarted"),
        "should identify event type"
    );
    assert!(
        result.contains("Project file:"),
        "ProjectStarted should show project file"
    );
}

#[test]
fn event_detail_out_of_range_returns_error() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "index": 999999 });
    let result = call_binlog_event_detail(&args, &mut sessions);
    assert!(result.is_err());
}

#[test]
fn event_detail_missing_index_returns_error() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_event_detail(&args, &mut sessions);
    assert!(result.is_err());
}

#[test]
fn event_detail_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999, "index": 0 });
    let result = call_binlog_event_detail(&args, &mut sessions);
    assert!(result.is_err());
}

// ── binlog_properties tests ───────────────────────────────────────────────────

#[test]
fn properties_returns_project_properties() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result =
        call_binlog_properties(&args, &mut sessions).expect("binlog_properties should succeed");

    assert!(
        result.contains("Project:") || result.contains("No properties"),
        "should either list properties or report none"
    );
}

#[test]
fn properties_name_filter_nonmatching() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "name": "ZZZZNONEXISTENT_ZZZZ" });
    let result =
        call_binlog_properties(&args, &mut sessions).expect("binlog_properties should succeed");

    assert!(
        result.contains("No properties found"),
        "non-matching name filter should return no properties"
    );
}

#[test]
fn properties_project_filter_nonmatching() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "project": "ZZZZNONEXISTENT_ZZZZ" });
    let result =
        call_binlog_properties(&args, &mut sessions).expect("binlog_properties should succeed");

    assert!(
        result.contains("No properties found"),
        "non-matching project filter should return no properties"
    );
}

#[test]
fn properties_with_limit() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "limit": 2 });
    let result =
        call_binlog_properties(&args, &mut sessions).expect("binlog_properties should succeed");

    if result.contains("Project:") {
        let prop_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.starts_with("  ") && l.contains(" = "))
            .collect();
        assert!(
            prop_lines.len() <= 2 || result.contains("truncated"),
            "limit should truncate properties"
        );
    }
}

#[test]
fn properties_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999 });
    let result = call_binlog_properties(&args, &mut sessions);
    assert!(result.is_err());
}

// ── binlog_items tests ────────────────────────────────────────────────────────

#[test]
fn items_returns_item_groups() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_items(&args, &mut sessions).expect("binlog_items should succeed");

    assert!(
        result.contains("Project:") || result.contains("No items"),
        "should either list items or report none"
    );
}

#[test]
fn items_type_filter_nonmatching() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "item_type": "ZZZZNONEXISTENT_ZZZZ" });
    let result = call_binlog_items(&args, &mut sessions).expect("binlog_items should succeed");

    assert!(
        result.contains("No items found"),
        "non-matching item_type filter should return no items"
    );
}

#[test]
fn items_project_filter_nonmatching() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "project": "ZZZZNONEXISTENT_ZZZZ" });
    let result = call_binlog_items(&args, &mut sessions).expect("binlog_items should succeed");

    assert!(
        result.contains("No items found"),
        "non-matching project filter should return no items"
    );
}

#[test]
fn items_spec_filter_nonmatching() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "spec": "ZZZZNONEXISTENT_ZZZZ" });
    let result = call_binlog_items(&args, &mut sessions).expect("binlog_items should succeed");

    assert!(
        result.contains("No items found"),
        "non-matching spec filter should return no items"
    );
}

#[test]
fn items_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999 });
    let result = call_binlog_items(&args, &mut sessions);
    assert!(result.is_err());
}

// ── dispatch routing tests for M3 tools ───────────────────────────────────────

#[test]
fn call_routes_to_binlog_project_tree() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call("binlog_project_tree", &args, &mut sessions);
    assert!(result.is_ok());
}

#[test]
fn call_routes_to_binlog_events() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call("binlog_events", &args, &mut sessions);
    assert!(result.is_ok());
}

#[test]
fn call_routes_to_binlog_event_detail() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "index": 0 });
    let result = call("binlog_event_detail", &args, &mut sessions);
    assert!(result.is_ok());
}

#[test]
fn call_routes_to_binlog_properties() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call("binlog_properties", &args, &mut sessions);
    assert!(result.is_ok());
}

#[test]
fn call_routes_to_binlog_items() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call("binlog_items", &args, &mut sessions);
    assert!(result.is_ok());
}

// ── binlog_error_context tests ────────────────────────────────────────────────

#[test]
fn error_context_on_event_zero() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    // Event 0 is BuildStarted — not an error, but the tool shows context for any event.
    let args = json!({ "session": handle, "index": 0 });
    let result =
        call_binlog_error_context(&args, &mut sessions).expect("error_context should succeed");

    assert!(
        result.contains("Context scope:"),
        "should report context scope"
    );
    assert!(
        result.contains("[0]"),
        "should include the target event in output"
    );
}

#[test]
fn error_context_marks_target_event() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "index": 0 });
    let result =
        call_binlog_error_context(&args, &mut sessions).expect("error_context should succeed");

    // The target event line should be marked with ">>>"
    let marked_lines: Vec<&str> = result.lines().filter(|l| l.starts_with(">>>")).collect();
    assert_eq!(
        marked_lines.len(),
        1,
        "exactly one event should be marked with >>>"
    );
    assert!(
        marked_lines[0].contains("[0]"),
        "marked line should be the target event"
    );
}

#[test]
fn error_context_with_project_started_event() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    // Find a ProjectStarted event index.
    let args = json!({ "session": handle, "kind": "ProjectStarted" });
    let events_result =
        call_binlog_events(&args, &mut sessions).expect("binlog_events should succeed");

    let first_event_line = events_result
        .lines()
        .find(|l| l.starts_with('['))
        .expect("should have at least one ProjectStarted event");
    let idx_str = &first_event_line[1..first_event_line.find(']').unwrap()];
    let idx: u64 = idx_str.parse().unwrap();

    let args = json!({ "session": handle, "index": idx });
    let result =
        call_binlog_error_context(&args, &mut sessions).expect("error_context should succeed");

    assert!(
        result.contains("Context scope:"),
        "should report context scope"
    );
    assert!(
        result.contains(&format!("[{idx}]")),
        "should include the target event"
    );
}

#[test]
fn error_context_with_custom_radius() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "index": 0, "radius": 2 });
    let result =
        call_binlog_error_context(&args, &mut sessions).expect("error_context should succeed");

    // With radius=2 from index 0, should show at most 3 events (0 + 2 after).
    let event_lines: Vec<&str> = result
        .lines()
        .filter(|l| l.contains("] ") && (l.starts_with(">>>") || l.starts_with("   ")))
        .collect();
    assert!(
        event_lines.len() <= 5,
        "radius=2 should show at most 5 events"
    );
}

#[test]
fn error_context_out_of_range_returns_error() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "index": 999999 });
    let result = call_binlog_error_context(&args, &mut sessions);
    assert!(result.is_err());
}

#[test]
fn error_context_missing_index_returns_error() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call_binlog_error_context(&args, &mut sessions);
    assert!(result.is_err());
}

#[test]
fn error_context_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999, "index": 0 });
    let result = call_binlog_error_context(&args, &mut sessions);
    assert!(result.is_err());
}

// ── binlog_task_timeline tests ────────────────────────────────────────────────

#[test]
fn task_timeline_shows_projects() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result =
        call_binlog_task_timeline(&args, &mut sessions).expect("task_timeline should succeed");

    assert!(
        result.contains("Project:"),
        "should list at least one project"
    );
}

#[test]
fn task_timeline_shows_targets_with_status() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result =
        call_binlog_task_timeline(&args, &mut sessions).expect("task_timeline should succeed");

    assert!(result.contains("Target:"), "should list targets");
    // Targets should have status indicators.
    assert!(
        result.contains("[ok]") || result.contains("[FAILED]") || result.contains("[?]"),
        "targets should have status indicators"
    );
}

#[test]
fn task_timeline_shows_tasks_with_status() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result =
        call_binlog_task_timeline(&args, &mut sessions).expect("task_timeline should succeed");

    assert!(result.contains("Task:"), "should list tasks");
}

#[test]
fn task_timeline_successful_build_has_ok_status() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result =
        call_binlog_task_timeline(&args, &mut sessions).expect("task_timeline should succeed");

    // hello.binlog is a successful build; all targets/tasks should be ok.
    assert!(
        result.contains("[ok]"),
        "successful build should have ok status"
    );
    assert!(
        !result.contains("[FAILED]"),
        "successful build should have no FAILED status"
    );
}

#[test]
fn task_timeline_project_filter() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "project": "ZZZZNONEXISTENT_ZZZZ" });
    let result =
        call_binlog_task_timeline(&args, &mut sessions).expect("task_timeline should succeed");

    assert_eq!(
        result, "No projects matched the filter.",
        "non-matching project filter should return no projects"
    );
}

#[test]
fn task_timeline_project_filter_matching() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    // hello.binlog should have a .csproj file.
    let args = json!({ "session": handle, "project": ".csproj" });
    let result =
        call_binlog_task_timeline(&args, &mut sessions).expect("task_timeline should succeed");

    assert!(
        result.contains("Project:"),
        "matching filter should show projects"
    );
}

#[test]
fn task_timeline_with_project_context_id() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);

    // Get the project tree to find a context ID.
    let tree_args = json!({ "session": handle });
    let tree_result =
        call_binlog_project_tree(&tree_args, &mut sessions).expect("project_tree should succeed");

    // Extract a project_context_id from the tree output.
    let ctx_line = tree_result
        .lines()
        .find(|l| l.contains("project_context_id="))
        .expect("should have context info");
    let ctx_id_str = ctx_line.split("project_context_id=").nth(1).unwrap().trim();
    let ctx_id: i64 = ctx_id_str.parse().unwrap();

    let args = json!({ "session": handle, "project_context_id": ctx_id });
    let result =
        call_binlog_task_timeline(&args, &mut sessions).expect("task_timeline should succeed");

    assert!(
        result.contains("Project:"),
        "should show the specific project"
    );
}

#[test]
fn task_timeline_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({ "session": 99999 });
    let result = call_binlog_task_timeline(&args, &mut sessions);
    assert!(result.is_err());
}

// ── binlog_feedback tests ─────────────────────────────────────────────────────

#[test]
fn feedback_appends_to_file() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);

    let dir = std::env::temp_dir().join("munin-binlog-mcp-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let feedback_path = dir.join("test_feedback.jsonl");
    // Clean up any previous run.
    let _ = std::fs::remove_file(&feedback_path);

    let path_str = feedback_path.to_string_lossy().to_string();
    let args = json!({
        "session": handle,
        "file": path_str,
        "text": "Build failed due to missing NuGet package."
    });
    let result = call_binlog_feedback(&args, &mut sessions).expect("feedback should succeed");

    assert!(
        result.contains("Feedback appended"),
        "should confirm append"
    );

    // Read back and verify.
    let contents = std::fs::read_to_string(&feedback_path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 1, "should have exactly one line");

    let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(
        parsed["text"].as_str().unwrap(),
        "Build failed due to missing NuGet package."
    );
    assert_eq!(parsed["session"].as_u64().unwrap(), handle);
    assert!(
        parsed["binlog"].as_str().is_some(),
        "should include binlog path"
    );

    // Clean up.
    let _ = std::fs::remove_file(&feedback_path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn feedback_appends_multiple_entries() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);

    let dir = std::env::temp_dir().join("munin-binlog-mcp-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let feedback_path = dir.join("test_feedback_multi.jsonl");
    let _ = std::fs::remove_file(&feedback_path);

    let path_str = feedback_path.to_string_lossy().to_string();

    let args1 = json!({
        "session": handle,
        "file": path_str,
        "text": "First entry."
    });
    call_binlog_feedback(&args1, &mut sessions).expect("first feedback should succeed");

    let args2 = json!({
        "session": handle,
        "file": path_str,
        "text": "Second entry.",
        "root_cause": "Missing dependency"
    });
    call_binlog_feedback(&args2, &mut sessions).expect("second feedback should succeed");

    let contents = std::fs::read_to_string(&feedback_path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 2, "should have exactly two lines");

    let p1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(p1["text"].as_str().unwrap(), "First entry.");

    let p2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(p2["text"].as_str().unwrap(), "Second entry.");
    assert_eq!(p2["root_cause"].as_str().unwrap(), "Missing dependency");

    let _ = std::fs::remove_file(&feedback_path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn feedback_with_event_indices() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);

    let dir = std::env::temp_dir().join("munin-binlog-mcp-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let feedback_path = dir.join("test_feedback_indices.jsonl");
    let _ = std::fs::remove_file(&feedback_path);

    let path_str = feedback_path.to_string_lossy().to_string();
    let args = json!({
        "session": handle,
        "file": path_str,
        "text": "These events are relevant.",
        "event_indices": [0, 5, 10]
    });
    call_binlog_feedback(&args, &mut sessions).expect("feedback should succeed");

    let contents = std::fs::read_to_string(&feedback_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(contents.lines().next().unwrap()).unwrap();
    let indices = parsed["event_indices"].as_array().unwrap();
    assert_eq!(indices.len(), 3);
    assert_eq!(indices[0].as_i64().unwrap(), 0);
    assert_eq!(indices[1].as_i64().unwrap(), 5);
    assert_eq!(indices[2].as_i64().unwrap(), 10);

    let _ = std::fs::remove_file(&feedback_path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn feedback_invalid_handle_returns_error() {
    let mut sessions = SessionMap::new();
    let args = json!({
        "session": 99999,
        "file": "test.jsonl",
        "text": "should fail"
    });
    let result = call_binlog_feedback(&args, &mut sessions);
    assert!(result.is_err());
}

#[test]
fn feedback_missing_text_returns_error() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({
        "session": handle,
        "file": "test.jsonl"
    });
    let result = call_binlog_feedback(&args, &mut sessions);
    assert!(result.is_err());
}

#[test]
fn feedback_missing_file_returns_error() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({
        "session": handle,
        "text": "should fail"
    });
    let result = call_binlog_feedback(&args, &mut sessions);
    assert!(result.is_err());
}

// ── dispatch routing tests for M4 tools ───────────────────────────────────────

#[test]
fn call_routes_to_binlog_error_context() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle, "index": 0 });
    let result = call("binlog_error_context", &args, &mut sessions);
    assert!(result.is_ok());
}

#[test]
fn call_routes_to_binlog_task_timeline() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);
    let args = json!({ "session": handle });
    let result = call("binlog_task_timeline", &args, &mut sessions);
    assert!(result.is_ok());
}

#[test]
fn call_routes_to_binlog_feedback() {
    let mut sessions = SessionMap::new();
    let handle = open_hello(&mut sessions);

    let dir = std::env::temp_dir().join("munin-binlog-mcp-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let feedback_path = dir.join("test_dispatch_feedback.jsonl");
    let _ = std::fs::remove_file(&feedback_path);

    let path_str = feedback_path.to_string_lossy().to_string();
    let args = json!({
        "session": handle,
        "file": path_str,
        "text": "dispatch test"
    });
    let result = call("binlog_feedback", &args, &mut sessions);
    assert!(result.is_ok());

    let _ = std::fs::remove_file(&feedback_path);
    let _ = std::fs::remove_dir(&dir);
}
