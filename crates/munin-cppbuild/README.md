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

## Library usage

```rust
use std::fs::File;
use std::io::BufReader;
use munin_msbuild::BinlogIndex;
use munin_cppbuild::{analyze, auto_detect_root, LocaleStrategy};

let f = File::open("build.binlog")?;
let index = BinlogIndex::open(BufReader::new(f))?;

let mut roots = Vec::new();
if let Some(r) = auto_detect_root(&index)? {
    roots.push(r);
}

let doc = analyze(
    &index,
    "build.binlog",
    &roots,
    LocaleStrategy::BestEffort,
)?;

let json = serde_json::to_string_pretty(&doc)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`LocaleStrategy::BestEffort` skips translation units whose
`/showIncludes` output cannot be parsed (typically non-English
compiler locales); `LocaleStrategy::Strict` returns an error
instead.
