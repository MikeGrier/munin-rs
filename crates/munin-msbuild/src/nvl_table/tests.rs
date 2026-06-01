// Copyright (c) Michael Grier

use super::*;
use crate::{error::MuninError, string_table::StringTable};

#[test]
fn new_table_is_empty() {
    let table = NameValueListTable::new();
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
}

#[test]
fn null_index_returns_none() {
    let table = NameValueListTable::new();
    assert_eq!(table.get(0).unwrap(), None);
}

#[test]
fn reserved_indices_1_through_9_are_invalid() {
    let table = NameValueListTable::new();
    for i in 1..10 {
        match table.get(i) {
            Err(MuninError::InvalidIndex { kind, index }) => {
                assert_eq!(kind, "name-value list");
                assert_eq!(index, i);
            }
            other => panic!("expected InvalidIndex for index {i}, got {other:?}"),
        }
    }
}

#[test]
fn first_list_gets_index_10() {
    let mut table = NameValueListTable::new();
    let pairs = vec![NameValuePair {
        key_index: 10,
        value_index: 11,
    }];
    let idx = table.add(pairs);
    assert_eq!(idx, 10);
    let result = table.get(10).unwrap().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key_index, 10);
    assert_eq!(result[0].value_index, 11);
}

#[test]
fn sequential_index_assignment() {
    let mut table = NameValueListTable::new();
    let idx0 = table.add(vec![NameValuePair {
        key_index: 10,
        value_index: 11,
    }]);
    let idx1 = table.add(vec![
        NameValuePair {
            key_index: 12,
            value_index: 13,
        },
        NameValuePair {
            key_index: 14,
            value_index: 15,
        },
    ]);
    assert_eq!(idx0, 10);
    assert_eq!(idx1, 11);
    assert_eq!(table.get(10).unwrap().unwrap().len(), 1);
    assert_eq!(table.get(11).unwrap().unwrap().len(), 2);
}

#[test]
fn empty_list_is_valid() {
    let mut table = NameValueListTable::new();
    let idx = table.add(vec![]);
    let result = table.get(idx).unwrap().unwrap();
    assert!(result.is_empty());
}

#[test]
fn out_of_range_index_is_invalid() {
    let mut table = NameValueListTable::new();
    table.add(vec![]);
    match table.get(11) {
        Err(MuninError::InvalidIndex { kind, index }) => {
            assert_eq!(kind, "name-value list");
            assert_eq!(index, 11);
        }
        other => panic!("expected InvalidIndex, got {other:?}"),
    }
}

#[test]
fn negative_index_is_invalid() {
    let table = NameValueListTable::new();
    match table.get(-1) {
        Err(MuninError::InvalidIndex { kind, index }) => {
            assert_eq!(kind, "name-value list");
            assert_eq!(index, -1);
        }
        other => panic!("expected InvalidIndex, got {other:?}"),
    }
}

#[test]
fn resolve_returns_none_for_null_index() {
    let table = NameValueListTable::new();
    let strings = StringTable::new();
    assert_eq!(table.resolve(0, &strings).unwrap(), None);
}

#[test]
fn resolve_produces_key_value_strings() {
    let mut strings = StringTable::new();
    let k_idx = strings.add("key1".to_string()); // 10
    let v_idx = strings.add("val1".to_string()); // 11
    let k2_idx = strings.add("key2".to_string()); // 12
    let v2_idx = strings.add("val2".to_string()); // 13

    let mut table = NameValueListTable::new();
    table.add(vec![
        NameValuePair {
            key_index: k_idx,
            value_index: v_idx,
        },
        NameValuePair {
            key_index: k2_idx,
            value_index: v2_idx,
        },
    ]);

    let resolved = table.resolve(10, &strings).unwrap().unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0], ("key1".to_string(), "val1".to_string()));
    assert_eq!(resolved[1], ("key2".to_string(), "val2".to_string()));
}

#[test]
fn resolve_null_key_is_skipped() {
    let mut strings = StringTable::new();
    let v_idx = strings.add("val".to_string()); // 10

    let mut table = NameValueListTable::new();
    // key_index=0 means null key → skipped
    table.add(vec![NameValuePair {
        key_index: 0,
        value_index: v_idx,
    }]);

    let resolved = table.resolve(10, &strings).unwrap().unwrap();
    assert!(resolved.is_empty());
}

#[test]
fn resolve_null_value_becomes_empty_string() {
    let mut strings = StringTable::new();
    let k_idx = strings.add("key".to_string()); // 10

    let mut table = NameValueListTable::new();
    // value_index=0 means null → replaced with ""
    table.add(vec![NameValuePair {
        key_index: k_idx,
        value_index: 0,
    }]);

    let resolved = table.resolve(10, &strings).unwrap().unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0], ("key".to_string(), String::new()));
}

#[test]
fn resolve_empty_value_stays_empty() {
    let mut strings = StringTable::new();
    let k_idx = strings.add("key".to_string()); // 10

    let mut table = NameValueListTable::new();
    // value_index=1 means empty string
    table.add(vec![NameValuePair {
        key_index: k_idx,
        value_index: 1,
    }]);

    let resolved = table.resolve(10, &strings).unwrap().unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0], ("key".to_string(), String::new()));
}

#[test]
fn len_tracks_additions() {
    let mut table = NameValueListTable::new();
    assert_eq!(table.len(), 0);
    table.add(vec![]);
    assert_eq!(table.len(), 1);
    table.add(vec![]);
    assert_eq!(table.len(), 2);
}

#[test]
fn default_is_empty() {
    let table = NameValueListTable::default();
    assert!(table.is_empty());
}
