<!-- Copyright (c) Michael Grier -->
# munin-cppbuild — Design Notes

## Purpose

`munin-cppbuild` derives a structured per-project view of C/C++ MSBuild
activity from `.binlog` files, focused on two signals that the standard
build emits when verbose output is enabled:

- `cl.exe /showIncludes` — per translation unit, the ordered tree of
  header files the compiler actually resolved.
- `link.exe /VERBOSE` — per output binary, the `.obj` / `.lib` inputs
  the linker searched, loaded, referenced, or dropped.

The crate joins these signals back to the owning project invocation and
emits a single JSON document per binlog.

## Scope and locked decisions

See [`CHECKLIST-cpp-analysis.md`](../../CHECKLIST-cpp-analysis.md) for
the binding scope decisions D-CPP-1..8. They are the source of truth
during initial development; canonical entries will move here as
milestones complete.

## Decision Index

| ID | Decision | Section |
|----|----------|---------|
| D-CPPSCHEMA-1 | JSON document schema for a single binlog's C++ build analysis | §D-CPPSCHEMA-1 |
| D-CPP-PATHROOT-1 | Multi-root path canonicalization rules | `src/path_root.rs` |
| D-CPP-ALIAS-1 | Alias-table construction algorithm | `src/alias.rs` |
| D-CPP-PROPSRC1 | All global properties are emitted with `source = command_line` | §D-CPP-PROPSRC1 |
| D-CPP-FIXTURE1 | Integration fixtures are synthesized programmatically, not checked in as binary `.binlog` files | §D-CPP-FIXTURE1 |
| D-CPP-CLCMDLINE1 | `cl.exe` command-line tokenizer scope and limits | §D-CPP-CLCMDLINE1 |
| D-CPP-SHOWINC-1 | `/showIncludes` parsing rules and non-English-locale detection | §D-CPP-SHOWINC-1 |
| D-CPP-SHOWINC2 | CL batch mode: per-TU boundary marker, multi-source split | §D-CPP-SHOWINC2 |
| D-CPP-LINK1 | `link.exe /VERBOSE` parsing rules and emission mapping | §D-CPP-LINK1 |

Decisions are added here as milestones land:

- **D-CPPSCHEMA-1** — JSON document schema (CPP-1.2). _added._
- **D-CPP-PATHROOT-1** — Multi-root path canonicalization rules
  (CPP-1.3). _added in `src/path_root.rs`._
- **D-CPP-ALIAS-1** — Alias-table construction algorithm (CPP-1.4).
  _added in `src/alias.rs`._
- **D-CPP-PROPSRC1** — Property source attribution rule (CPP-2.2).
  _added._
- **D-CPP-FIXTURE1** — Synthetic in-memory integration fixtures
  (CPP-2.4). _added._
- **D-CPP-CLCMDLINE1** — `cl.exe` command-line tokenizer scope and
  limits (CPP-3.2). _added._
- **D-CPP-SHOWINC-1** — `/showIncludes` parsing rules and
  non-English-locale detection (CPP-3.3). _added._
- **D-CPP-SHOWINC2** — CL batch-mode per-TU boundary marker and
  multi-source split rule, discovered when real-corpus verification
  surfaced a 13-source-in-one-task case (CPP-4.6.1). _added._
- **D-CPP-LINK1** — `link /VERBOSE` parsing rules derived from the
  spike against the real binlog corpus (CPP-4.1). _added._

---

### D-CPPSCHEMA-1: JSON document schema

This section is our specification. Implementation lives in
`src/schema.rs`. Changing the shape or any field name is a breaking
change; bump [`SCHEMA_VERSION`](../src/schema.rs) when doing so.

The document represents the C++ build activity recorded in a single
`.binlog`. Schema scope is **per binlog → one document** (D-CPP-1).
The alias table is **per project invocation** (D-CPP-3); the path-root
list is **per document** (D-CPP-2).

#### Top level

