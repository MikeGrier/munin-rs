// Copyright (c) Michael Grier

//! Seekable indexed data model over a `.binlog` file.
//!
//! [`BinlogIndex`] is built via a single-pass read of the decompressed binlog
//! stream. After construction, events can be accessed by sequential index or
//! filtered by record kind, project, target, or task — without re-reading the
//! file. Individual events are deserialized on demand from stored payloads,
//! keeping memory usage proportional to the compressed event data rather than
//! the fully expanded object graph.

use std::io::{Cursor, Read};

use crate::{
    context::{read_build_event_context, BuildEventContext},
    error::MuninError,
    field_flags::BuildEventArgsFieldFlags,
    header::{open_binlog, BinlogHeader},
    nvl_table::{NameValueListTable, NameValuePair},
    primitives::{read_7bit_count, read_7bit_int},
    reader::{dispatch_event, ArchiveEntry, BinlogEvent},
    record_kind::BinaryLogRecordKind,
    string_table::StringTable,
};

// ---------------------------------------------------------------------------
// EventMeta — lightweight per-event metadata captured during first-pass
// ---------------------------------------------------------------------------

/// Lightweight metadata about one event in the index.
///
/// Captured during the first-pass read without fully deserializing the event
/// payload. Sufficient for filtering and navigation.
#[derive(Debug, Clone)]
pub struct EventMeta {
    /// Record kind discriminant.
    pub record_kind: BinaryLogRecordKind,

    /// Byte offset of this record's kind byte in the decompressed stream.
    pub byte_offset: u64,

    /// Byte length of the record payload (excluding kind and length prefix).
    pub payload_len: usize,

    /// `BuildEventContext` extracted from the common fields prefix, if present.
    pub context: Option<BuildEventContext>,
}

// ---------------------------------------------------------------------------
// IndexEntry — metadata + raw payload for deferred deserialization
// ---------------------------------------------------------------------------

/// An indexed entry: metadata plus the stored raw payload.
///
/// The payload bytes are exactly the record payload (after the record kind
/// and length prefix). They can be deserialized on demand given the string
/// table and NVL table captured during the same first pass.
#[derive(Debug, Clone)]
struct IndexEntry {
    meta: EventMeta,
    payload: Vec<u8>,
}

// ---------------------------------------------------------------------------
// BinlogIndex — seekable indexed data model
// ---------------------------------------------------------------------------

/// Seekable indexed data model over a `.binlog` file.
///
/// Built by reading the entire decompressed stream once. Each event record's
/// raw payload is stored alongside lightweight metadata ([`EventMeta`]).
/// Events are deserialized on demand via [`get`](Self::get), avoiding the
/// cost of expanding the full object graph for events that are never accessed.
///
/// The string table, name-value-list table, and embedded archives captured
/// during the first pass are retained for use by on-demand deserialization
/// and by callers who need string resolution.
#[derive(Debug)]
pub struct BinlogIndex {
    header: BinlogHeader,
    strings: StringTable,
    nvl_table: NameValueListTable,
    archives: Vec<Vec<u8>>,
    entries: Vec<IndexEntry>,
}

impl BinlogIndex {
    // -- Construction -------------------------------------------------------

