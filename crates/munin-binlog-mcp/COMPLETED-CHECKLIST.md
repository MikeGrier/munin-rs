<!-- Copyright (c) Michael Grier -->
# munin-binlog-mcp — Completed Checklist

## Moved 2026-08-09 — All milestones complete (M1–M5, MBM-1 through MBM-21)

### M1: Foundation — crate scaffold, MCP server loop, binlog_open (5 items)

- [x] MBM-1: Create Cargo.toml (bin crate, deps: munin, serde, serde_json)
- [x] MBM-2: Implement JSON-RPC 2.0 stdio event loop (initialize, tools/list, tools/call, shutdown/ping) following tpu-mcp/cargo-mcp pattern
- [x] MBM-3: Implement session management (open/close binlog files, session handle map)
- [x] MBM-4: Implement `binlog_open` tool — load binlog via `BinlogIndex::open`, return session handle + summary (version, event count, error/warning counts)
- [x] MBM-5: Implement `binlog_close` tool — drop session by handle

### M2: Tier 1 — discovery tools (4 items)

- [x] MBM-6: Implement `binlog_summary` tool — build result, project list, error/warning counts, build duration
- [x] MBM-7: Implement `binlog_errors` tool — all Error events formatted with file, line, code, message, project context
- [x] MBM-8: Implement `binlog_warnings` tool — Warning events with optional filtering by code or project
- [x] MBM-9: Unit tests for Tier 1 tools (mock binlog or small real binlog in test data)

### M3: Tier 2 — context tools (5 items)

- [x] MBM-10: Implement `binlog_project_tree` tool — hierarchical project/target/task view
- [x] MBM-11: Implement `binlog_events` tool — filtered event listing (by kind, project, target, task, message text search)
- [x] MBM-12: Implement `binlog_event_detail` tool — full event detail by index
- [x] MBM-13: Implement `binlog_properties` tool — MSBuild properties from ProjectStarted events
- [x] MBM-14: Implement `binlog_items` tool — item groups from ProjectStarted events

### M4: Tier 3 — analysis tools + feedback (4 items)

- [x] MBM-15: Implement `binlog_error_context` tool — surrounding events for a given error (same task/target/project neighborhood)
- [x] MBM-16: Implement `binlog_task_timeline` tool — chronological task list for a project with success/failure status
- [x] MBM-17: Implement `binlog_feedback` tool — append structured feedback to JSONL file
- [x] MBM-18: Unit tests for Tier 2–3 tools and feedback JSONL output

### M5: Integration and polish (3 items)

- [x] MBM-19: Add workspace member to root Cargo.toml, add .vscode/mcp.json entry for munin-binlog-mcp
- [x] MBM-20: Write README.md with tool table, installation, and VS Code configuration
- [x] MBM-21: End-to-end test with a real binlog file (integration test in tests/ directory)