```jsonc
{
  "schema_version": 1,
  "header": {
    "tool_version": "<munin-cppbuild crate version>",
    "source_binlog": "<original binlog path as supplied>",
    "roots": [
      { "name": "primary",     "path": "C:\\src\\product" },
      { "name": "nuget_cache", "path": "D:\\nuget" }
    ]
  },
  "projects": [ /* one entry per project invocation, see below */ ]
}
```

- `schema_version` is an integer matching `SCHEMA_VERSION`. Readers
  must reject documents with an unknown value.
- `roots` is the ordered list of `--root` directories supplied (or
  derived by `--auto-root`). Order is significant: a path inside
  multiple roots is attributed to the first match. Names are
  caller-supplied free-form strings (defaulting to `root0`, `root1`,
  … when not named).

#### Path encoding

Every file or directory path that appears in this document is
expressed in one of two forms:

- An **alias** — a short string defined in the enclosing project's
  `alias_table` (per-project scope; see D-CPP-ALIAS-1). Used for
  `sources[].path`, `sources[].include_paths[]`, `sources[].includes`
  values, `sources[].included_files[]`, `outputs[].path`,
  `outputs[].inputs[].path`, `outputs[].dropped[].path`.
- A **rooted path** — `{ "root": <index|null>, "path": "<string>" }`.
  Used inside `alias_table` entries and inside the document `header`
  (where there is no project-level alias scope yet). When `root` is
  non-null the `path` is relative to `roots[root].path`. When `root`
  is `null` the path is absolute (i.e. it was outside every supplied
  root).

The project file path itself (`projects[].project_path`) is emitted
both ways for self-containment: as a rooted path under the project's
own entry, and as an alias if it appears in any other path slot.

#### Per project

```jsonc
{
  "project_path":  { "root": 0, "path": "src\\foo\\foo.vcxproj" },
  "platform":      "x64",
  "configuration": "Release",
  "global_properties": [
    { "name": "Configuration", "value": "Release", "source": "command_line" },
    { "name": "Platform",      "value": "x64",     "source": "command_line" }
    // "source" is "command_line" (set via /p:) or "project"
    // (declared in the project file / inherited).
  ],
  "alias_table": {
    "<alias>": { "root": 0, "path": "src\\foo\\foo.cpp" }
    // every alias used anywhere in this project resolves here
  },
  "sources": [ /* see "Per source" */ ],
  "outputs": [ /* see "Per output" */ ]
}
```

#### Per source (one entry per CL translation-unit invocation)

```jsonc
{
  "path":         "<alias of the .cpp/.c file>",
  "command_line": "<verbatim cl.exe command line as recorded>",
  "include_paths": [ "<alias>", "<alias>", "..." ],
  "defines": [
    { "name": "FOO",       "value": null     },
    { "name": "BAR",       "value": "1"      },
    { "name": "_WIN32_WINNT", "value": "0x0A00" }
  ],
  "includes": {
    // tree keyed (v1) by resolved-file alias; in a follow-on this
    // may switch to "#include" directive text keys per CPP-3.4.
    "<alias of header>": {
      "file":     "<alias of header>",     // duplicated for clarity
      "children": { /* same shape, recursive */ }
    }
  },
  "included_files": [ "<alias>", "<alias>", "..." ]
}
```

- `include_paths` is the ordered list of `/I` directories from the
  command line (after response-file expansion).
- `defines[].value` is `null` for a bare `/Dfoo` and a string for
  `/Dfoo=bar` (including the empty string for `/Dfoo=`).
- `includes` describes the resolved include tree from
  `/showIncludes`. Each node's `children` map preserves first-seen
  order. Repeated includes appear once at the deepest position where
  they were resolved (rationale: matches `/showIncludes` indent
  semantics).
- `included_files` is the deduplicated flat list of every header
  consumed by this TU, in first-encounter order.

#### Per output (one entry per Link invocation)

