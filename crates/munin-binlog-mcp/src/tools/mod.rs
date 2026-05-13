// Copyright (c) Michael Grier

//! Tool definitions, session management, and dispatch for `munin-binlog-mcp`.
//!
//! ## Tool set
//!
//! - `binlog_open`           — load a `.binlog` file, return session handle + summary
//! - `binlog_close`          — drop a session by handle
//! - `binlog_summary`        — build result, project list, error/warning counts
//! - `binlog_errors`         — all Error events with diagnostic detail
//! - `binlog_warnings`       — all Warning events, optionally filtered
//! - `binlog_project_tree`   — hierarchical project/target/task view
//! - `binlog_events`         — filtered event listing (by kind, project, text)
//! - `binlog_event_detail`   — full event detail by index
//! - `binlog_properties`     — MSBuild properties from ProjectStarted events
//! - `binlog_items`          — item groups from ProjectStarted events
//! - `binlog_error_context`  — surrounding events for a given error
//! - `binlog_task_timeline`  — chronological task list with success/failure status
//! - `binlog_feedback`       — append structured feedback to JSONL file
//! - `binlog_setup`          — emit canonical Copilot-instructions text

use std::{collections::HashMap, fmt::Write as _, fs::File, io::BufReader};

use munin::{
    events::DiagnosticLocation, fields::BuildEventArgsFields, index::BinlogIndex,
    reader::BinlogEvent, record_kind::BinaryLogRecordKind,
};
use serde_json::Value;

// ── setup-tool content ────────────────────────────────────────────────────────

/// Short snippet for the repo's main Copilot instructions file
/// (`.github/copilot-instructions.md`). Kept under ~100 words and focused on
/// the two facts an agent must know: *use the binlog MCP*, and *enable
/// binary logging when the user runs MSBuild*. The full tool list, trigger
/// keywords, and workflow live in [`BINLOG_SETUP_FULL`] so this stub stays
/// short and stable.
const BINLOG_SETUP_SHORT: &str = "\
## MSBuild binary logs \u{2014} use the munin-binlog MCP

When diagnosing any MSBuild, `dotnet build`, `dotnet test`, NuGet restore, or \
Visual Studio build problem, use the `binlog_*` MCP tools to inspect the \
`.binlog` rather than re-running the build or scraping console output.

When invoking MSBuild yourself (or asking the user to), always enable binary \
logging: `dotnet build -bl:msbuild.binlog` or `msbuild /bl:msbuild.binlog`. \
Then call `binlog_open` on the resulting file.

See [.github/instructions/munin-binlog-mcp.instructions.md](.github/instructions/munin-binlog-mcp.instructions.md) \
for the full tool list, trigger keywords, and recommended workflow.
";

/// Full instructions file written to
/// `.github/instructions/munin-binlog-mcp.instructions.md`. Loaded
/// automatically by VS Code via the `applyTo: '**'` frontmatter so the
/// trigger-keyword block is always available to the model.
const BINLOG_SETUP_FULL: &str = "\
---
applyTo: '**'
---
# munin-binlog-mcp \u{2014} MSBuild binary log analysis

**Trigger keywords** (use the `binlog_*` tools whenever the user mentions any \
of these): MSBuild, msbuild.exe, `dotnet build`, `dotnet publish`, \
`dotnet test`, `dotnet restore`, NuGet restore, Visual Studio build, \
`.binlog`, binary log, structured log, MSBuild SDK, target/task ordering, \
incremental build, `Directory.Build.props`, `Directory.Packages.props`, \
project SDK, build failure, slow build, 'works on my machine' build, \
`/bl`, `-bl`.

## Producing a binlog

If the user does not have a `.binlog` file yet, ask them to re-run the \
failing build with binary logging enabled:

- `dotnet build -bl:msbuild.binlog` \u{2014} writes `msbuild.binlog` in the \
  current directory.
- `msbuild /bl:msbuild.binlog` \u{2014} same, for full-Framework MSBuild.
- Visual Studio: install the *MSBuild Binary and Structured Log Viewer* \
  extension, or set `MSBuildDebugEngine=1`.

## Tools

| Tool | Purpose |
|---|---|
| `binlog_open` | Open a `.binlog`, return a session handle + summary. Always first. |
| `binlog_close` | Release a session. |
| `binlog_summary` | Build success/failure, project list, error/warning counts. |
| `binlog_errors` | All Error events with code, file, line, message, project. |
| `binlog_warnings` | Warning events, optionally filtered by `code` or `project`. |
| `binlog_project_tree` | Hierarchical project / target / task view. |
| `binlog_events` | Filtered event listing (kind, project, target, task, text). |
| `binlog_event_detail` | Full detail for one event by index. |
| `binlog_properties` | MSBuild properties from ProjectStarted events. |
| `binlog_items` | Item groups (Compile, Reference, PackageReference, ...). |
| `binlog_error_context` | Surrounding events for a given error index. |
| `binlog_task_timeline` | Chronological task list with success/failure status. |
| `binlog_feedback` | Append structured analysis notes to a JSONL file. |

## Recommended workflow

1. `binlog_open` with the absolute path. Keep the returned `session` handle.
2. `binlog_summary` to confirm overall success/failure and project list.
3. **Failures:** `binlog_errors` first, then `binlog_error_context` and/or \
   `binlog_event_detail` on the most relevant error indices.
4. **Warnings or noise:** `binlog_warnings` (filter by `code` or `project`).
5. **'Why did MSBuild do X?'**: `binlog_project_tree`, `binlog_task_timeline`, \
   `binlog_properties`, `binlog_items`, or `binlog_events` with filters.
6. `binlog_close` when finished, or leave open if more questions are likely.

## Citation rule

Always cite the binlog as the source of any diagnostic claim (error code, \
file/line, task that failed, property value). If a binlog field contradicts \
the user's assumption, surface the contradiction explicitly.
";

// ── session management ────────────────────────────────────────────────────────

/// Opaque, monotonically increasing session handle.
///
/// Changing this type's representation is a breaking change for any client
/// that persists handles across calls.
type SessionHandle = u64;

/// Per-session state: the loaded index and original file path.
struct Session {
    path: String,
    index: BinlogIndex,
}

/// Map of open sessions keyed by handle.
pub struct SessionMap {
    next_handle: SessionHandle,
    sessions: HashMap<SessionHandle, Session>,
}

impl Default for SessionMap {
    fn default() -> Self {
        Self {
            next_handle: 1,
            sessions: HashMap::new(),
        }
    }
}

impl SessionMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a binlog file and return the handle.
    fn open(&mut self, path: &str) -> Result<SessionHandle, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let index = BinlogIndex::open(reader)?;

        let handle = self.next_handle;
        self.next_handle += 1;
        self.sessions.insert(
            handle,
            Session {
                path: path.to_owned(),
                index,
            },
        );
        Ok(handle)
    }

    /// Close a session by handle. Returns true if the session existed.
    fn close(&mut self, handle: SessionHandle) -> bool {
        self.sessions.remove(&handle).is_some()
    }

    /// Look up a session by handle.
    fn get(&self, handle: SessionHandle) -> Option<&Session> {
        self.sessions.get(&handle)
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Extract an optional string field from JSON args.
fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

/// Extract an optional integer field from JSON args.
fn opt_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(|v| v.as_i64())
}

/// Extract a required string field, returning an error if missing.
fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    opt_str(args, key).ok_or_else(|| format!("missing required parameter: {key}").into())
}

/// Extract a session handle from the `session` argument.
fn req_session_handle(args: &Value) -> Result<SessionHandle, Box<dyn std::error::Error>> {
    args.get("session")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing or invalid 'session' parameter (expected integer handle)".into())
}

// ── tool list ─────────────────────────────────────────────────────────────────

