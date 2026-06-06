<!-- Copyright (c) Michael Grier -->
# Completed Checklist

Append-only record of completed checklist groups.

## Moved 2026-06-01 --- Jsonlog format + dumper, packer, and `munin-jsonlog` CLI

<!-- Copyright (c) Michael Grier -->
# Jsonlog Checklist

Design decisions for this work live in [DESIGN-NOTES.md](DESIGN-NOTES.md)
(decision IDs `D-JL-*`).

## M1 — Schema and module scaffold

- [x] **JL-1.1** Add `crates/munin-msbuild/src/jsonlog.rs` and declare
  `pub mod jsonlog;` in `crates/munin-msbuild/src/lib.rs`. The module is
  empty apart from a doc comment.
- [x] **JL-1.2** Add `serde` (with `derive`), `serde_json`, and
  `base64` as dependencies of `crates/munin-msbuild` via `cargo_add`.
- [x] **JL-1.3** Define the on-disk schema in `jsonlog::schema`:
  `JsonlogFile { munin_jsonlog_version: u32, header, strings:
  Vec<String>, name_value_lists: Vec<Vec<[u32; 2]>>, archives:
  Vec<ArchiveB64>, events: Vec<JsonlogEvent> }` and `JsonlogEvent`
  tagged by `kind` with either a `decoded` payload or `payload_b64`
  for unknowns. Include `byte_offset: u64` per event.
- [x] **JL-1.4** In `jsonlog::decoded`, add one serde struct per
  `BinlogEvent` variant mirroring its fields with JSON-friendly types
  (no `Cursor`, no raw bytes). One file per ~10 variants is fine.
- [x] **JL-1.5** Add `From<&events::FooEvent> for decoded::Foo` and
  `From<decoded::Foo> for events::FooEvent` for every variant, with a
  unit test per variant asserting `Foo -> JSON -> Foo` equality.

## M2 — Dumper (binlog → jsonlog)

- [x] **JL-2.1** Expose a crate-private `BinlogIndex::payload_bytes(i)
  -> &[u8]` accessor (or `entries_raw()`) so the dumper can read the
  stored payload without re-parsing.
- [x] **JL-2.2** Implement `jsonlog::dump_index(&BinlogIndex, impl
  Write) -> Result<()>`: write header, strings, NVL pairs, archives
  (base64), and each event in stored order. On decode success emit
  `decoded`; on decode failure emit `payload_b64` of the stored bytes.
- [x] **JL-2.3** Add a `--pretty` analogue: `dump_index_pretty` that
  uses `serde_json::to_writer_pretty`. The non-pretty form uses
  `to_writer`.
- [x] **JL-2.4** Unit tests in `jsonlog/tests.rs` covering at least 10
  cases: empty index, header-only, each major event kind, an archive
  blob, and a forced-fallback unknown-kind payload.
- [x] **JL-2.5** Integration test
  `crates/munin-msbuild/tests/jsonlog_dump.rs`: open
  `tests/data/hello.binlog`, dump to a `Vec<u8>`, deserialize as
  `JsonlogFile`, assert `events.len() == index.len()` and that every
  event either has `decoded` matching `index.get(i)` or a
  `payload_b64` matching `payload_bytes(i)`.

## M3 — Writer path (jsonlog → binlog)

- [x] **JL-3.1** Add `events::write_*` for every `read_*` in
  `crates/munin-msbuild/src/events.rs`. Each writer takes a context
  carrying the string and NVL dedup tables and emits the exact byte
  sequence the matching reader would consume. Unit-test each writer
  against its reader (round-trip on random-but-fixed inputs).
- [x] **JL-3.2** Implement `BinlogIndex::open_json(impl Read) ->
  Result<Self>`: parse `JsonlogFile`; for `payload_b64` events use
  bytes verbatim; for `decoded` events convert to `BinlogEvent`, then
  re-encode via the M3.1 writers into payload bytes; populate the
  same private fields as `open(...)`.
- [x] **JL-3.3** Implement `BinlogIndex::write_binlog(&self, impl
  Write) -> Result<()>`: emit a gzip-compressed stream containing
  header, interleaved `String` / `NameValueList` aux records sized to
  match the original index, each event record with 7-bit kind/length
  framing, terminated by `EndOfFile`.
- [x] **JL-3.4** Integration test
  `crates/munin-msbuild/tests/jsonlog_open.rs`: hand-author a small
  `.jsonlog` literal as `include_str!("data/fake.jsonlog")`, open via
  `open_json`, verify `len()`, each `meta(i)`, and each decoded
  `get(i)`.