```jsonc
{
  "path":         "<alias of the .lib/.dll/.exe/.sys output>",
  "command_line": "<verbatim link.exe command line as recorded>",
  "inputs": [
    {
      "path":       "<alias>",
      "kind":       "obj",    // "obj" | "lib"
      "origin":     "direct", // "direct" (on cmd line)
                              // "transitive" (pulled in by a .lib)
                              // "searched"   (located via /LIBPATH)
      "referenced": true       // true if linker reported a symbol
                              // reference, false if loaded but unused
    }
  ],
  "dropped": [
    {
      "path":   "<alias>",
      "reason": "unused" // free-form short tag; concrete vocabulary
                         // is fixed in D-CPP-LINK1 (CPP-4.1 spike)
    }
  ]
}
```

The exact set of `origin`, `referenced`, and `reason` values is
finalized in D-CPP-LINK1 after the M4 spike against real
`link /VERBOSE` output. M1 only defines the carrier shape.

#### Determinism

Every collection in the document has a defined order:

- `projects[]`: project-started order as observed in the binlog.
- `sources[]`: CL-task-started order within the project.
- `outputs[]`: Link-task-started order within the project.
- `global_properties[]`, `defines[]`, `include_paths[]`,
  `included_files[]`: first-seen order from the source data.
- `inputs[]`, `dropped[]`: linker-message order.
- `alias_table` and `includes` maps: alphabetical by key on emit
  (rationale: maps have no source order; alphabetical is the only
  reproducible choice).

---

### D-CPP-PROPSRC1: Property source attribution

Implementation lives in `src/walk.rs` (`ProjectInvocation::to_global_properties`).

**Decision.** Every entry in
[`schema::Project::global_properties`](../src/schema.rs) is emitted
with `source = command_line`. The `project` variant of
[`schema::PropertySource`](../src/schema.rs) is **reserved** for a
future signal but is never produced by v1.

**Constraint.** The MSBuild binlog records a `global_properties`
dictionary on `ProjectStarted`, but does not record which entries
originated from:

- the MSBuild process command line (`/p:Foo=Bar`),
- a parent project that passed them via the `<MSBuild>` task's
  `Properties` parameter,
- an `IBuildEngine` API caller,
- or an inherited environment variable.

All of these arrive at the child project as indistinguishable global
properties. There is no field, flag, or correlated event that
distinguishes them.

**Rationale.** Marking every entry `command_line` for v1 keeps the
schema honest (the value really did override anything in the project
file) without making up provenance. If a future MSBuild format
revision adds an attribution signal, or we add separate event
correlation (e.g. inspecting `ResponseFileUsed` or environment
records), individual entries can be re-tagged `project` without a
schema break.

**Scope.** Properties that appear only in
`ProjectStartedEvent::property_list` but **not** in
`ProjectStartedEvent::global_properties` are not emitted as
`GlobalProperty` entries at all — `property_list` typically contains
hundreds of evaluated defaults that would drown the output. Their
values remain available internally for fallback lookups (e.g.
deriving `Configuration` / `Platform` when those weren't passed via
`/p:`).

---

### D-CPP-FIXTURE1: Synthetic in-memory fixtures, not checked-in binlogs

Implementation lives in `tests/common/mod.rs`
(`synthetic_vcxproj_binlog()`).

**Decision.** Integration tests under this crate construct their
`.binlog` byte streams programmatically at runtime via
[`munin_msbuild::BinlogIndex::from_jsonlog`] +
[`munin_msbuild::BinlogIndex::write_binlog`] rather than checking
binary `.binlog` files into `tests/data/`.

**Rationale.**

- The fixture is a behavioural spec, not a sample. Hand-crafted JSON
  of `ProjectStarted` / `ProjectFinished` makes the test intent
  obvious to a reader; a binary blob does not.