/// Return the MCP `tools/list` payload (an array of tool descriptors).
pub fn list() -> Value {
    serde_json::json!([
        {
            "name": "binlog_open",
            "description":
                "Open an MSBuild binary log (.binlog) file and return a session handle \
                 along with a build summary. The session handle is used in subsequent \
                 tool calls to query the loaded binlog. The summary includes the binlog \
                 format version, total event count, and counts of error and warning events.\n\
                 \n\
                 Use this tool whenever the user is diagnosing an MSBuild, `dotnet build`, \
                 `dotnet test`, NuGet, or Visual Studio build failure or warning. If no \
                 .binlog file exists yet, ask the user to re-run the failing build with \
                 `dotnet build -bl:msbuild.binlog` (or `msbuild /bl:msbuild.binlog`) and \
                 pass the resulting file path here. Common locations to check: the repo \
                 root, the failing project directory, the user's working directory, or \
                 paths the user mentions in their prompt.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description":
                            "Absolute path to the .binlog file to open. Typically \
                             produced by `dotnet build -bl:<path>` or `msbuild /bl:<path>`."
                    }
                },
                "required": ["file"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_close",
            "description":
                "Close a previously opened binlog session and free its resources. \
                 The session handle becomes invalid after this call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    }
                },
                "required": ["session"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false }
        },
        {
            "name": "binlog_summary",
            "description":
                "Return a high-level build summary for an open binlog session. \
                 Includes: build succeeded/failed, list of projects built, \
                 total error and warning counts, and build start/finish \
                 timestamps when available.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    }
                },
                "required": ["session"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_errors",
            "description":
                "Return all Error events from the binlog. Each error includes \
                 the error code, file path, line/column, message text, and the \
                 project file that produced it. Use this as the first step when \
                 diagnosing a build failure.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    },
                    "limit": {
                        "type": "integer",
                        "description":
                            "Maximum number of errors to return. Omit for all errors."
                    }
                },
                "required": ["session"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_warnings",
            "description":
                "Return Warning events from the binlog. Each warning includes \
                 the warning code, file path, line/column, message text, and \
                 the project file. Supports optional filtering by warning code \
                 or project file path substring.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    },
                    "code": {
                        "type": "string",
                        "description":
                            "Filter to warnings with this code (e.g. 'CS0168'). \
                             Case-insensitive. Omit to include all warning codes."
                    },
                    "project": {
                        "type": "string",
                        "description":
                            "Filter to warnings whose project file path contains \
                             this substring. Case-insensitive. Omit for all projects."
                    },
                    "limit": {
                        "type": "integer",
                        "description":
                            "Maximum number of warnings to return. Omit for all warnings."
                    }
                },
                "required": ["session"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_project_tree",
            "description":
                "Return a hierarchical view of the build: projects, their targets, \
                 and tasks within each target. Useful for understanding build structure \
                 and identifying which targets and tasks ran in each project.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    }
                },
                "required": ["session"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_events",
            "description":
                "Return a filtered list of events from the binlog. Filters can \
                 be combined: kind (by record type name), project_context_id, \
                 target_id, task_id, and text (substring match on message). \
                 Returns compact one-line-per-event summaries.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    },
                    "kind": {
                        "type": "string",
                        "description":
                            "Filter by record kind name (e.g. 'Message', 'TaskStarted', \
                             'ProjectStarted'). Case-insensitive."
                    },
                    "project_context_id": {
                        "type": "integer",
                        "description":
                            "Filter to events with this project context ID."
                    },
                    "target_id": {
                        "type": "integer",
                        "description":
                            "Filter to events with this target ID."
                    },
                    "task_id": {
                        "type": "integer",
                        "description":
                            "Filter to events with this task ID."
                    },
                    "text": {
                        "type": "string",
                        "description":
                            "Filter to events whose message contains this substring. \
                             Case-insensitive."
                    },
                    "limit": {
                        "type": "integer",
                        "description":
                            "Maximum number of events to return. Omit for all matching."
                    }
                },
                "required": ["session"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_event_detail",
            "description":
                "Return full detail for a single event by its index. Includes all \
                 fields available for that event type. Use this after binlog_events \
                 or binlog_errors to drill into a specific event.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    },
                    "index": {
                        "type": "integer",
                        "description":
                            "The event index (0-based) to retrieve."
                    }
                },
                "required": ["session", "index"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_properties",
            "description":
                "Return MSBuild properties captured in ProjectStarted events. \
                 Properties are key-value pairs representing the project's \
                 evaluation-time configuration. Optionally filter by project \
                 file path substring or property name substring.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    },
                    "project": {
                        "type": "string",
                        "description":
                            "Filter to properties from project files matching this \
                             substring. Case-insensitive."
                    },
                    "name": {
                        "type": "string",
                        "description":
                            "Filter to properties whose name contains this substring. \
                             Case-insensitive."
                    },
                    "limit": {
                        "type": "integer",
                        "description":
                            "Maximum number of properties to return per project. \
                             Omit for all."
                    }
                },
                "required": ["session"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_items",
            "description":
                "Return MSBuild item groups captured in ProjectStarted events. \
                 Items are grouped by type (e.g. 'Compile', 'Reference', \
                 'PackageReference'). Optionally filter by project file, item \
                 type, or item spec substring.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    },
                    "project": {
                        "type": "string",
                        "description":
                            "Filter to items from project files matching this \
                             substring. Case-insensitive."
                    },
                    "item_type": {
                        "type": "string",
                        "description":
                            "Filter to items of this type (e.g. 'Compile', \
                             'Reference'). Case-insensitive."
                    },
                    "spec": {
                        "type": "string",
                        "description":
                            "Filter to items whose item spec contains this substring. \
                             Case-insensitive."
                    },
                    "limit": {
                        "type": "integer",
                        "description":
                            "Maximum number of items to return per item type. \
                             Omit for all."
                    }
                },
                "required": ["session"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_error_context",
            "description":
                "Given an error event index, show surrounding events from the same \
                 task, target, or project context. This helps understand what was \
                 happening when an error occurred. Returns events before and after \
                 the error in the same scope.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    },
                    "index": {
                        "type": "integer",
                        "description":
                            "The event index of the error to get context for."
                    },
                    "radius": {
                        "type": "integer",
                        "description":
                            "Number of events to show before and after the error \
                             within the same context. Default is 10."
                    }
                },
                "required": ["session", "index"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_task_timeline",
            "description":
                "Show a chronological timeline of tasks for a project, including \
                 success/failure status. Useful for understanding build order and \
                 identifying which tasks failed. Filter by project_context_id or \
                 project file path substring.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle returned by binlog_open."
                    },
                    "project_context_id": {
                        "type": "integer",
                        "description":
                            "Show tasks for the project with this context ID. \
                             Takes precedence over 'project' if both are specified."
                    },
                    "project": {
                        "type": "string",
                        "description":
                            "Filter to projects whose file path contains this \
                             substring. Case-insensitive. Ignored if \
                             project_context_id is specified."
                    }
                },
                "required": ["session"]
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        },
        {
            "name": "binlog_feedback",
            "description":
                "Record structured feedback about a binlog analysis session. \
                 Appends a JSON object to a feedback JSONL file. Use this to \
                 record what was useful, what the root cause was, or notes for \
                 future analysis.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": {
                        "type": "integer",
                        "description":
                            "The session handle to associate this feedback with."
                    },
                    "file": {
                        "type": "string",
                        "description":
                            "Path to the feedback JSONL file. Will be created if \
                             it does not exist. Feedback is appended."
                    },
                    "text": {
                        "type": "string",
                        "description":
                            "Free-form feedback text."
                    },
                    "root_cause": {
                        "type": "string",
                        "description":
                            "Optional: identified root cause of the build failure."
                    },
                    "event_indices": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description":
                            "Optional: event indices relevant to this feedback."
                    }
                },
                "required": ["session", "file", "text"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false }
        },
        {
            "name": "binlog_setup",
            "description":
                "Returns the canonical munin-binlog-mcp instructions to add to \
                 this repository so future Copilot sessions automatically reach \
                 for the binlog tools when diagnosing MSBuild / dotnet build / \
                 Visual Studio build problems. The output contains TWO files: a \
                 short stub to append to `.github/copilot-instructions.md` and a \
                 full instructions file to write to \
                 `.github/instructions/munin-binlog-mcp.instructions.md`. After \
                 receiving this output, YOU should create or update those files \
                 in the user's workspace; adapt the wording to fit existing \
                 conventions but preserve the trigger keywords and tool list. \
                 Run this once per repository that builds .NET / MSBuild projects.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false }
        }
    ])
}

