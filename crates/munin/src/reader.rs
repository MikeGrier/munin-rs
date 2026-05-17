// Copyright (c) Michael Grier

//! Sequential binlog reader — reads a `.binlog` stream record by record,
//! ingesting auxiliary data (string table, name-value lists, embedded
//! archives) and yielding typed build events.
//!
//! The reader uses a buffer-per-record approach: each record's payload is
//! read into a `Vec<u8>`, then deserialized via a `Cursor`. This gives
//! forward-compatibility for free — if a newer MSBuild version appends
//! trailing fields to a known record, the extra bytes are harmlessly
//! discarded when the `Cursor` is dropped. Unknown record kinds are
//! skipped entirely.

use std::io::{BufReader, Cursor, Read};

use flate2::read::GzDecoder;

use crate::{
    error::MuninError,
    events::*,
    header::{open_binlog, BinlogHeader},
    nvl_table::{NameValueListTable, NameValuePair},
    primitives::{read_7bit_int, read_7bit_count, read_7bit_length},
    record_kind::BinaryLogRecordKind,
    string_table::StringTable,
};

// ---------------------------------------------------------------------------
// BinlogEvent enum — one variant per event type
// ---------------------------------------------------------------------------

/// A typed build event read from the binlog stream.
///
/// Each variant wraps the corresponding event struct from [`crate::events`].
/// The variant name preserves the record kind, so `BuildCheckMessage` and
/// `Message` are distinct even though they share the same struct layout.
#[derive(Debug, Clone)]
pub enum BinlogEvent {
    BuildStarted(BuildStartedEvent),
    BuildFinished(BuildFinishedEvent),
    ProjectStarted(ProjectStartedEvent),
    ProjectFinished(ProjectFinishedEvent),
    TargetStarted(TargetStartedEvent),
    TargetFinished(TargetFinishedEvent),
    TargetSkipped(TargetSkippedEvent),
    TaskStarted(TaskStartedEvent),
    TaskFinished(TaskFinishedEvent),
    TaskCommandLine(TaskCommandLineEvent),
    TaskParameter(TaskParameterEvent),
    Error(BuildErrorEvent),
    Warning(BuildWarningEvent),
    Message(BuildMessageEvent),
    CriticalBuildMessage(CriticalBuildMessageEvent),
    ProjectEvaluationStarted(ProjectEvaluationStartedEvent),
    ProjectEvaluationFinished(ProjectEvaluationFinishedEvent),
    PropertyReassignment(PropertyReassignmentEvent),
    UninitializedPropertyRead(UninitializedPropertyReadEvent),
    PropertyInitialValueSet(PropertyInitialValueSetEvent),
    EnvironmentVariableRead(EnvironmentVariableReadEvent),
    ResponseFileUsed(ResponseFileUsedEvent),
    AssemblyLoad(AssemblyLoadEvent),
    ProjectImported(ProjectImportedEvent),
    BuildCheckMessage(BuildCheckMessageEvent),
    BuildCheckWarning(BuildCheckWarningEvent),
    BuildCheckError(BuildCheckErrorEvent),
    BuildCheckTracing(BuildCheckTracingEvent),
    BuildCheckAcquisition(BuildCheckAcquisitionEvent),
    BuildSubmissionStarted(BuildSubmissionStartedEvent),
    BuildCanceled(BuildCanceledEvent),
}