- No dependency on a working MSBuild installation at test time.
- Diffable, no git LFS pressure, no fixture-regeneration step.
- The round-trip through `write_binlog` exercises the full binlog
  encode → decode path the production code will see, so we still
  validate the real binary format, not just our in-memory shape.

**Scope.** Real-binlog testing (full corpus of cloudbuild-emitted
`.binlog` files, end-to-end with `cl /showIncludes` and `link
/VERBOSE` output) is the explicit job of M4 (CPP-4.1 spike) and is
not in scope for M2/M3 fixtures.

**Tradeoff accepted.** If `munin-msbuild`'s writer drifts from real
MSBuild output (an event format we don't decode, a new aux record,
etc.), our synthetic fixtures will not detect it. The M4 corpus is
the safety net.

---

### D-CPP-CLCMDLINE1: `cl.exe` command-line tokenizer scope and limits

Implementation lives in `src/cl_cmdline.rs` (`parse()` →
`ClCommandLine`).

**Decision.** Parse a `cl.exe` command line into four fields:

1. `executable` — the first token, captured verbatim and not
   otherwise interpreted.
2. `source` — a single positional file whose extension matches
   `.cpp`, `.cxx`, `.cc`, `.c`, `.c++` (case-insensitive). When
   multiple matches appear the last wins; earlier matches move to
   `other_switches` so nothing is lost.
3. `include_paths` — every `/I` (or `-I`) value, in command-line
   order, with the switch prefix stripped and surrounding quotes
   removed. Both attached (`/Ipath`) and separated (`/I path`) forms
   are recognized.
4. `defines` — every `/D` (or `-D`) value, in command-line order,
   split into `name` and optional `value` at the first `=`. Both
   attached and separated forms are recognized.

Every other token — recognized switches without a schema slot,
unknown positionals, response-file directives — is preserved
**verbatim** in `other_switches` so the analysis layer can flag or
re-interpret them without losing data.

**Tokenization.** Approximates `CommandLineToArgvW` for the inputs
MSBuild actually emits: whitespace splits at top level, double-quoted
runs are one token (with `""` denoting a literal `"`), and
backslashes are treated as literal characters so Windows paths
(`C:\foo\bar`) round-trip verbatim. This differs from
`CommandLineToArgvW`'s backslash-before-quote rule, which
MSBuild-emitted CL command lines do not exercise.

**Explicit non-goals (v1).**

- **No response-file expansion.** `@response.rsp` tokens land in
  `other_switches` verbatim; their contents are not read or
  re-parsed. CPP-3.x may revisit if real binlogs expose include /
  define data only via response files.
- **No `/U`, `/FI`, `/Yc`, `/Yu`, etc.** All other switches are
  preserved verbatim; only `/I` and `/D` are structurally surfaced.
- **No semantic interpretation** of paths or values (no
  normalization, no resolution against project roots). That is the
  job of later layers.

**Tradeoff accepted.** A project that hides its include paths or
defines inside a response file will appear to have no include search
paths in the schema. The presence of `@…` tokens in `other_switches`
is the signal that this has happened.

---

### D-CPP-SHOWINC-1: `/showIncludes` parsing rules

Implementation lives in `src/cl_showincludes.rs` (`parse()` →
`ShowIncludes`). Directive-text reconstruction is addressed in a
follow-on section once CPP-3.4 lands.

**Line format.** The English `cl.exe` emits one line per resolved
header:

```
Note: including file: <N spaces><resolved path>
```

The literal prefix is exactly `Note: including file:` (case-sensitive,
ASCII). Following the colon, `cl.exe` emits **one or more** spaces;
the count of spaces is the include **depth**:

- 1 space  → depth 1 (header included directly from the source)
- 2 spaces → depth 2 (header included by a depth-1 header)
- N spaces → depth N

The resolved path is the rest of the line, taken verbatim. No
normalization or stripping is performed at this layer.

**Tree construction.** A depth-N line is the next child of the
current depth-(N-1) parent. The parser maintains a path of ancestor
indices keyed by depth and pops/pushes as depth changes. Sibling
order within each node matches the order of emission.

