// Copyright (c) Michael Grier

use super::*;
use crate::error::MuninError;

#[test]
fn new_table_is_empty() {
    let table = StringTable::new();
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
}

#[test]
fn null_index_returns_none() {
    let table = StringTable::new();
    assert_eq!(table.get(0).unwrap(), None);
}

#[test]
fn empty_index_returns_empty_string() {
    let table = StringTable::new();
    assert_eq!(table.get(1).unwrap(), Some(""));
}

#[test]
fn reserved_indices_2_through_9_are_invalid() {
    let table = StringTable::new();
    for i in 2..10 {
        match table.get(i) {
            Err(MuninError::InvalidIndex { kind, index }) => {
                assert_eq!(kind, "string");
                assert_eq!(index, i);
            }
            other => panic!("expected InvalidIndex for index {i}, got {other:?}"),
        }
    }
}

#[test]
fn first_string_gets_index_10() {
    let mut table = StringTable::new();
    let idx = table.add("hello".to_string());
    assert_eq!(idx, 10);
    assert_eq!(table.get(10).unwrap(), Some("hello"));
}

#[test]
fn sequential_index_assignment() {
    let mut table = StringTable::new();
    let idx0 = table.add("alpha".to_string());
    let idx1 = table.add("beta".to_string());
    let idx2 = table.add("gamma".to_string());
    assert_eq!(idx0, 10);
    assert_eq!(idx1, 11);
    assert_eq!(idx2, 12);
    assert_eq!(table.get(10).unwrap(), Some("alpha"));
    assert_eq!(table.get(11).unwrap(), Some("beta"));
    assert_eq!(table.get(12).unwrap(), Some("gamma"));
}

#[test]
fn out_of_range_index_is_invalid() {
    let mut table = StringTable::new();
    table.add("only".to_string());
    // index 11 would be the second entry, but we only have one
    match table.get(11) {
        Err(MuninError::InvalidIndex { kind, index }) => {
            assert_eq!(kind, "string");
            assert_eq!(index, 11);
        }
        other => panic!("expected InvalidIndex, got {other:?}"),
    }
}

#[test]
fn negative_index_is_invalid() {
    let table = StringTable::new();
    match table.get(-1) {
        Err(MuninError::InvalidIndex { kind, index }) => {
            assert_eq!(kind, "string");
            assert_eq!(index, -1);
        }
        other => panic!("expected InvalidIndex, got {other:?}"),
    }
}

#[test]
fn len_tracks_additions() {
    let mut table = StringTable::new();
    assert_eq!(table.len(), 0);
    table.add("a".to_string());
    assert_eq!(table.len(), 1);
    table.add("b".to_string());
    assert_eq!(table.len(), 2);
}

#[test]
fn empty_string_can_be_stored() {
    let mut table = StringTable::new();
    let idx = table.add(String::new());
    assert_eq!(table.get(idx).unwrap(), Some(""));
}

#[test]
fn unicode_strings() {
    let mut table = StringTable::new();
    let idx = table.add("日本語テスト".to_string());
    assert_eq!(table.get(idx).unwrap(), Some("日本語テスト"));
}

#[test]
fn many_entries() {
    let mut table = StringTable::new();
    for i in 0..100_i32 {
        let expected_idx = 10 + i;
        let idx = table.add(format!("string_{i}"));
        assert_eq!(idx, expected_idx);
    }
    assert_eq!(table.len(), 100);
    for i in 0..100_i32 {
        let expected = format!("string_{i}");
        assert_eq!(table.get(10 + i).unwrap(), Some(expected.as_str()));
    }
}

#[test]
fn default_is_empty() {
    let table = StringTable::default();
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
}
