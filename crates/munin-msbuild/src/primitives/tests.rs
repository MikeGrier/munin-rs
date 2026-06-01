// Copyright (c) Michael Grier

use std::io::Cursor;

use super::*;

// ── 7-bit variable-length integer ──────────────────────────────────────

#[test]
fn read_7bit_zero() {
    let mut c = Cursor::new([0x00u8]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), 0);
}

#[test]
fn read_7bit_one() {
    let mut c = Cursor::new([0x01u8]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), 1);
}

#[test]
fn read_7bit_max_single_byte() {
    // 127 = 0x7F — fits in one byte.
    let mut c = Cursor::new([0x7Fu8]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), 127);
}

#[test]
fn read_7bit_128() {
    // 128 = 0x80 in two bytes: [0x80, 0x01]
    let mut c = Cursor::new([0x80u8, 0x01]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), 128);
}

#[test]
fn read_7bit_300() {
    // 300 = 0x12C → low 7 bits: 0x2C | 0x80, high 7 bits: 0x02
    let mut c = Cursor::new([0xACu8, 0x02]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), 300);
}

#[test]
fn read_7bit_16384() {
    // 16384 = 0x4000 → three bytes: [0x80, 0x80, 0x01]
    let mut c = Cursor::new([0x80u8, 0x80, 0x01]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), 16384);
}

#[test]
fn read_7bit_max_i32() {
    // i32::MAX = 2147483647 = 0x7FFFFFFF
    // Encoded: [0xFF, 0xFF, 0xFF, 0xFF, 0x07]
    let mut c = Cursor::new([0xFFu8, 0xFF, 0xFF, 0xFF, 0x07]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), i32::MAX);
}

#[test]
fn read_7bit_negative_one() {
    // -1 as u32 = 0xFFFFFFFF
    // Encoded: [0xFF, 0xFF, 0xFF, 0xFF, 0x0F]
    let mut c = Cursor::new([0xFFu8, 0xFF, 0xFF, 0xFF, 0x0F]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), -1);
}

#[test]
fn read_7bit_overlong_error() {
    // 6 continuation bytes — exceeds the 5-byte limit.
    let mut c = Cursor::new([0x80u8, 0x80, 0x80, 0x80, 0x80, 0x01]);
    assert!(matches!(
        read_7bit_int(&mut c),
        Err(MuninError::OverlongVarInt)
    ));
}

#[test]
fn read_7bit_eof_error() {
    // Continuation bit set but no more data.
    let mut c = Cursor::new([0x80u8]);
    assert!(matches!(read_7bit_int(&mut c), Err(MuninError::Io(_))));
}

#[test]
fn read_7bit_two_byte_boundary() {
    // 127 is single-byte, 128 is two-byte.
    let mut c = Cursor::new([0x7Fu8]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), 127);
    let mut c = Cursor::new([0x80u8, 0x01]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), 128);
}

#[test]
fn read_7bit_three_byte_boundary() {
    // 16383 = single + continuation, 16384 = three-byte.
    let mut c = Cursor::new([0xFFu8, 0x7F]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), 16383);
    let mut c = Cursor::new([0x80u8, 0x80, 0x01]);
    assert_eq!(read_7bit_int(&mut c).unwrap(), 16384);
}

// ── .NET string reader ─────────────────────────────────────────────────

#[test]
fn read_empty_string() {
    let mut c = Cursor::new([0x00u8]); // length = 0
    assert_eq!(read_dotnet_string(&mut c).unwrap(), "");
}

#[test]
fn read_ascii_string() {
    // "hello" = 5 bytes
    let mut data = vec![0x05u8];
    data.extend_from_slice(b"hello");
    let mut c = Cursor::new(data);
    assert_eq!(read_dotnet_string(&mut c).unwrap(), "hello");
}

#[test]
fn read_utf8_string() {
    let s = "日本語"; // 9 UTF-8 bytes
    let bytes = s.as_bytes();
    let mut data = vec![bytes.len() as u8];
    data.extend_from_slice(bytes);
    let mut c = Cursor::new(data);
    assert_eq!(read_dotnet_string(&mut c).unwrap(), "日本語");
}

