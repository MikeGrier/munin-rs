// Copyright (c) Michael Grier

//! Munin — MSBuild binary log (.binlog) reader and seekable data model.
//!
//! This crate reads and decodes the MSBuild binary log format, providing
//! both sequential streaming access and a seekable indexed data model over
//! the build events recorded in a `.binlog` file.
//!
//! The binary log format is a GZip-compressed stream of serialized
//! `BuildEventArgs` records with deduplicated string and name-value-list
//! tables. See the MSBuild repository for the canonical format documentation:
//! <https://github.com/dotnet/msbuild/blob/main/documentation/wiki/Binary-Log.md>

pub mod context;
pub mod error;
pub mod events;
pub mod field_flags;
pub mod fields;
pub mod header;
pub mod index;
pub mod nvl_table;
pub mod primitives;
pub mod reader;
pub mod readers;
pub mod record_kind;
pub mod string_table;

pub use context::BuildEventContext;
pub use error::MuninError;
pub use field_flags::BuildEventArgsFieldFlags;
pub use fields::{BuildEventArgsFields, ExtendedDataFields};
pub use header::{open_binlog, BinlogHeader};
pub use index::{BinlogIndex, EventMeta};
pub use nvl_table::{NameValueListTable, NameValuePair};
pub use primitives::BinlogDateTime;
pub use reader::{ArchiveEntry, BinlogEvent, BinlogReader};
pub use readers::{read_dedup_string, read_optional_string, read_string_dictionary};
pub use record_kind::BinaryLogRecordKind;
pub use string_table::StringTable;

/// Alias for `Result<T, MuninError>`.
pub type Result<T> = std::result::Result<T, MuninError>;
