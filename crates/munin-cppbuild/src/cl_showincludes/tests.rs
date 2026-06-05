// Copyright (c) Michael Grier

use super::*;

fn n(path: &str, children: Vec<RawIncludeNode>) -> RawIncludeNode {
    RawIncludeNode {
        resolved_path: path.into(),
        children,
    }
}

// ── parse_include_line ──

#[test]
fn parse_include_line_depth_1() {
    let r = parse_include_line(r"Note: including file: C:\a\top.h");
    assert_eq!(r, Some((1, r"C:\a\top.h")));
}

#[test]
fn parse_include_line_depth_3() {
    let r = parse_include_line(r"Note: including file:   C:\a\nested.h");
    assert_eq!(r, Some((3, r"C:\a\nested.h")));
}

#[test]
fn parse_include_line_rejects_non_prefix() {
    assert!(parse_include_line("just a message").is_none());
    assert!(parse_include_line("Note: including file:").is_none());
    // No leading space after the colon → not a valid include line.
    assert!(parse_include_line("Note: including file:nospace.h").is_none());
}

#[test]
fn parse_include_line_is_case_sensitive() {
    // cl.exe emits exactly the canonical casing; non-matching case
    // is treated as not-an-include-line.
    assert!(parse_include_line("NOTE: INCLUDING FILE: x.h").is_none());
}

// ── parse: structural cases ──

#[test]
fn parse_empty_input_yields_empty_result() {
    let r = parse::<&str>(&[]).unwrap();
    assert!(r.tree.is_empty());
    assert!(r.included_files.is_empty());
    assert_eq!(r.malformed_message_count, 0);
}

#[test]
fn parse_only_non_include_messages_yields_empty_result() {
    let msgs = ["compiling main.cpp", "warning: blah", "ok"];
    let r = parse(&msgs).unwrap();
    assert!(r.tree.is_empty());
    assert!(r.included_files.is_empty());
}

#[test]
fn parse_single_top_level_include() {
    let msgs = [r"Note: including file: C:\inc\a.h"];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.tree, vec![n(r"C:\inc\a.h", vec![])]);
    assert_eq!(r.included_files, vec![r"C:\inc\a.h"]);
}

#[test]
fn parse_two_siblings_at_depth_1() {
    let msgs = [
        r"Note: including file: C:\inc\a.h",
        r"Note: including file: C:\inc\b.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(
        r.tree,
        vec![n(r"C:\inc\a.h", vec![]), n(r"C:\inc\b.h", vec![]),]
    );
    assert_eq!(r.included_files, vec![r"C:\inc\a.h", r"C:\inc\b.h"]);
}

#[test]
fn parse_nested_chain_three_deep() {
    let msgs = [
        r"Note: including file: a.h",
        r"Note: including file:  b.h",
        r"Note: including file:   c.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(
        r.tree,
        vec![n("a.h", vec![n("b.h", vec![n("c.h", vec![])])])]
    );
    assert_eq!(r.included_files, vec!["a.h", "b.h", "c.h"]);
}

#[test]
fn parse_pop_back_to_top_then_new_branch() {
    // a → b, a → c (sibling of b), then top-level d.
    let msgs = [
        r"Note: including file: a.h",
        r"Note: including file:  b.h",
        r"Note: including file:  c.h",
        r"Note: including file: d.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(
        r.tree,
        vec![
            n("a.h", vec![n("b.h", vec![]), n("c.h", vec![])]),
            n("d.h", vec![]),
        ]
    );
    assert_eq!(r.included_files, vec!["a.h", "b.h", "c.h", "d.h"]);
}

#[test]
fn parse_duplicates_appear_in_tree_but_dedup_in_flat_list() {
    // a includes b. d also includes b. Both occurrences of b live in
    // the tree but the flat list mentions b once.
    let msgs = [
        r"Note: including file: a.h",
        r"Note: including file:  b.h",
        r"Note: including file: d.h",
        r"Note: including file:  b.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(
        r.tree,
        vec![
            n("a.h", vec![n("b.h", vec![])]),
            n("d.h", vec![n("b.h", vec![])]),
        ]
    );
    assert_eq!(r.included_files, vec!["a.h", "b.h", "d.h"]);
}

