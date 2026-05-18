# GitHub Copilot Instructions — cargo-mcp-rs

## Cargo commands — use MCP tools, never the terminal

This repository ships a **cargo-mcp MCP server** that exposes every common
`cargo` command as a first-class MCP tool. The server provides structured
output, streaming progress notifications, and safe elicitation for destructive
operations.

**Rule:** When working in any Rust/Cargo project, ALWAYS use the `cargo_*` MCP
tools listed below instead of running `cargo` commands in a PowerShell or bash
terminal. This applies even inside a larger workflow — do not switch to the
terminal for cargo just because a previous step used the terminal.

| MCP tool | Replaces |
|---|---|
| `cargo_metadata` | `cargo metadata` |
| `cargo_check` | `cargo check` |
| `cargo_build` | `cargo build` |
| `cargo_test` | `cargo test` |
| `cargo_clippy` | `cargo clippy` |
| `cargo_fmt_check` | `cargo fmt --check` |
| `cargo_fmt` | `cargo fmt` |
| `cargo_tree` | `cargo tree` |
| `cargo_doc` | `cargo doc` |
| `cargo_clean` | `cargo clean` |
| `cargo_update` | `cargo update` |
| `cargo_fix` | `cargo fix` |
| `cargo_add` | `cargo add` |
| `cargo_remove` | `cargo remove` |
| `cargo_publish` | `cargo publish` |

### When to use each tool

- **Check / build / test / clippy / doc** — always prefer these over terminal;
  they stream structured progress back to VS Code.
- **`cargo_fmt`** — run before every commit; fix all formatting issues before
  pushing. Use `cargo_fmt_check` in CI-like workflows to enforce this.
- **`cargo_clippy`** — run before every commit; fix all warnings before pushing.
- **`cargo_clean`** — use before a clean rebuild; do not run `cargo clean` in
  the terminal.
- **`cargo_add` / `cargo_remove` / `cargo_update`** — always use for
  dependency management; never manually edit Cargo.toml for dependency version
  changes when these tools are available.
- **`cargo_fix`** — use after `cargo_check` or `cargo_clippy` to apply
  machine-applicable fixes in bulk.
- **`cargo_publish`** — always run with `dry_run: true` first to validate;
  only publish for real when the dry-run succeeds.

## C# projects — always build with a binlog

Building with an MSBuild binary log (`.binlog`) is good hygiene: it captures a
complete, structured record of the build that can be inspected after the fact
with tools like the MSBuild Structured Log Viewer or this repository's own
reader. It costs almost nothing and is invaluable for diagnosing build issues.

**Rule:** Any command that builds a C# project or solution MUST emit a binlog
next to what is being built. This applies whether the build is invoked via
`dotnet build`, `dotnet msbuild`, `MSBuild.exe` directly, a `dotnet test` /
`dotnet publish` / `dotnet pack` that performs a build, or a custom script
that wraps any of the above. See the [C# Dev Kit / C# extension build
documentation](https://learn.microsoft.com/dotnet/core/tools/dotnet-build)
and the [MSBuild command-line reference](https://learn.microsoft.com/visualstudio/msbuild/msbuild-command-line-reference)
for the full set of build entry points; the rule applies to all of them.

How to satisfy the rule:

- **Pass `-bl:<path>.binlog`** to whichever tool is doing the build. The
  `-bl` (a.k.a. `/bl` or `--binaryLogger`) switch is understood by
  `dotnet build`, `dotnet msbuild`, `dotnet test`, `dotnet publish`,
  `dotnet pack`, and `MSBuild.exe` alike.
- **Place the binlog next to the project or solution being built** —
  `<project-dir>/msbuild.binlog` for a `.csproj`, or `<sln-dir>/<sln>.binlog`
  for a `.sln`. Do not write binlogs into `bin/` or `obj/` (they get cleaned).
- **Update existing tasks rather than adding parallel ones.** If a
  `.vscode/tasks.json`, `Makefile`, PowerShell script, or CI workflow already
  has a build step for a C# project, add the `-bl:` flag to that existing
  command. Do not add a second "build with binlog" task that duplicates it.
- **Leave Dev Kit's auto-generated build/run actions alone.** They do not
  emit binlogs, but they also are not under our control. If you need a binlog
  from a Dev Kit-driven build, run the corresponding explicit task instead.

Example task in `.vscode/tasks.json`:

```jsonc
{
  "label": "build <project>",
  "type": "process",
  "command": "dotnet",
  "args": [
    "build",
    "${workspaceFolder}/path/to/<project>/<project>.csproj",
    "-bl:${workspaceFolder}/path/to/<project>/msbuild.binlog"
  ],
  "problemMatcher": "$msCompile",
  "group": "build"
}
```

For a solution-level build, target the `.sln` and place the binlog beside it:

```jsonc
"args": [
  "build",
  "${workspaceFolder}/path/to/MySolution.sln",
  "-bl:${workspaceFolder}/path/to/MySolution.binlog"
]
```

### Covering Dev Kit-driven builds (ask the user first)

The per-task `-bl:` flag does not cover builds that the C# Dev Kit triggers
on its own (Solution Explorer build/run, IntelliSense design-time builds,
test discovery, etc.). The cleanest way to cover those uniformly is a
`Directory.Build.rsp` file at the repo root containing `-bl:msbuild.binlog`,
which MSBuild auto-prepends to every command line.

This is a repo-wide change that affects every contributor's local builds, so
**do not add it silently**. When you are about to set up or modify C# build
configuration in a repo that does not already have a `Directory.Build.rsp`
covering binlogs, surface the choice to the user before doing anything. Ask
something like:

> This repo doesn't capture binlogs for Dev Kit-driven builds (Solution
> Explorer, design-time, test discovery). How would you like to handle that?
>
> 1. Add a repo-wide `Directory.Build.rsp` so every MSBuild invocation emits
>    a binlog (covers Dev Kit, CLI, and CI uniformly).
> 2. Only add `-bl:` to the specific task I'm setting up right now.
> 3. Skip — don't capture binlogs from Dev Kit.

Proceed based on the user's answer. If they pick option 1, create
`Directory.Build.rsp` at the repo root with the single line `-bl:msbuild.binlog`
and mention the caveats: the path is relative to the build's working
directory, and concurrent builds will overwrite the same file.

## File encoding

Source files in this repository may contain non-ASCII characters. When editing
files, prefer the editor's built-in edit tools over PowerShell file I/O
(`Set-Content`, `Out-File`, `>`) to avoid encoding corruption.