impl BinlogEvent {
    /// Returns the record kind that produced this event.
    pub fn record_kind(&self) -> BinaryLogRecordKind {
        match self {
            Self::BuildStarted(_) => BinaryLogRecordKind::BuildStarted,
            Self::BuildFinished(_) => BinaryLogRecordKind::BuildFinished,
            Self::ProjectStarted(_) => BinaryLogRecordKind::ProjectStarted,
            Self::ProjectFinished(_) => BinaryLogRecordKind::ProjectFinished,
            Self::TargetStarted(_) => BinaryLogRecordKind::TargetStarted,
            Self::TargetFinished(_) => BinaryLogRecordKind::TargetFinished,
            Self::TargetSkipped(_) => BinaryLogRecordKind::TargetSkipped,
            Self::TaskStarted(_) => BinaryLogRecordKind::TaskStarted,
            Self::TaskFinished(_) => BinaryLogRecordKind::TaskFinished,
            Self::TaskCommandLine(_) => BinaryLogRecordKind::TaskCommandLine,
            Self::TaskParameter(_) => BinaryLogRecordKind::TaskParameter,
            Self::Error(_) => BinaryLogRecordKind::Error,
            Self::Warning(_) => BinaryLogRecordKind::Warning,
            Self::Message(_) => BinaryLogRecordKind::Message,
            Self::CriticalBuildMessage(_) => BinaryLogRecordKind::CriticalBuildMessage,
            Self::ProjectEvaluationStarted(_) => BinaryLogRecordKind::ProjectEvaluationStarted,
            Self::ProjectEvaluationFinished(_) => BinaryLogRecordKind::ProjectEvaluationFinished,
            Self::PropertyReassignment(_) => BinaryLogRecordKind::PropertyReassignment,
            Self::UninitializedPropertyRead(_) => BinaryLogRecordKind::UninitializedPropertyRead,
            Self::PropertyInitialValueSet(_) => BinaryLogRecordKind::PropertyInitialValueSet,
            Self::EnvironmentVariableRead(_) => BinaryLogRecordKind::EnvironmentVariableRead,
            Self::ResponseFileUsed(_) => BinaryLogRecordKind::ResponseFileUsed,
            Self::AssemblyLoad(_) => BinaryLogRecordKind::AssemblyLoad,
            Self::ProjectImported(_) => BinaryLogRecordKind::ProjectImported,
            Self::BuildCheckMessage(_) => BinaryLogRecordKind::BuildCheckMessage,
            Self::BuildCheckWarning(_) => BinaryLogRecordKind::BuildCheckWarning,
            Self::BuildCheckError(_) => BinaryLogRecordKind::BuildCheckError,
            Self::BuildCheckTracing(_) => BinaryLogRecordKind::BuildCheckTracing,
            Self::BuildCheckAcquisition(_) => BinaryLogRecordKind::BuildCheckAcquisition,
            Self::BuildSubmissionStarted(_) => BinaryLogRecordKind::BuildSubmissionStarted,
            Self::BuildCanceled(_) => BinaryLogRecordKind::BuildCanceled,
        }
    }
}

// ---------------------------------------------------------------------------
// BinlogReader
// ---------------------------------------------------------------------------

/// Sequential reader over a `.binlog` stream.
///
/// Opens the binlog header, decompresses the GZip body, and yields typed
/// [`BinlogEvent`] values one at a time via [`next_event`](Self::next_event).
///
/// Auxiliary records (string table entries, name-value lists, embedded
/// archives) are ingested automatically and are accessible through the
/// corresponding accessor methods.
#[derive(Debug)]
pub struct BinlogReader<R: Read> {
    reader: BufReader<GzDecoder<R>>,
    header: BinlogHeader,
    strings: StringTable,
    nvl_table: NameValueListTable,
    /// Embedded zip archives from ProjectImportArchive records.
    archives: Vec<Vec<u8>>,
    /// Whether we have reached EndOfFile.
    finished: bool,
}

impl<R: Read> BinlogReader<R> {
    /// Open a binlog stream and prepare for sequential reading.
    ///
    /// Reads the uncompressed header, validates the format version, and sets
    /// up GZip decompression for the record body.
    pub fn open(reader: R) -> Result<Self, MuninError> {
        let (header, gz_reader) = open_binlog(reader)?;
        Ok(Self {
            reader: gz_reader,
            header,
            strings: StringTable::new(),
            nvl_table: NameValueListTable::new(),
            archives: Vec::new(),
            finished: false,
        })
    }

