<!-- Copyright (c) Michael Grier -->
# munin-msbuild

A Rust reader and seekable indexed data model for MSBuild binary log
(`.binlog`) files.

`munin-msbuild` decodes the GZip-compressed event stream produced by MSBuild's
`/bl` switch into typed Rust structures, and builds an in-memory index that
supports random access to events without rescanning the file.

The MSBuild binary log format is documented at:
<https://github.com/dotnet/msbuild/blob/main/documentation/wiki/Binary-Log.md>

## Status

Early development. The reader and index types in the public API are
considered stable enough to depend on; expect additive changes only until
1.0.

## jsonlog: the text-format companion

`munin-msbuild` also defines a lossless JSON-Lines representation of an
indexed binlog called **jsonlog**. Encode an in-memory [`BinlogIndex`]
with [`jsonlog::dump_index`] and decode one with [`BinlogIndex::open_json`].

The [`munin-jsonlog`](../munin-jsonlog-cli) CLI wraps these entry points
with `dump` / `pack` subcommands and adds redaction flags
(`--redact-token`, `--redact-regex`, `--redact-username`,
`--redact-common`) so secrets can be scrubbed before sharing a build log.

## Example

```rust
use munin_msbuild::{open_binlog, BinlogIndex};

let header = open_binlog("build.binlog")?;
let index = BinlogIndex::build(header)?;
println!("indexed {} events", index.len());
# Ok::<_, munin_msbuild::MuninError>(())
```

## License

MIT
