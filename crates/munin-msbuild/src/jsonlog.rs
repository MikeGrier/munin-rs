// Copyright (c) Michael Grier

//! Jsonlog — a JSON-based, human-authorable mirror of the binlog format.
//!
//! A `.jsonlog` file carries the same information as a `.binlog` (header,
//! string and name-value-list dedup tables, embedded archives, and event
//! records) but encoded as UTF-8 JSON. Each event is serialized either as
//! a **decoded** JSON object (when the current `munin-msbuild` decoder
//! understands the record kind) or as an opaque `payload_b64` containing
//! the original payload bytes (for forward-compatibility with unknown or
//! newer record kinds).
//!
//! See the workspace [`DESIGN-NOTES.md`] for decision IDs `D-JL-*`.
//!
//! [`DESIGN-NOTES.md`]: https://github.com/MikeGrier/munin-rs/blob/main/DESIGN-NOTES.md

pub mod decoded;
pub mod dumper;
pub mod schema;

pub use dumper::{build, dump_index, dump_index_pretty};
pub use schema::{
    ArchiveB64, JsonlogEvent, JsonlogEventBody, JsonlogFile, JsonlogHeader, MUNIN_JSONLOG_VERSION,
};
