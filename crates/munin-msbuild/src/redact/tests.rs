// Copyright (c) Michael Grier

use crate::{index::BinlogIndex, redact::Redactor, string_table::StringTable};

/// Build a minimal in-memory `BinlogIndex` whose string table contains
/// the given strings. Other fields are left at their defaults.
fn index_with_strings(strings: &[&str]) -> BinlogIndex {
    // BinlogIndex is constructed by `open` / `from_jsonlog`; for unit
    // testing we round-trip a synthesized JsonlogFile.
    use crate::jsonlog::schema::{JsonlogFile, JsonlogHeader};

    let file = JsonlogFile {
        munin_jsonlog_version: 1,
        header: JsonlogHeader {
            file_format_version: 18,
            min_reader_version: 18,
        },
        strings: strings.iter().map(|s| s.to_string()).collect(),
        name_value_lists: Vec::new(),
        archives: Vec::new(),
        events: Vec::new(),
    };
    BinlogIndex::from_jsonlog(file).expect("from_jsonlog")
}

fn strings_of(index: &BinlogIndex) -> Vec<String> {
    index.strings().entries().to_vec()
}

#[test]
fn empty_redactor_is_a_noop() {
    let mut idx = index_with_strings(&["hello", "world"]);
    Redactor::new().apply(&mut idx);
    assert_eq!(strings_of(&idx), vec!["hello", "world"]);
}

#[test]
fn with_token_replaces_substring_occurrences() {
    let mut idx = index_with_strings(&["C:\\Users\\alice\\src\\foo.cs", "no match here", "alice"]);
    Redactor::new().with_token("alice").apply(&mut idx);
    let s = strings_of(&idx);
    assert_eq!(s[0], "C:\\Users\\*****\\src\\foo.cs");
    assert_eq!(s[1], "no match here");
    assert_eq!(s[2], "*****");
}

#[test]
fn with_token_longest_first_avoids_short_token_preempting_long() {
    // If "a" runs first it would consume the "a" inside "alice"; the
    // longest-first ordering must apply "alice" first.
    let mut idx = index_with_strings(&["alice and bob"]);
    Redactor::new()
        .with_token("a")
        .with_token("alice")
        .apply(&mut idx);
    assert_eq!(strings_of(&idx)[0], "***** *****nd bob");
}

#[test]
fn with_regex_uses_capture_references() {
    let mut idx = index_with_strings(&["build id 12345 done", "no digits"]);
    let r = Redactor::new()
        .with_regex(r"\d+", "<NUM>")
        .expect("compile");
    r.apply(&mut idx);
    let s = strings_of(&idx);
    assert_eq!(s[0], "build id <NUM> done");
    assert_eq!(s[1], "no digits");
}

#[test]
fn with_regex_invalid_pattern_returns_error() {
    let err = Redactor::new().with_regex("(unbalanced", "x");
    assert!(err.is_err());
}

#[test]
fn entries_mut_keeps_indices_stable() {
    let mut table = StringTable::new();
    let idx_a = table.add("aaaa".to_string());
    let idx_b = table.add("bbbb".to_string());
    for entry in table.entries_mut() {
        *entry = entry.replace('a', "X").replace('b', "Y");
    }
    assert_eq!(table.get(idx_a).unwrap(), Some("XXXX"));
    assert_eq!(table.get(idx_b).unwrap(), Some("YYYY"));
}

#[test]
fn redacted_index_still_round_trips_through_jsonlog() {
    use std::io::Cursor;

    let mut idx = index_with_strings(&["alice was here", "noop"]);
    Redactor::new().with_token("alice").apply(&mut idx);

    let mut json_bytes = Vec::new();
    crate::jsonlog::dump_index(&idx, &mut json_bytes).expect("dump");
    let reopened = BinlogIndex::open_json(Cursor::new(&json_bytes)).expect("open_json");
    assert_eq!(reopened.strings().entries()[0], "***** was here");
    assert_eq!(reopened.strings().entries()[1], "noop");
}