#[test]
fn read_string_with_multibyte_length() {
    // 200-byte string with a 7-bit-encoded length prefix.
    let payload: Vec<u8> = (0..200).map(|i| b'A' + (i % 26)).collect();
    // 200 = 0xC8 → [0xC8, 0x01]
    let mut data = vec![0xC8u8, 0x01];
    data.extend_from_slice(&payload);
    let mut c = Cursor::new(data);
    let result = read_dotnet_string(&mut c).unwrap();
    assert_eq!(result.len(), 200);
}

#[test]
fn read_string_invalid_utf8() {
    let mut data = vec![0x02u8]; // length = 2
    data.extend_from_slice(&[0xFF, 0xFE]); // invalid UTF-8
    let mut c = Cursor::new(data);
    assert!(matches!(
        read_dotnet_string(&mut c),
        Err(MuninError::InvalidUtf8)
    ));
}

#[test]
fn read_string_truncated_data() {
    // Claims 10 bytes but only provides 3.
    let mut data = vec![0x0Au8]; // length = 10
    data.extend_from_slice(b"abc");
    let mut c = Cursor::new(data);
    assert!(matches!(read_dotnet_string(&mut c), Err(MuninError::Io(_))));
}

#[test]
fn read_string_negative_length() {
    // -1 encoded as 7-bit int: [0xFF, 0xFF, 0xFF, 0xFF, 0x0F]
    let mut c = Cursor::new([0xFFu8, 0xFF, 0xFF, 0xFF, 0x0F]);
    assert!(matches!(
        read_dotnet_string(&mut c),
        Err(MuninError::InvalidFormat(_))
    ));
}

// ── 7-bit length helper ────────────────────────────────────────────────

#[test]
fn read_7bit_length_zero() {
    let mut c = Cursor::new([0x00u8]);
    assert_eq!(read_7bit_length(&mut c, "test").unwrap(), 0);
}

#[test]
fn read_7bit_length_positive() {
    let mut c = Cursor::new([0xACu8, 0x02]); // 300
    assert_eq!(read_7bit_length(&mut c, "test").unwrap(), 300);
}

#[test]
fn read_7bit_length_rejects_negative() {
    // -1 encoded as 7-bit int: [0xFF, 0xFF, 0xFF, 0xFF, 0x0F]
    let mut c = Cursor::new([0xFFu8, 0xFF, 0xFF, 0xFF, 0x0F]);
    let err = read_7bit_length(&mut c, "widget count").unwrap_err();
    match err {
        MuninError::InvalidFormat(msg) => {
            assert!(msg.contains("widget count"), "message was: {msg}");
            assert!(msg.contains("-1"), "message was: {msg}");
        }
        other => panic!("expected InvalidFormat, got {other:?}"),
    }
}

#[test]
fn read_7bit_length_rejects_i32_min() {
    // i32::MIN = 0x80000000 → encoded [0x80, 0x80, 0x80, 0x80, 0x08]
    let mut c = Cursor::new([0x80u8, 0x80, 0x80, 0x80, 0x08]);
    assert!(matches!(
        read_7bit_length(&mut c, "test"),
        Err(MuninError::InvalidFormat(_))
    ));
}

#[test]
fn read_7bit_length_rejects_above_max() {
    // MAX_BINLOG_FIELD_LEN + 1 = 0x1000_0001 = 268,435,457
    // 7-bit encoding of 0x1000_0001:
    //   byte 0: 0x01 | 0x80 = 0x81  (data=0x01, more follows)
    //   byte 1: 0x00 | 0x80 = 0x80  (data=0x00, more follows)
    //   byte 2: 0x00 | 0x80 = 0x80  (data=0x00, more follows)
    //   byte 3: 0x00 | 0x80 = 0x80  (data=0x00, more follows)
    //   byte 4: 0x01               (data=0x01, stop)
    let mut c = Cursor::new([0x81u8, 0x80, 0x80, 0x80, 0x01]);
    let err = read_7bit_length(&mut c, "field").unwrap_err();
    match err {
        MuninError::InvalidFormat(msg) => {
            assert!(msg.contains("too large"), "message was: {msg}");
        }
        other => panic!("expected InvalidFormat, got {other:?}"),
    }
}

