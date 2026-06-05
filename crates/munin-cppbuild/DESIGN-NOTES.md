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
- **D-CPP-SHOWINC-1** — `/showIncludes` parsing and directive-text
  reconstruction heuristic (CPP-3.x).
- **D-CPP-LINK1** — `link /VERBOSE` parsing rules derived from the
  spike against the real binlog corpus (CPP-4.1).

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