#[test]
fn common_patterns_all_compile() {
    // Forces evaluation of every entry in the catalog.
    let mut idx = index_with_strings(&[""]);
    Redactor::new().with_common_patterns().apply(&mut idx);
}

#[test]
fn common_pattern_url_with_credentials() {
    let mut idx = index_with_strings(&[
        "git clone https://alice:hunter2@github.com/x/y.git",
        "ftp://u:p@host/path",
        "no creds: https://github.com/x/y",
    ]);
    Redactor::new().with_common_patterns().apply(&mut idx);
    let s = strings_of(&idx);
    assert_eq!(s[0], "git clone https://*****:*****@github.com/x/y.git");
    assert_eq!(s[1], "ftp://*****:*****@host/path");
    assert_eq!(s[2], "no creds: https://github.com/x/y");
}

#[test]
fn common_pattern_github_pat() {
    let mut idx = index_with_strings(&[
        "token=ghp_abcdefghijklmnopqrstuvwxyz0123456789AB",
        "old style ghs_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ab tail",
        "ghr_short",
    ]);
    Redactor::new().with_common_patterns().apply(&mut idx);
    let s = strings_of(&idx);
    assert_eq!(s[0], "token=gh*_*****");
    assert_eq!(s[1], "old style gh*_***** tail");
    assert_eq!(s[2], "ghr_short");
}

#[test]
fn common_pattern_azure_devops_pat() {
    // 52 lowercase base32 chars
    let pat = "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrst";
    assert_eq!(pat.len(), 52);
    let mut idx = index_with_strings(&[&format!("ado token: {pat}"), "short abc"]);
    Redactor::new().with_common_patterns().apply(&mut idx);
    let s = strings_of(&idx);
    assert_eq!(s[0], "ado token: *****");
    assert_eq!(s[1], "short abc");
}

#[test]
fn common_pattern_bearer_token() {
    let mut idx = index_with_strings(&[
        "Authorization: Bearer eyJhbGciOi.JIUzI1.token-stuff",
        "bearer abc.def-ghi_jkl",
        "not a bearer",
    ]);
    Redactor::new().with_common_patterns().apply(&mut idx);
    let s = strings_of(&idx);
    assert_eq!(s[0], "Authorization: Bearer *****");
    assert_eq!(s[1], "bearer *****");
    assert_eq!(s[2], "not a bearer");
}

#[test]
fn common_pattern_email() {
    let mut idx = index_with_strings(&["contact alice@example.com please", "no email here"]);
    Redactor::new().with_common_patterns().apply(&mut idx);
    let s = strings_of(&idx);
    assert_eq!(s[0], "contact *****@***** please");
    assert_eq!(s[1], "no email here");
}

#[test]
fn common_patterns_run_after_exact_tokens() {
    // Exact-token rules run first; a token consumed by an exact match is
    // not available to the regex stage.
    let mut idx = index_with_strings(&["mail alice@example.com here"]);
    Redactor::new()
        .with_token("alice@example.com")
        .with_common_patterns()
        .apply(&mut idx);
    assert_eq!(strings_of(&idx)[0], "mail ***** here");
}

/// Build an index with a single `BuildStarted` event whose `environment`
/// dictionary has the given key/value pairs.
fn index_with_build_started_env(env: &[(&str, &str)]) -> BinlogIndex {
    use crate::events::BuildStartedEvent;
    use crate::jsonlog::schema::{JsonlogEvent, JsonlogEventBody, JsonlogFile, JsonlogHeader};

    let ev = BuildStartedEvent {
        fields: Default::default(),
        environment: Some(
            env.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ),
    };
    let value = serde_json::to_value(&ev).expect("serialize BuildStartedEvent");

    let file = JsonlogFile {
        munin_jsonlog_version: 1,
        header: JsonlogHeader {
            file_format_version: 18,
            min_reader_version: 18,
        },
        strings: Vec::new(),
        name_value_lists: Vec::new(),
        archives: Vec::new(),
        events: vec![JsonlogEvent {
            kind: "BuildStarted".to_string(),
            byte_offset: 0,
            body: JsonlogEventBody::Decoded(value),
        }],
    };
    BinlogIndex::from_jsonlog(file).expect("from_jsonlog")
}