#[test]
fn read_7bit_length_accepts_at_max() {
    // MAX_BINLOG_FIELD_LEN = 0x1000_0000 should be accepted.
    // 7-bit encoding of 0x1000_0000:
    //   byte 0: 0x00 | 0x80 = 0x80
    //   byte 1: 0x00 | 0x80 = 0x80
    //   byte 2: 0x00 | 0x80 = 0x80
    //   byte 3: 0x00 | 0x80 = 0x80
    //   byte 4: 0x01
    let mut c = Cursor::new([0x80u8, 0x80, 0x80, 0x80, 0x01]);
    assert_eq!(
        read_7bit_length(&mut c, "test").unwrap(),
        MAX_BINLOG_FIELD_LEN
    );
}

// ── 7-bit count helper ─────────────────────────────────────────────────

#[test]
fn read_7bit_count_zero() {
    let mut c = Cursor::new([0x00u8]);
    assert_eq!(read_7bit_count(&mut c, "test").unwrap(), 0);
}

#[test]
fn read_7bit_count_positive() {
    let mut c = Cursor::new([0xACu8, 0x02]); // 300
    assert_eq!(read_7bit_count(&mut c, "test").unwrap(), 300);
}

#[test]
fn read_7bit_count_rejects_negative() {
    // -1 encoded as 7-bit int: [0xFF, 0xFF, 0xFF, 0xFF, 0x0F]
    let mut c = Cursor::new([0xFFu8, 0xFF, 0xFF, 0xFF, 0x0F]);
    let err = read_7bit_count(&mut c, "widget count").unwrap_err();
    match err {
        MuninError::InvalidFormat(msg) => {
            assert!(msg.contains("widget count"), "message was: {msg}");
            assert!(msg.contains("-1"), "message was: {msg}");
        }
        other => panic!("expected InvalidFormat, got {other:?}"),
    }
}

#[test]
fn read_7bit_count_rejects_above_max() {
    // MAX_BINLOG_ELEMENT_COUNT + 1 = 0x10_0001 = 1,048,577
    // 7-bit encoding of 0x100001 (3 bytes, LSB-first 7-bit groups):
    //   bits 0-6:   0x01 | 0x80 = 0x81  (more follows)
    //   bits 7-13:  0x00 | 0x80 = 0x80  (more follows)
    //   bits 14-20: 0x40               (stop; 0x40 << 14 = 0x10_0000, total = 0x10_0001)
    let mut c = Cursor::new([0x81u8, 0x80, 0x40]);
    let err = read_7bit_count(&mut c, "item count").unwrap_err();
    match err {
        MuninError::InvalidFormat(msg) => {
            assert!(msg.contains("too large"), "message was: {msg}");
        }
        other => panic!("expected InvalidFormat, got {other:?}"),
    }
}

#[test]
fn read_7bit_count_accepts_at_max() {
    // MAX_BINLOG_ELEMENT_COUNT = 0x10_0000 = 1,048,576
    // 7-bit encoding of 0x100000 (3 bytes, LSB-first 7-bit groups):
    //   bits 0-6:   0x00 | 0x80 = 0x80  (more follows)
    //   bits 7-13:  0x00 | 0x80 = 0x80  (more follows)
    //   bits 14-20: 0x40               (stop; 0x40 << 14 = 0x10_0000)
    let mut c = Cursor::new([0x80u8, 0x80, 0x40]);
    assert_eq!(
        read_7bit_count(&mut c, "test").unwrap(),
        MAX_BINLOG_ELEMENT_COUNT
    );
}

// ── Boolean ────────────────────────────────────────────────────────────

#[test]
fn read_bool_false() {
    let mut c = Cursor::new([0x00u8]);
    assert!(!read_bool(&mut c).unwrap());
}

#[test]
fn read_bool_true_one() {
    let mut c = Cursor::new([0x01u8]);
    assert!(read_bool(&mut c).unwrap());
}

#[test]
fn read_bool_true_nonzero() {
    let mut c = Cursor::new([0xFFu8]);
    assert!(read_bool(&mut c).unwrap());
}

// ── i32 little-endian ──────────────────────────────────────────────────