- [x] **JL-3.5** Integration test
  `crates/munin-msbuild/tests/jsonlog_roundtrip.rs`: load
  `hello.binlog`, dump to jsonlog, pack jsonlog back to a fresh binlog
  via `write_binlog`, re-open the packed binlog with `open`, assert
  every `meta` and decoded `get` matches the original index.

## M3.5 — Redaction

Provide a string-table-based redactor on `BinlogIndex` for stripping
usernames, paths, secrets, and other sensitive data from a captured
binlog before sharing it (e.g. as a `.jsonlog`). Functional parity with
`binlogtool redact` (MIT-licensed reference, source not open) plus
caller-extensible regex and exact-token rules.

- [x] **JL-3.5-R1** Add `regex = "1"` to `munin-msbuild` and define
  `pub mod redact` in `lib.rs`. In `src/redact.rs` define
  `pub struct Redactor { rules: Vec<Rule> }` with private `enum Rule
  { Exact { needle, replacement }, Regex { re, replacement } }`. Add
  `Redactor::new()`, `with_token(value)`, `with_regex(pat, repl) ->
  Result<Self, MuninError>`, and `apply(&self, index: &mut
  BinlogIndex)` that rewrites every entry of the index's string table
  in place (longest-needle-first for exact rules; regex rules applied
  in insertion order after exact rules). Add a `BinlogIndex` method
  giving `redact` `&mut` access to the underlying `StringTable`
  (private to the crate). Unit-test: a tiny index whose strings
  contain a known token gets that token replaced; round-trip via
  `dump_index` + `open_json` still works.
- [x] **JL-3.5-R2** Add `with_common_patterns(self) -> Self` builtin.
  Document in `DESIGN-NOTES.md` (`D-RDX-1`) that this is munin's own
  specification of "common sensitive patterns", not derived from any
  closed-source catalog. Initial set, each with a fixed replacement
  literal (replacement strings recorded in design notes): URLs with
  embedded `user:pass@`, GitHub PATs (`ghp_`, `gho_`, `ghu_`, `ghs_`,
  `ghr_` prefixes), Azure DevOps PATs (52-char base32),
  `Bearer <token>` HTTP header values, and email addresses. Each
  pattern gets its own unit test against a synthesized string.
