<!-- Copyright (c) Michael Grier -->
# munin-binlog-mcp

MCP (Model Context Protocol) server for querying MSBuild binary log (`.binlog`) files.
Communicates over JSON-RPC 2.0 on stdio.

## Tools

| Tool | Description |
|---|---|
| `binlog_open` | Open a `.binlog` file, return a session handle and build summary |
| `binlog_close` | Close a session and free resources |
| `binlog_summary` | Build result, project list, error/warning counts, duration |
| `binlog_errors` | All Error events with file, line, code, message, project |
| `binlog_warnings` | Warning events, optionally filtered by code or project |
| `binlog_project_tree` | Hierarchical project/target/task view |
| `binlog_events` | Filtered event listing by kind, project, target, task, text |
| `binlog_event_detail` | Full detail for a single event by index |
| `binlog_properties` | MSBuild properties from ProjectStarted events |
| `binlog_items` | Item groups from ProjectStarted events |
| `binlog_error_context` | Surrounding events for a given error in scope |
| `binlog_task_timeline` | Chronological task list with success/failure status |
| `binlog_feedback` | Append structured feedback to a JSONL file |

## Build

```powershell
cargo build --release -p munin-binlog-mcp
```

The binary is produced at `target/release/munin-binlog-mcp.exe`.

## VS Code configuration

Add to `.vscode/mcp.json`:

```json
{
    "servers": {
        "munin-binlog-mcp": {
            "type": "stdio",
            "command": "${workspaceFolder}/target/release/munin-binlog-mcp.exe",
            "args": []
        }
    }
}
```

After adding or changing this file, reload the VS Code window or use
**MCP: List Servers** to pick up the new configuration.

## Usage

All interaction is through MCP tool calls. A typical workflow:

1. `binlog_open` with the path to a `.binlog` file — returns a session handle.
2. `binlog_summary` — get the build outcome at a glance.
3. `binlog_errors` / `binlog_warnings` — inspect diagnostics.
4. `binlog_error_context` — understand what was happening around an error.
5. `binlog_event_detail` — drill into a specific event.
6. `binlog_task_timeline` — see which tasks ran and their status.
7. `binlog_feedback` — record analysis notes for future reference.
8. `binlog_close` — release the session when done.
