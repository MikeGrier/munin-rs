<!-- Copyright (c) Michael Grier -->
# munin-rs — Repository Design Notes

Workspace-level design decisions. Per-crate decisions belong in the
crate's own `DESIGN-NOTES.md` (e.g.
[crates/munin-msbuild/DESIGN-NOTES.md](crates/munin-msbuild/DESIGN-NOTES.md)).

## Decision Index

| ID | Decision | Section |
|----|----------|---------|
| D-JL-1 | Jsonlog is a hybrid decoded/opaque format | §D-JL-1 |
| D-JL-2 | Jsonlog API surface: a second `BinlogIndex` constructor | §D-JL-2 |
| D-JL-3 | Jsonlog file extension and on-disk encoding | §D-JL-3 |
| D-JL-4 | CLI is a separate workspace crate | §D-JL-4 |
| D-JL-5 | MCP server stays binlog-only in production | §D-JL-5 |
| D-JL-6 | Jsonlog is fully read into memory; no streaming | §D-JL-6 |
| D-JL-7 | Schema versioning via `munin_jsonlog_version` | §D-JL-7 |

---

### D-JL-1: Jsonlog is a hybrid decoded/opaque format

Each event in a `.jsonlog` file is serialized as one of two shapes:

- A **decoded** JSON object when the kind is known to the current
  `munin-msbuild` decoder. This is the form humans author and edit.
- An **opaque** `payload_b64` (base64 of the original stored payload
  bytes) when the record kind or field shape is not understood by the
  current decoder. This guarantees future-compatibility: a jsonlog
  produced by a newer `munin-msbuild` can still be read and re-packed
  by an older one without losing bytes.

The strings table is serialized as a plain JSON array; NVL tables as
arrays of `[key_index, value_index]` pairs. Embedded archive blobs are
base64.

### D-JL-2: Jsonlog API surface

A second constructor on the existing type: `BinlogIndex::open_json(impl
Read) -> Result<Self>`. No trait, no enum, no extension sniffing. The
existing `BinlogIndex::open(...)` is unchanged. Two constructors is the
minimum surface that satisfies every use case in the workspace.

### D-JL-3: File extension and on-disk encoding

The file extension is `.jsonlog`. The on-disk encoding is UTF-8 JSON,
not gzip-compressed (unlike `.binlog`). Compression is the user's
responsibility if they want it; pipes (`gzip < x.jsonlog`) suffice.

### D-JL-4: CLI is a separate workspace crate

The bidirectional converter lives in a new workspace crate
`crates/munin-jsonlog-cli` producing the binary `munin-jsonlog` with
subcommands `dump` and `pack`. It depends on `munin-msbuild` and does
not duplicate parsing logic. Keeping it separate from `munin-msbuild`
preserves the library crate's minimal dependency surface (no `clap`).

### D-JL-5: MCP server stays binlog-only in production

The `binlog_open` MCP tool does not sniff `.jsonlog` paths. Jsonlog is
a test-and-tooling vehicle, not a runtime input to the MCP server.
Tests in `munin-binlog-mcp` may construct a `BinlogIndex` via
`open_json` directly to feed fake data into the MCP-level handlers
without involving the tool's path resolver.

### D-JL-6: Jsonlog is fully read into memory; no streaming

`BinlogIndex::open_json` reads the entire file into memory before
returning, matching the behavior of `BinlogIndex::open`. Streaming
jsonlog parsing is explicitly out of scope.

### D-JL-7: Schema versioning via `munin_jsonlog_version`

The top-level JSON object carries `"munin_jsonlog_version": 1`.
Readers reject any other value. Future schema changes will bump this
field and add migration logic; v1 is the only supported version
today.
