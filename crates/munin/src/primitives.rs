// Copyright (c) Michael Grier

//! Low-level binary primitives: 7-bit variable-length integers, .NET-style
//! length-prefixed strings, and other primitive type readers matching the
//! binlog wire format.

use std::io::Read;

use crate::error::MuninError;

/// Maximum number of bytes a 7-bit encoded `i32` can occupy.
/// .NET `BinaryReader.Read7BitEncodedInt` allows up to 5 bytes for 32-bit values.
const MAX_VARINT_BYTES: usize = 5;

/// Read a 7-bit variable-length encoded `i32` from the stream.
///
/// This mirrors .NET's `BinaryReader.Read7BitEncodedInt`: the low 7 bits of
/// each byte carry data and the high bit signals whether more bytes follow.
pub fn read_7bit_int(reader: &mut impl Read) -> Result<i32, MuninError> {
    let mut result: u32 = 0;
    let mut shift: u32 = 0;
    let mut buf = [0u8; 1];

    for _ in 0..MAX_VARINT_BYTES {
        reader.read_exact(&mut buf)?;
        let byte = buf[0];
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return Ok(result as i32);
        }
        shift += 7;
    }

    Err(MuninError::OverlongVarInt)
}

/// Maximum byte-buffer length accepted by [`read_7bit_length`].
///
/// 256 MiB is well above the largest record found in real MSBuild binlogs
/// and well below `i32::MAX` (~2 GiB). Values larger than this from an
/// untrusted binlog are treated as malformed input rather than being passed
/// to `vec![0u8; n]`, which would attempt a huge allocation and abort or
/// exhaust memory. For collection element counts use [`MAX_BINLOG_ELEMENT_COUNT`]
/// and [`read_7bit_count`] instead.
pub const MAX_BINLOG_FIELD_LEN: usize = 256 * 1024 * 1024; // 256 MiB

/// Maximum element count accepted by [`read_7bit_count`].
///
/// 1,048,576 (2²⁰) elements is far above any count found in real MSBuild
/// binlogs. Unlike byte buffers, each element in a `Vec<String>`,
/// `Vec<TaskItem>`, or `Vec<NameValuePair>` multiplies the allocation by the
/// element size, which varies by type and can be much larger than 24 bytes.
/// Using the same 256 MiB ceiling as byte lengths would permit multi-GiB
/// allocations from a count field alone; this lower limit bounds that risk.
pub const MAX_BINLOG_ELEMENT_COUNT: usize = 1 << 20; // 1,048,576

/// Read a 7-bit variable-length encoded byte-buffer length and validate that
/// it is non-negative and within [`MAX_BINLOG_FIELD_LEN`] before casting
/// to `usize`.
///
/// On the wire, lengths are encoded as signed `i32` (matching .NET's
/// `BinaryWriter.Write7BitEncodedInt`). A malformed binlog could supply a
/// negative value which, if cast directly with `as usize`, would sign-extend
/// to a huge `usize` and cause `vec![0u8; n]` to abort or exhaust memory.
/// Even a large *positive* value can trigger the same abort. Use this helper
/// only for byte-buffer lengths (e.g. `read_dotnet_string`, record payloads).
/// For collection element counts use [`read_7bit_count`] instead.
///
/// `what` names the field being read; it is included in the error message
/// to aid debugging malformed inputs.
pub fn read_7bit_length(reader: &mut impl Read, what: &'static str) -> Result<usize, MuninError> {
    let n = read_7bit_int(reader)?;
    if n < 0 {
        return Err(MuninError::InvalidFormat(format!("negative {what}: {n}")));
    }
    let n = n as usize;
    if n > MAX_BINLOG_FIELD_LEN {
        return Err(MuninError::InvalidFormat(format!(
            "{what} too large: {n} (max {MAX_BINLOG_FIELD_LEN})"
        )));
    }
    Ok(n)
}

/// Read a 7-bit variable-length encoded element count and validate that it
/// is non-negative and within [`MAX_BINLOG_ELEMENT_COUNT`] before casting
/// to `usize`.
///
/// Use this instead of [`read_7bit_length`] at every call site that turns a
/// parsed varint into a collection capacity (`Vec::with_capacity`). Each
/// element type (`String`, `TaskItem`, `NameValuePair`, `ProfileEntry`, …)
/// varies in size; using the 256 MiB byte-length ceiling for element counts
/// would allow multi-GiB allocations from a single count field.
///
/// `what` names the field being read; it is included in the error message
/// to aid debugging malformed inputs.
pub fn read_7bit_count(reader: &mut impl Read, what: &'static str) -> Result<usize, MuninError> {
    let n = read_7bit_int(reader)?;
    if n < 0 {
        return Err(MuninError::InvalidFormat(format!("negative {what}: {n}")));
    }
    let n = n as usize;
    if n > MAX_BINLOG_ELEMENT_COUNT {
        return Err(MuninError::InvalidFormat(format!(
            "{what} too large: {n} (max {MAX_BINLOG_ELEMENT_COUNT})"
        )));
    }
    Ok(n)
}

/// Read a .NET `BinaryWriter`-style length-prefixed UTF-8 string.
///
/// The length (in bytes) is encoded as a 7-bit variable-length integer,
/// followed by that many bytes of UTF-8 data.
pub fn read_dotnet_string(reader: &mut impl Read) -> Result<String, MuninError> {
    let len = read_7bit_length(reader, "string length")?;
    if len == 0 {
        return Ok(String::new());
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|_| MuninError::InvalidUtf8)
}

/// Read a `bool` (1 byte: 0 = false, non-zero = true).
pub fn read_bool(reader: &mut impl Read) -> Result<bool, MuninError> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    Ok(buf[0] != 0)
}

/// Read an `i32` as 4 bytes little-endian (NOT 7-bit encoded).
pub fn read_i32_le(reader: &mut impl Read) -> Result<i32, MuninError> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

/// Read an `i64` as 8 bytes little-endian.
pub fn read_i64_le(reader: &mut impl Read) -> Result<i64, MuninError> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(i64::from_le_bytes(buf))
}

/// Read a GUID as 16 raw bytes.
pub fn read_guid(reader: &mut impl Read) -> Result<[u8; 16], MuninError> {
    let mut buf = [0u8; 16];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

/// A .NET `DateTime` decoded from the binlog stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinlogDateTime {
    /// Ticks since 0001-01-01T00:00:00.
    pub ticks: i64,
    /// The `DateTimeKind` value (0 = Unspecified, 1 = Utc, 2 = Local).
    pub kind: i32,
}

/// Read a .NET `DateTime` (i64 ticks + i32 DateTimeKind).
pub fn read_datetime(reader: &mut impl Read) -> Result<BinlogDateTime, MuninError> {
    let ticks = read_i64_le(reader)?;
    let kind = read_7bit_int(reader)?;
    Ok(BinlogDateTime { ticks, kind })
}

/// Read a .NET `TimeSpan` (i64 ticks).
pub fn read_timespan_ticks(reader: &mut impl Read) -> Result<i64, MuninError> {
    read_i64_le(reader)
}

/// Skip `n` bytes in the stream.
pub fn skip_bytes(reader: &mut impl Read, n: usize) -> Result<(), MuninError> {
    // For non-seekable streams, read and discard.
    let mut remaining = n;
    let mut buf = [0u8; 4096];
    while remaining > 0 {
        let to_read = remaining.min(buf.len());
        reader.read_exact(&mut buf[..to_read])?;
        remaining -= to_read;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