**Flat dedup list.** [`ShowIncludes::included_files`] is the
depth-first first-encounter walk of the tree with duplicates
removed. Duplicate occurrences remain in the tree (so the structural
record is exact), but each path appears at most once in the flat
list.

**Diagnostics and non-include messages.** Any message that does not
match the exact prefix is silently ignored. `cl.exe` interleaves
warnings, errors, file-name banners, and progress text with the
include notes; the parser treats all of these as out-of-band.

**Non-English locale detection.** The parser scans the input for a
small allowlist of known localized equivalents (French, German,
Spanish, Italian, Simplified Chinese, Japanese) and returns
[`LocaleNotSupportedError`] if any match. The list is **not
exhaustive**; an unrecognized locale will produce an empty tree with
no error. Callers that need higher-confidence detection can check
whether `included_files` is empty on a TU known to have includes.

**Malformed input.** If a line's depth exceeds the current ancestor
chain depth by more than 1 (a "skipped level"), the line is dropped
and [`ShowIncludes::malformed_message_count`] is incremented. Well-
formed `cl.exe` output never triggers this; a non-zero count
indicates the message stream was reordered or truncated upstream.

**Explicit non-goals (v1).**

- **No directive-text reconstruction.** Mapping a resolved-path node
  back to its source `#include "..."` / `#include <...>` directive
  is the job of CPP-3.4 and a follow-on section in this design note.
- **No path normalization.** Resolved paths are not canonicalized,
  rooted, or de-cased. That is the job of `path_root` and `alias`.
- **No deduplication inside the tree.** Duplicate occurrences are
  preserved structurally; only the flat list deduplicates.


**Directive-text → resolved-file mapping (CPP-3.4).** The
`#include "..."` or `#include <...>` directive text that appears in
the original source file is **not** present in `/showIncludes`
output. `cl.exe` only records the resolved absolute path of each
header it consumed. Reconstructing the directive text requires
re-reading the source file and aligning its top-level `#include`
directives with the depth-1 nodes of the include tree in order.

In the v1 schema (D-CPPSCHEMA-1), the per-TU `includes` map is
keyed by the **alias of the resolved header path**, not by directive
text. This is an explicit decision; raising directive-text keys is a
candidate follow-on if the alignment heuristic proves reliable.

**Heuristic (for future implementation).**

1. Tokenize the source file's preprocessor directives.
2. Collect top-level `#include` directives in source-text order,
   ignoring those guarded out by `#if 0`, `#if false`, or trivially-
   false `#if` arms.
3. Pair the i-th surviving directive with the i-th depth-1 node of
   the include tree.

**Known limits.**

- `#include` directives reached via macro expansion
  (`#include FOO_HEADER`) cannot be aligned by this heuristic — the
  source-text directive doesn't name the file.
- `#include` directives inside non-trivial `#if` arms whose
  truthiness depends on macros defined at compile time are
  effectively conditional; aligning them requires running the
  preprocessor.
- Headers included multiple times at depth 1 (rare with include
  guards) align ambiguously.
- Headers excluded by include guards on a re-include path don't
  re-appear in `/showIncludes`, so the i-th alignment can drift.

These limits make directive-text alignment a best-effort enrichment,
not a primary key. The schema's resolved-path-aliased keys remain
authoritative.

---

### D-CPP-SHOWINC2: CL batch mode and per-TU boundary markers

Implementation lives in `src/cl_cmdline.rs` and
`src/cl_showincludes.rs` (CPP-4.6 series). This section supersedes
the implicit single-TU assumption left over from D-CPP-SHOWINC-1.

**Discovery.** End-to-end verification of M3 against the real
binlog corpus (CPP-4.6 spike, see
`.scratch/cl-spike-samples/`) revealed that `cl.exe` is routinely
invoked in **batch mode** with N source files on a single command
line (N ≥ 1). The MSBuild CL task does not split these into N
separate `Task` invocations — the entire batch runs under one
`TaskStarted` / `TaskFinished` bracket with one `TaskCommandLine`
event and one merged `/showIncludes` message stream.

