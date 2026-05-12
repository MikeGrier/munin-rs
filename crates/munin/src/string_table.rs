// Copyright (c) Michael Grier

//! Deduplicated string table for binlog reading.
//!
//! Strings in the binlog are assigned sequential indices starting at 10.
//! Index 0 means null (absent), index 1 means empty string, and indices 2–9
//! are reserved. Changing these constants is a breaking change.

use crate::error::MuninError;

/// First index assigned to a stored string record.
const STRING_START_INDEX: i32 = 10;

/// Index value representing a null/absent string.
const NULL_INDEX: i32 = 0;

/// Index value representing an empty string.
const EMPTY_INDEX: i32 = 1;

/// A table of deduplicated strings read from String records in the binlog stream.
///
/// Strings are appended in the order they appear (each String record gets the
/// next sequential index starting at [`STRING_START_INDEX`]). Event payloads
/// reference strings by index.
#[derive(Debug, Default)]
pub struct StringTable {
    entries: Vec<String>,
}

impl StringTable {
    /// Create an empty string table.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a string record. Returns the index assigned to it.
    pub fn add(&mut self, value: String) -> i32 {
        let index = STRING_START_INDEX + self.entries.len() as i32;
        self.entries.push(value);
        index
    }

    /// Look up a string by its on-disk index.
    ///
    /// - Index 0 → `Ok(None)` (null)
    /// - Index 1 → `Ok(Some(""))` (empty)
    /// - Index 2–9 → `Err(InvalidIndex)` (reserved, not yet assigned meaning)
    /// - Index ≥ 10 → lookup in the table
    pub fn get(&self, index: i32) -> Result<Option<&str>, MuninError> {
        if index == NULL_INDEX {
            return Ok(None);
        }
        if index == EMPTY_INDEX {
            return Ok(Some(""));
        }
        let table_index = index - STRING_START_INDEX;
        if table_index < 0 || table_index as usize >= self.entries.len() {
            return Err(MuninError::InvalidIndex {
                kind: "string",
                index,
            });
        }
        Ok(Some(&self.entries[table_index as usize]))
    }

    /// Number of string records ingested so far.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty (no string records ingested).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests;