#[test]
fn autodetect_username_is_noop_when_no_build_started() {
    let mut idx = index_with_strings(&["hello alice"]);
    Redactor::new().with_autodetect_username().apply(&mut idx);
    assert_eq!(strings_of(&idx)[0], "hello alice");
}

#[test]
fn autodetect_username_is_noop_when_no_environment() {
    // index_with_build_started_env supplies env, so for the "no env"
    // case we have to construct one explicitly.
    use crate::events::BuildStartedEvent;
    use crate::jsonlog::schema::{JsonlogEvent, JsonlogEventBody, JsonlogFile, JsonlogHeader};

    let ev = BuildStartedEvent {
        fields: Default::default(),
        environment: None,
    };
    let value = serde_json::to_value(&ev).unwrap();
    let file = JsonlogFile {
        munin_jsonlog_version: 1,
        header: JsonlogHeader {
            file_format_version: 18,
            min_reader_version: 18,
        },
        strings: vec!["hello alice".to_string()],
        name_value_lists: Vec::new(),
        archives: Vec::new(),
        events: vec![JsonlogEvent {
            kind: "BuildStarted".to_string(),
            byte_offset: 0,
            body: JsonlogEventBody::Decoded(value),
        }],
    };
    let mut idx = BinlogIndex::from_jsonlog(file).unwrap();
    Redactor::new().with_autodetect_username().apply(&mut idx);
    // String table preserves the original "hello alice" — though the
    // table also contains "BuildStarted"-related dedup strings, we only
    // care that "alice" was NOT scrubbed (no current-process leak).
    let entries = idx.strings().entries();
    assert!(
        entries.iter().any(|s| s.contains("alice")),
        "expected 'alice' to remain when env is None: {entries:?}",
    );
}

#[test]
fn autodetect_username_replaces_username_value_in_strings() {
    let mut idx = index_with_build_started_env(&[("USERNAME", "alice")]);
    // The index's string table currently contains only what the
    // BuildStarted event introduced. Add an entry by going through a
    // round-trip with a string that references the username.
    idx.strings_mut().entries_mut(); // ensure mutable borrow path compiles
    // Push a string by reaching into the table.
    let s_idx = idx.strings_mut().add("hello alice and Alice".to_string());
    Redactor::new().with_autodetect_username().apply(&mut idx);
    assert_eq!(
        idx.strings().get(s_idx).unwrap(),
        Some("hello REDACTED-USER and Alice"),
    );
}

#[test]
fn autodetect_username_takes_basename_for_path_values() {
    let mut idx = index_with_build_started_env(&[("USERPROFILE", "C:\\Users\\alice")]);
    let s = idx
        .strings_mut()
        .add("file=C:\\Users\\alice\\src\\foo.cs".to_string());
    Redactor::new().with_autodetect_username().apply(&mut idx);
    // The "C:\Users\alice\" prefix rule scrubs the full prefix, leaving
    // the trailing path unchanged.
    assert_eq!(
        idx.strings().get(s).unwrap(),
        Some("file=C:\\Users\\REDACTED-USER\\src\\foo.cs"),
    );
}

#[test]
fn autodetect_username_skips_empty_values() {
    let mut idx = index_with_build_started_env(&[("USERNAME", ""), ("USER", "bob")]);
    let s = idx.strings_mut().add("hello bob".to_string());
    Redactor::new().with_autodetect_username().apply(&mut idx);
    assert_eq!(idx.strings().get(s).unwrap(), Some("hello REDACTED-USER"));
}
