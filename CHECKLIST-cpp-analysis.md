<!-- Copyright (c) Michael Grier -->
# CHECKLIST — C/C++ binlog analysis (`munin-cppbuild`)

Derive, from MSBuild `.binlog` files produced by `.vcxproj` builds, a
structured JSON document describing what each project built and what it
consumed — sourced from `cl.exe /showIncludes` and `link.exe /VERBOSE`
output captured in the binlog.

## Scope decisions (locked)

- **D-CPP-1.** One JSON file per binlog. Top-level shape:
  `{ header, projects: [...] }`.
- **D-CPP-2.** Path rooting: `--root <dir>` (repeatable to support
  separate drives, e.g. NuGet cache on D:\). Paths inside any root are
  emitted relative to that root with the root's index/name; paths
  outside all roots are emitted absolute. `--auto-root` derives one
  root from the longest common ancestor of project paths in the
  binlog. Roots are recorded in the JSON header.
- **D-CPP-3.** Alias scope is **per project-invocation**. Cloudbuild
  binlogs are per-project, so the per-project alias table keeps each
  project self-contained.
- **D-CPP-4.** Alias algorithm: leaf-name if unique → else last two
  segments → else `<first-segment>\..\<leaf>` → else `<leaf>#n`
  (per-leaf counter, deterministic order). Universe = every path
  appearing inside a single project.
- **D-CPP-5.** Locale: English `cl.exe` / `link.exe` only for v1.
  Detect non-English and emit a clear "unsupported locale" diagnostic.
- **D-CPP-6.** Crate layout: new library crate
  `crates/munin-cppbuild/` (no bin); new subcommand in
  `munin-jsonlog-cli` (`analyze-cpp` — final name TBD M5) that wraps
  it. Library is reusable by future MCP / index tools.
- **D-CPP-7.** Test corpus: tiny sanitized synthetic binlogs checked
  in under `crates/munin-cppbuild/tests/data/`; large real corpus
  gated on env var `MUNIN_CPPBUILD_TEST_BINLOGS=<dir>`.
- **D-CPP-8.** Linker `/VERBOSE` granularity (`/VERBOSE` vs
  `/VERBOSE:LIB|REF|ICF|UNUSEDLIBS`) is **deferred to M4** — concrete
  rules will be written once we inspect real binlog text. M4 begins
  with a spike against the user's corpus.

## Open questions (non-blocking)

- Exact subcommand name (`analyze-cpp`? `cpp`? `cppbuild`?). Resolve
  at M5.
- Whether the C++ JSON ever folds into the `jsonlog` stream (per the
  "kind of like to see this be able to be added to the jsonlog format"
  remark). Treat as a follow-on after v1 ships.

---

## Milestone 1 — Foundations: crate, schema, path aliasing

- [x] **CPP-1.1.** Create `crates/munin-cppbuild/` library crate
  (`Cargo.toml`, `src/lib.rs`, `README.md`, `LICENSE`,
  `DESIGN-NOTES.md`). Wire into root `Cargo.toml` workspace.
  Dependencies: `munin-msbuild`, `serde`, `serde_json`, `thiserror`.
- [x] **CPP-1.2.** Write the JSON schema in
  `crates/munin-cppbuild/DESIGN-NOTES.md` as **D-CPPSCHEMA-1**.
  Top-level: `header { tool_version, roots: [{name, path}],
  source_binlog }`, `projects[]`. Per project:
  `{ project_path, platform, configuration, global_properties,
  alias_table, sources[], outputs[] }`. Per source:
  `{ path, command_line, include_paths[], defines[], includes (tree),
  included_files (flat) }`. Per output: `{ path, command_line,
  inputs[], dropped[] }`. Record schema version constant.
- [x] **CPP-1.3.** Implement multi-root path canonicalizer in
  `path_root.rs`: input = absolute path + ordered root list; output =
  `Rooted { root_index: Option<usize>, relative_or_absolute: String }`.
  Deterministic; case-insensitive match on Windows. Unit tests.
- [x] **CPP-1.4.** Implement alias builder in `alias.rs` per D-CPP-4.
  Input = set of `Rooted` paths; output = `AliasTable` (path → alias)
  built deterministically. Unit tests covering: all-unique leaves,
  last-two-segments tie-break, first+leaf tie-break, `#n` fallback,
  stable ordering across runs.
- [x] **CPP-1.5.** Integration test: feed a synthetic in-memory
  project description through schema + aliaser, assert JSON shape and
  alias choices match expectations.

## Milestone 2 — Project-invocation extraction from binlogs

- [x] **CPP-2.1.** Add `walk.rs`: iterate `munin-msbuild`
  events; identify project-start / project-finish bracketing; collect
  per-project state (project file path, platform, configuration,
  global properties from the project-started payload).
- [x] **CPP-2.2.** Surface command-line / global properties: extract
  `/p:` properties set on the MSBuild invocation versus per-project
  properties. Record both under `global_properties` with a `source`
  discriminator (`command_line` | `project`).
- [x] **CPP-2.3.** Implement project-level JSON emitter: produces the
  M1 schema's `projects[]` entries with empty `sources` / `outputs`.