#[test]
fn read_i32_le_zero() {
    let mut c = Cursor::new([0x00u8, 0x00, 0x00, 0x00]);
    assert_eq!(read_i32_le(&mut c).unwrap(), 0);
}

#[test]
fn read_i32_le_positive() {
    let mut c = Cursor::new(42i32.to_le_bytes());
    assert_eq!(read_i32_le(&mut c).unwrap(), 42);
}

#[test]
fn read_i32_le_negative() {
    let mut c = Cursor::new((-1i32).to_le_bytes());
    assert_eq!(read_i32_le(&mut c).unwrap(), -1);
}

#[test]
fn read_i32_le_max() {
    let mut c = Cursor::new(i32::MAX.to_le_bytes());
    assert_eq!(read_i32_le(&mut c).unwrap(), i32::MAX);
}

// ── i64 little-endian ──────────────────────────────────────────────────

#[test]
fn read_i64_le_zero() {
    let mut c = Cursor::new(0i64.to_le_bytes());
    assert_eq!(read_i64_le(&mut c).unwrap(), 0);
}

#[test]
fn read_i64_le_large() {
    let val: i64 = 637_800_000_000_000_000; // realistic ticks value
    let mut c = Cursor::new(val.to_le_bytes());
    assert_eq!(read_i64_le(&mut c).unwrap(), val);
}

// ── GUID ───────────────────────────────────────────────────────────────

#[test]
fn read_guid_all_zeros() {
    let mut c = Cursor::new([0u8; 16]);
    assert_eq!(read_guid(&mut c).unwrap(), [0u8; 16]);
}

#[test]
fn read_guid_specific_bytes() {
    let expected: [u8; 16] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10,
    ];
    let mut c = Cursor::new(expected);
    assert_eq!(read_guid(&mut c).unwrap(), expected);
}

// ── DateTime ───────────────────────────────────────────────────────────

#[test]
fn read_datetime_utc() {
    let ticks: i64 = 637_800_000_000_000_000;
    let kind: i32 = 1; // Utc
    let mut data = Vec::new();
    data.extend_from_slice(&ticks.to_le_bytes());
    data.push(kind as u8); // kind fits in single 7-bit byte
    let mut c = Cursor::new(data);
    let dt = read_datetime(&mut c).unwrap();
    assert_eq!(dt.ticks, ticks);
    assert_eq!(dt.kind, kind);
}

#[test]
fn read_datetime_local() {
    let ticks: i64 = 0;
    let kind: i32 = 2; // Local
    let mut data = Vec::new();
    data.extend_from_slice(&ticks.to_le_bytes());
    data.push(kind as u8);
    let mut c = Cursor::new(data);
    let dt = read_datetime(&mut c).unwrap();
    assert_eq!(dt.ticks, 0);
    assert_eq!(dt.kind, 2);
}

// ── TimeSpan ───────────────────────────────────────────────────────────

#[test]
fn read_timespan_zero() {
    let mut c = Cursor::new(0i64.to_le_bytes());
    assert_eq!(read_timespan_ticks(&mut c).unwrap(), 0);
}

#[test]
fn read_timespan_one_second() {
    let one_second_ticks: i64 = 10_000_000;
    let mut c = Cursor::new(one_second_ticks.to_le_bytes());
    assert_eq!(read_timespan_ticks(&mut c).unwrap(), one_second_ticks);
}

// ── skip_bytes ─────────────────────────────────────────────────────────

#[test]
fn skip_bytes_zero() {
    let mut c = Cursor::new(Vec::<u8>::new());
    skip_bytes(&mut c, 0).unwrap();
}

#[test]
fn skip_bytes_some() {
    let data = vec![0u8; 100];
    let mut c = Cursor::new(data);
    skip_bytes(&mut c, 50).unwrap();
    // Verify cursor advanced.
    assert_eq!(c.position(), 50);
}

#[test]
fn skip_bytes_exact_length() {
    let data = vec![0u8; 4096];
    let mut c = Cursor::new(data);
    skip_bytes(&mut c, 4096).unwrap();
    assert_eq!(c.position(), 4096);
}

#[test]
fn skip_bytes_past_end() {
    let data = vec![0u8; 10];
    let mut c = Cursor::new(data);
    assert!(skip_bytes(&mut c, 20).is_err());
}