In the corpus, 1 of 3 binlogs had a 13-source batch
(`AgentMonitoring`); the other 2 had 1-source CL tasks. The
single-source case is just N=1 of the same batch shape — the
boundary marker described below is emitted in **both** cases.

**Boundary-marker rule.** Immediately before the
`/showIncludes` output of each TU in the batch, `cl.exe` emits a
single Message line containing only the **bare basename** of that
TU's source file, at column 0, with no prefix or trailing
punctuation. Example slice from a real 13-source CL task:

```
All source files are not up-to-date: missing command TLog "{0}".
AgentEventObject.cpp                              ← TU 1 boundary
Note: including file: ...AgentEventObject.h
Note: including file:  ...RdCommon.h
...
Note: including file:      ...VmAuthUtil.h        ← last include of TU 1
AgentMonitoring.cpp                               ← TU 2 boundary
Note: including file: ...MonitoringAgentOutputChannel.h
...
```

Boundary markers are emitted in the **same order** as the source
files appear on the command line. There is no separator between
the last include of one TU and the next TU's boundary; nothing
else appears between them in well-formed output.

**Recognition.** A message line is a TU boundary marker iff:

- The line has no leading or embedded whitespace.
- Every byte is ASCII alphanumeric or one of `_ . + -`.
- The line ends (case-insensitive) with a recognized C/C++
  source extension: `.c`, `.cc`, `.cpp`, `.cxx`, `.c++`.
- The stem before the extension is non-empty.

This is strict enough to never false-positive on a compiler
diagnostic (which always contains spaces or quotes), an MSBuild
progress line (which contains text like `"Building..."`), or an
`Note: including file:` line (which starts with `Note: `). The
parser does **not** need the cmdline source list to disambiguate;
the regex alone is sufficient against real cl.exe output.

**Per-TU split algorithm.**

1. Walk the message stream maintaining a `current_tu` index.
2. On a boundary marker: open a new TU keyed by the marker
   basename; reset the depth path.
3. On a `Note: including file:` line: if no TU is open, open an
   anonymous TU (`source_name: None`) and append; otherwise
   append to the currently open TU.
4. On any other message: ignore.
5. After the walk, build each TU's `included_files` (flat dedup,
   per-TU, in depth-first first-encounter order).

Anonymous-leading TUs are rare; in well-formed batch-mode output
the first message after build setup is always the first marker.

**Schema invariance.** The on-the-wire schema (D-CPPSCHEMA-1) is
**unchanged**. `Project::sources` was already `Vec<Source>`; the
emitter will emit one `Source` per cmdline source instead of one
per CL task (CPP-4.6.4). Each `Source.path` is the alias of that
cmdline source, matched to its per-TU `TuIncludes` by
case-insensitive basename.

**Join rule (emitter).**

- For each cmdline source, find the `TuIncludes` whose marker
  basename matches case-insensitively; emit a `Source` carrying
  that TU's tree and flat list.
- A cmdline source with no matching marker emits a `Source` with
  an empty tree and empty `included_files` — the TU produced no
  includes (unusual but legal; e.g. `cl /c` on an `.obj`-only TU,
  or a TU that errored out before any includes were resolved).
- An orphan marker (no matching cmdline source) emits an extra
  `Source` whose `path` is the alias of the bare basename and
  increments `orphan_marker_count`. This indicates either a
  cmdline-parser bug or a non-standard `cl.exe` invocation.

**Diagnostics.** `malformed_message_count` from D-CPP-SHOWINC-1
retains its meaning (depth-jump within a TU) and is summed across
all TUs. No additional counters are needed in the v1 surface; the
anonymous-TU representation captures the includes-before-marker
case structurally rather than via a count.

