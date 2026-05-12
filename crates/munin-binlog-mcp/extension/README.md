# Munin Binlog MCP for VS Code

**Let GitHub Copilot read your MSBuild binary logs.**

Drop a `.binlog` file into your workspace and ask Copilot what went wrong.
Instead of scrolling thousands of lines of build output, Copilot calls a set
of structured tools that pull just the errors, warnings, project tree, and
task timeline it needs.

Install, reload, point Copilot at a `.binlog` file. That's it.

> **Platforms:** Pre-built binaries ship for **Windows x64 and arm64**.
> Linux/macOS users should
> [build from source](https://github.com/MikeGrier/munin-rs#building-from-source).

---

## What you get

- **Structured access to `.binlog` files** -- errors, warnings, project tree,
  task timeline, item groups, and properties exposed as MCP tools instead of
  unstructured text.
- **Multiple binlogs at once** -- each open binlog gets a session handle so
  Copilot can compare two builds side by side.
- **Indexed event lookup** -- jump from an error to the events surrounding
  it in the same project/target scope.
- **Zero MCP config** -- the extension bundles the server and registers it
  with VS Code automatically. No `mcp.json` to edit.

---

## Quick start

1. **Install this extension.**
2. **Reload VS Code** so the MCP server registers.
3. Open Copilot Chat in **Agent mode** and ask, for example:

   > "Open `build.binlog` and tell me what errors occurred."

   Copilot will use the `binlog_open` and `binlog_errors` tools to answer.

---

## Tools

| Tool | Purpose |
|---|---|
| `binlog_open` | Open a `.binlog` file; return a session handle and build summary |
| `binlog_close` | Close a session and free resources |
| `binlog_summary` | Build result, project list, error/warning counts, duration |
| `binlog_errors` | All Error events with file, line, code, message, project |
| `binlog_warnings` | Warning events, optionally filtered by code or project |
| `binlog_project_tree` | Hierarchical project / target / task view |
| `binlog_events` | Filtered event listing by kind, project, target, task, text |
| `binlog_event_detail` | Full detail for a single event by index |
| `binlog_properties` | MSBuild properties from ProjectStarted events |
| `binlog_items` | Item groups from ProjectStarted events |
| `binlog_error_context` | Surrounding events for a given error in scope |
| `binlog_task_timeline` | Chronological task list with success/failure status |
| `binlog_feedback` | Append structured feedback to a JSONL file |

[Full tool reference -> ](https://github.com/MikeGrier/munin-rs/tree/main/crates/munin-binlog-mcp)

---

## Trust & transparency

Installing a VS Code extension that ships a native binary is a real trust
decision. Here's what's in the box:

- **Written in Rust.** Both the parser (`munin`) and the MCP server
  (`munin-binlog-mcp`) are pure-Rust crates. Rust's memory-safety
  guarantees apply by default -- no `unsafe` is used in either crate.
- **Read-only.** The server only reads `.binlog` files; it never modifies
  them. The single write path is the optional `binlog_feedback` tool, which
  appends to a user-specified JSONL file.
- **No telemetry, no network calls.** The server speaks JSON-RPC on stdio
  and does nothing else.
- **Releases are built entirely in GitHub Actions.** Every published VSIX
  is produced from a tagged commit by the
  [`publish-extension`](https://github.com/MikeGrier/munin-rs/blob/main/.github/workflows/publish-extension.yml)
  workflow on GitHub-hosted runners -- `cargo build --release --locked` for
  the bundled binary, then `vsce package --no-dependencies`, then Marketplace
  upload. The publish step is gated by a required-reviewer environment, so
  no commit reaches the Marketplace without a human approval *after* CI has
  built the artifacts. **No developer machine ever touches the published bits.**
- **Reproducible inputs.** Both the Rust build (`--locked`) and the
  extension's `npm` install (`npm ci` against a checked-in
  `package-lock.json`) refuse to use any dependency version not pinned in
  the lockfiles.
- **The source is the source.** The full repository, including all CI
  configuration, is at
  [github.com/MikeGrier/munin-rs](https://github.com/MikeGrier/munin-rs).

---

## Requirements

- **VS Code** 1.101 or later
- **GitHub Copilot Chat** with Agent mode enabled

No Rust toolchain or .NET install is required to *use* the extension.

---

## Settings

| Setting | Default | Description |
|---|---|---|
| `munin-binlog-mcp.binaryPath` | _(bundled)_ | Override the path to the `munin-binlog-mcp` binary. Intended for development against a locally-built server. |
| `munin-binlog-mcp.extraArgs` | `[]` | Extra command-line arguments passed to the server on startup. |

---

## Commands

- **munin-binlog-mcp: Copy bundled server binary path** -- copies the
  bundled binary path to the clipboard.
- **munin-binlog-mcp: Show bundled server version** -- displays the bundled
  server version.

---

## Links

- [Source code](https://github.com/MikeGrier/munin-rs)
- [Full documentation](https://github.com/MikeGrier/munin-rs/tree/main/crates/munin-binlog-mcp)
- [Report a bug](https://github.com/MikeGrier/munin-rs/issues)
- [Discussions / Q&A](https://github.com/MikeGrier/munin-rs/discussions)
- [Release notes](https://github.com/MikeGrier/munin-rs/releases)

## License

MIT
