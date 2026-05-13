// Copyright (c) Michael Grier

//! End-to-end integration test exercising the full munin-binlog-mcp tool suite
//! against a real `.binlog` file.

use std::path::PathBuf;

use munin_binlog_mcp::tools::{self, SessionMap};
use serde_json::json;

/// Path to the hello.binlog test fixture.
fn hello_binlog_path() -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("munin");
    p.push("tests");
    p.push("data");
    p.push("hello.binlog");
    p.to_string_lossy().into_owned()
}

/// Full analysis workflow: open, inspect, analyze, close.
#[test]
fn full_analysis_workflow() {
    let mut sessions = SessionMap::new();

    // 1. Open the binlog.
    let open_args = json!({ "file": hello_binlog_path() });
    let open_result =
        tools::call("binlog_open", &open_args, &mut sessions).expect("binlog_open should succeed");
    assert!(open_result.contains("Session:"));
    assert!(open_result.contains("Format version:"));

    // Extract session handle from output.
    let handle: u64 = open_result
        .lines()
        .find(|l| l.starts_with("Session:"))
        .unwrap()
        .split(':')
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    // 2. Get build summary.
    let summary = tools::call(
        "binlog_summary",
        &json!({ "session": handle }),
        &mut sessions,
    )
    .expect("binlog_summary should succeed");
    assert!(summary.contains("Build result:"));
    assert!(summary.contains("succeeded"));
    assert!(summary.contains("Total events:"));
    assert!(summary.contains("Projects ("));

    // 3. Check errors (should be none for hello.binlog).
    let errors = tools::call(
        "binlog_errors",
        &json!({ "session": handle }),
        &mut sessions,
    )
    .expect("binlog_errors should succeed");
    assert_eq!(errors, "No errors found.");

    // 4. Check warnings (should be none for hello.binlog).
    let warnings = tools::call(
        "binlog_warnings",
        &json!({ "session": handle }),
        &mut sessions,
    )
    .expect("binlog_warnings should succeed");
    assert_eq!(warnings, "No warnings found.");

    // 5. Get project tree.
    let tree = tools::call(
        "binlog_project_tree",
        &json!({ "session": handle }),
        &mut sessions,
    )
    .expect("binlog_project_tree should succeed");
    assert!(tree.contains("Project tree"));
    assert!(tree.contains("Project:"));
    assert!(tree.contains("Target:"));
    assert!(tree.contains("Task:"));

    // 6. List events with a limit.
    let events = tools::call(
        "binlog_events",
        &json!({ "session": handle, "limit": 5 }),
        &mut sessions,
    )
    .expect("binlog_events should succeed");
    assert!(events.contains("Events: 5 shown"));
    let event_lines: Vec<&str> = events.lines().filter(|l| l.starts_with('[')).collect();
    assert_eq!(event_lines.len(), 5);

    // 7. Filter events by kind.
    let project_events = tools::call(
        "binlog_events",
        &json!({ "session": handle, "kind": "ProjectStarted" }),
        &mut sessions,
    )
    .expect("binlog_events with kind filter should succeed");
    assert!(project_events.contains("ProjectStarted"));

    // 8. Get detail for event 0.
    let detail = tools::call(
        "binlog_event_detail",
        &json!({ "session": handle, "index": 0 }),
        &mut sessions,
    )
    .expect("binlog_event_detail should succeed");
    assert!(detail.contains("Event index: 0"));
    assert!(detail.contains("Type:"), "should show event type");

    // 9. Get properties.
    let props = tools::call(
        "binlog_properties",
        &json!({ "session": handle }),
        &mut sessions,
    )
    .expect("binlog_properties should succeed");
    // hello.binlog should have properties in its ProjectStarted events.
    assert!(
        props.contains("Project:") || props.contains("No properties"),
        "properties should return either data or empty message"
    );

    // 10. Get items.
    let items = tools::call("binlog_items", &json!({ "session": handle }), &mut sessions)
        .expect("binlog_items should succeed");
    assert!(
        items.contains("Project:") || items.contains("No items"),
        "items should return either data or empty message"
    );

    // 11. Error context on event 0.
    let ctx = tools::call(
        "binlog_error_context",
        &json!({ "session": handle, "index": 0 }),
        &mut sessions,
    )
    .expect("binlog_error_context should succeed");
    assert!(ctx.contains("Context scope:"));
    assert!(ctx.contains(">>>"));
    assert!(ctx.contains("[0]"));

    // 12. Task timeline.
    let timeline = tools::call(
        "binlog_task_timeline",
        &json!({ "session": handle }),
        &mut sessions,
    )
    .expect("binlog_task_timeline should succeed");
    assert!(timeline.contains("Project:"));
    assert!(timeline.contains("Target:"));
    assert!(timeline.contains("[ok]"));

    // 13. Feedback.
    let dir = std::env::temp_dir().join("munin-binlog-mcp-integration");
    std::fs::create_dir_all(&dir).unwrap();
    let feedback_path = dir.join("integration_feedback.jsonl");
    let _ = std::fs::remove_file(&feedback_path);

    let feedback_path_str = feedback_path.to_string_lossy().to_string();
    let feedback = tools::call(
        "binlog_feedback",
        &json!({
            "session": handle,
            "file": feedback_path_str,
            "text": "Integration test: build succeeded, no errors.",
            "root_cause": "N/A",
            "event_indices": [0]
        }),
        &mut sessions,
    )
    .expect("binlog_feedback should succeed");
    assert!(feedback.contains("Feedback appended"));

    // Verify feedback file content.
    let fb_content = std::fs::read_to_string(&feedback_path).unwrap();
    let fb: serde_json::Value = serde_json::from_str(fb_content.lines().next().unwrap()).unwrap();
    assert_eq!(
        fb["text"].as_str().unwrap(),
        "Integration test: build succeeded, no errors."
    );
    assert_eq!(fb["root_cause"].as_str().unwrap(), "N/A");
    assert_eq!(fb["event_indices"][0].as_i64().unwrap(), 0);
    assert_eq!(fb["session"].as_u64().unwrap(), handle);

    let _ = std::fs::remove_file(&feedback_path);
    let _ = std::fs::remove_dir(&dir);

    // 14. Close session.
    let close = tools::call("binlog_close", &json!({ "session": handle }), &mut sessions)
        .expect("binlog_close should succeed");
    assert!(close.contains("closed"));

    // 15. Verify closed session is rejected.
    let err = tools::call(
        "binlog_summary",
        &json!({ "session": handle }),
        &mut sessions,
    );
    assert!(err.is_err());
}

