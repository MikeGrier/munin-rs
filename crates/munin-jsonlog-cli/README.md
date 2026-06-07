# munin-jsonlog

CLI for converting between MSBuild binary log (`.binlog`) and munin's
jsonlog format, with optional redaction.

## Usage

```text
munin-jsonlog dump <INPUT>... [-o <FILE|DIR>] [--pretty] [REDACT-FLAGS] [-v]
munin-jsonlog pack <INPUT>... [-o <FILE|DIR>] [REDACT-FLAGS] [-v]
munin-jsonlog analyze-cpp <INPUT>... [-o <FILE|DIR>] [--pretty]
                          [--root [NAME=]PATH ...] [--auto-root]
                          [--locale-strict] [-v]
```

### Input expansion

Every subcommand accepts one or more positional inputs. Each input is
either a literal path or a glob pattern (`*`, `?`, `[...]`, and `**`
for recursive descent). Patterns are expanded by the CLI itself —
no shell expansion is required, which makes the same invocation work
identically on Windows (`cmd.exe` and PowerShell) and POSIX shells.
A pattern that matches no files is an error.

Example:

```text
munin-jsonlog analyze-cpp "build/**/*.binlog" -o out/ --auto-root
```

### Output routing

- **Single input, `-o` omitted**: result is written to stdout.
- **Single input, `-o FILE`**: result is written to that file.
- **Multiple inputs, `-o` omitted**: each result is written next to
  its input as `<stem>.<ext>` (`.jsonlog` for `dump`, `.binlog` for
  `pack`, `.cpp.json` for `analyze-cpp`).
- **Multiple inputs, `-o DIR`**: `DIR` must already exist and be a
  directory; each result is written there as `<stem>.<ext>`. Passing
  a non-directory `-o` with multiple inputs is an error.

### `-v` / `--verbose`

Pass `-v` (or `--verbose`) at any position to print a per-input
progress line to **stderr** as each glob-expanded input is processed:

```text
[1/3] ci/runs/a.binlog -> ci/cpp-json/a.cpp.json
[2/3] ci/runs/b.binlog -> ci/cpp-json/b.cpp.json
[3/3] ci/runs/c.binlog -> ci/cpp-json/c.cpp.json
```

When output is going to stdout, the destination is shown as
`<stdout>`. The flag is global and applies to all subcommands.

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
in the binlog. When multiple binlogs are processed in one
invocation, `--auto-root` is computed independently per binlog.

By default `analyze-cpp` is best-effort across `/showIncludes`
locales (non-English compiler output yields empty include lists for
the affected TUs). Pass `--locale-strict` to fail the run instead
when a TU's `/showIncludes` output cannot be parsed.

Example:

```text
munin-jsonlog analyze-cpp build.binlog --auto-root --pretty -o build.cpp.json
munin-jsonlog analyze-cpp "ci/runs/**/*.binlog" --auto-root -o ci/cpp-json/
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