// ── dispatch ──────────────────────────────────────────────────────────────────

/// Route a tool call to the appropriate handler.
pub fn call(
    name: &str,
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    match name {
        "binlog_open" => call_binlog_open(args, sessions),
        "binlog_close" => call_binlog_close(args, sessions),
        "binlog_summary" => call_binlog_summary(args, sessions),
        "binlog_errors" => call_binlog_errors(args, sessions),
        "binlog_warnings" => call_binlog_warnings(args, sessions),
        "binlog_project_tree" => call_binlog_project_tree(args, sessions),
        "binlog_events" => call_binlog_events(args, sessions),
        "binlog_event_detail" => call_binlog_event_detail(args, sessions),
        "binlog_properties" => call_binlog_properties(args, sessions),
        "binlog_items" => call_binlog_items(args, sessions),
        "binlog_error_context" => call_binlog_error_context(args, sessions),
        "binlog_task_timeline" => call_binlog_task_timeline(args, sessions),
        "binlog_feedback" => call_binlog_feedback(args, sessions),
        "binlog_setup" => call_binlog_setup(args),
        _ => Err(format!("unknown tool: {name}").into()),
    }
}

// ── tool implementations ──────────────────────────────────────────────────────

fn call_binlog_open(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let path = req_str(args, "file")?;
    let handle = sessions.open(path)?;

    let session = sessions.get(handle).expect("just inserted");
    let index = &session.index;

    let error_count = index.indices_by_kind(BinaryLogRecordKind::Error).len();
    let warning_count = index.indices_by_kind(BinaryLogRecordKind::Warning).len();
    let version = index.header().file_format_version;

    let mut out = String::new();
    writeln!(out, "Session: {handle}")?;
    writeln!(out, "File: {}", session.path)?;
    writeln!(out, "Format version: {version}")?;
    writeln!(out, "Total events: {}", index.len())?;
    writeln!(out, "Errors: {error_count}")?;
    writeln!(out, "Warnings: {warning_count}")?;

    Ok(out)
}

fn call_binlog_close(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    if sessions.close(handle) {
        Ok(format!("Session {handle} closed."))
    } else {
        Err(format!("no session with handle {handle}").into())
    }
}

fn call_binlog_summary(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let index = &session.index;

    let error_count = index.indices_by_kind(BinaryLogRecordKind::Error).len();
    let warning_count = index.indices_by_kind(BinaryLogRecordKind::Warning).len();

    // Find BuildFinished to determine success/failure.
    let build_finished_indices = index.indices_by_kind(BinaryLogRecordKind::BuildFinished);
    let build_result = if let Some(&idx) = build_finished_indices.first() {
        if let Ok(Some(BinlogEvent::BuildFinished(ref bf))) = index.get(idx) {
            if bf.succeeded {
                "succeeded"
            } else {
                "FAILED"
            }
        } else {
            "unknown"
        }
    } else {
        "unknown (no BuildFinished event)"
    };

    // Collect project file paths from ProjectStarted events.
    let project_indices = index.indices_by_kind(BinaryLogRecordKind::ProjectStarted);
    let mut project_files: Vec<String> = Vec::new();
    for &idx in &project_indices {
        if let Ok(Some(BinlogEvent::ProjectStarted(ref ps))) = index.get(idx) {
            if let Some(ref pf) = ps.project_file {
                if !project_files.contains(pf) {
                    project_files.push(pf.clone());
                }
            }
        }
    }

    // Extract timestamps from BuildStarted/BuildFinished for duration.
    let build_started_indices = index.indices_by_kind(BinaryLogRecordKind::BuildStarted);
    let start_ticks = build_started_indices.first().and_then(|&idx| {
        if let Ok(Some(BinlogEvent::BuildStarted(ref bs))) = index.get(idx) {
            bs.fields.timestamp.map(|ts| ts.ticks)
        } else {
            None
        }
    });
    let finish_ticks = build_finished_indices.first().and_then(|&idx| {
        if let Ok(Some(BinlogEvent::BuildFinished(ref bf))) = index.get(idx) {
            bf.fields.timestamp.map(|ts| ts.ticks)
        } else {
            None
        }
    });

    let mut out = String::new();
    writeln!(out, "Build result: {build_result}")?;
    writeln!(out, "File: {}", session.path)?;
    writeln!(
        out,
        "Format version: {}",
        index.header().file_format_version
    )?;
    writeln!(out, "Total events: {}", index.len())?;
    writeln!(out, "Errors: {error_count}")?;
    writeln!(out, "Warnings: {warning_count}")?;

    if let (Some(start), Some(finish)) = (start_ticks, finish_ticks) {
        // .NET ticks are 100-nanosecond intervals.
        let duration_ms = (finish - start) / 10_000;
        let secs = duration_ms / 1000;
        let ms = duration_ms % 1000;
        writeln!(out, "Duration: {secs}.{ms:03}s")?;
    }

    writeln!(out, "Projects ({}):", project_files.len())?;
    for pf in &project_files {
        writeln!(out, "  {pf}")?;
    }

    Ok(out)
}

fn call_binlog_errors(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let limit = opt_i64(args, "limit").map(|n| n as usize);
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let index = &session.index;

    let error_indices = index.indices_by_kind(BinaryLogRecordKind::Error);

    if error_indices.is_empty() {
        return Ok("No errors found.".to_owned());
    }

    let mut out = String::new();
    let total = error_indices.len();
    let showing = limit.map_or(total, |n| n.min(total));
    writeln!(out, "Errors: {showing} of {total}")?;
    writeln!(out)?;

    for (i, &idx) in error_indices.iter().enumerate() {
        if limit.is_some_and(|n| i >= n) {
            break;
        }
        if let Ok(Some(BinlogEvent::Error(ref e))) = index.get(idx) {
            format_diagnostic(&mut out, "Error", idx, &e.fields, &e.location)?;
        }
    }

    Ok(out)
}

fn call_binlog_warnings(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let code_filter = opt_str(args, "code").map(|s| s.to_ascii_lowercase());
    let project_filter = opt_str(args, "project").map(|s| s.to_ascii_lowercase());
    let limit = opt_i64(args, "limit").map(|n| n as usize);
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let index = &session.index;

    let warning_indices = index.indices_by_kind(BinaryLogRecordKind::Warning);

    if warning_indices.is_empty() {
        return Ok("No warnings found.".to_owned());
    }

    let mut out = String::new();
    let mut count = 0usize;
    let mut skipped = 0usize;

    for &idx in &warning_indices {
        if limit.is_some_and(|n| count >= n) {
            break;
        }
        if let Ok(Some(BinlogEvent::Warning(ref w))) = index.get(idx) {
            // Apply code filter.
            if let Some(ref cf) = code_filter {
                let event_code = w
                    .location
                    .code
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if event_code != *cf {
                    skipped += 1;
                    continue;
                }
            }
            // Apply project filter.
            if let Some(ref pf) = project_filter {
                let event_project = w
                    .location
                    .project_file
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !event_project.contains(pf.as_str()) {
                    skipped += 1;
                    continue;
                }
            }
            format_diagnostic(&mut out, "Warning", idx, &w.fields, &w.location)?;
            count += 1;
        }
    }

    if count == 0 {
        if skipped > 0 {
            return Ok(format!(
                "No warnings matched the filter ({skipped} warnings filtered out)."
            ));
        }
        return Ok("No warnings found.".to_owned());
    }

    // Prepend header.
    let total = warning_indices.len();
    let header = if skipped > 0 {
        format!("Warnings: {count} shown ({skipped} filtered out, {total} total)\n\n")
    } else {
        format!("Warnings: {count} of {total}\n\n")
    };
    out.insert_str(0, &header);

    Ok(out)
}

