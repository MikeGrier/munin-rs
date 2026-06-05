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
| _none yet_ | — | — |

Decisions are added here as milestones land:

- **D-CPPSCHEMA-1** — JSON document schema (CPP-1.2).
- **D-CPP-PATHROOT-1** — Multi-root path canonicalization rules
  (CPP-1.3).
- **D-CPP-ALIAS-1** — Alias-table construction algorithm (CPP-1.4).
- **D-CPP-SHOWINC-1** — `/showIncludes` parsing and directive-text
  reconstruction heuristic (CPP-3.x).
- **D-CPP-LINK1** — `link /VERBOSE` parsing rules derived from the
  spike against the real binlog corpus (CPP-4.1).
