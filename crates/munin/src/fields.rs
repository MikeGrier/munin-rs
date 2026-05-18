// Copyright (c) Michael Grier

//! `BuildEventArgsFields` — the common field block that prefixes every
//! serialized build event.
//!
//! The field block starts with a `BuildEventArgsFieldFlags` bitmask that
//! indicates which fields are present, followed by the fields themselves
//! in a fixed order.

use std::io::Read;

use crate::{
    context::{read_build_event_context, BuildEventContext},
    error::MuninError,
    field_flags::BuildEventArgsFieldFlags,
    primitives::{read_7bit_count, read_7bit_int, read_datetime, BinlogDateTime},
    string_table::StringTable,
};

/// Extended data fields for custom/extended event types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedDataFields {
    pub extended_type: Option<String>,
    pub extended_metadata_index: i32,
    pub extended_data: Option<String>,
}

/// The common fields decoded from the flag-driven prefix of a build event.
#[derive(Debug, Clone, Default)]
pub struct BuildEventArgsFields {
    pub flags: BuildEventArgsFieldFlags,
    pub message: Option<String>,
    pub build_event_context: Option<BuildEventContext>,
    pub thread_id: i32,
    pub help_keyword: Option<String>,
    pub sender_name: Option<String>,
    pub timestamp: Option<BinlogDateTime>,
    pub extended: Option<ExtendedDataFields>,

    // Location fields (filled by diagnostic readers, not by ReadBuildEventArgsFields)
    pub subcategory: Option<String>,
    pub code: Option<String>,
    pub file: Option<String>,
    pub project_file: Option<String>,
    pub line_number: i32,
    pub column_number: i32,
    pub end_line_number: i32,
    pub end_column_number: i32,

    // Arguments
    pub arguments: Option<Vec<Option<String>>>,

    // Importance (for message-derived events)
    pub importance: i32,
}

/// Read the deduplicated-string index and resolve it against the string table.
fn read_dedup_string(
    reader: &mut impl Read,
    strings: &StringTable,
) -> Result<Option<String>, MuninError> {
    let index = read_7bit_int(reader)?;
    Ok(strings.get(index)?.map(|s| s.to_string()))
}

/// Read the common `BuildEventArgsFields` prefix from the stream.
///
/// `read_importance`: if true, importance is read even when the IMPORTANCE
/// flag is not set (for format versions < 13 where importance was always
/// written for message events).
pub fn read_build_event_args_fields(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
    read_importance: bool,
) -> Result<BuildEventArgsFields, MuninError> {
    let flags_raw = read_7bit_int(reader)? as u32;
    let flags = BuildEventArgsFieldFlags::from_raw(flags_raw);
    let mut fields = BuildEventArgsFields {
        flags,
        ..Default::default()
    };

    // Base fields — order matches the C# reference exactly.

    if flags.contains(BuildEventArgsFieldFlags::MESSAGE) {
        fields.message = read_dedup_string(reader, strings)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT) {
        fields.build_event_context = Some(read_build_event_context(reader, file_format_version)?);
    }

    if flags.contains(BuildEventArgsFieldFlags::THREAD_ID) {
        fields.thread_id = read_7bit_int(reader)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::HELP_KEYWORD) {
        fields.help_keyword = read_dedup_string(reader, strings)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::SENDER_NAME) {
        fields.sender_name = read_dedup_string(reader, strings)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::TIMESTAMP) {
        fields.timestamp = Some(read_datetime(reader)?);
    }

    if flags.contains(BuildEventArgsFieldFlags::EXTENDED) {
        let extended_type = read_dedup_string(reader, strings)?;
        // Extended metadata is a name-value list index — we store the raw
        // index here; resolution happens when the event is fully decoded.
        let extended_metadata_index = read_7bit_int(reader)?;
        let extended_data = read_dedup_string(reader, strings)?;
        fields.extended = Some(ExtendedDataFields {
            extended_type,
            extended_metadata_index,
            extended_data,
        });
    }

    // End of base fields. Diagnostic location fields follow.

    if flags.contains(BuildEventArgsFieldFlags::SUBCATEGORY) {
        fields.subcategory = read_dedup_string(reader, strings)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::CODE) {
        fields.code = read_dedup_string(reader, strings)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::FILE) {
        fields.file = read_dedup_string(reader, strings)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::PROJECT_FILE) {
        fields.project_file = read_dedup_string(reader, strings)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::LINE_NUMBER) {
        fields.line_number = read_7bit_int(reader)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::COLUMN_NUMBER) {
        fields.column_number = read_7bit_int(reader)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::END_LINE_NUMBER) {
        fields.end_line_number = read_7bit_int(reader)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::END_COLUMN_NUMBER) {
        fields.end_column_number = read_7bit_int(reader)?;
    }

    if flags.contains(BuildEventArgsFieldFlags::ARGUMENTS) {
        let count = read_7bit_count(reader, "event arguments count")?;
        let mut args = Vec::with_capacity(count);
        for _ in 0..count {
            args.push(read_dedup_string(reader, strings)?);
        }
        fields.arguments = Some(args);
    }

    // Importance: in format < 13, importance was unconditionally written for
    // message events (caller sets read_importance=true). In format >= 13,
    // presence is controlled by the IMPORTANCE flag.
    if (file_format_version < 13 && read_importance)
        || (file_format_version >= 13 && flags.contains(BuildEventArgsFieldFlags::IMPORTANCE))
    {
        fields.importance = read_7bit_int(reader)?;
    }

    Ok(fields)
}

#[cfg(test)]
mod tests;