#[test]
fn parse_interleaved_diagnostics_do_not_break_tree() {
    let msgs = [
        r"Note: including file: a.h",
        "warning C4xxx: something",
        r"Note: including file:  b.h",
        "info: still compiling",
        r"Note: including file: c.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(
        r.tree,
        vec![n("a.h", vec![n("b.h", vec![])]), n("c.h", vec![]),]
    );
    assert_eq!(r.included_files, vec!["a.h", "b.h", "c.h"]);
}

#[test]
fn parse_first_line_with_excessive_depth_is_skipped_as_malformed() {
    let msgs = [r"Note: including file:    skipped.h"];
    let r = parse(&msgs).unwrap();
    assert!(r.tree.is_empty());
    assert_eq!(r.malformed_message_count, 1);
}

#[test]
fn parse_jump_two_levels_deep_skipped() {
    let msgs = [
        r"Note: including file: a.h",
        // Depth 3 with no depth-2 in between.
        r"Note: including file:   c.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.tree, vec![n("a.h", vec![])]);
    assert_eq!(r.malformed_message_count, 1);
}

#[test]
fn parse_realistic_stdlib_style_chain() {
    // <iostream> pulls in a deep MSVC CRT chain.  Verify the tree
    // shape and dedup behaviour on a realistic-looking sequence.
    let msgs = [
        r"Note: including file: C:\VC\include\iostream",
        r"Note: including file:  C:\VC\include\istream",
        r"Note: including file:   C:\VC\include\ostream",
        r"Note: including file:    C:\VC\include\ios",
        r"Note: including file:     C:\VC\include\xlocnum",
        r"Note: including file:      C:\VC\include\climits",
        r"Note: including file:       C:\VC\include\yvals.h",
        r"Note: including file:        C:\VC\include\vcruntime.h",
        // iostream → istream → ostream is already noted; now a second
        // top-level include with a shared sub-header.
        r"Note: including file: C:\proj\include\my.h",
        r"Note: including file:  C:\VC\include\vcruntime.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.tree.len(), 2);
    assert_eq!(r.tree[0].resolved_path, r"C:\VC\include\iostream");
    assert_eq!(r.tree[1].resolved_path, r"C:\proj\include\my.h");
    // vcruntime.h appears twice in the tree but once in the flat list.
    let vcrt_count = r
        .included_files
        .iter()
        .filter(|p| p.ends_with("vcruntime.h"))
        .count();
    assert_eq!(vcrt_count, 1);
    // First-encounter ordering: iostream branch comes before my.h.
    assert_eq!(r.included_files[0], r"C:\VC\include\iostream");
    assert!(
        r.included_files
            .contains(&r"C:\proj\include\my.h".to_string())
    );
}

// ── locale detection ──

#[test]
fn parse_french_locale_yields_locale_not_supported_error() {
    // U+00A0 NO-BREAK SPACE before the colons matches French
    // typographic rules; the parser registers both common forms.
    let msgs = ["Remarque\u{a0}: inclusion du fichier\u{a0}: a.h".to_string()];
    let err = parse(&msgs).unwrap_err();
    assert_eq!(err.locale, "fr");
}

#[test]
fn parse_german_locale_yields_locale_not_supported_error() {
    let msgs = ["Hinweis: Einlesen der Datei: a.h"];
    let err = parse(&msgs).unwrap_err();
    assert_eq!(err.locale, "de");
}

#[test]
fn parse_locale_error_carries_sample_message() {
    let msgs = ["Hinweis: Einlesen der Datei: c:\\path\\a.h"];
    let err = parse(&msgs).unwrap_err();
    assert!(err.sample_message.contains("Einlesen"));
}

#[test]
fn parse_locale_error_display_includes_locale_and_message() {
    let err = LocaleNotSupportedError {
        locale: "de".into(),
        sample_message: "Hinweis: ...".into(),
    };
    let s = format!("{err}");
    assert!(s.contains("'de'"));
    assert!(s.contains("Hinweis"));
}