- [x] **JL-3.5-R3** Add `with_autodetect_username(self) -> Self`
  builtin. On `apply`, walk the index's events looking for a
  `BuildStarted` event and its captured environment dictionary; pull
  `USERNAME` / `USER` / `USERPROFILE` / `HOME` values; for each
  non-empty value, register `Exact` rules that map the literal
  username and its appearance inside the standard user-home path
  prefixes (`C:\Users\<u>\`, `/home/<u>/`, `/Users/<u>/`) to a fixed
  `REDACTED-USER` replacement. If no `BuildStarted` env data is
  available, the rule is a no-op (do NOT fall back to the host's
  current username — that would leak the redactor's environment).
  Unit-test against a synthesized index whose `BuildStarted` event
  contains a known `USERNAME`.
- [x] **JL-3.5-R4** Integration test
  `crates/munin-msbuild/tests/redact_roundtrip.rs`: open
  `hello.binlog`, run a `Redactor` configured with
  `with_token("HelloBinlog")` + `with_common_patterns()` +
  `with_autodetect_username()`, dump to jsonlog, re-open via
  `open_json`, assert (a) the token does not appear in any string,
  (b) at least one string changed vs. the unredacted index, and (c)
  the index still round-trips through `write_binlog` + `open`.

## M4 — CLI, MCP test helper, docs

- [x] **JL-4.1** Create `crates/munin-jsonlog-cli` (binary name
  `munin-jsonlog`) as a new workspace member; add `clap` (derive
  feature) and depend on `munin-msbuild`. Define the `Cli` struct
  with `dump` and `pack` subcommands but no logic yet.
- [x] **JL-4.2** Implement `dump <input.binlog> [-o <out>] [--pretty]
  [--redact-token VAL]... [--redact-regex 'PAT=>REPL']...
  [--redact-username] [--redact-common]` using `BinlogIndex::open` +
  `Redactor` + `jsonlog::dump_index`. Defaults to stdout. Route all
  output through a single writer abstraction (one trait, one method)
  — no scattered `println!` calls.
- [x] **JL-4.3** Implement `pack <input.jsonlog> [-o <out>]` with the
  same redaction flags as `dump`, using `BinlogIndex::open_json` +
  `Redactor` + `BinlogIndex::write_binlog`. Defaults to stdout. Use
  the same writer abstraction as `dump`.
- [x] **JL-4.4** In `crates/munin-binlog-mcp/tests`, add a test helper
  `fn open_jsonlog_fixture(name: &str) -> BinlogIndex` and port one
  existing hand-rolled-fixture test to use it. The `binlog_open` MCP
  tool itself stays binlog-only.
- [x] **JL-4.5** Integration test
  `crates/munin-jsonlog-cli/tests/cli_roundtrip.rs`: invoke the CLI
  binary via `assert_cmd` (add as dev-dep) on `hello.binlog`: `dump`
  to a temp file, `pack` it back, then open both with
  `BinlogIndex::open` and assert event equivalence. Update root
  `README.md`, `crates/munin-msbuild/README.md`, and add
  `crates/munin-jsonlog-cli/README.md`.


## Moved 2026-04-19 — CPP-4.x Milestone 4: `link.exe /VERBOSE` parsing

- [x] **CPP-4.1.** **Spike.** Run the M3-stage pipeline against
  ≥5 real binlogs from the user's corpus; capture raw Link task
  message streams to `.scratch/link-verbose-samples/`. Catalog
  observed line patterns (searched / loaded / found / referenced /
  unused / dropped / unresolved). Write findings as D-CPP-LINK1 in
  `crates/munin-cppbuild/DESIGN-NOTES.md`.
- [x] **CPP-4.2.** Link task identification and command-line
  tokenizer (`link_cmdline.rs`): output file, `/LIBPATH`, listed `.obj`
  and `.lib` inputs, other switches.
- [x] **CPP-4.3.** Verbose output parser in `link_verbose.rs` based
  on D-CPP-LINK1. Produce: `inputs[] { path, kind: obj|lib,
  origin: direct|transitive|searched, referenced: bool }` and
  `dropped[] { path, reason }`. Unit tests against captured samples.
- [x] **CPP-4.4.** Wire link analysis into project-invocation
  emitter; aliases applied via the per-project alias table built
  after both CL and Link data are collected.
- [x] **CPP-4.5.** Integration test against a synthetic C++ project
  that links a static lib and an exe with verbose output; verify
  inputs / dropped / aliases.

End-to-end verified against the real corpus binlog
`src__agent__lib__PreRestartState_Retail_Amd64.binlog`: the
`link_pipeline_demo` example reports 13/236 inputs referenced and
214 unused libraries (the entire `absl_*` family) — confirming the
pipeline surfaces unreferenced `AdditionalDependencies`.


## Moved 2026-06-05 — CPP-1.x..3.x, 4.6, 5.x: C/C++ binlog analysis (`munin-cppbuild`)

Derive, from MSBuild `.binlog` files produced by `.vcxproj` builds, a
structured JSON document describing what each project built and what
it consumed — sourced from `cl.exe /showIncludes` and `link.exe
/VERBOSE` output captured in the binlog.

(CPP-4.1..4.5 — Milestone 4 link parsing — were moved separately on
2026-04-19, above.)

### Scope decisions (locked)

- **D-CPP-1.** One JSON file per binlog. Top-level shape:
  `{ header, projects: [...] }`.
- **D-CPP-2.** Path rooting: `--root <dir>` (repeatable to support
  separate drives, e.g. NuGet cache on D:\). Paths inside any root are
  emitted relative to that root with the root's index/name; paths
  outside all roots are emitted absolute. `--auto-root` derives one
  root from the longest common ancestor of project paths in the
  binlog. Roots are recorded in the JSON header.
- **D-CPP-3.** Alias scope is **per project-invocation**.
- **D-CPP-4.** Alias algorithm: leaf-name if unique → else last two
  segments → else `<first-segment>\..\<leaf>` → else `<leaf>#n`.
- **D-CPP-5.** English `cl.exe` / `link.exe` only for v1.
- **D-CPP-6.** New library crate `crates/munin-cppbuild/`; new
  subcommand `analyze-cpp` in `munin-jsonlog-cli`.
- **D-CPP-7.** Tiny sanitized synthetic binlogs in tests; large real
  corpus gated on env var `MUNIN_CPPBUILD_TEST_BINLOGS=<dir>`.
- **D-CPP-8.** Linker `/VERBOSE` granularity deferred to M4 (resolved
  in CPP-4.1 spike, see D-CPP-LINK1 in crate DESIGN-NOTES).

### Milestone 1 — Foundations: crate, schema, path aliasing

- [x] **CPP-1.1.** Create `crates/munin-cppbuild/` library crate
  (`Cargo.toml`, `src/lib.rs`, `README.md`, `LICENSE`,
  `DESIGN-NOTES.md`). Wire into root `Cargo.toml` workspace.
  Dependencies: `munin-msbuild`, `serde`, `serde_json`.
- [x] **CPP-1.2.** Write the JSON schema in
  `crates/munin-cppbuild/DESIGN-NOTES.md` as **D-CPPSCHEMA-1**.
- [x] **CPP-1.3.** Implement multi-root path canonicalizer in
  `path_root.rs`.
- [x] **CPP-1.4.** Implement alias builder in `alias.rs` per D-CPP-4.
- [x] **CPP-1.5.** Integration test: synthetic in-memory project
  through schema + aliaser.

### Milestone 2 — Project-invocation extraction from binlogs

- [x] **CPP-2.1.** Add `walk.rs`.
- [x] **CPP-2.2.** Surface command-line / global properties with a
  `source` discriminator (`command_line` | `project`).
- [x] **CPP-2.3.** Project-level JSON emitter producing M1
  `projects[]` entries with empty `sources` / `outputs`.
- [x] **CPP-2.4.** Synthetic-binlog fixture (built programmatically
  per D-CPP-FIXTURE1).
- [x] **CPP-2.5.** Integration test against the M2.4 fixture.

### Milestone 3 — `cl.exe /showIncludes` parsing

- [x] **CPP-3.1.** CL task identification.
- [x] **CPP-3.2.** Command-line tokenizer in `cl_cmdline.rs`.
- [x] **CPP-3.3.** `/showIncludes` parser in `cl_showincludes.rs`.
- [x] **CPP-3.4.** Directive-text → resolved-file mapping derivation.
- [x] **CPP-3.5.** Integration test for `a.cpp → a.h → b.h → c.h`
  with duplicates.

### Milestone 4.5 — CL batch-mode support (CPP-4.6)

Real-corpus verification of M3 surfaced a defect class: cl.exe is
routinely invoked in batch mode with N source files in one command
line. The pipeline now splits per-TU on bare-basename markers.

- [x] **CPP-4.6.1.** Capture findings in `DESIGN-NOTES.md` as
  D-CPP-SHOWINC2.
- [x] **CPP-4.6.2.** `cl_cmdline.rs`: `source: Option<String>` →
  `sources: Vec<String>`.
- [x] **CPP-4.6.3.** `cl_showincludes.rs`: per-TU split on marker
  lines.
- [x] **CPP-4.6.4.** `emit.rs`: one `Source` per cmdline source,
  joined by case-insensitive basename.
- [x] **CPP-4.6.5.** Integration test: synthetic batch-mode CL binlog
  with three sources.
- [x] **CPP-4.6.6.** Re-run `cl_pipeline_demo` against the real
  corpus; AgentMonitoring reports 13 sources with correct per-TU
  counts.

### Milestone 5 — CLI surface and corpus harness

- [x] **CPP-5.1.** Add `analyze-cpp` subcommand to
  `munin-jsonlog-cli`: `input`, `--output / -o`, `--root [NAME=]PATH`
  (repeatable), `--auto-root`, `--pretty`, `--locale-strict`. Wires
  to `munin-cppbuild::analyze`.
- [x] **CPP-5.2.** Update `munin-jsonlog-cli` README and add a usage
  example to `crates/munin-cppbuild/README.md`.
- [x] **CPP-5.3.** CLI roundtrip integration test against the M2/M3
  synthetic binlogs (`tests/cli_analyze_cpp.rs`). Synthetic fixture
  builders relocated to `munin_cppbuild::testkit` so downstream
  crates can build the same fixtures.
- [x] **CPP-5.4.** Env-var-gated test
  (`MUNIN_CPPBUILD_TEST_BINLOGS=<dir>`): iterate every `.binlog`,
  run the pipeline, assert no errors and JSON roundtrips through
  `serde_json::from_value::<CppBuildAnalysis>`. Skips cleanly when
  unset.
- [x] **CPP-5.5.** Root `README.md` updated with a paragraph
  pointing at `munin-jsonlog analyze-cpp` and the `munin-cppbuild`
  crate.