**Non-goals (v1).**

- **No `/MP` (parallel) ordering recovery.** `cl /MP` may
  interleave TUs from worker processes; in that case
  `includes_before_first_marker_count` will be non-zero and the
  TUs will look chaotic. The parser does not attempt to
  re-sequence them; the binlog itself records messages in arrival
  order. If `/MP` proves common in the corpus, M5 may add a
  diagnostic-only flag.
- **No support for `@response.rsp` source files.** Sources listed
  inside a response file are not currently extracted (response
  files are preserved verbatim in `other_switches`). The boundary
  markers will still appear, and orphan markers will surface them
  — that's the v1 escape hatch.
- **No PCH-specific handling.** Precompiled-header creation
  (`/Yc`) and consumption (`/Yu`) both still emit `/showIncludes`
  output normally; PCH metadata enrichment is a follow-on.

---

### D-CPP-LINK1: `link.exe /VERBOSE` parsing rules

Implementation will live in `src/link_cmdline.rs` (CPP-4.2) and
`src/link_verbose.rs` (CPP-4.3). This section is the parser spec
derived from CPP-4.1 spike data (see
[`examples/link_spike.rs`](../examples/link_spike.rs)) captured into
`.scratch/link-verbose-samples/` from real `.binlog` files.

**Spike corpus.** Three real Microsoft cloudbuild binlogs were
available to the spike; one contained a Link task (the other two are
static-lib projects with no link step). The single Link task is
~12k messages, ~2MB, with `/VERBOSE` (not `/VERBOSE:LIB|REF|ICF`) on
an exe link consuming ~250 input `.lib` files and ~150 `/DEFAULTLIB`
resolutions. This is a small but representative sample for v1; the
rules below will be re-validated against the larger corpus when
CPP-5.4's env-var-gated test runs.

**Message granularity.** MSBuild's binlog records every line of
`link.exe` console output as an individual `Message` event under the
Link task bracket. The parser therefore operates on `&[String]`,
one element per emitted line, with no further splitting required.

**Line classes.** Each message is classified by **exact prefix
match** (case-sensitive, ASCII). The leading-space count is
structural and is part of the prefix — the linker emits a stable
indentation scheme:

| Class | Prefix (literal, including leading spaces) | Trailing capture |
|-------|--------------------------------------------|------------------|
| `PassStart`            | `Starting pass `              | pass number (1 or 2) |
| `PassEnd`              | `Finished pass `              | pass number |
| `DefaultLibProcessed`  | `Processed /DEFAULTLIB:`      | library name (no `.lib` required) |
| `SearchingSection`     | `Searching libraries` (exact) | — |
| `SearchingSectionEnd`  | `Finished searching libraries` (exact) | — |
| `LibrarySearched`      | `    Searching ` (4 spaces) and trailing `:` | path between prefix and trailing `:` |
| `SymbolFound`          | `      Found ` (6 spaces)     | symbol name (rest of line) |
| `ReferencedIn`         | `        Referenced in ` (8 spaces) | referencing OBJ basename |
| `LoadedMember`         | `        Loaded ` (8 spaces)  | `<lib-basename>(<member>)` |
| `UnusedSection`        | `Unused libraries:` (exact)   | — |
| `UnusedEntry`          | `  ` (2 spaces) **inside** the `UnusedSection` | path (rest of line) |
| `Discarded`            | `Discarded `                  | full text (recorded but not surfaced in v1) |
| `Other`                | anything else                 | passed through unchanged |

**State.** The parser tracks two pieces of state:

1. The **current `LibrarySearched` scope**, opened by a `LibrarySearched`
   line and closed by the next `LibrarySearched`, `SearchingSectionEnd`,
   or `UnusedSection`. Within this scope, `SymbolFound` / `ReferencedIn`
   / `LoadedMember` lines attach to the open library.