    /// The parsed binlog file header.
    pub fn header(&self) -> &BinlogHeader {
        &self.header
    }

    /// The string table accumulated so far.
    pub fn strings(&self) -> &StringTable {
        &self.strings
    }

    /// The name-value list table accumulated so far.
    pub fn nvl_table(&self) -> &NameValueListTable {
        &self.nvl_table
    }

    /// Embedded zip archives read from ProjectImportArchive records.
    pub fn archives(&self) -> &[Vec<u8>] {
        &self.archives
    }

    /// Extract all files from all embedded ProjectImportArchive zip archives.
    ///
    /// Returns a list of `(path, contents)` pairs. Paths are as stored in the
    /// zip (typically relative project file paths). If a zip entry cannot be
    /// read (e.g. unsupported compression), that entry is silently skipped.
    pub fn extract_archives(&self) -> Result<Vec<ArchiveEntry>, MuninError> {
        let mut entries = Vec::new();
        for archive_bytes in &self.archives {
            extract_zip_entries(archive_bytes, &mut entries)?;
        }
        Ok(entries)
    }

    /// Read the next build event from the stream.
    ///
    /// Returns `Ok(None)` when the `EndOfFile` record is reached.
    ///
    /// Auxiliary records (String, NameValueList, ProjectImportArchive) are
    /// ingested internally and do not produce events. Unknown record kinds
    /// are silently skipped (forward-compatibility).
    pub fn next_event(&mut self) -> Result<Option<BinlogEvent>, MuninError> {
        if self.finished {
            return Ok(None);
        }

        let version = self.header.file_format_version;

        loop {
            // Record kind: 7-bit variable-length encoded i32.
            let kind_raw = read_7bit_int(&mut self.reader)?;

            if kind_raw == BinaryLogRecordKind::EndOfFile as i32 {
                self.finished = true;
                return Ok(None);
            }

            // Record length (bytes of payload). Always present for v18+,
            // and we only support v18+.
            let record_length = read_7bit_length(&mut self.reader, "record length")?;

            // Read the entire payload into a buffer. This gives us
            // forward-compatibility: if the payload is longer than what
            // the deserializer consumes, the extra bytes are discarded.
            let mut payload = vec![0u8; record_length];
            self.reader.read_exact(&mut payload)?;

            let kind = BinaryLogRecordKind::from_raw(kind_raw);

            match kind {
                // --- Auxiliary: string table entry ---
                // The payload is the raw UTF-8 string bytes; the record
                // length provides the string length (no dotnet length prefix).
                Some(BinaryLogRecordKind::String) => {
                    let s = String::from_utf8(payload).map_err(|_| MuninError::InvalidUtf8)?;
                    self.strings.add(s);
                }

                // --- Auxiliary: name-value list entry ---
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
                    self.nvl_table.add(pairs);
                }

                // --- Auxiliary: embedded archive ---
                Some(BinaryLogRecordKind::ProjectImportArchive) => {
                    self.archives.push(payload);
                }

                // --- Known event record ---
                Some(record_kind) => {
                    let mut cursor = Cursor::new(&payload);
                    let event = dispatch_event(
                        &mut cursor,
                        record_kind,
                        &self.strings,
                        &self.nvl_table,
                        version,
                    )?;
                    return Ok(Some(event));
                }

                // --- Unknown record kind (forward-compatibility) ---
                None => {
                    // Payload already consumed into `payload`; discard it.
                }
            }
        }
    }

    /// Collect all remaining events into a `Vec`.
    ///
    /// Reads events until `EndOfFile` is reached. Auxiliary records are
    /// ingested along the way.
    pub fn read_all_events(&mut self) -> Result<Vec<BinlogEvent>, MuninError> {
        let mut events = Vec::new();
        while let Some(event) = self.next_event()? {
            events.push(event);
        }
        Ok(events)
    }
}

// ---------------------------------------------------------------------------
// Event dispatch
// ---------------------------------------------------------------------------