fn call_binlog_project_tree(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let index = &session.index;

    let mut out = String::new();

    // Iterate ProjectStarted events; for each, find its targets and tasks.
    let project_indices = index.indices_by_kind(BinaryLogRecordKind::ProjectStarted);
    if project_indices.is_empty() {
        return Ok("No projects found.".to_owned());
    }

    writeln!(out, "Project tree ({} projects):", project_indices.len())?;
    writeln!(out)?;

    for &proj_idx in &project_indices {
        let Ok(Some(BinlogEvent::ProjectStarted(ref ps))) = index.get(proj_idx) else {
            continue;
        };
        let proj_file = ps.project_file.as_deref().unwrap_or("<unknown project>");
        let targets = ps.target_names.as_deref().unwrap_or("");
        let ctx = ps.fields.build_event_context;

        writeln!(out, "Project: {proj_file}")?;
        writeln!(out, "  Project ID: {}", ps.project_id)?;
        if !targets.is_empty() {
            writeln!(out, "  Requested targets: {targets}")?;
        }
        if let Some(ref ctx) = ctx {
            writeln!(
                out,
                "  Context: project_context_id={}",
                ctx.project_context_id
            )?;
        }

        // Find targets belonging to this project via project_context_id.
        if let Some(ref ctx) = ctx {
            let target_indices = index.query(
                Some(BinaryLogRecordKind::TargetStarted),
                Some(ctx.project_context_id),
                None,
                None,
            );
            for &tgt_idx in &target_indices {
                let Ok(Some(BinlogEvent::TargetStarted(ref ts))) = index.get(tgt_idx) else {
                    continue;
                };
                let tgt_name = ts.target_name.as_deref().unwrap_or("<unnamed>");
                write!(out, "  Target: {tgt_name}")?;

                // Find tasks belonging to this target.
                if let Some(ref tgt_ctx) = ts.fields.build_event_context {
                    let task_indices = index.query(
                        Some(BinaryLogRecordKind::TaskStarted),
                        Some(ctx.project_context_id),
                        Some(tgt_ctx.target_id),
                        None,
                    );
                    if task_indices.is_empty() {
                        writeln!(out)?;
                    } else {
                        writeln!(out, " ({} tasks)", task_indices.len())?;
                        for &task_idx in &task_indices {
                            let Ok(Some(BinlogEvent::TaskStarted(ref tsk))) = index.get(task_idx)
                            else {
                                continue;
                            };
                            let task_name = tsk.task_name.as_deref().unwrap_or("<unnamed>");
                            writeln!(out, "    Task: {task_name}")?;
                        }
                    }
                } else {
                    writeln!(out)?;
                }
            }
        }
        writeln!(out)?;
    }

    Ok(out)
}

fn call_binlog_events(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let kind_filter = opt_str(args, "kind").and_then(parse_record_kind);
    let project_ctx_filter = opt_i64(args, "project_context_id").map(|n| n as i32);
    let target_id_filter = opt_i64(args, "target_id").map(|n| n as i32);
    let task_id_filter = opt_i64(args, "task_id").map(|n| n as i32);
    let text_filter = opt_str(args, "text").map(|s| s.to_ascii_lowercase());
    let limit = opt_i64(args, "limit").map(|n| n as usize);
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let index = &session.index;

    // Use query() if any structured filter is set; otherwise iterate all.
    let candidate_indices: Vec<usize> = if kind_filter.is_some()
        || project_ctx_filter.is_some()
        || target_id_filter.is_some()
        || task_id_filter.is_some()
    {
        index.query(
            kind_filter,
            project_ctx_filter,
            target_id_filter,
            task_id_filter,
        )
    } else {
        (0..index.len()).collect()
    };

    let mut out = String::new();
    let mut count = 0usize;

    for &idx in &candidate_indices {
        if limit.is_some_and(|n| count >= n) {
            break;
        }

        // Apply text filter if present.
        if let Some(ref tf) = text_filter {
            let msg = event_message(index, idx);
            if !msg.to_ascii_lowercase().contains(tf.as_str()) {
                continue;
            }
        }

        let meta = index.meta(idx);
        let kind_name = meta.map_or("???", |m| record_kind_name(m.record_kind));
        let msg_preview = event_message_preview(index, idx);
        writeln!(out, "[{idx}] {kind_name}: {msg_preview}")?;
        count += 1;
    }

    if count == 0 {
        return Ok("No events matched the filter.".to_owned());
    }

    let header = format!("Events: {count} shown\n\n");
    out.insert_str(0, &header);

    Ok(out)
}

fn call_binlog_event_detail(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let event_index = args
        .get("index")
        .and_then(|v| v.as_u64())
        .ok_or("missing or invalid 'index' parameter")? as usize;
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let index = &session.index;

    if event_index >= index.len() {
        return Err(format!(
            "event index {event_index} out of range (total events: {})",
            index.len()
        )
        .into());
    }

    let meta = index.meta(event_index);
    let event = index.get(event_index)?;

    let mut out = String::new();
    writeln!(out, "Event index: {event_index}")?;

    if let Some(meta) = meta {
        writeln!(out, "Kind: {}", record_kind_name(meta.record_kind))?;
        writeln!(out, "Byte offset: {}", meta.byte_offset)?;
        writeln!(out, "Payload length: {}", meta.payload_len)?;
        if let Some(ref ctx) = meta.context {
            writeln!(out, "Context:")?;
            writeln!(out, "  node_id: {}", ctx.node_id)?;
            writeln!(out, "  project_context_id: {}", ctx.project_context_id)?;
            writeln!(out, "  target_id: {}", ctx.target_id)?;
            writeln!(out, "  task_id: {}", ctx.task_id)?;
            writeln!(out, "  submission_id: {}", ctx.submission_id)?;
            writeln!(out, "  project_instance_id: {}", ctx.project_instance_id)?;
            writeln!(out, "  evaluation_id: {}", ctx.evaluation_id)?;
        }
    }

    match event {
        Some(ref ev) => {
            writeln!(out)?;
            format_event_detail(&mut out, ev)?;
        }
        None => {
            writeln!(out, "\n(Event could not be deserialized)")?;
        }
    }

    Ok(out)
}

fn call_binlog_properties(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let project_filter = opt_str(args, "project").map(|s| s.to_ascii_lowercase());
    let name_filter = opt_str(args, "name").map(|s| s.to_ascii_lowercase());
    let limit = opt_i64(args, "limit").map(|n| n as usize);
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let index = &session.index;

    let project_indices = index.indices_by_kind(BinaryLogRecordKind::ProjectStarted);
    if project_indices.is_empty() {
        return Ok("No projects found.".to_owned());
    }

    let mut out = String::new();
    let mut any_output = false;

    for &idx in &project_indices {
        let Ok(Some(BinlogEvent::ProjectStarted(ref ps))) = index.get(idx) else {
            continue;
        };
        let proj_file = ps.project_file.as_deref().unwrap_or("<unknown>");

        // Apply project filter.
        if let Some(ref pf) = project_filter {
            if !proj_file.to_ascii_lowercase().contains(pf.as_str()) {
                continue;
            }
        }

        let Some(ref props) = ps.property_list else {
            continue;
        };
        if props.is_empty() {
            continue;
        }

        writeln!(out, "Project: {proj_file}")?;
        let mut prop_count = 0usize;
        for (k, v) in props {
            // Apply name filter.
            if let Some(ref nf) = name_filter {
                if !k.to_ascii_lowercase().contains(nf.as_str()) {
                    continue;
                }
            }
            if limit.is_some_and(|n| prop_count >= n) {
                writeln!(out, "  ... (truncated at {prop_count})")?;
                break;
            }
            writeln!(out, "  {k} = {v}")?;
            prop_count += 1;
        }
        writeln!(out)?;
        any_output = true;
    }

    if !any_output {
        return Ok("No properties found matching the filter.".to_owned());
    }
    Ok(out)
}