2. Whether we are currently **inside `UnusedSection`** — set when
   `UnusedSection` is seen, cleared on the next line that is not an
   `UnusedEntry` (so the section is terminated by any non-2-space line).

All other classes are stateless and emit their data immediately.

**Reference inference.** A library `L` is considered
**referenced** (i.e. actually contributed code to the final image)
if **at least one `LoadedMember` line appears inside an open
`LibrarySearched(L)` scope**. The bare presence of `SymbolFound` /
`ReferencedIn` lines is not sufficient — the linker prints those
even for symbols it ultimately satisfies from another source.

A library appearing in the `UnusedSection` is always
**unreferenced**; if it also appears in some `LibrarySearched` scope
with zero `LoadedMember` lines (the normal case), the two signals
agree.

**Emission to D-CPPSCHEMA-1.** For each Link task:

- `outputs[].command_line` ← verbatim `TaskCommandLine` value.
- `outputs[].path` ← `/OUT:<path>` switch from the command line.
- `outputs[].inputs[]`: one entry per **command-line lib/obj input
  and per `/DEFAULTLIB`**:
  - `path` ← rooted path of the input as it appeared on the command
    line (for command-line inputs) or as resolved by the first
    `LibrarySearched` whose basename matches the `/DEFAULTLIB` name
    (for `DefaultLibProcessed`). When neither yields a path, the
    `/DEFAULTLIB` name is emitted as an absolute synthetic path
    `defaultlib:<name>`.
  - `kind` ← `lib` (`.lib`) or `obj` (`.obj`), by extension.
  - `origin` ← `direct` (on the command line),
    `searched` (from `/DEFAULTLIB` or via `/LIBPATH` discovery), or
    `transitive` (Loaded but never explicitly named — rare, v1
    emits only when observed).
  - `referenced` ← per the inference rule above.
- `outputs[].dropped[]`: one entry per `UnusedEntry`:
  - `path` ← rooted path of the unused library.
  - `reason` ← string literal `"unused"` (the only v1 vocabulary).

**Explicit non-goals (v1).**

- **No symbol-level data.** `SymbolFound` / `ReferencedIn` /
  `Discarded` lines are parsed for state but not surfaced in the
  schema. A future schema revision may add a `symbols[]` field per
  input.
- **No COMDAT-folding / ICF detail.** `Selected symbol:` /
  `Replaced symbol(s):` / `ICF total savings:` blocks are passed
  through as `Other`.
- **No resource compiler sub-output.** `Microsoft (R) ... Resource
  Compiler` banners and `adding resource. type:...` lines are
  `Other`.
- **No pass 2 listing.** The indented `<lib>(<member>)` list that
  appears after `Starting pass 2` is redundant with the pass-1
  `LoadedMember` lines and is currently treated as `Other`. May be
  used as a cross-check later.
- **No `/VERBOSE:LIB|REF|ICF|UNUSEDLIBS` differential parsing.** v1
  assumes `/VERBOSE` (full). When only `/VERBOSE:UNUSEDLIBS` is
  used, the `Searching libraries` block is absent but the
  `Unused libraries:` block is still present; the parser tolerates
  this naturally (no state transition fails).
- **No localization.** Locale handling mirrors D-CPP-SHOWINC-1:
  English `link.exe` only. Non-English output appears entirely as
  `Other` and produces empty inputs/dropped data; CPP-5.x callers
  may add a heuristic guard if needed.

**Tradeoff accepted.** A linker that consumes a library purely via
an obj that already has a `/DEFAULTLIB` reference (where the obj is
on the command line, not the lib) will record the lib as `inputs[]`
entry of `origin: searched` and `referenced: true` only if a
`LoadedMember` line appears. If `link.exe` resolves the symbol
entirely from another source on the same pass, the lib will look
unused even though it satisfied a defaultlib reference. The
`UnusedEntry` cross-check catches this for the explicit
`Unused libraries:` set; corner cases outside that set may be
mis-classified and are accepted for v1.
