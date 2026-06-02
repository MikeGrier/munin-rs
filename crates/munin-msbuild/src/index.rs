// Copyright (c) Michael Grier

//! Seekable indexed data model over a `.binlog` file.
//!
//! [`BinlogIndex`] is built via a single-pass read of the decompressed binlog
//! stream. After construction, events can be accessed by sequential index or
//! filtered by record kind, project, target, or task — without re-reading the
//! file. Individual events are deserialized on demand from stored payloads,
//! keeping memory usage proportional to the compressed event data rather than
//! the fully expanded object graph.

use std::io::{Cursor, Read, Write};

use crate::{
    context::{BuildEventContext, read_build_event_context},
    error::MuninError,
    field_flags::BuildEventArgsFieldFlags,
    header::{BinlogHeader, open_binlog},
    jsonlog::schema::{JsonlogEventBody, JsonlogFile, MUNIN_JSONLOG_VERSION},
    nvl_table::{NameValueListTable, NameValuePair},
    primitives::{read_7bit_count, read_7bit_int},
    reader::{ArchiveEntry, BinlogEvent, dispatch_event},
    record_kind::BinaryLogRecordKind,
    string_table::StringTable,
    writers::{WriteContext, write_7bit_int as w_7bit},
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

    /// Build an index from a `.jsonlog` document (see
    /// [`crate::jsonlog`]).
    ///
    /// Reconstructs the string and name-value-list tables verbatim from
    /// the document so that dedup indices are preserved. Each event is
    /// either base64-decoded (when stored as `payload_b64`) or
    /// re-encoded via the `events::write_*` functions (when stored as
    /// `decoded`) so that the resulting payload is byte-equivalent to
    /// what the binlog reader would have consumed.
    pub fn open_json(reader: impl Read) -> Result<Self, MuninError> {
        let file: JsonlogFile = serde_json::from_reader(reader)
            .map_err(|e| MuninError::InvalidFormat(format!("jsonlog parse: {e}")))?;
        Self::from_jsonlog(file)
    }

    /// Build an index from an already-parsed [`JsonlogFile`].
    pub fn from_jsonlog(file: JsonlogFile) -> Result<Self, MuninError> {
        if file.munin_jsonlog_version != MUNIN_JSONLOG_VERSION {
            return Err(MuninError::InvalidFormat(format!(
                "unsupported jsonlog version {} (expected {})",
                file.munin_jsonlog_version, MUNIN_JSONLOG_VERSION
            )));
        }

        let header: BinlogHeader = file.header.into();
        let version = header.file_format_version;

        // Rebuild dedup tables verbatim to preserve indices.
        let mut strings = StringTable::new();
        for s in file.strings {
            strings.add(s);
        }

        let mut nvl_table = NameValueListTable::new();
        for entry in file.name_value_lists {
            let pairs = entry
                .into_iter()
                .map(|pair| NameValuePair {
                    key_index: pair[0] as i32,
                    value_index: pair[1] as i32,
                })
                .collect();
            nvl_table.add(pairs);
        }

        let archives: Vec<Vec<u8>> = file
            .archives
            .into_iter()
            .map(|a| {
                use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
                BASE64
                    .decode(a.data_b64.as_bytes())
                    .map_err(|e| MuninError::InvalidFormat(format!("archive base64: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut entries: Vec<IndexEntry> = Vec::with_capacity(file.events.len());
        let mut ctx = WriteContext::with_tables(version, strings, nvl_table);
        for ev in file.events {
            let record_kind = BinaryLogRecordKind::from_name(&ev.kind).ok_or_else(|| {
                MuninError::InvalidFormat(format!("unknown event kind: {}", ev.kind))
            })?;
            let payload = match ev.body {
                JsonlogEventBody::PayloadB64(s) => {
                    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
                    BASE64
                        .decode(s.as_bytes())
                        .map_err(|e| MuninError::InvalidFormat(format!("payload base64: {e}")))?
                }
                JsonlogEventBody::Decoded(value) => {
                    encode_decoded_event(record_kind, value, &mut ctx)?
                }
            };
            let context = extract_context(&payload, version);
            entries.push(IndexEntry {
                meta: EventMeta {
                    record_kind,
                    byte_offset: ev.byte_offset,
                    payload_len: payload.len(),
                    context,
                },
                payload,
            });
        }
        let strings = ctx.strings;
        let nvl_table = ctx.nvl_table;

        Ok(Self {
            header,
            strings,
            nvl_table,
            archives,
            entries,
        })
    }

    /// Write `self` as a `.binlog` byte stream to `writer`.
    ///
    /// Emits the binlog header, then all `String` and `NameValueList`
    /// aux records (in their stored order), then any
    /// `ProjectImportArchive` blobs, then every event record in stored
    /// order, terminated by an `EndOfFile` sentinel. The whole stream
    /// is gzip-compressed to match the on-disk binlog format.
    ///
    /// The output is semantically equivalent to a roundtrip of the
    /// original binlog — every `meta(i)` and decoded `get(i)` will
    /// match — but is not guaranteed to be byte-exact, since the
    /// original interleaving of aux and event records is not
    /// preserved by [`crate::jsonlog::JsonlogFile`].
    pub fn write_binlog<W: Write>(&self, writer: W) -> Result<(), MuninError> {
        use flate2::{Compression, write::GzEncoder};

        let mut gz = GzEncoder::new(writer, Compression::default());

        // Header: file_format_version + min_reader_version, both little-endian i32.
        gz.write_all(&self.header.file_format_version.to_le_bytes())?;
        gz.write_all(&self.header.min_reader_version.to_le_bytes())?;

        // All String records first, preserving original indices.
        for s in self.strings.entries() {
            let bytes = s.as_bytes();
            w_7bit(&mut gz, BinaryLogRecordKind::String as i32)?;
            w_7bit(&mut gz, bytes.len() as i32)?;
            gz.write_all(bytes)?;
        }

        // All NameValueList records next.
        for list in self.nvl_table.entries() {
            // Pre-encode payload to a buffer to know length.
            let mut buf = Vec::new();
            w_7bit(&mut buf, list.len() as i32)?;
            for pair in list {
                w_7bit(&mut buf, pair.key_index)?;
                w_7bit(&mut buf, pair.value_index)?;
            }
            w_7bit(&mut gz, BinaryLogRecordKind::NameValueList as i32)?;
            w_7bit(&mut gz, buf.len() as i32)?;
            gz.write_all(&buf)?;
        }

        // ProjectImportArchive blobs.
        for archive in &self.archives {
            w_7bit(&mut gz, BinaryLogRecordKind::ProjectImportArchive as i32)?;
            w_7bit(&mut gz, archive.len() as i32)?;
            gz.write_all(archive)?;
        }

        // Event records.
        for entry in &self.entries {
            w_7bit(&mut gz, entry.meta.record_kind as i32)?;
            w_7bit(&mut gz, entry.payload.len() as i32)?;
            gz.write_all(&entry.payload)?;
        }

        // Terminating EndOfFile sentinel.
        w_7bit(&mut gz, BinaryLogRecordKind::EndOfFile as i32)?;

        gz.finish()?;
        Ok(())
    }

    /// The parsed binlog file header.
    pub fn header(&self) -> &BinlogHeader {
        &self.header
    }

    /// The string table accumulated during indexing.
    pub fn strings(&self) -> &StringTable {
        &self.strings
    }

    /// Mutable access to the string table, for in-place rewrites by
    /// [`crate::redact::Redactor`]. Replacing an entry's contents keeps
    /// every existing string-index reference valid, since indices are
    /// positional.
    pub fn strings_mut(&mut self) -> &mut StringTable {
        &mut self.strings
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

    /// Raw stored payload bytes for the event at `index`, or `None` if
    /// out of range. The slice contains the record payload exactly as
    /// it appeared after the record kind / length prefix in the
    /// decompressed binlog stream.
    ///
    /// Primarily intended for the jsonlog dumper's `payload_b64`
    /// fallback and for round-trip integrity checks.
    pub fn payload_bytes(&self, index: usize) -> Option<&[u8]> {
        self.entries.get(index).map(|e| e.payload.as_slice())
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
                if let Some(k) = kind
                    && e.meta.record_kind != k
                {
                    return false;
                }
                if let Some(pid) = project_context_id
                    && e.meta.context.is_none_or(|c| c.project_context_id != pid)
                {
                    return false;
                }
                if let Some(tid) = target_id
                    && e.meta.context.is_none_or(|c| c.target_id != tid)
                {
                    return false;
                }
                if let Some(tsk) = task_id
                    && e.meta.context.is_none_or(|c| c.task_id != tsk)
                {
                    return false;
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

// ---------------------------------------------------------------------------
// Decoded-event re-encoder (jsonlog → binlog payload bytes)
// ---------------------------------------------------------------------------

/// Re-encode a decoded event (deserialized as a `serde_json::Value`) into
/// the byte sequence the binlog reader for the same record kind would
/// consume. Used by [`BinlogIndex::open_json`].
fn encode_decoded_event(
    kind: BinaryLogRecordKind,
    value: serde_json::Value,
    ctx: &mut WriteContext,
) -> Result<Vec<u8>, MuninError> {
    use crate::events as ev;
    let mut buf = Vec::new();
    macro_rules! enc {
        ($Ty:ty, $write:path) => {{
            let parsed: $Ty = serde_json::from_value(value)
                .map_err(|e| MuninError::InvalidFormat(format!("decoded event: {e}")))?;
            $write(&mut buf, ctx, &parsed)
                .map_err(|e| MuninError::InvalidFormat(format!("encode event: {e}")))?;
        }};
    }
    match kind {
        BinaryLogRecordKind::BuildStarted => enc!(ev::BuildStartedEvent, ev::write_build_started),
        BinaryLogRecordKind::BuildFinished => {
            enc!(ev::BuildFinishedEvent, ev::write_build_finished)
        }
        BinaryLogRecordKind::ProjectStarted => {
            enc!(ev::ProjectStartedEvent, ev::write_project_started)
        }
        BinaryLogRecordKind::ProjectFinished => {
            enc!(ev::ProjectFinishedEvent, ev::write_project_finished)
        }
        BinaryLogRecordKind::TargetStarted => {
            enc!(ev::TargetStartedEvent, ev::write_target_started)
        }
        BinaryLogRecordKind::TargetFinished => {
            enc!(ev::TargetFinishedEvent, ev::write_target_finished)
        }
        BinaryLogRecordKind::TargetSkipped => {
            enc!(ev::TargetSkippedEvent, ev::write_target_skipped)
        }
        BinaryLogRecordKind::TaskStarted => enc!(ev::TaskStartedEvent, ev::write_task_started),
        BinaryLogRecordKind::TaskFinished => enc!(ev::TaskFinishedEvent, ev::write_task_finished),
        BinaryLogRecordKind::TaskCommandLine => {
            enc!(ev::TaskCommandLineEvent, ev::write_task_command_line)
        }
        BinaryLogRecordKind::TaskParameter => {
            enc!(ev::TaskParameterEvent, ev::write_task_parameter)
        }
        BinaryLogRecordKind::Error => enc!(ev::BuildErrorEvent, ev::write_build_error),
        BinaryLogRecordKind::Warning => enc!(ev::BuildWarningEvent, ev::write_build_warning),
        BinaryLogRecordKind::Message => enc!(ev::BuildMessageEvent, ev::write_build_message),
        BinaryLogRecordKind::CriticalBuildMessage => {
            enc!(
                ev::CriticalBuildMessageEvent,
                ev::write_critical_build_message
            )
        }
        BinaryLogRecordKind::ProjectEvaluationStarted => enc!(
            ev::ProjectEvaluationStartedEvent,
            ev::write_project_evaluation_started
        ),
        BinaryLogRecordKind::ProjectEvaluationFinished => enc!(
            ev::ProjectEvaluationFinishedEvent,
            ev::write_project_evaluation_finished
        ),
        BinaryLogRecordKind::PropertyReassignment => enc!(
            ev::PropertyReassignmentEvent,
            ev::write_property_reassignment
        ),
        BinaryLogRecordKind::UninitializedPropertyRead => enc!(
            ev::UninitializedPropertyReadEvent,
            ev::write_uninitialized_property_read
        ),
        BinaryLogRecordKind::PropertyInitialValueSet => enc!(
            ev::PropertyInitialValueSetEvent,
            ev::write_property_initial_value_set
        ),
        BinaryLogRecordKind::EnvironmentVariableRead => enc!(
            ev::EnvironmentVariableReadEvent,
            ev::write_environment_variable_read
        ),
        BinaryLogRecordKind::ResponseFileUsed => {
            enc!(ev::ResponseFileUsedEvent, ev::write_response_file_used)
        }
        BinaryLogRecordKind::AssemblyLoad => enc!(ev::AssemblyLoadEvent, ev::write_assembly_load),
        BinaryLogRecordKind::ProjectImported => {
            enc!(ev::ProjectImportedEvent, ev::write_project_imported)
        }
        BinaryLogRecordKind::BuildCheckMessage => {
            enc!(ev::BuildCheckMessageEvent, ev::write_build_message)
        }
        BinaryLogRecordKind::BuildCheckWarning => {
            enc!(ev::BuildCheckWarningEvent, ev::write_build_warning)
        }
        BinaryLogRecordKind::BuildCheckError => {
            enc!(ev::BuildCheckErrorEvent, ev::write_build_error)
        }
        BinaryLogRecordKind::BuildCheckTracing => {
            enc!(ev::BuildCheckTracingEvent, ev::write_build_check_tracing)
        }
        BinaryLogRecordKind::BuildCheckAcquisition => enc!(
            ev::BuildCheckAcquisitionEvent,
            ev::write_build_check_acquisition
        ),
        BinaryLogRecordKind::BuildSubmissionStarted => enc!(
            ev::BuildSubmissionStartedEvent,
            ev::write_build_submission_started
        ),
        BinaryLogRecordKind::BuildCanceled => {
            enc!(ev::BuildCanceledEvent, ev::write_build_canceled)
        }
        BinaryLogRecordKind::EndOfFile
        | BinaryLogRecordKind::String
        | BinaryLogRecordKind::NameValueList
        | BinaryLogRecordKind::ProjectImportArchive => {
            return Err(MuninError::InvalidFormat(format!(
                "cannot encode auxiliary record kind: {:?}",
                kind
            )));
        }
    }
    Ok(buf)
}
