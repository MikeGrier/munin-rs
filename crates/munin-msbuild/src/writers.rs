// Copyright (c) Michael Grier

//! Writer primitives and per-event encoders mirroring [`crate::readers`] and
//! [`crate::primitives`].
//!
//! Every `read_*` in this crate has a paired `write_*` here (or in
//! [`crate::events`] for event encoders). Writers operate against a
//! [`WriteContext`] that holds the in-progress string and name-value-list
//! dedup tables, supporting both pre-populated tables (the
//! [`BinlogIndex::open_json`](crate::index::BinlogIndex::open_json) path,
//! which reconstructs the tables from a [`crate::jsonlog::JsonlogFile`])
//! and incremental interning (hand-authored jsonlog and unit tests).
//!
//! Writers emit the exact byte sequence the paired reader would consume.
//! This is a hard contract — round-trip tests in
//! [`crate::events::tests`] assert it for every event variant.

use std::{
    collections::HashMap,
    io::{self, Write},
};

use crate::{
    nvl_table::{NameValueListTable, NameValuePair},
    primitives::BinlogDateTime,
    string_table::StringTable,
};

// ---------------------------------------------------------------------------
// WriteContext — interning state shared by all encoders
// ---------------------------------------------------------------------------

/// Mutable interning state used by every `write_*` encoder.
///
/// The dedup tables are append-only: once a string or NVL gets an index it
/// keeps it. Strings and NVLs added through [`Self::intern_string`] /
/// [`Self::intern_nvl`] are tracked in `pending_*` so the binlog writer can
/// flush them as `String` / `NameValueList` aux records before the next
/// event that references them.
#[derive(Debug)]
pub struct WriteContext {
    pub strings: StringTable,
    string_index: HashMap<String, i32>,
    pub nvl_table: NameValueListTable,
    nvl_index: HashMap<Vec<(i32, i32)>, i32>,
    pub file_format_version: i32,
    pending_strings: Vec<i32>,
    pending_nvls: Vec<i32>,
}

impl WriteContext {
    /// Empty context. Strings and NVLs will be interned on demand.
    pub fn new(file_format_version: i32) -> Self {
        Self {
            strings: StringTable::new(),
            string_index: HashMap::new(),
            nvl_table: NameValueListTable::new(),
            nvl_index: HashMap::new(),
            file_format_version,
            pending_strings: Vec::new(),
            pending_nvls: Vec::new(),
        }
    }

    /// Pre-populate from existing dedup tables (e.g. those reconstructed
    /// from a [`crate::jsonlog::JsonlogFile`]). All entries are considered
    /// already-flushed (not pending).
    pub fn with_tables(
        file_format_version: i32,
        strings: StringTable,
        nvl_table: NameValueListTable,
    ) -> Self {
        let mut string_index: HashMap<String, i32> = HashMap::new();
        for (i, s) in strings.entries().iter().enumerate() {
            // String table starts at index 10.
            string_index.entry(s.clone()).or_insert(10 + i as i32);
        }
        let mut nvl_index: HashMap<Vec<(i32, i32)>, i32> = HashMap::new();
        for (i, list) in nvl_table.entries().iter().enumerate() {
            let key: Vec<(i32, i32)> = list.iter().map(|p| (p.key_index, p.value_index)).collect();
            // NVL table starts at index 10.
            nvl_index.entry(key).or_insert(10 + i as i32);
        }
        Self {
            strings,
            string_index,
            nvl_table,
            nvl_index,
            file_format_version,
            pending_strings: Vec::new(),
            pending_nvls: Vec::new(),
        }
    }

    /// Resolve a string to its dedup index, appending a new entry if it
    /// isn't already present.
    ///
    /// - `None` → index 0 (null sentinel).
    /// - `Some("")` → index 1 (empty sentinel).
    /// - `Some(s)` → existing index, or a new index assigned by appending.
    pub fn intern_string(&mut self, s: Option<&str>) -> i32 {
        match s {
            None => 0,
            Some("") => 1,
            Some(s) => {
                if let Some(&idx) = self.string_index.get(s) {
                    return idx;
                }
                let idx = self.strings.add(s.to_string());
                self.string_index.insert(s.to_string(), idx);
                self.pending_strings.push(idx);
                idx
            }
        }
    }

