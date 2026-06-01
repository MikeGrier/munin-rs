// Copyright (c) Michael Grier

//! On-disk schema for `.jsonlog` files.
//!
//! See `D-JL-1` in the workspace `DESIGN-NOTES.md`. The schema is versioned
//! by the top-level [`JsonlogFile::munin_jsonlog_version`] field; v1 is the
//! only supported version today.

use serde::{Deserialize, Serialize};

use crate::header::BinlogHeader;

/// Current schema version. Readers reject any other value.
pub const MUNIN_JSONLOG_VERSION: u32 = 1;

/// Root object of a `.jsonlog` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlogFile {
    /// Schema version. Must equal [`MUNIN_JSONLOG_VERSION`].
    pub munin_jsonlog_version: u32,

    /// Binlog header (format version and minimum reader version).
    pub header: JsonlogHeader,

    /// Deduplicated string table. Index into this array is what every
    /// "dedup string" reference in event payloads points at.
    pub strings: Vec<String>,

    /// Name-value list table. Each entry is a list of `[key, value]` index
    /// pairs into [`strings`](Self::strings).
    pub name_value_lists: Vec<Vec<[u32; 2]>>,

    /// Embedded archive blobs (e.g. project imports), in the order they were
    /// encountered in the binlog stream.
    pub archives: Vec<ArchiveB64>,

    /// Event records in stream order.
    pub events: Vec<JsonlogEvent>,
}

/// JSON-friendly mirror of [`BinlogHeader`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct JsonlogHeader {
    pub file_format_version: i32,
    pub min_reader_version: i32,
}

impl From<BinlogHeader> for JsonlogHeader {
    fn from(h: BinlogHeader) -> Self {
        Self {
            file_format_version: h.file_format_version,
            min_reader_version: h.min_reader_version,
        }
    }
}

impl From<JsonlogHeader> for BinlogHeader {
    fn from(h: JsonlogHeader) -> Self {
        Self {
            file_format_version: h.file_format_version,
            min_reader_version: h.min_reader_version,
        }
    }
}

/// An embedded archive blob, base64-encoded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveB64 {
    /// Base64 (standard alphabet, with padding) of the archive bytes.
    pub data_b64: String,
}

/// One event record.
///
/// `kind` is the record-kind name (matching [`crate::BinaryLogRecordKind`]
/// debug formatting, e.g. `"BuildStarted"`). The body carries either a
/// `decoded` JSON object (when the current decoder understands the kind) or
/// an opaque `payload_b64` fallback for forward-compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlogEvent {
    /// Record kind name (e.g. `"BuildStarted"`, `"Message"`).
    pub kind: String,

    /// Byte offset of the record's kind byte in the original decompressed
    /// stream. Diagnostic parity with [`crate::EventMeta::byte_offset`].
    pub byte_offset: u64,

    /// Event body.
    #[serde(flatten)]
    pub body: JsonlogEventBody,
}

/// Decoded-vs-opaque payload for a [`JsonlogEvent`].
///
/// Serialized as one of two shapes:
///
/// ```json
/// { "decoded": { ... event-specific fields ... } }
/// ```
///
/// or
///
/// ```json
/// { "payload_b64": "...base64..." }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonlogEventBody {
    /// Decoded event payload, schema specific to the event kind.
    /// Populated by JL-1.4 / JL-1.5 with typed variants; for now the
    /// payload is carried as a raw JSON value.
    Decoded(serde_json::Value),

    /// Opaque payload — base64 (standard alphabet, with padding) of the
    /// stored record payload bytes. Used when the current decoder does
    /// not understand the record kind.
    PayloadB64(String),
}
