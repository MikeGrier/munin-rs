<!-- Copyright (c) Michael Grier -->
# munin-cppbuild

Derive structured C/C++ build data from MSBuild binary log (`.binlog`)
files produced by `.vcxproj` builds.

`munin-cppbuild` reads a binlog captured with `cl.exe /showIncludes` and
`link.exe /VERBOSE` enabled and emits a JSON document describing, per
project invocation, what was compiled and linked and what each step
consumed:

- Per translation unit: command line, include paths, preprocessor
  defines, the include tree, and a flat list of every header consumed.
- Per linked output: command line, inputs actually consumed, and
  inputs dropped or unreferenced.

## Status

Early development. See
[CHECKLIST](../../CHECKLIST-cpp-analysis.md) and
[DESIGN-NOTES](DESIGN-NOTES.md) for the in-progress design and
roadmap.

## Companion CLI

A `munin-jsonlog` subcommand wraps this library to convert a binlog into
a C++ analysis JSON document directly. See
[`munin-jsonlog-cli`](../munin-jsonlog-cli).