fn call_binlog_items(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let project_filter = opt_str(args, "project").map(|s| s.to_ascii_lowercase());
    let type_filter = opt_str(args, "item_type").map(|s| s.to_ascii_lowercase());
    let spec_filter = opt_str(args, "spec").map(|s| s.to_ascii_lowercase());
    let limit = opt_i64(args, "limit").map(|n| n as usize);
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let index = &session.index;

    let project_indices = index.indices_by_kind(BinaryLogRecordKind::ProjectStarted);
    if project_indices.is_empty() {
        return Ok("No projects found.".to_owned());
    }

    let mut out = String::new();
    let mut any_output = false;

    for &idx in &project_indices {
        let Ok(Some(BinlogEvent::ProjectStarted(ref ps))) = index.get(idx) else {
            continue;
        };
        let proj_file = ps.project_file.as_deref().unwrap_or("<unknown>");

        // Apply project filter.
        if let Some(ref pf) = project_filter {
            if !proj_file.to_ascii_lowercase().contains(pf.as_str()) {
                continue;
            }
        }

        let Some(ref item_groups) = ps.item_list else {
            continue;
        };

        let mut project_header_written = false;

        for group in item_groups {
            // Apply item type filter.
            if let Some(ref tf) = type_filter {
                if group.item_type.to_ascii_lowercase() != *tf {
                    continue;
                }
            }

            if !project_header_written {
                writeln!(out, "Project: {proj_file}")?;
                project_header_written = true;
            }

            writeln!(out, "  [{} ({} items)]", group.item_type, group.items.len())?;
            let mut item_count = 0usize;
            for item in &group.items {
                let spec = item.item_spec.as_deref().unwrap_or("");

                // Apply spec filter.
                if let Some(ref sf) = spec_filter {
                    if !spec.to_ascii_lowercase().contains(sf.as_str()) {
                        continue;
                    }
                }

                if limit.is_some_and(|n| item_count >= n) {
                    writeln!(out, "    ... (truncated at {item_count})")?;
                    break;
                }

                writeln!(out, "    {spec}")?;
                if let Some(ref md) = item.metadata {
                    for (k, v) in md {
                        writeln!(out, "      {k} = {v}")?;
                    }
                }
                item_count += 1;
            }
        }

        if project_header_written {
            writeln!(out)?;
            any_output = true;
        }
    }

    if !any_output {
        return Ok("No items found matching the filter.".to_owned());
    }
    Ok(out)
}

fn call_binlog_error_context(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let event_index = args
        .get("index")
        .and_then(|v| v.as_u64())
        .ok_or("missing or invalid 'index' parameter")? as usize;
    let radius = opt_i64(args, "radius").unwrap_or(10) as usize;
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let index = &session.index;

    if event_index >= index.len() {
        return Err(format!(
            "event index {event_index} out of range (total events: {})",
            index.len()
        )
        .into());
    }

    let meta = index.meta(event_index);

    let mut out = String::new();

    // Determine the scope to search for context events.
    let context_indices: Vec<usize> = if let Some(meta) = meta {
        if let Some(ref ctx) = meta.context {
            // Try narrowest scope first: same task.
            if ctx.task_id != 0 {
                let task_events = index.query(
                    None,
                    Some(ctx.project_context_id),
                    Some(ctx.target_id),
                    Some(ctx.task_id),
                );
                if task_events.len() > 1 {
                    writeln!(
                        out,
                        "Context scope: task (project_context_id={}, target_id={}, task_id={})",
                        ctx.project_context_id, ctx.target_id, ctx.task_id
                    )?;
                    task_events
                } else if ctx.target_id != 0 {
                    let target_events = index.query(
                        None,
                        Some(ctx.project_context_id),
                        Some(ctx.target_id),
                        None,
                    );
                    writeln!(
                        out,
                        "Context scope: target (project_context_id={}, target_id={})",
                        ctx.project_context_id, ctx.target_id
                    )?;
                    target_events
                } else {
                    let project_events =
                        index.query(None, Some(ctx.project_context_id), None, None);
                    writeln!(
                        out,
                        "Context scope: project (project_context_id={})",
                        ctx.project_context_id
                    )?;
                    project_events
                }
            } else if ctx.target_id != 0 {
                let target_events = index.query(
                    None,
                    Some(ctx.project_context_id),
                    Some(ctx.target_id),
                    None,
                );
                writeln!(
                    out,
                    "Context scope: target (project_context_id={}, target_id={})",
                    ctx.project_context_id, ctx.target_id
                )?;
                target_events
            } else if ctx.project_context_id != 0 {
                let project_events = index.query(None, Some(ctx.project_context_id), None, None);
                writeln!(
                    out,
                    "Context scope: project (project_context_id={})",
                    ctx.project_context_id
                )?;
                project_events
            } else {
                // No useful context; fall back to index neighborhood.
                writeln!(out, "Context scope: index neighborhood")?;
                let start = event_index.saturating_sub(radius);
                let end = (event_index + radius + 1).min(index.len());
                (start..end).collect()
            }
        } else {
            // No BuildEventContext on this event; use index neighborhood.
            writeln!(out, "Context scope: index neighborhood")?;
            let start = event_index.saturating_sub(radius);
            let end = (event_index + radius + 1).min(index.len());
            (start..end).collect()
        }
    } else {
        writeln!(out, "Context scope: index neighborhood")?;
        let start = event_index.saturating_sub(radius);
        let end = (event_index + radius + 1).min(index.len());
        (start..end).collect()
    };

    // Find position of target event in the context list.
    let position = context_indices.iter().position(|&i| i == event_index);

    // Determine the window to show.
    let (window_start, window_end) = if let Some(pos) = position {
        let start = pos.saturating_sub(radius);
        let end = (pos + radius + 1).min(context_indices.len());
        (start, end)
    } else {
        (0, context_indices.len().min(radius * 2 + 1))
    };

    writeln!(out, "Showing events around index {event_index}:")?;
    writeln!(out)?;

    for &idx in &context_indices[window_start..window_end] {
        let marker = if idx == event_index { ">>>" } else { "   " };
        let kind = index
            .meta(idx)
            .map_or("???", |m| record_kind_name(m.record_kind));
        let preview = event_message_preview(index, idx);
        writeln!(out, "{marker} [{idx}] {kind}: {preview}")?;
    }

    Ok(out)
}