    /// Build an index by reading all records from the given binlog stream.
    ///
    /// This performs a single decompression pass over the entire file,
    /// ingesting auxiliary records (strings, NVL entries, archives) and
    /// storing each event record's metadata and raw payload.
    pub fn open(reader: impl Read) -> Result<Self, MuninError> {
        let (header, mut gz_reader) = open_binlog(reader)?;
        let version = header.file_format_version;

        let mut strings = StringTable::new();
        let mut nvl_table = NameValueListTable::new();
        let mut archives = Vec::new();
        let mut entries = Vec::new();

        // Track the byte offset in the decompressed stream. We count bytes
        // consumed for record-kind, record-length, and payload.
        let mut offset: u64 = 8; // header is 8 bytes (two i32 LE)

        loop {
            let kind_start = offset;

            // Record kind: 7-bit variable-length encoded i32.
            let (kind_raw, kind_bytes) = read_7bit_int_counted(&mut gz_reader)?;
            offset += kind_bytes;

            if kind_raw == BinaryLogRecordKind::EndOfFile as i32 {
                break;
            }

            // Record length (bytes of payload). Always present for v18+.
            let (record_length, len_bytes) = read_7bit_int_counted(&mut gz_reader)?;
            if record_length < 0 {
                return Err(MuninError::InvalidFormat(format!(
                    "negative record length: {record_length}"
                )));
            }
            let record_length = record_length as usize;
            if record_length > crate::primitives::MAX_BINLOG_FIELD_LEN {
                return Err(MuninError::InvalidFormat(format!(
                    "record length too large: {record_length} (max {})",
                    crate::primitives::MAX_BINLOG_FIELD_LEN
                )));
            }
            offset += len_bytes;

            // Read the full payload.
            let mut payload = vec![0u8; record_length];
            gz_reader.read_exact(&mut payload)?;
            offset += record_length as u64;

            let kind = BinaryLogRecordKind::from_raw(kind_raw);

            match kind {
                Some(BinaryLogRecordKind::String) => {
                    let s = String::from_utf8(payload).map_err(|_| MuninError::InvalidUtf8)?;
                    strings.add(s);
                }

                Some(BinaryLogRecordKind::NameValueList) => {
                    let mut cursor = Cursor::new(&payload);
                    let count = read_7bit_count(&mut cursor, "name-value list count")?;
                    let mut pairs = Vec::with_capacity(count);
                    for _ in 0..count {
                        let key_index = read_7bit_int(&mut cursor)?;
                        let value_index = read_7bit_int(&mut cursor)?;
                        pairs.push(NameValuePair {
                            key_index,
                            value_index,
                        });
                    }
                    nvl_table.add(pairs);
                }

                Some(BinaryLogRecordKind::ProjectImportArchive) => {
                    archives.push(payload);
                }

                Some(record_kind) if !record_kind.is_auxiliary() => {
                    // Extract lightweight metadata from the common fields
                    // prefix without fully deserializing the event.
                    let context = extract_context(&payload, version);

                    let meta = EventMeta {
                        record_kind,
                        byte_offset: kind_start,
                        payload_len: record_length,
                        context,
                    };
                    entries.push(IndexEntry { meta, payload });
                }

                // Unknown or other auxiliary — skip.
                _ => {}
            }
        }

        Ok(Self {
            header,
            strings,
            nvl_table,
            archives,
            entries,
        })
    }

    // -- Accessors ----------------------------------------------------------

    /// The parsed binlog file header.
    pub fn header(&self) -> &BinlogHeader {
        &self.header
    }

    /// The string table accumulated during indexing.
    pub fn strings(&self) -> &StringTable {
        &self.strings
    }

    /// The name-value list table accumulated during indexing.
    pub fn nvl_table(&self) -> &NameValueListTable {
        &self.nvl_table
    }

    /// Embedded zip archives from `ProjectImportArchive` records.
    pub fn archives(&self) -> &[Vec<u8>] {
        &self.archives
    }

    /// Number of event records in the index.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index contains zero events.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Metadata for the event at the given sequential index (0-based).
    ///
    /// Returns `None` if `index` is out of range.
    pub fn meta(&self, index: usize) -> Option<&EventMeta> {
        self.entries.get(index).map(|e| &e.meta)
    }

    /// Iterator over all event metadata entries.
    pub fn iter_meta(&self) -> impl Iterator<Item = (usize, &EventMeta)> {
        self.entries.iter().enumerate().map(|(i, e)| (i, &e.meta))
    }

    // -- Random-access deserialization --------------------------------------

    /// Deserialize the event at the given sequential index (0-based).
    ///
    /// Returns `None` if `index` is out of range. Returns an error if the
    /// stored payload cannot be deserialized.
    pub fn get(&self, index: usize) -> Result<Option<BinlogEvent>, MuninError> {
        let entry = match self.entries.get(index) {
            Some(e) => e,
            None => return Ok(None),
        };

        let mut cursor = Cursor::new(&entry.payload);
        let event = dispatch_event(
            &mut cursor,
            entry.meta.record_kind,
            &self.strings,
            &self.nvl_table,
            self.header.file_format_version,
        )?;
        Ok(Some(event))
    }

    /// Deserialize all events and collect them into a `Vec`.
    pub fn get_all(&self) -> Result<Vec<BinlogEvent>, MuninError> {
        let mut events = Vec::with_capacity(self.entries.len());
        for i in 0..self.entries.len() {
            if let Some(event) = self.get(i)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    // -- Query / filter API -------------------------------------------------

    /// Indices of events matching the given record kind.
    pub fn indices_by_kind(&self, kind: BinaryLogRecordKind) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.meta.record_kind == kind)
            .map(|(i, _)| i)
            .collect()
    }

