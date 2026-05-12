<!-- Copyright (c) Michael Grier -->
# munin-binlog-mcp — Design Notes

## D-1: Purpose and scope

`munin-binlog-mcp` is an MCP (Model Context Protocol) server that exposes MSBuild binary
log (`.binlog`) files to Copilot and other MCP clients. Its primary use case is build
failure diagnosis: an AI agent opens a binlog, queries it for errors, warnings, and
contextual information, and produces actionable guidance.

The server is a stdio-based JSON-RPC 2.0 process, following the same pattern as `tpu-mcp`
and `cargo-mcp` in this workspace.

## D-2: Architecture — session model

The server maintains an in-memory `BinlogIndex` (from the `munin` crate) per opened file.
Multiple binlog files can be open simultaneously, keyed by an opaque session handle
returned from `binlog_open`. This lets the agent compare logs or examine multiple builds.

Each session handle maps to a loaded `BinlogIndex` that supports random-access
deserialization and filtering by record kind, project context, target, and task.

## D-3: Tool design philosophy — layered querying

Tools are organized into three tiers:

**Tier 1 — Discovery (what happened?)**
- `binlog_open` — load a binlog, return session handle + build summary (version, event count, error/warning counts)
- `binlog_summary` — high-level build result: succeeded/failed, project list, error count, warning count, duration
- `binlog_errors` — all Error events with file, line, code, message, project context
- `binlog_warnings` — all Warning events (same fields), optionally filtered by code or project

**Tier 2 — Context (why did it happen?)**
- `binlog_project_tree` — hierarchical view of projects/targets/tasks (what built what)
- `binlog_events` — filtered event listing (by record kind, project, target, task, text search in message)
- `binlog_event_detail` — full deserialized event by index (all fields)
- `binlog_properties` — MSBuild properties captured in ProjectStarted events
- `binlog_items` — item groups captured in ProjectStarted events

**Tier 3 — Analysis (what do I do about it?)**
- `binlog_error_context` — for a given error event, return surrounding events in the same task/target/project
  (the "neighborhood" that explains what the build was doing when the error occurred)
- `binlog_task_timeline` — chronological task execution for a project (which tasks ran, which succeeded/failed)

## D-4: Feedback loop — self-improving diagnostics

A key design goal is that the MCP server can help Copilot identify *gaps in its own
diagnostic capabilities*. After diagnosing a build failure (or failing to diagnose one),
Copilot should be able to record what queries were useful and what information was missing.

This is implemented via a `binlog_feedback` tool that accepts structured feedback:

```json
{
  "session": "<handle>",
  "diagnosis_succeeded": true|false,
  "useful_tools": ["binlog_errors", "binlog_error_context"],
  "missing_information": "I needed to see which NuGet packages were restored and their versions",
  "suggested_tool": {
    "name": "binlog_nuget_packages",
    "description": "List NuGet packages restored during the build with versions",
    "rationale": "Package version mismatches are a common cause of build failures"
  }
}
```

Feedback is appended to a local JSONL file (configurable path, defaults to
`.scratch/binlog-mcp-feedback.jsonl` relative to the workspace root). This creates a
durable record that can be reviewed by the developer to prioritize new tool development.

The feedback file is intentionally simple (append-only JSONL) so it can be read by any
text editor or processed by scripts. No database or service dependency.

## D-5: Query patterns for build failure diagnosis

Based on real-world MSBuild failure scenarios, the following query patterns are expected
to be most valuable:

1. **"What failed?"** — `binlog_errors` returns all error events. This is the first thing
   any agent will ask.

2. **"What was building when it failed?"** — `binlog_error_context` returns the task,
   target, and project that produced each error. This disambiguates errors from multi-project
   builds.

3. **"Did this project even build?"** — `binlog_project_tree` shows which projects were
   evaluated and which targets ran. Useful for "why didn't my project get built?" questions.

4. **"What property values were used?"** — `binlog_properties` returns the MSBuild
   property bag at project evaluation time. Critical for conditional compilation issues
   (wrong Configuration, Platform, TargetFramework, etc.).

5. **"What command was run?"** — TaskCommandLine events show the exact compiler/linker
   invocations. Essential for "the compiler got the wrong flags" problems.

6. **"Were there warnings I should care about?"** — `binlog_warnings` with filtering.
   Some warnings presage errors (e.g. missing references that later cause CS0246).

7. **"What order did things happen?"** — `binlog_task_timeline` shows execution order
   within a project. Useful for understanding dependency ordering issues.

## D-6: Output format

All tool responses return structured text designed for LLM consumption. Events are
formatted as compact, readable blocks rather than raw JSON, to minimize token usage
while preserving essential information. Example error output:

```
Error CS0246 in src/Foo.cs(42,10): The type or namespace name 'Bar' could not be found
  Project: src/Foo/Foo.csproj
  Target: CoreCompile
  Task: Csc
```

Full JSON detail is available via `binlog_event_detail` when the agent needs to inspect
all fields of a specific event.

## D-7: Binlog lifecycle

Sessions are lightweight — the `BinlogIndex` holds compressed payloads and deserializes
on demand. A typical binlog (10–50 MB compressed) produces an index of similar size in
memory. Sessions can be explicitly closed via `binlog_close` or are dropped when the
server process exits.

The server does not watch for file changes. If the user rebuilds and wants to examine the
new binlog, they open a new session (possibly closing the old one).