fn call_binlog_task_timeline(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let project_ctx_id = opt_i64(args, "project_context_id").map(|n| n as i32);
    let project_filter = opt_str(args, "project").map(|s| s.to_ascii_lowercase());
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let index = &session.index;

    // Collect (project_context_id, project_file) pairs to process.
    let project_contexts: Vec<(i32, String)> = if let Some(ctx_id) = project_ctx_id {
        // Specific project_context_id requested. Find the project file for display.
        let project_indices = index.indices_by_kind(BinaryLogRecordKind::ProjectStarted);
        let proj_file = project_indices.iter().find_map(|&idx| {
            if let Ok(Some(BinlogEvent::ProjectStarted(ref ps))) = index.get(idx) {
                if let Some(ref ctx) = ps.fields.build_event_context {
                    if ctx.project_context_id == ctx_id {
                        return ps.project_file.clone();
                    }
                }
            }
            None
        });
        vec![(
            ctx_id,
            proj_file.unwrap_or_else(|| format!("<project_context_id={ctx_id}>")),
        )]
    } else {
        // Iterate all projects, optionally filtering by file path.
        let project_indices = index.indices_by_kind(BinaryLogRecordKind::ProjectStarted);
        let mut contexts = Vec::new();
        for &idx in &project_indices {
            let Ok(Some(BinlogEvent::ProjectStarted(ref ps))) = index.get(idx) else {
                continue;
            };
            let proj_file = ps.project_file.as_deref().unwrap_or("<unknown>");
            if let Some(ref pf) = project_filter {
                if !proj_file.to_ascii_lowercase().contains(pf.as_str()) {
                    continue;
                }
            }
            if let Some(ref ctx) = ps.fields.build_event_context {
                contexts.push((ctx.project_context_id, proj_file.to_owned()));
            }
        }
        contexts
    };

    if project_contexts.is_empty() {
        return Ok("No projects matched the filter.".to_owned());
    }

    let mut out = String::new();

    for (ctx_id, proj_file) in &project_contexts {
        writeln!(out, "Project: {proj_file} (context_id={ctx_id})")?;

        // Find all targets for this project.
        let target_indices = index.query(
            Some(BinaryLogRecordKind::TargetStarted),
            Some(*ctx_id),
            None,
            None,
        );

        if target_indices.is_empty() {
            writeln!(out, "  (no targets)")?;
            writeln!(out)?;
            continue;
        }

        for &tgt_idx in &target_indices {
            let Ok(Some(BinlogEvent::TargetStarted(ref ts))) = index.get(tgt_idx) else {
                continue;
            };
            let tgt_name = ts.target_name.as_deref().unwrap_or("<unnamed>");
            let tgt_ctx = ts.fields.build_event_context.as_ref();

            // Find target's result from TargetFinished.
            let tgt_result = tgt_ctx.and_then(|tc| {
                let finished = index.query(
                    Some(BinaryLogRecordKind::TargetFinished),
                    Some(*ctx_id),
                    Some(tc.target_id),
                    None,
                );
                finished.first().and_then(|&fi| {
                    if let Ok(Some(BinlogEvent::TargetFinished(ref tf))) = index.get(fi) {
                        Some(tf.succeeded)
                    } else {
                        None
                    }
                })
            });
            let tgt_status = match tgt_result {
                Some(true) => "ok",
                Some(false) => "FAILED",
                None => "?",
            };

            writeln!(out, "  Target: {tgt_name} [{tgt_status}]")?;

            // Find tasks in this target.
            let task_indices = tgt_ctx.map_or_else(Vec::new, |tc| {
                index.query(
                    Some(BinaryLogRecordKind::TaskStarted),
                    Some(*ctx_id),
                    Some(tc.target_id),
                    None,
                )
            });

            for &task_idx in &task_indices {
                let Ok(Some(BinlogEvent::TaskStarted(ref tsk))) = index.get(task_idx) else {
                    continue;
                };
                let task_name = tsk.task_name.as_deref().unwrap_or("<unnamed>");
                let task_ctx = tsk.fields.build_event_context.as_ref();

                // Find task result from TaskFinished.
                let task_result = task_ctx.and_then(|tc| {
                    let finished = tgt_ctx.map_or_else(Vec::new, |tgt_c| {
                        index.query(
                            Some(BinaryLogRecordKind::TaskFinished),
                            Some(*ctx_id),
                            Some(tgt_c.target_id),
                            Some(tc.task_id),
                        )
                    });
                    finished.first().and_then(|&fi| {
                        if let Ok(Some(BinlogEvent::TaskFinished(ref tf))) = index.get(fi) {
                            Some(tf.succeeded)
                        } else {
                            None
                        }
                    })
                });
                let task_status = match task_result {
                    Some(true) => "ok",
                    Some(false) => "FAILED",
                    None => "?",
                };

                writeln!(out, "    Task: {task_name} [{task_status}]")?;
            }
        }
        writeln!(out)?;
    }

    Ok(out)
}

fn call_binlog_feedback(
    args: &Value,
    sessions: &mut SessionMap,
) -> Result<String, Box<dyn std::error::Error>> {
    let handle = req_session_handle(args)?;
    let file_path = req_str(args, "file")?;
    let text = req_str(args, "text")?;
    let root_cause = opt_str(args, "root_cause");
    let event_indices: Option<Vec<i64>> = args
        .get("event_indices")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect());

    // Verify session exists (for association).
    let session = sessions
        .get(handle)
        .ok_or_else(|| format!("no session with handle {handle}"))?;
    let binlog_path = session.path.clone();

    // Build the feedback record.
    let mut record = serde_json::Map::new();
    record.insert("session".to_owned(), Value::from(handle));
    record.insert("binlog".to_owned(), Value::from(binlog_path));
    record.insert("text".to_owned(), Value::from(text));
    if let Some(rc) = root_cause {
        record.insert("root_cause".to_owned(), Value::from(rc));
    }
    if let Some(indices) = event_indices {
        record.insert(
            "event_indices".to_owned(),
            Value::Array(indices.into_iter().map(Value::from).collect()),
        );
    }

    let json_line = serde_json::to_string(&Value::Object(record))?;

    // Append to file (create if needed).
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)?;
    use std::io::Write as _;
    writeln!(file, "{json_line}")?;

    Ok(format!("Feedback appended to {file_path}"))
}

fn call_binlog_setup(_args: &Value) -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!(
        "Add munin-binlog-mcp instructions to this repository so future Copilot \
         sessions automatically use the binlog tools when diagnosing MSBuild / \
         dotnet build / Visual Studio build problems.\n\
         \n\
         Two files are required. Create or update both. Adapt wording to fit any \
         existing conventions, but preserve the trigger keywords and tool list.\n\
         \n\
         ── 1. Append to `.github/copilot-instructions.md` (create if missing) ──\n\
         \n\
         ```markdown\n\
         {short}\
         ```\n\
         \n\
         ── 2. Write `.github/instructions/munin-binlog-mcp.instructions.md` (overwrite if present) ──\n\
         \n\
         ```markdown\n\
         {full}\
         ```\n",
        short = BINLOG_SETUP_SHORT,
        full = BINLOG_SETUP_FULL,
    ))
}

// ── formatting helpers ────────────────────────────────────────────────────────

/// Format a diagnostic event (error or warning) as compact, readable text.
fn format_diagnostic(
    out: &mut String,
    severity: &str,
    event_index: usize,
    fields: &BuildEventArgsFields,
    location: &DiagnosticLocation,
) -> std::fmt::Result {
    let code = location.code.as_deref().unwrap_or("???");
    let file = location.file.as_deref().unwrap_or("<unknown>");
    let line = location.line_number;
    let col = location.column_number;
    let msg = fields.message.as_deref().unwrap_or("");

    write!(out, "{severity} {code} in {file}")?;
    if line > 0 {
        write!(out, "({line}")?;
        if col > 0 {
            write!(out, ",{col}")?;
        }
        write!(out, ")")?;
    }
    writeln!(out, ": {msg}")?;

    if let Some(ref pf) = location.project_file {
        writeln!(out, "  Project: {pf}")?;
    }
    writeln!(out, "  Event index: {event_index}")?;
    writeln!(out)?;

    Ok(())
}

/// Get the message text for an event, returning empty string if unavailable.
fn event_message(index: &BinlogIndex, idx: usize) -> String {
    match index.get(idx) {
        Ok(Some(ref ev)) => event_fields(ev)
            .and_then(|f| f.message.as_deref())
            .unwrap_or("")
            .to_owned(),
        _ => String::new(),
    }
}

/// Get a short preview of an event for one-line summaries.
fn event_message_preview(index: &BinlogIndex, idx: usize) -> String {
    match index.get(idx) {
        Ok(Some(ref ev)) => format_event_one_line(ev),
        Ok(None) => "(not deserializable)".to_owned(),
        Err(_) => "(read error)".to_owned(),
    }
}