    /// Resolve a list of (key, value) pairs to a NVL dedup index, interning
    /// the strings as needed.
    pub fn intern_nvl(&mut self, pairs: &[(String, String)]) -> i32 {
        let resolved: Vec<(i32, i32)> = pairs
            .iter()
            .map(|(k, v)| (self.intern_string(Some(k)), self.intern_string(Some(v))))
            .collect();
        if let Some(&idx) = self.nvl_index.get(&resolved) {
            return idx;
        }
        let typed: Vec<NameValuePair> = resolved
            .iter()
            .map(|(k, v)| NameValuePair {
                key_index: *k,
                value_index: *v,
            })
            .collect();
        let idx = self.nvl_table.add(typed.clone());
        self.nvl_index.insert(resolved, idx);
        self.pending_nvls.push(idx);
        idx
    }

    /// Take the list of string indices appended since the last drain.
    pub fn take_pending_strings(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.pending_strings)
    }

    /// Take the list of NVL indices appended since the last drain.
    pub fn take_pending_nvls(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.pending_nvls)
    }
}

// ---------------------------------------------------------------------------
// Primitive encoders (mirror of crate::primitives readers)
// ---------------------------------------------------------------------------

/// Encode `value` as a 7-bit variable-length integer.
pub fn write_7bit_int<W: Write>(w: &mut W, value: i32) -> io::Result<()> {
    let mut v = value as u32;
    loop {
        let mut b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            w.write_all(&[b])?;
            return Ok(());
        } else {
            b |= 0x80;
            w.write_all(&[b])?;
        }
    }
}

/// Encode a `bool` as 1 byte.
pub fn write_bool<W: Write>(w: &mut W, v: bool) -> io::Result<()> {
    w.write_all(&[u8::from(v)])
}

/// Encode an `i32` as 4 bytes little-endian.
pub fn write_i32_le<W: Write>(w: &mut W, v: i32) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

/// Encode an `i64` as 8 bytes little-endian.
pub fn write_i64_le<W: Write>(w: &mut W, v: i64) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

/// Encode a 16-byte GUID.
pub fn write_guid<W: Write>(w: &mut W, mvid: &[u8; 16]) -> io::Result<()> {
    w.write_all(mvid)
}

/// Encode a .NET `BinaryWriter` length-prefixed UTF-8 string.
pub fn write_dotnet_string<W: Write>(w: &mut W, s: &str) -> io::Result<()> {
    write_7bit_int(w, s.len() as i32)?;
    w.write_all(s.as_bytes())
}

/// Encode a .NET `DateTime` (i64 ticks + 7-bit kind).
pub fn write_datetime<W: Write>(w: &mut W, dt: BinlogDateTime) -> io::Result<()> {
    write_i64_le(w, dt.ticks)?;
    write_7bit_int(w, dt.kind)
}

// ---------------------------------------------------------------------------
// Composite encoders (mirror of crate::readers)
// ---------------------------------------------------------------------------

/// Encode a deduplicated string reference.
pub fn write_dedup_string<W: Write>(
    w: &mut W,
    ctx: &mut WriteContext,
    s: Option<&str>,
) -> io::Result<()> {
    let idx = ctx.intern_string(s);
    write_7bit_int(w, idx)
}

/// Encode an "optional string" — bool-prefixed raw string for format < 10,
/// dedup-string index for ≥ 10.
pub fn write_optional_string<W: Write>(
    w: &mut W,
    ctx: &mut WriteContext,
    s: Option<&str>,
) -> io::Result<()> {
    if ctx.file_format_version < 10 {
        match s {
            None => write_bool(w, false),
            Some(s) => {
                write_bool(w, true)?;
                write_dotnet_string(w, s)
            }
        }
    } else {
        write_dedup_string(w, ctx, s)
    }
}

/// Encode a string dictionary (NVL).
///
/// - Format version < 10: inline (count, then count × (raw key, raw value)).
/// - Format version ≥ 10: 7-bit NVL index (0 = null).
pub fn write_string_dictionary<W: Write>(
    w: &mut W,
    ctx: &mut WriteContext,
    dict: Option<&[(String, String)]>,
) -> io::Result<()> {
    if ctx.file_format_version < 10 {
        match dict {
            None => write_7bit_int(w, 0),
            Some(pairs) => {
                write_7bit_int(w, pairs.len() as i32)?;
                for (k, v) in pairs {
                    write_dotnet_string(w, k)?;
                    write_dotnet_string(w, v)?;
                }
                Ok(())
            }
        }
    } else {
        match dict {
            None => write_7bit_int(w, 0),
            Some(pairs) => {
                let idx = ctx.intern_nvl(pairs);
                write_7bit_int(w, idx)
            }
        }
    }
}
