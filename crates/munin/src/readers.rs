// Copyright (c) Michael Grier

//! Higher-level read helpers that compose primitive readers with the
//! string and name-value-list tables.
//!
//! These correspond to the C# `ReadOptionalString`, `ReadDeduplicatedString`,
//! and `ReadStringDictionary` methods in `BuildEventArgsReader`.

use std::io::Read;

use crate::{
    error::MuninError,
    nvl_table::NameValueListTable,
    primitives::{read_7bit_count, read_7bit_int, read_bool, read_dotnet_string},
    string_table::StringTable,
};

/// Read a deduplicated string (format version ≥ 10).
///
/// Reads a 7-bit-encoded index and resolves it against the string table.
pub fn read_dedup_string(
    reader: &mut impl Read,
    strings: &StringTable,
) -> Result<Option<String>, MuninError> {
    let index = read_7bit_int(reader)?;
    Ok(strings.get(index)?.map(|s| s.to_string()))
}

/// Read an "optional string" from the stream.
///
/// - Format version < 10: reads a bool prefix; if true, reads a raw .NET string.
/// - Format version ≥ 10: reads a deduplicated string index.
pub fn read_optional_string(
    reader: &mut impl Read,
    strings: &StringTable,
    file_format_version: i32,
) -> Result<Option<String>, MuninError> {
    if file_format_version < 10 {
        if read_bool(reader)? {
            Ok(Some(read_dotnet_string(reader)?))
        } else {
            Ok(None)
        }
    } else {
        read_dedup_string(reader, strings)
    }
}

/// Read a string dictionary (name-value list).
///
/// - Format version < 10: reads a legacy inline dictionary (count, then
///   count × (key-string, value-string) pairs).
/// - Format version ≥ 10: reads a 7-bit int index into the NVL table.
///   Index 0 means null (returns `None`).
pub fn read_string_dictionary(
    reader: &mut impl Read,
    strings: &StringTable,
    nvl_table: &NameValueListTable,
    file_format_version: i32,
) -> Result<Option<Vec<(String, String)>>, MuninError> {
    if file_format_version < 10 {
        read_legacy_string_dictionary(reader)
    } else {
        let index = read_7bit_int(reader)?;
        if index == 0 {
            return Ok(None);
        }
        nvl_table.resolve(index, strings)
    }
}

/// Read a legacy (pre-v10) inline string dictionary.
fn read_legacy_string_dictionary(
    reader: &mut impl Read,
) -> Result<Option<Vec<(String, String)>>, MuninError> {
    let count = read_7bit_count(reader, "legacy string-dictionary count")?;
    if count == 0 {
        return Ok(None);
    }
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let key = read_dotnet_string(reader)?;
        let value = read_dotnet_string(reader)?;
        result.push((key, value));
    }
    Ok(Some(result))
}

#[cfg(test)]
mod tests;