/// Extract the common `BuildEventArgsFields` from any event variant.
fn event_fields(ev: &BinlogEvent) -> Option<&BuildEventArgsFields> {
    match ev {
        BinlogEvent::BuildStarted(e) => Some(&e.fields),
        BinlogEvent::BuildFinished(e) => Some(&e.fields),
        BinlogEvent::ProjectStarted(e) => Some(&e.fields),
        BinlogEvent::ProjectFinished(e) => Some(&e.fields),
        BinlogEvent::TargetStarted(e) => Some(&e.fields),
        BinlogEvent::TargetFinished(e) => Some(&e.fields),
        BinlogEvent::TargetSkipped(e) => Some(&e.fields),
        BinlogEvent::TaskStarted(e) => Some(&e.fields),
        BinlogEvent::TaskFinished(e) => Some(&e.fields),
        BinlogEvent::TaskCommandLine(e) => Some(&e.fields),
        BinlogEvent::TaskParameter(e) => Some(&e.fields),
        BinlogEvent::Error(e) => Some(&e.fields),
        BinlogEvent::Warning(e) => Some(&e.fields),
        BinlogEvent::Message(e) => Some(&e.fields),
        BinlogEvent::CriticalBuildMessage(e) => Some(&e.fields),
        BinlogEvent::ProjectEvaluationStarted(e) => Some(&e.fields),
        BinlogEvent::ProjectEvaluationFinished(e) => Some(&e.fields),
        BinlogEvent::PropertyReassignment(e) => Some(&e.fields),
        BinlogEvent::UninitializedPropertyRead(e) => Some(&e.fields),
        BinlogEvent::PropertyInitialValueSet(e) => Some(&e.fields),
        BinlogEvent::EnvironmentVariableRead(e) => Some(&e.fields),
        BinlogEvent::ResponseFileUsed(e) => Some(&e.fields),
        BinlogEvent::AssemblyLoad(e) => Some(&e.fields),
        BinlogEvent::ProjectImported(e) => Some(&e.fields),
        _ => None,
    }
}

/// Format a one-line summary for any event variant.
fn format_event_one_line(ev: &BinlogEvent) -> String {
    match ev {
        BinlogEvent::ProjectStarted(ps) => {
            let pf = ps.project_file.as_deref().unwrap_or("?");
            let targets = ps.target_names.as_deref().unwrap_or("");
            if targets.is_empty() {
                format!("project={pf}")
            } else {
                format!("project={pf} targets={targets}")
            }
        }
        BinlogEvent::TargetStarted(ts) => {
            let name = ts.target_name.as_deref().unwrap_or("?");
            format!("target={name}")
        }
        BinlogEvent::TaskStarted(ts) => {
            let name = ts.task_name.as_deref().unwrap_or("?");
            format!("task={name}")
        }
        BinlogEvent::Error(e) => {
            let code = e.location.code.as_deref().unwrap_or("???");
            let msg = e.fields.message.as_deref().unwrap_or("");
            let truncated = truncate_str(msg, 120);
            format!("{code}: {truncated}")
        }
        BinlogEvent::Warning(w) => {
            let code = w.location.code.as_deref().unwrap_or("???");
            let msg = w.fields.message.as_deref().unwrap_or("");
            let truncated = truncate_str(msg, 120);
            format!("{code}: {truncated}")
        }
        _ => {
            let msg = event_fields(ev)
                .and_then(|f| f.message.as_deref())
                .unwrap_or("");
            truncate_str(msg, 120).to_owned()
        }
    }
}

/// Format full event detail for `binlog_event_detail`.
fn format_event_detail(out: &mut String, ev: &BinlogEvent) -> std::fmt::Result {
    match ev {
        BinlogEvent::BuildStarted(bs) => {
            writeln!(out, "Type: BuildStarted")?;
            format_fields_detail(out, &bs.fields)?;
        }
        BinlogEvent::BuildFinished(bf) => {
            writeln!(out, "Type: BuildFinished")?;
            writeln!(out, "Succeeded: {}", bf.succeeded)?;
            format_fields_detail(out, &bf.fields)?;
        }
        BinlogEvent::ProjectStarted(ps) => {
            writeln!(out, "Type: ProjectStarted")?;
            if let Some(ref pf) = ps.project_file {
                writeln!(out, "Project file: {pf}")?;
            }
            writeln!(out, "Project ID: {}", ps.project_id)?;
            if let Some(ref tn) = ps.target_names {
                writeln!(out, "Target names: {tn}")?;
            }
            if let Some(ref tv) = ps.tools_version {
                writeln!(out, "Tools version: {tv}")?;
            }
            format_fields_detail(out, &ps.fields)?;
            if let Some(ref props) = ps.property_list {
                writeln!(out, "Properties: {} entries", props.len())?;
            }
            if let Some(ref items) = ps.item_list {
                let total: usize = items.iter().map(|g| g.items.len()).sum();
                writeln!(out, "Items: {} groups, {} total items", items.len(), total)?;
            }
        }
        BinlogEvent::ProjectFinished(pf) => {
            writeln!(out, "Type: ProjectFinished")?;
            writeln!(out, "Succeeded: {}", pf.succeeded)?;
            if let Some(ref f) = pf.project_file {
                writeln!(out, "Project file: {f}")?;
            }
            format_fields_detail(out, &pf.fields)?;
        }
        BinlogEvent::TargetStarted(ts) => {
            writeln!(out, "Type: TargetStarted")?;
            if let Some(ref n) = ts.target_name {
                writeln!(out, "Target name: {n}")?;
            }
            if let Some(ref pf) = ts.project_file {
                writeln!(out, "Project file: {pf}")?;
            }
            if let Some(ref tf) = ts.target_file {
                writeln!(out, "Target file: {tf}")?;
            }
            if let Some(ref pt) = ts.parent_target {
                writeln!(out, "Parent target: {pt}")?;
            }
            writeln!(out, "Build reason: {}", ts.build_reason)?;
            format_fields_detail(out, &ts.fields)?;
        }
        BinlogEvent::TargetFinished(tf) => {
            writeln!(out, "Type: TargetFinished")?;
            writeln!(out, "Succeeded: {}", tf.succeeded)?;
            if let Some(ref n) = tf.target_name {
                writeln!(out, "Target name: {n}")?;
            }
            format_fields_detail(out, &tf.fields)?;
        }
        BinlogEvent::TaskStarted(ts) => {
            writeln!(out, "Type: TaskStarted")?;
            if let Some(ref n) = ts.task_name {
                writeln!(out, "Task name: {n}")?;
            }
            if let Some(ref pf) = ts.project_file {
                writeln!(out, "Project file: {pf}")?;
            }
            format_fields_detail(out, &ts.fields)?;
        }
        BinlogEvent::TaskFinished(tf) => {
            writeln!(out, "Type: TaskFinished")?;
            writeln!(out, "Succeeded: {}", tf.succeeded)?;
            if let Some(ref n) = tf.task_name {
                writeln!(out, "Task name: {n}")?;
            }
            format_fields_detail(out, &tf.fields)?;
        }
        BinlogEvent::Error(e) => {
            writeln!(out, "Type: Error")?;
            format_diagnostic_detail(out, &e.fields, &e.location)?;
        }
        BinlogEvent::Warning(w) => {
            writeln!(out, "Type: Warning")?;
            format_diagnostic_detail(out, &w.fields, &w.location)?;
        }
        BinlogEvent::Message(m) => {
            writeln!(out, "Type: Message")?;
            format_fields_detail(out, &m.fields)?;
        }
        _ => {
            writeln!(out, "Type: {}", event_variant_name(ev))?;
            if let Some(f) = event_fields(ev) {
                format_fields_detail(out, f)?;
            }
        }
    }
    Ok(())
}

