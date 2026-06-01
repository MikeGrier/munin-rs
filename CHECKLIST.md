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
- [ ] **JL-1.4** In `jsonlog::decoded`, add one serde struct per
  `BinlogEvent` variant mirroring its fields with JSON-friendly types
  (no `Cursor`, no raw bytes). One file per ~10 variants is fine.
- [ ] **JL-1.5** Add `From<&events::FooEvent> for decoded::Foo` and
  `From<decoded::Foo> for events::FooEvent` for every variant, with a
  unit test per variant asserting `Foo -> JSON -> Foo` equality.

## M2 — Dumper (binlog → jsonlog)

- [ ] **JL-2.1** Expose a crate-private `BinlogIndex::payload_bytes(i)
  -> &[u8]` accessor (or `entries_raw()`) so the dumper can read the
  stored payload without re-parsing.
- [ ] **JL-2.2** Implement `jsonlog::dump_index(&BinlogIndex, impl
  Write) -> Result<()>`: write header, strings, NVL pairs, archives
  (base64), and each event in stored order. On decode success emit
  `decoded`; on decode failure emit `payload_b64` of the stored bytes.
- [ ] **JL-2.3** Add a `--pretty` analogue: `dump_index_pretty` that
  uses `serde_json::to_writer_pretty`. The non-pretty form uses
  `to_writer`.
- [ ] **JL-2.4** Unit tests in `jsonlog/tests.rs` covering at least 10
  cases: empty index, header-only, each major event kind, an archive
  blob, and a forced-fallback unknown-kind payload.
- [ ] **JL-2.5** Integration test
  `crates/munin-msbuild/tests/jsonlog_dump.rs`: open
  `tests/data/hello.binlog`, dump to a `Vec<u8>`, deserialize as
  `JsonlogFile`, assert `events.len() == index.len()` and that every
  event either has `decoded` matching `index.get(i)` or a
  `payload_b64` matching `payload_bytes(i)`.

## M3 — Writer path (jsonlog → binlog)

- [ ] **JL-3.1** Add `events::write_*` for every `read_*` in
  `crates/munin-msbuild/src/events.rs`. Each writer takes a context
  carrying the string and NVL dedup tables and emits the exact byte
  sequence the matching reader would consume. Unit-test each writer
  against its reader (round-trip on random-but-fixed inputs).
- [ ] **JL-3.2** Implement `BinlogIndex::open_json(impl Read) ->
  Result<Self>`: parse `JsonlogFile`; for `payload_b64` events use
  bytes verbatim; for `decoded` events convert to `BinlogEvent`, then
  re-encode via the M3.1 writers into payload bytes; populate the
  same private fields as `open(...)`.
- [ ] **JL-3.3** Implement `BinlogIndex::write_binlog(&self, impl
  Write) -> Result<()>`: emit a gzip-compressed stream containing
  header, interleaved `String` / `NameValueList` aux records sized to
  match the original index, each event record with 7-bit kind/length
  framing, terminated by `EndOfFile`.
- [ ] **JL-3.4** Integration test
  `crates/munin-msbuild/tests/jsonlog_open.rs`: hand-author a small
  `.jsonlog` literal as `include_str!("data/fake.jsonlog")`, open via
  `open_json`, verify `len()`, each `meta(i)`, and each decoded
  `get(i)`.
- [ ] **JL-3.5** Integration test
  `crates/munin-msbuild/tests/jsonlog_roundtrip.rs`: load
  `hello.binlog`, dump to jsonlog, pack jsonlog back to a fresh binlog
  via `write_binlog`, re-open the packed binlog with `open`, assert
  every `meta` and decoded `get` matches the original index.

## M4 — CLI, MCP test helper, docs

- [ ] **JL-4.1** Create `crates/munin-jsonlog-cli` (binary name
  `munin-jsonlog`) as a new workspace member; add `clap` (derive
  feature) and depend on `munin-msbuild`. Define the `Cli` struct
  with `dump` and `pack` subcommands but no logic yet.
- [ ] **JL-4.2** Implement `dump <input.binlog> [-o <out>] [--pretty]`
  using `BinlogIndex::open` + `jsonlog::dump_index`. Defaults to
  stdout. Route all output through a single writer abstraction (one
  trait, one method) — no scattered `println!` calls.
- [ ] **JL-4.3** Implement `pack <input.jsonlog> [-o <out>]` using
  `BinlogIndex::open_json` + `BinlogIndex::write_binlog`. Defaults to
  stdout. Use the same writer abstraction as `dump`.
- [ ] **JL-4.4** In `crates/munin-binlog-mcp/tests`, add a test helper
  `fn open_jsonlog_fixture(name: &str) -> BinlogIndex` and port one
  existing hand-rolled-fixture test to use it. The `binlog_open` MCP
  tool itself stays binlog-only.
- [ ] **JL-4.5** Integration test
  `crates/munin-jsonlog-cli/tests/cli_roundtrip.rs`: invoke the CLI
  binary via `assert_cmd` (add as dev-dep) on `hello.binlog`: `dump`
  to a temp file, `pack` it back, then open both with
  `BinlogIndex::open` and assert event equivalence. Update root
  `README.md`, `crates/munin-msbuild/README.md`, and add
  `crates/munin-jsonlog-cli/README.md`.