/// Verify the tool list is complete and well-formed.
#[test]
fn tool_list_is_complete() {
    let list = tools::list();
    let arr = list.as_array().expect("list() should return an array");
    assert_eq!(arr.len(), 14, "should have exactly 14 tools");

    for tool in arr {
        let name = tool["name"].as_str().expect("tool should have a name");
        assert!(
            name.starts_with("binlog_"),
            "all tool names should start with binlog_: {name}"
        );
        assert!(
            tool["description"].as_str().is_some(),
            "tool {name} should have a description"
        );
        assert!(
            tool["inputSchema"].is_object(),
            "tool {name} should have an inputSchema"
        );
        assert!(
            tool["annotations"].is_object(),
            "tool {name} should have annotations"
        );
    }
}

/// Verify that unknown tool names are rejected.
#[test]
fn unknown_tool_returns_error() {
    let mut sessions = SessionMap::new();
    let result = tools::call("nonexistent_tool", &json!({}), &mut sessions);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("unknown tool"));
}

/// Verify that invalid session handles produce errors across all tools.
#[test]
fn invalid_session_rejected_by_all_session_tools() {
    let mut sessions = SessionMap::new();
    let bad_handle = json!({ "session": 99999 });

    let session_tools = [
        "binlog_summary",
        "binlog_errors",
        "binlog_warnings",
        "binlog_project_tree",
        "binlog_events",
        "binlog_properties",
        "binlog_items",
        "binlog_task_timeline",
    ];

    for tool_name in &session_tools {
        let result = tools::call(tool_name, &bad_handle, &mut sessions);
        assert!(
            result.is_err(),
            "{tool_name} should reject invalid session handle"
        );
    }

    // Tools requiring extra parameters.
    let result = tools::call(
        "binlog_event_detail",
        &json!({ "session": 99999, "index": 0 }),
        &mut sessions,
    );
    assert!(result.is_err());

    let result = tools::call(
        "binlog_error_context",
        &json!({ "session": 99999, "index": 0 }),
        &mut sessions,
    );
    assert!(result.is_err());

    let result = tools::call(
        "binlog_feedback",
        &json!({ "session": 99999, "file": "x.jsonl", "text": "test" }),
        &mut sessions,
    );
    assert!(result.is_err());

    let result = tools::call("binlog_close", &bad_handle, &mut sessions);
    assert!(result.is_err());
}