- [x] **CPP-2.4.** Synthetic-binlog fixture: write a minimal
  `.vcxproj` (or reuse `testprojects/`) that produces a binlog with
  one C++ project, two platform/configuration combos. Check in the
  resulting binlog under `tests/data/` (≤10 KB target).
  _Deviation: per D-CPP-FIXTURE1 the fixture is built programmatically
  at test time in `tests/common/mod.rs` (round-trips through
  `munin-msbuild`'s binlog writer for full-format coverage) rather than
  checked in as a binary `.binlog`. Real-binlog testing is M4's job._
- [x] **CPP-2.5.** Integration test that runs the M2 pipeline against
  the M2.4 fixture and asserts project metadata.

## Milestone 3 — `cl.exe /showIncludes` parsing

- [x] **CPP-3.1.** CL task identification: in `binlog_walk.rs`,
  recognize `CL` task invocations under a C++ project; capture the
  command line and the per-TU message stream.
- [x] **CPP-3.2.** Command-line tokenizer in `cl_cmdline.rs`: split
  the `cl.exe` command line; extract `/I` include paths, `/D` defines
  (both `/Dfoo` and `/Dfoo=bar`), source file. Preserve unknown
  switches verbatim under `other_switches[]`. Unit tests for response
  files, quoted paths, `/D` edge cases.
- [x] **CPP-3.3.** `/showIncludes` parser in `cl_showincludes.rs`:
  parse `Note: including file:` lines (English locale per D-CPP-5).
  Indent depth = include depth. Build an ordered tree of
  `IncludeNode { resolved_path, children[] }`. Also produce the flat
  `included_files` list in first-encounter order. Detect non-English
  output and surface a typed error. Unit tests for nested includes,
  duplicate includes, error lines interleaved.
- [x] **CPP-3.4.** Join: map the `#include "..."` /
  `#include <...>` directive text to its resolved file. The
  directive-text → resolved-file mapping is not in `/showIncludes`
  output directly; derive from indent transitions (each top-level
  resolved file corresponds in order to the source's top-level
  `#include` directives). Document the heuristic and its limits in
  D-CPP-SHOWINC. The `includes` map uses resolved-path keys at this
  layer; raising directive-text keys is a follow-on if heuristic is
  reliable.
- [x] **CPP-3.5.** Integration test against a tiny synthetic project
  with header chain `a.cpp → a.h → b.h → c.h` and a duplicate
  include; verify tree, flat list, aliases.

## Milestone 4.5 — CL batch-mode support (CPP-4.6)

Real-corpus verification of M3 surfaced a defect class: cl.exe is
routinely invoked in **batch mode** with N source files in one
command line (N ≥ 1). The current pipeline treats each CL task as a
single TU, silently dropping all but the last source and merging all
TUs' `/showIncludes` output into one bogus tree. cl.exe always emits
a bare-basename TU-boundary marker before each TU's includes (even
when N = 1), so a clean split is possible.

- [x] **CPP-4.6.1.** Capture findings in `DESIGN-NOTES.md` as
  D-CPP-SHOWINC2: batch-mode invariant, boundary-marker rule,
  per-TU split algorithm, basename-join rule, and the
  schema-invariance decision (one `Source` per cmdline source).
- [ ] **CPP-4.6.2.** `cl_cmdline.rs`: replace
  `ClCommandLine.source: Option<String>` with
  `sources: Vec<String>` in command-line order; update tokenizer
  and tests. Keep `other_switches` semantics intact for non-source
  positional tokens.
- [ ] **CPP-4.6.3.** `cl_showincludes.rs`: parser takes the
  cmdline source list and splits the message stream on
  bare-basename boundary markers. Return per-TU trees in
  command-line order. Detect orphan markers (no cmdline match)
  and includes-before-first-marker as new diagnostic counters.
  Update unit tests including a batch-mode case.
- [ ] **CPP-4.6.4.** `emit.rs`: emit one `Source` per cmdline
  source, joined to the corresponding TU by case-insensitive
  basename. Cmdline sources with no TU emit an empty include tree;
  orphan TUs emit a `Source` with the marker basename and a
  diagnostic. Update emit unit tests.
- [ ] **CPP-4.6.5.** Integration test: synthetic batch-mode CL
  binlog with three sources `a.cpp`, `b.cpp`, `c.cpp` (each with
  distinct includes); assert three `Source` entries with correct
  attribution and that no headers cross-contaminate between TUs.
- [ ] **CPP-4.6.6.** Re-run `cl_pipeline_demo` against the real
  corpus; confirm `AgentMonitoring` now reports 13 sources with
  per-TU counts (not one bogus 1806-header source). Capture the
  new output to `.scratch/cl_pipeline_demo_real_corpus.txt`.

## Milestone 5 — CLI surface and corpus harness

- [ ] **CPP-5.1.** Add `analyze-cpp` (final name TBD) subcommand to
  `munin-jsonlog-cli`: `input` (binlog), `--output / -o`,
  `--root <dir>` (repeatable), `--auto-root`, `--pretty`, `--locale`
  guard. Wire to `munin-cppbuild` library entry point.
- [ ] **CPP-5.2.** Update `munin-jsonlog-cli` README and add a usage
  example to `crates/munin-cppbuild/README.md`.
- [ ] **CPP-5.3.** CLI roundtrip integration test against the M2/M3
  synthetic binlogs.
- [ ] **CPP-5.4.** Env-var-gated test
  (`MUNIN_CPPBUILD_TEST_BINLOGS=<dir>`): iterate every `.binlog`,
  run the pipeline, assert no errors and that emitted JSON
  validates against the schema. Skip cleanly when unset.
- [ ] **CPP-5.5.** Update root `README.md` with a one-paragraph
  pointer to the new analysis subcommand.