/// Dispatch a record payload to the appropriate typed event reader.
///
/// The caller has already consumed the record kind and length from the
/// outer stream; `reader` is positioned at the start of the payload.
pub(crate) fn dispatch_event(
    reader: &mut impl Read,
    kind: BinaryLogRecordKind,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    version: i32,
) -> Result<BinlogEvent, MuninError> {
    match kind {
        BinaryLogRecordKind::BuildStarted => {
            let e = read_build_started(reader, strings, nvl_table, version)?;
            Ok(BinlogEvent::BuildStarted(e))
        }
        BinaryLogRecordKind::BuildFinished => {
            let e = read_build_finished(reader, strings, version)?;
            Ok(BinlogEvent::BuildFinished(e))
        }
        BinaryLogRecordKind::ProjectStarted => {
            let e = read_project_started(reader, strings, nvl_table, version)?;
            Ok(BinlogEvent::ProjectStarted(e))
        }
        BinaryLogRecordKind::ProjectFinished => {
            let e = read_project_finished(reader, strings, version)?;
            Ok(BinlogEvent::ProjectFinished(e))
        }
        BinaryLogRecordKind::TargetStarted => {
            let e = read_target_started(reader, strings, version)?;
            Ok(BinlogEvent::TargetStarted(e))
        }
        BinaryLogRecordKind::TargetFinished => {
            let e = read_target_finished(reader, strings, nvl_table, version)?;
            Ok(BinlogEvent::TargetFinished(e))
        }
        BinaryLogRecordKind::TargetSkipped => {
            let e = read_target_skipped(reader, strings, version)?;
            Ok(BinlogEvent::TargetSkipped(e))
        }
        BinaryLogRecordKind::TaskStarted => {
            let e = read_task_started(reader, strings, version)?;
            Ok(BinlogEvent::TaskStarted(e))
        }
        BinaryLogRecordKind::TaskFinished => {
            let e = read_task_finished(reader, strings, version)?;
            Ok(BinlogEvent::TaskFinished(e))
        }
        BinaryLogRecordKind::TaskCommandLine => {
            let e = read_task_command_line(reader, strings, version)?;
            Ok(BinlogEvent::TaskCommandLine(e))
        }
        BinaryLogRecordKind::TaskParameter => {
            let e = read_task_parameter(reader, strings, nvl_table, version)?;
            Ok(BinlogEvent::TaskParameter(e))
        }
        BinaryLogRecordKind::Error => {
            let e = read_build_error(reader, strings, version)?;
            Ok(BinlogEvent::Error(e))
        }
        BinaryLogRecordKind::Warning => {
            let e = read_build_warning(reader, strings, version)?;
            Ok(BinlogEvent::Warning(e))
        }
        BinaryLogRecordKind::Message => {
            let e = read_build_message(reader, strings, version)?;
            Ok(BinlogEvent::Message(e))
        }
        BinaryLogRecordKind::CriticalBuildMessage => {
            let e = read_critical_build_message(reader, strings, version)?;
            Ok(BinlogEvent::CriticalBuildMessage(e))
        }
        BinaryLogRecordKind::ProjectEvaluationStarted => {
            let e = read_project_evaluation_started(reader, strings, version)?;
            Ok(BinlogEvent::ProjectEvaluationStarted(e))
        }
        BinaryLogRecordKind::ProjectEvaluationFinished => {
            let e = read_project_evaluation_finished(reader, strings, nvl_table, version)?;
            Ok(BinlogEvent::ProjectEvaluationFinished(e))
        }
        BinaryLogRecordKind::PropertyReassignment => {
            let e = read_property_reassignment(reader, strings, version)?;
            Ok(BinlogEvent::PropertyReassignment(e))
        }
        BinaryLogRecordKind::UninitializedPropertyRead => {
            let e = read_uninitialized_property_read(reader, strings, version)?;
            Ok(BinlogEvent::UninitializedPropertyRead(e))
        }
        BinaryLogRecordKind::PropertyInitialValueSet => {
            let e = read_property_initial_value_set(reader, strings, version)?;
            Ok(BinlogEvent::PropertyInitialValueSet(e))
        }
        BinaryLogRecordKind::EnvironmentVariableRead => {
            let e = read_environment_variable_read(reader, strings, version)?;
            Ok(BinlogEvent::EnvironmentVariableRead(e))
        }
        BinaryLogRecordKind::ResponseFileUsed => {
            let e = read_response_file_used(reader, strings, version)?;
            Ok(BinlogEvent::ResponseFileUsed(e))
        }
        BinaryLogRecordKind::AssemblyLoad => {
            let e = read_assembly_load(reader, strings, version)?;
            Ok(BinlogEvent::AssemblyLoad(e))
        }
        BinaryLogRecordKind::ProjectImported => {
            let e = read_project_imported(reader, strings, version)?;
            Ok(BinlogEvent::ProjectImported(e))
        }
        BinaryLogRecordKind::BuildCheckMessage => {
            let e = read_build_message(reader, strings, version)?;
            Ok(BinlogEvent::BuildCheckMessage(e))
        }
        BinaryLogRecordKind::BuildCheckWarning => {
            let e = read_build_warning(reader, strings, version)?;
            Ok(BinlogEvent::BuildCheckWarning(e))
        }
        BinaryLogRecordKind::BuildCheckError => {
            let e = read_build_error(reader, strings, version)?;
            Ok(BinlogEvent::BuildCheckError(e))
        }
        BinaryLogRecordKind::BuildCheckTracing => {
            let e = read_build_check_tracing(reader, strings, nvl_table, version)?;
            Ok(BinlogEvent::BuildCheckTracing(e))
        }
        BinaryLogRecordKind::BuildCheckAcquisition => {
            let e = read_build_check_acquisition(reader, strings, version)?;
            Ok(BinlogEvent::BuildCheckAcquisition(e))
        }
        BinaryLogRecordKind::BuildSubmissionStarted => {
            let e = read_build_submission_started(reader, strings, nvl_table, version)?;
            Ok(BinlogEvent::BuildSubmissionStarted(e))
        }
        BinaryLogRecordKind::BuildCanceled => {
            let e = read_build_canceled(reader, strings, version)?;
            Ok(BinlogEvent::BuildCanceled(e))
        }

        // Auxiliary records are handled in next_event() before dispatch.
        BinaryLogRecordKind::EndOfFile
        | BinaryLogRecordKind::String
        | BinaryLogRecordKind::NameValueList
        | BinaryLogRecordKind::ProjectImportArchive => {
            unreachable!("auxiliary records handled before dispatch")
        }
    }
}

// ---------------------------------------------------------------------------
// Archive extraction
// ---------------------------------------------------------------------------

/// A file extracted from a ProjectImportArchive zip.
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    /// Path as stored in the zip (typically a relative project file path).
    pub path: String,
    /// UTF-8 file contents.
    pub contents: String,
}

/// Extract all entries from a single zip archive into `out`.
fn extract_zip_entries(
    archive_bytes: &[u8],
    out: &mut Vec<ArchiveEntry>,
) -> Result<(), MuninError> {
    let cursor = Cursor::new(archive_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| MuninError::InvalidFormat(format!("invalid ProjectImportArchive zip: {e}")))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| MuninError::InvalidFormat(format!("cannot read zip entry {i}: {e}")))?;

        // Skip directories.
        if file.is_dir() {
            continue;
        }

        let path = file.name().to_string();
        let mut contents = String::new();
        // Best-effort read: if the file is not valid UTF-8, skip it.
        match file.read_to_string(&mut contents) {
            Ok(_) => {
                out.push(ArchiveEntry { path, contents });
            }
            Err(_) => {
                // Non-UTF-8 binary content — skip silently.
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