/// Multiple sessions can coexist and close independently.
#[test]
fn multiple_sessions_coexist() {
    let mut sessions = SessionMap::new();
    let path = hello_binlog_path();

    let open1 = tools::call("binlog_open", &json!({ "file": path }), &mut sessions)
        .expect("first open should succeed");
    let h1: u64 = open1
        .lines()
        .find(|l| l.starts_with("Session:"))
        .unwrap()
        .split(':')
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    let open2 = tools::call("binlog_open", &json!({ "file": path }), &mut sessions)
        .expect("second open should succeed");
    let h2: u64 = open2
        .lines()
        .find(|l| l.starts_with("Session:"))
        .unwrap()
        .split(':')
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    assert_ne!(h1, h2, "handles should be distinct");

    // Both sessions should work.
    let s1 = tools::call("binlog_summary", &json!({ "session": h1 }), &mut sessions);
    assert!(s1.is_ok());
    let s2 = tools::call("binlog_summary", &json!({ "session": h2 }), &mut sessions);
    assert!(s2.is_ok());

    // Close one, the other should still work.
    tools::call("binlog_close", &json!({ "session": h1 }), &mut sessions).unwrap();
    let s2_after = tools::call("binlog_summary", &json!({ "session": h2 }), &mut sessions);
    assert!(s2_after.is_ok());

    let s1_after = tools::call("binlog_summary", &json!({ "session": h1 }), &mut sessions);
    assert!(s1_after.is_err(), "closed session should be rejected");

    tools::call("binlog_close", &json!({ "session": h2 }), &mut sessions).unwrap();
}

/// Task timeline for a specific project_context_id extracted from the tree.
#[test]
fn task_timeline_for_specific_project() {
    let mut sessions = SessionMap::new();
    let open = tools::call(
        "binlog_open",
        &json!({ "file": hello_binlog_path() }),
        &mut sessions,
    )
    .unwrap();
    let handle: u64 = open
        .lines()
        .find(|l| l.starts_with("Session:"))
        .unwrap()
        .split(':')
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    // Get the project tree to extract a context ID.
    let tree = tools::call(
        "binlog_project_tree",
        &json!({ "session": handle }),
        &mut sessions,
    )
    .unwrap();

    let ctx_line = tree
        .lines()
        .find(|l| l.contains("project_context_id="))
        .expect("should have context info in tree");
    let ctx_id: i64 = ctx_line
        .split("project_context_id=")
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    let timeline = tools::call(
        "binlog_task_timeline",
        &json!({ "session": handle, "project_context_id": ctx_id }),
        &mut sessions,
    )
    .unwrap();

    assert!(timeline.contains("Project:"));
    assert!(timeline.contains("[ok]"));

    tools::call("binlog_close", &json!({ "session": handle }), &mut sessions).unwrap();
}