/// Format common BuildEventArgsFields for detail output.
fn format_fields_detail(out: &mut String, fields: &BuildEventArgsFields) -> std::fmt::Result {
    if let Some(ref msg) = fields.message {
        writeln!(out, "Message: {msg}")?;
    }
    if let Some(ref ts) = fields.timestamp {
        writeln!(out, "Timestamp ticks: {}", ts.ticks)?;
    }
    if let Some(ref ctx) = fields.build_event_context {
        writeln!(
            out,
            "Context: project_context_id={} target_id={} task_id={}",
            ctx.project_context_id, ctx.target_id, ctx.task_id
        )?;
    }
    Ok(())
}

/// Format full diagnostic detail (error or warning) for event_detail.
fn format_diagnostic_detail(
    out: &mut String,
    fields: &BuildEventArgsFields,
    location: &DiagnosticLocation,
) -> std::fmt::Result {
    if let Some(ref code) = location.code {
        writeln!(out, "Code: {code}")?;
    }
    if let Some(ref file) = location.file {
        write!(out, "File: {file}")?;
        if location.line_number > 0 {
            write!(out, "({}", location.line_number)?;
            if location.column_number > 0 {
                write!(out, ",{}", location.column_number)?;
            }
            write!(out, ")")?;
        }
        writeln!(out)?;
    }
    if let Some(ref pf) = location.project_file {
        writeln!(out, "Project: {pf}")?;
    }
    if let Some(ref sub) = location.subcategory {
        writeln!(out, "Subcategory: {sub}")?;
    }
    format_fields_detail(out, fields)?;
    Ok(())
}

/// Get a human-readable name for a BinlogEvent variant.
fn event_variant_name(ev: &BinlogEvent) -> &'static str {
    match ev {
        BinlogEvent::BuildStarted(_) => "BuildStarted",
        BinlogEvent::BuildFinished(_) => "BuildFinished",
        BinlogEvent::ProjectStarted(_) => "ProjectStarted",
        BinlogEvent::ProjectFinished(_) => "ProjectFinished",
        BinlogEvent::TargetStarted(_) => "TargetStarted",
        BinlogEvent::TargetFinished(_) => "TargetFinished",
        BinlogEvent::TargetSkipped(_) => "TargetSkipped",
        BinlogEvent::TaskStarted(_) => "TaskStarted",
        BinlogEvent::TaskFinished(_) => "TaskFinished",
        BinlogEvent::TaskCommandLine(_) => "TaskCommandLine",
        BinlogEvent::TaskParameter(_) => "TaskParameter",
        BinlogEvent::Error(_) => "Error",
        BinlogEvent::Warning(_) => "Warning",
        BinlogEvent::Message(_) => "Message",
        BinlogEvent::CriticalBuildMessage(_) => "CriticalBuildMessage",
        BinlogEvent::ProjectEvaluationStarted(_) => "ProjectEvaluationStarted",
        BinlogEvent::ProjectEvaluationFinished(_) => "ProjectEvaluationFinished",
        BinlogEvent::PropertyReassignment(_) => "PropertyReassignment",
        BinlogEvent::UninitializedPropertyRead(_) => "UninitializedPropertyRead",
        BinlogEvent::PropertyInitialValueSet(_) => "PropertyInitialValueSet",
        BinlogEvent::EnvironmentVariableRead(_) => "EnvironmentVariableRead",
        BinlogEvent::ResponseFileUsed(_) => "ResponseFileUsed",
        BinlogEvent::AssemblyLoad(_) => "AssemblyLoad",
        BinlogEvent::ProjectImported(_) => "ProjectImported",
        _ => "Other",
    }
}

/// Map a `BinaryLogRecordKind` to its display name.
fn record_kind_name(kind: BinaryLogRecordKind) -> &'static str {
    match kind {
        BinaryLogRecordKind::EndOfFile => "EndOfFile",
        BinaryLogRecordKind::BuildStarted => "BuildStarted",
        BinaryLogRecordKind::BuildFinished => "BuildFinished",
        BinaryLogRecordKind::ProjectStarted => "ProjectStarted",
        BinaryLogRecordKind::ProjectFinished => "ProjectFinished",
        BinaryLogRecordKind::TargetStarted => "TargetStarted",
        BinaryLogRecordKind::TargetFinished => "TargetFinished",
        BinaryLogRecordKind::TaskStarted => "TaskStarted",
        BinaryLogRecordKind::TaskFinished => "TaskFinished",
        BinaryLogRecordKind::Error => "Error",
        BinaryLogRecordKind::Warning => "Warning",
        BinaryLogRecordKind::Message => "Message",
        BinaryLogRecordKind::TaskCommandLine => "TaskCommandLine",
        BinaryLogRecordKind::CriticalBuildMessage => "CriticalBuildMessage",
        BinaryLogRecordKind::ProjectEvaluationStarted => "ProjectEvaluationStarted",
        BinaryLogRecordKind::ProjectEvaluationFinished => "ProjectEvaluationFinished",
        BinaryLogRecordKind::ProjectImported => "ProjectImported",
        BinaryLogRecordKind::TargetSkipped => "TargetSkipped",
        BinaryLogRecordKind::PropertyReassignment => "PropertyReassignment",
        BinaryLogRecordKind::UninitializedPropertyRead => "UninitializedPropertyRead",
        BinaryLogRecordKind::EnvironmentVariableRead => "EnvironmentVariableRead",
        BinaryLogRecordKind::PropertyInitialValueSet => "PropertyInitialValueSet",
        BinaryLogRecordKind::TaskParameter => "TaskParameter",
        BinaryLogRecordKind::ResponseFileUsed => "ResponseFileUsed",
        BinaryLogRecordKind::AssemblyLoad => "AssemblyLoad",
        _ => "Unknown",
    }
}

/// Parse a case-insensitive record kind name string to the enum.
fn parse_record_kind(name: &str) -> Option<BinaryLogRecordKind> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "buildstarted" => Some(BinaryLogRecordKind::BuildStarted),
        "buildfinished" => Some(BinaryLogRecordKind::BuildFinished),
        "projectstarted" => Some(BinaryLogRecordKind::ProjectStarted),
        "projectfinished" => Some(BinaryLogRecordKind::ProjectFinished),
        "targetstarted" => Some(BinaryLogRecordKind::TargetStarted),
        "targetfinished" => Some(BinaryLogRecordKind::TargetFinished),
        "taskstarted" => Some(BinaryLogRecordKind::TaskStarted),
        "taskfinished" => Some(BinaryLogRecordKind::TaskFinished),
        "error" => Some(BinaryLogRecordKind::Error),
        "warning" => Some(BinaryLogRecordKind::Warning),
        "message" => Some(BinaryLogRecordKind::Message),
        "taskcommandline" => Some(BinaryLogRecordKind::TaskCommandLine),
        "criticalbuildmessage" => Some(BinaryLogRecordKind::CriticalBuildMessage),
        "projectedevaluationstarted" | "projectevaluationstarted" => {
            Some(BinaryLogRecordKind::ProjectEvaluationStarted)
        }
        "projectedevaluationfinished" | "projectevaluationfinished" => {
            Some(BinaryLogRecordKind::ProjectEvaluationFinished)
        }
        "projectimported" => Some(BinaryLogRecordKind::ProjectImported),
        "targetskipped" => Some(BinaryLogRecordKind::TargetSkipped),
        "propertyreassignment" => Some(BinaryLogRecordKind::PropertyReassignment),
        "uninitializedpropertyread" => Some(BinaryLogRecordKind::UninitializedPropertyRead),
        "environmentvariableread" => Some(BinaryLogRecordKind::EnvironmentVariableRead),
        "propertyinitialvalueset" => Some(BinaryLogRecordKind::PropertyInitialValueSet),
        "taskparameter" => Some(BinaryLogRecordKind::TaskParameter),
        "responsefileused" => Some(BinaryLogRecordKind::ResponseFileUsed),
        "assemblyload" => Some(BinaryLogRecordKind::AssemblyLoad),
        _ => None,
    }
}

/// Truncate a string to `max_len` characters, appending "..." if truncated.
fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // Find a safe char boundary.
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

#[cfg(test)]
mod tests;