    /// Indices of events whose `BuildEventContext` has the given
    /// `project_context_id`.
    pub fn indices_by_project_context(&self, project_context_id: i32) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.meta
                    .context
                    .is_some_and(|c| c.project_context_id == project_context_id)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Indices of events whose `BuildEventContext` has the given `target_id`.
    pub fn indices_by_target_id(&self, target_id: i32) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.meta.context.is_some_and(|c| c.target_id == target_id))
            .map(|(i, _)| i)
            .collect()
    }

    /// Indices of events whose `BuildEventContext` has the given `task_id`.
    pub fn indices_by_task_id(&self, task_id: i32) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.meta.context.is_some_and(|c| c.task_id == task_id))
            .map(|(i, _)| i)
            .collect()
    }

    /// A combined filter: returns indices of events matching ALL of the
    /// specified criteria. `None` fields are ignored (wildcard).
    pub fn query(
        &self,
        kind: Option<BinaryLogRecordKind>,
        project_context_id: Option<i32>,
        target_id: Option<i32>,
        task_id: Option<i32>,
    ) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if let Some(k) = kind {
                    if e.meta.record_kind != k {
                        return false;
                    }
                }
                if let Some(pid) = project_context_id {
                    if e.meta.context.is_none_or(|c| c.project_context_id != pid) {
                        return false;
                    }
                }
                if let Some(tid) = target_id {
                    if e.meta.context.is_none_or(|c| c.target_id != tid) {
                        return false;
                    }
                }
                if let Some(tsk) = task_id {
                    if e.meta.context.is_none_or(|c| c.task_id != tsk) {
                        return false;
                    }
                }
                true
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Extract all files from embedded `ProjectImportArchive` zip archives.
    pub fn extract_archives(&self) -> Result<Vec<ArchiveEntry>, MuninError> {
        // Re-use the same extraction logic as BinlogReader.
        let mut entries = Vec::new();
        for archive_bytes in &self.archives {
            let cursor = Cursor::new(archive_bytes);
            let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
                MuninError::InvalidFormat(format!("invalid ProjectImportArchive zip: {e}"))
            })?;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).map_err(|e| {
                    MuninError::InvalidFormat(format!("cannot read zip entry {i}: {e}"))
                })?;
                if file.is_dir() {
                    continue;
                }
                let path = file.name().to_string();
                let mut contents = String::new();
                if file.read_to_string(&mut contents).is_ok() {
                    entries.push(ArchiveEntry { path, contents });
                }
                // Non-UTF-8 binary — skip silently.
            }
        }
        Ok(entries)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read a 7-bit variable-length encoded `i32`, also returning the number of
/// bytes consumed.
fn read_7bit_int_counted(reader: &mut impl Read) -> Result<(i32, u64), MuninError> {
    let mut result: u32 = 0;
    let mut shift: u32 = 0;
    let mut buf = [0u8; 1];
    let mut count: u64 = 0;

    for _ in 0..5 {
        reader.read_exact(&mut buf)?;
        count += 1;
        let byte = buf[0];
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return Ok((result as i32, count));
        }
        shift += 7;
    }

    Err(MuninError::OverlongVarInt)
}

/// Extract the `BuildEventContext` from the common fields prefix of an event
/// payload, without fully deserializing the event.
///
/// Returns `None` if the flags do not include `BUILD_EVENT_CONTEXT` or if
/// partial parsing fails (in which case we gracefully degrade to no context).
fn extract_context(payload: &[u8], file_format_version: i32) -> Option<BuildEventContext> {
    let mut cursor = Cursor::new(payload);

    // Read the BuildEventArgsFieldFlags bitmask.
    let flags_raw = read_7bit_int(&mut cursor).ok()? as u32;
    let flags = BuildEventArgsFieldFlags::from_raw(flags_raw);

    // Skip the MESSAGE field if present (it's a dedup string index = one 7-bit int).
    if flags.contains(BuildEventArgsFieldFlags::MESSAGE) {
        let _ = read_7bit_int(&mut cursor).ok()?;
    }

    // Read the BuildEventContext if present.
    if flags.contains(BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT) {
        read_build_event_context(&mut cursor, file_format_version).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