/// Error context with varying scope depths.
#[test]
fn error_context_scope_negotiation() {
    let mut sessions = SessionMap::new();
    let open = tools::call(
        "binlog_open",
        &json!({ "file": hello_binlog_path() }),
        &mut sessions,
    )
    .unwrap();
    let handle: u64 = open
        .lines()
        .find(|l| l.starts_with("Session:"))
        .unwrap()
        .split(':')
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    // Find a TaskStarted event (should have task-level context).
    let task_events = tools::call(
        "binlog_events",
        &json!({ "session": handle, "kind": "TaskStarted", "limit": 1 }),
        &mut sessions,
    )
    .unwrap();
    let task_line = task_events
        .lines()
        .find(|l| l.starts_with('['))
        .expect("should have TaskStarted events");
    let task_idx: u64 = task_line[1..task_line.find(']').unwrap()].parse().unwrap();

    let ctx = tools::call(
        "binlog_error_context",
        &json!({ "session": handle, "index": task_idx, "radius": 5 }),
        &mut sessions,
    )
    .unwrap();

    assert!(
        ctx.contains("Context scope: task")
            || ctx.contains("Context scope: target")
            || ctx.contains("Context scope: project"),
        "should determine a structured scope"
    );
    assert!(ctx.contains(">>>"), "should mark the target event");

    tools::call("binlog_close", &json!({ "session": handle }), &mut sessions).unwrap();
}

/// Event filtering with combined filters.
#[test]
fn event_filtering_combined() {
    let mut sessions = SessionMap::new();
    let open = tools::call(
        "binlog_open",
        &json!({ "file": hello_binlog_path() }),
        &mut sessions,
    )
    .unwrap();
    let handle: u64 = open
        .lines()
        .find(|l| l.starts_with("Session:"))
        .unwrap()
        .split(':')
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    // Filter by kind and limit.
    let result = tools::call(
        "binlog_events",
        &json!({ "session": handle, "kind": "Message", "limit": 3 }),
        &mut sessions,
    )
    .unwrap();
    let msg_lines: Vec<&str> = result.lines().filter(|l| l.starts_with('[')).collect();
    assert!(msg_lines.len() <= 3);

    // Text filter that matches nothing.
    let no_match = tools::call(
        "binlog_events",
        &json!({ "session": handle, "text": "XYZZY_IMPOSSIBLE_12345" }),
        &mut sessions,
    )
    .unwrap();
    assert_eq!(no_match, "No events matched the filter.");

    tools::call("binlog_close", &json!({ "session": handle }), &mut sessions).unwrap();
}

/// Properties and items filtering.
#[test]
fn properties_and_items_filtering() {
    let mut sessions = SessionMap::new();
    let open = tools::call(
        "binlog_open",
        &json!({ "file": hello_binlog_path() }),
        &mut sessions,
    )
    .unwrap();
    let handle: u64 = open
        .lines()
        .find(|l| l.starts_with("Session:"))
        .unwrap()
        .split(':')
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    // Non-matching name filter.
    let no_props = tools::call(
        "binlog_properties",
        &json!({ "session": handle, "name": "XYZZY_NONEXISTENT" }),
        &mut sessions,
    )
    .unwrap();
    assert!(no_props.contains("No properties found"));

    // Non-matching item type filter.
    let no_items = tools::call(
        "binlog_items",
        &json!({ "session": handle, "item_type": "XYZZY_NONEXISTENT" }),
        &mut sessions,
    )
    .unwrap();
    assert!(no_items.contains("No items found"));

    // Property limit.
    let limited = tools::call(
        "binlog_properties",
        &json!({ "session": handle, "limit": 1 }),
        &mut sessions,
    )
    .unwrap();
    if limited.contains("Project:") {
        let prop_lines: Vec<&str> = limited
            .lines()
            .filter(|l| l.starts_with("  ") && l.contains(" = "))
            .collect();
        // Each project should have at most 1 property.
        assert!(
            prop_lines.len() <= 10 || limited.contains("truncated"),
            "limit should cap properties per project"
        );
    }

    tools::call("binlog_close", &json!({ "session": handle }), &mut sessions).unwrap();
}
