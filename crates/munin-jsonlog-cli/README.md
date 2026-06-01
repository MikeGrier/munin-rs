# munin-jsonlog

CLI for converting between MSBuild binary log (`.binlog`) and munin's
jsonlog format, with optional redaction.

## Usage

```text
munin-jsonlog dump <input.binlog> [-o <output.jsonlog>] [--pretty] [REDACT-FLAGS]
munin-jsonlog pack <input.jsonlog> [-o <output.binlog>] [REDACT-FLAGS]
```

Output defaults to stdout when `-o` is omitted.

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
