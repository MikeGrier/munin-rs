<!-- Copyright (c) Michael Grier -->
# munin

A Rust reader and seekable indexed data model for MSBuild binary log
(`.binlog`) files.

`munin` decodes the GZip-compressed event stream produced by MSBuild's
`/bl` switch into typed Rust structures, and builds an in-memory index that
supports random access to events without rescanning the file.

The MSBuild binary log format is documented at:
<https://github.com/dotnet/msbuild/blob/main/documentation/wiki/Binary-Log.md>

## Status

Early development. The reader and index types in the public API are
considered stable enough to depend on; expect additive changes only until
1.0.

## Example

```rust
use munin::{open_binlog, BinlogIndex};

let header = open_binlog("build.binlog")?;
let index = BinlogIndex::build(header)?;
println!("indexed {} events", index.len());
# Ok::<_, munin::MuninError>(())
```

## License

MIT
