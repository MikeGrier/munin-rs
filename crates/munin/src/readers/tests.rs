// Copyright (c) Michael Grier

use std::io::Cursor;

use super::*;
use crate::{
    nvl_table::{NameValueListTable, NameValuePair},
    string_table::StringTable,
};

/// Encode a single i32 as a 7-bit variable-length integer.
fn encode_7bit(value: i32) -> Vec<u8> {
    let mut v = value as u32;
    let mut buf = Vec::new();
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if v == 0 {
            break;
        }
    }
    buf
}

/// Encode a .NET-style length-prefixed string.
fn encode_dotnet_string(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut buf = encode_7bit(bytes.len() as i32);
    buf.extend_from_slice(bytes);
    buf
}

// --- read_dedup_string ---

#[test]
fn dedup_string_null_index() {
    let strings = StringTable::new();
    let data = encode_7bit(0);
    let mut cursor = Cursor::new(data);
    assert_eq!(read_dedup_string(&mut cursor, &strings).unwrap(), None);
}

#[test]
fn dedup_string_empty_index() {
    let strings = StringTable::new();
    let data = encode_7bit(1);
    let mut cursor = Cursor::new(data);
    assert_eq!(
        read_dedup_string(&mut cursor, &strings).unwrap(),
        Some(String::new())
    );
}

#[test]
fn dedup_string_valid_index() {
    let mut strings = StringTable::new();
    let idx = strings.add("hello".to_string());
    let data = encode_7bit(idx);
    let mut cursor = Cursor::new(data);
    assert_eq!(
        read_dedup_string(&mut cursor, &strings).unwrap(),
        Some("hello".to_string())
    );
}

#[test]
fn dedup_string_invalid_index() {
    let strings = StringTable::new();
    let data = encode_7bit(10); // no entries added
    let mut cursor = Cursor::new(data);
    assert!(read_dedup_string(&mut cursor, &strings).is_err());
}

// --- read_optional_string ---

#[test]
fn optional_string_v10_dedup() {
    let mut strings = StringTable::new();
    let idx = strings.add("test".to_string());
    let data = encode_7bit(idx);
    let mut cursor = Cursor::new(data);
    assert_eq!(
        read_optional_string(&mut cursor, &strings, 18).unwrap(),
        Some("test".to_string())
    );
}

#[test]
fn optional_string_v10_null() {
    let strings = StringTable::new();
    let data = encode_7bit(0);
    let mut cursor = Cursor::new(data);
    assert_eq!(
        read_optional_string(&mut cursor, &strings, 18).unwrap(),
        None
    );
}

#[test]
fn optional_string_pre_v10_present() {
    let strings = StringTable::new();
    let mut data = vec![1u8]; // bool true
    data.extend_from_slice(&encode_dotnet_string("legacy"));
    let mut cursor = Cursor::new(data);
    assert_eq!(
        read_optional_string(&mut cursor, &strings, 9).unwrap(),
        Some("legacy".to_string())
    );
}

#[test]
fn optional_string_pre_v10_absent() {
    let strings = StringTable::new();
    let data = vec![0u8]; // bool false
    let mut cursor = Cursor::new(data);
    assert_eq!(
        read_optional_string(&mut cursor, &strings, 9).unwrap(),
        None
    );
}

// --- read_string_dictionary ---

#[test]
fn dictionary_v10_null_index() {
    let strings = StringTable::new();
    let nvl = NameValueListTable::new();
    let data = encode_7bit(0);
    let mut cursor = Cursor::new(data);
    assert_eq!(
        read_string_dictionary(&mut cursor, &strings, &nvl, 18).unwrap(),
        None
    );
}

#[test]
fn dictionary_v10_resolved() {
    let mut strings = StringTable::new();
    let k_idx = strings.add("Configuration".to_string()); // 10
    let v_idx = strings.add("Release".to_string()); // 11

    let mut nvl = NameValueListTable::new();
    let nvl_idx = nvl.add(vec![NameValuePair {
        key_index: k_idx,
        value_index: v_idx,
    }]);

    let data = encode_7bit(nvl_idx);
    let mut cursor = Cursor::new(data);
    let result = read_string_dictionary(&mut cursor, &strings, &nvl, 18)
        .unwrap()
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0],
        ("Configuration".to_string(), "Release".to_string())
    );
}

#[test]
fn dictionary_pre_v10_none() {
    let strings = StringTable::new();
    let nvl = NameValueListTable::new();
    let data = encode_7bit(0); // count=0
    let mut cursor = Cursor::new(data);
    assert_eq!(
        read_string_dictionary(&mut cursor, &strings, &nvl, 9).unwrap(),
        None
    );
}

#[test]
fn dictionary_pre_v10_entries() {
    let strings = StringTable::new();
    let nvl = NameValueListTable::new();
    let mut data = encode_7bit(2); // count=2
    data.extend_from_slice(&encode_dotnet_string("k1"));
    data.extend_from_slice(&encode_dotnet_string("v1"));
    data.extend_from_slice(&encode_dotnet_string("k2"));
    data.extend_from_slice(&encode_dotnet_string("v2"));
    let mut cursor = Cursor::new(data);
    let result = read_string_dictionary(&mut cursor, &strings, &nvl, 9)
        .unwrap()
        .unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], ("k1".to_string(), "v1".to_string()));
    assert_eq!(result[1], ("k2".to_string(), "v2".to_string()));
}

#[test]
fn dictionary_v10_multiple_pairs() {
    let mut strings = StringTable::new();
    let k1 = strings.add("Platform".to_string()); // 10
    let v1 = strings.add("x64".to_string()); // 11
    let k2 = strings.add("Config".to_string()); // 12
    let v2 = strings.add("Debug".to_string()); // 13

    let mut nvl = NameValueListTable::new();
    let nvl_idx = nvl.add(vec![
        NameValuePair {
            key_index: k1,
            value_index: v1,
        },
        NameValuePair {
            key_index: k2,
            value_index: v2,
        },
    ]);

    let data = encode_7bit(nvl_idx);
    let mut cursor = Cursor::new(data);
    let result = read_string_dictionary(&mut cursor, &strings, &nvl, 18)
        .unwrap()
        .unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], ("Platform".to_string(), "x64".to_string()));
    assert_eq!(result[1], ("Config".to_string(), "Debug".to_string()));
}

#[test]
fn dictionary_v10_invalid_nvl_index() {
    let strings = StringTable::new();
    let nvl = NameValueListTable::new();
    let data = encode_7bit(10); // no entries added
    let mut cursor = Cursor::new(data);
    assert!(read_string_dictionary(&mut cursor, &strings, &nvl, 18).is_err());
}

#[test]
fn optional_string_v10_empty_string_index() {
    let strings = StringTable::new();
    let data = encode_7bit(1); // empty string
    let mut cursor = Cursor::new(data);
    assert_eq!(
        read_optional_string(&mut cursor, &strings, 18).unwrap(),
        Some(String::new())
    );
}

// --- read_legacy_string_dictionary: malformed-input rejection ---

#[test]
fn legacy_string_dictionary_rejects_negative_count() {
    // -1 encoded as 7-bit int — would sign-extend to a huge usize via `as`.
    let data: [u8; 5] = [0xFF, 0xFF, 0xFF, 0xFF, 0x0F];
    let mut cursor = Cursor::new(data);
    let err = read_legacy_string_dictionary(&mut cursor).unwrap_err();
    assert!(matches!(err, crate::error::MuninError::InvalidFormat(_)));
}
