# munin-jsonlog

CLI for converting between MSBuild binary log (`.binlog`) and munin's
jsonlog format, with optional redaction.

## Usage

```text
munin-jsonlog dump <input.binlog> [-o <output.jsonlog>] [--pretty] [REDACT-FLAGS]
munin-jsonlog pack <input.jsonlog> [-o <output.binlog>] [REDACT-FLAGS]
munin-jsonlog analyze-cpp <input.binlog> [-o <output.json>] [--pretty]
                          [--root [NAME=]PATH ...] [--auto-root]
                          [--locale-strict]
```

Output defaults to stdout when `-o` is omitted.

### `analyze-cpp` subcommand

Parses an MSBuild `.binlog` and emits a structured
`CppBuildAnalysis` JSON document describing every C++ project in
the build: one entry per `cl.exe` translation unit (with parsed
defines, includes, and per-TU sources) and one per `link.exe`
invocation (with parsed inputs and dropped libraries).

Roots are user-named directories used to convert absolute paths
into portable `roots[i]/relative/path` references. Provide them
with `--root NAME=PATH` (or `--root PATH` to derive the name from
the path's leaf component), or pass `--auto-root` to derive a
single root from the longest common ancestor of every project file
in the binlog.

By default `analyze-cpp` is best-effort across `/showIncludes`
locales (non-English compiler output yields empty include lists for
the affected TUs). Pass `--locale-strict` to fail the run instead
when a TU's `/showIncludes` output cannot be parsed.

Example:

```text
munin-jsonlog analyze-cpp build.binlog --auto-root --pretty -o build.cpp.json
```

### Redaction flags (shared)

- `--redact-token VAL` — replace every occurrence of the literal `VAL`
  with `*****`. Repeatable.
- `--redact-regex 'PAT=>REPL'` — replace every match of regex `PAT` with
  `REPL`. The literal sequence `=>` separates the two. Repeatable.
- `--redact-username` — walk the `BuildStarted` event's environment for
  `USERNAME` / `USER` / `USERPROFILE` / `HOME` and scrub those values
  to `REDACTED-USER`.
- `--redact-common` — install munin's built-in common-pattern catalog
  (see `D-RDX-1` in `crates/munin-msbuild/DESIGN-NOTES.md`).
