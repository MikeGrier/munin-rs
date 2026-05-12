// Copyright (c) Michael Grier

//! Deduplicated name-value list table for binlog reading.
//!
//! Name-value lists store pairs of (key-index, value-index) referencing
//! the string table. Lists are assigned sequential indices starting at 10.
//! Index 0 means null (absent). Indices 1–9 are reserved.
//! Changing these constants is a breaking change.

use crate::{error::MuninError, string_table::StringTable};

/// First index assigned to a stored name-value list record.
const NVL_START_INDEX: i32 = 10;

/// Index value representing a null/absent name-value list.
const NULL_INDEX: i32 = 0;

/// A single entry in a name-value list: indices into the string table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameValuePair {
    /// String table index of the key.
    pub key_index: i32,
    /// String table index of the value.
    pub value_index: i32,
}

/// A table of deduplicated name-value lists read from NameValueList records.
///
/// Each NameValueList record stores a list of `(key_index, value_index)` pairs
/// referencing the string table. Lists are assigned sequential indices starting
/// at [`NVL_START_INDEX`].
#[derive(Debug, Default)]
pub struct NameValueListTable {
    entries: Vec<Vec<NameValuePair>>,
}

impl NameValueListTable {
    /// Create an empty name-value list table.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a name-value list record. Returns the index assigned to it.
    pub fn add(&mut self, pairs: Vec<NameValuePair>) -> i32 {
        let index = NVL_START_INDEX + self.entries.len() as i32;
        self.entries.push(pairs);
        index
    }

    /// Look up a name-value list by its on-disk index, returning the raw
    /// `(key_index, value_index)` pairs.
    ///
    /// - Index 0 → `Ok(None)` (null)
    /// - Index 1–9 → `Err(InvalidIndex)` (reserved)
    /// - Index ≥ 10 → lookup in the table
    pub fn get(&self, index: i32) -> Result<Option<&[NameValuePair]>, MuninError> {
        if index == NULL_INDEX {
            return Ok(None);
        }
        let table_index = index - NVL_START_INDEX;
        if table_index < 0 || table_index as usize >= self.entries.len() {
            return Err(MuninError::InvalidIndex {
                kind: "name-value list",
                index,
            });
        }
        Ok(Some(&self.entries[table_index as usize]))
    }

    /// Resolve a name-value list index into a `Vec<(key, value)>` of resolved
    /// strings, using the provided string table.
    ///
    /// - Null keys are skipped (matching C# reference behavior).
    /// - Null values are replaced with empty strings.
    pub fn resolve(
        &self,
        index: i32,
        strings: &StringTable,
    ) -> Result<Option<Vec<(String, String)>>, MuninError> {
        let pairs = match self.get(index)? {
            Some(p) => p,
            None => return Ok(None),
        };
        let mut result = Vec::with_capacity(pairs.len());
        for pair in pairs {
            let key = match strings.get(pair.key_index)? {
                Some(k) => k.to_string(),
                None => continue, // null keys are skipped
            };
            let value = strings.get(pair.value_index)?.unwrap_or("").to_string();
            result.push((key, value));
        }
        Ok(Some(result))
    }

    /// Number of name-value list records ingested.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests;
