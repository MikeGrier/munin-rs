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
    assert!(r.translation_units.is_empty());
    assert_eq!(r.malformed_message_count, 0);
}

#[test]
fn parse_only_non_include_messages_yields_empty_result() {
    let msgs = ["compiling foo", "warning: blah", "ok"];
    let r = parse(&msgs).unwrap();
    assert!(r.translation_units.is_empty());
}

#[test]
fn parse_single_top_level_include() {
    let msgs = [r"Note: including file: C:\inc\a.h"];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.translation_units[0].tree, vec![n(r"C:\inc\a.h", vec![])]);
    assert_eq!(r.translation_units[0].included_files, vec![r"C:\inc\a.h"]);
}

#[test]
fn parse_two_siblings_at_depth_1() {
    let msgs = [
        r"Note: including file: C:\inc\a.h",
        r"Note: including file: C:\inc\b.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(
        r.translation_units[0].tree,
        vec![n(r"C:\inc\a.h", vec![]), n(r"C:\inc\b.h", vec![]),]
    );
    assert_eq!(
        r.translation_units[0].included_files,
        vec![r"C:\inc\a.h", r"C:\inc\b.h"]
    );
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
        r.translation_units[0].tree,
        vec![n("a.h", vec![n("b.h", vec![n("c.h", vec![])])])]
    );
    assert_eq!(
        r.translation_units[0].included_files,
        vec!["a.h", "b.h", "c.h"]
    );
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
        r.translation_units[0].tree,
        vec![
            n("a.h", vec![n("b.h", vec![]), n("c.h", vec![])]),
            n("d.h", vec![]),
        ]
    );
    assert_eq!(
        r.translation_units[0].included_files,
        vec!["a.h", "b.h", "c.h", "d.h"]
    );
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
        r.translation_units[0].tree,
        vec![
            n("a.h", vec![n("b.h", vec![])]),
            n("d.h", vec![n("b.h", vec![])]),
        ]
    );
    assert_eq!(
        r.translation_units[0].included_files,
        vec!["a.h", "b.h", "d.h"]
    );
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
        r.translation_units[0].tree,
        vec![n("a.h", vec![n("b.h", vec![])]), n("c.h", vec![]),]
    );
    assert_eq!(
        r.translation_units[0].included_files,
        vec!["a.h", "b.h", "c.h"]
    );
}

#[test]
fn parse_first_line_with_excessive_depth_is_skipped_as_malformed() {
    let msgs = [r"Note: including file:    skipped.h"];
    let r = parse(&msgs).unwrap();
    assert!(r.translation_units.is_empty());
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
    assert_eq!(r.translation_units[0].tree, vec![n("a.h", vec![])]);
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
    assert_eq!(r.translation_units[0].tree.len(), 2);
    assert_eq!(
        r.translation_units[0].tree[0].resolved_path,
        r"C:\VC\include\iostream"
    );
    assert_eq!(
        r.translation_units[0].tree[1].resolved_path,
        r"C:\proj\include\my.h"
    );
    // vcruntime.h appears twice in the tree but once in the flat list.
    let vcrt_count = r.translation_units[0]
        .included_files
        .iter()
        .filter(|p| p.ends_with("vcruntime.h"))
        .count();
    assert_eq!(vcrt_count, 1);
    // First-encounter ordering: iostream branch comes before my.h.
    assert_eq!(
        r.translation_units[0].included_files[0],
        r"C:\VC\include\iostream"
    );
    assert!(
        r.translation_units[0]
            .included_files
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

// ── boundary_marker recognition (D-CPP-SHOWINC2) ──

#[test]
fn boundary_marker_recognizes_bare_basename_with_known_extension() {
    for name in [
        "a.cpp",
        "main.cpp",
        "Foo_Bar-1.cxx",
        "file.cc",
        "thing.c",
        "WeirdName.c++",
        "AgentMonitoring.cpp",
    ] {
        assert_eq!(boundary_marker(name), Some(name), "should match: {name}");
    }
}

#[test]
fn boundary_marker_case_insensitive_extension() {
    for name in ["a.CPP", "b.Cxx", "c.CC", "d.C", "e.C++"] {
        assert_eq!(boundary_marker(name), Some(name), "should match: {name}");
    }
}

#[test]
fn boundary_marker_rejects_unknown_extensions() {
    for name in ["foo.h", "foo.hpp", "foo.txt", "foo.obj", "foo.cpp.bak"] {
        assert_eq!(boundary_marker(name), None, "should not match: {name}");
    }
}

#[test]
fn boundary_marker_rejects_paths_and_embedded_spaces() {
    for s in [
        r"C:\src\foo.cpp",
        "src/foo.cpp",
        "compiling foo.cpp",
        " foo.cpp",
        "foo.cpp ",
        "foo.cpp extra",
        "Note: including file: foo.cpp",
    ] {
        assert_eq!(boundary_marker(s), None, "should not match: {s:?}");
    }
}

#[test]
fn boundary_marker_rejects_empty_or_bare_extension() {
    assert_eq!(boundary_marker(""), None);
    assert_eq!(boundary_marker(".cpp"), None);
    assert_eq!(boundary_marker(".c"), None);
}

// ── batch mode (D-CPP-SHOWINC2) ──

#[test]
fn parse_single_tu_with_boundary_marker_records_source_name() {
    let msgs = ["main.cpp", r"Note: including file: a.h"];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.translation_units.len(), 1);
    let tu = &r.translation_units[0];
    assert_eq!(tu.source_name.as_deref(), Some("main.cpp"));
    assert_eq!(tu.tree, vec![n("a.h", vec![])]);
    assert_eq!(tu.included_files, vec!["a.h"]);
}

#[test]
fn parse_batch_three_tus_split_on_markers() {
    let msgs = [
        "a.cpp",
        r"Note: including file: a.h",
        r"Note: including file:  a_dep.h",
        "b.cpp",
        r"Note: including file: b.h",
        "c.cpp",
        r"Note: including file: c.h",
        r"Note: including file:  c_dep.h",
        r"Note: including file:   c_deep.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.translation_units.len(), 3);
    assert_eq!(r.translation_units[0].source_name.as_deref(), Some("a.cpp"));
    assert_eq!(
        r.translation_units[0].tree,
        vec![n("a.h", vec![n("a_dep.h", vec![])])]
    );
    assert_eq!(r.translation_units[1].source_name.as_deref(), Some("b.cpp"));
    assert_eq!(r.translation_units[1].tree, vec![n("b.h", vec![])]);
    assert_eq!(r.translation_units[2].source_name.as_deref(), Some("c.cpp"));
    assert_eq!(
        r.translation_units[2].tree,
        vec![n("c.h", vec![n("c_dep.h", vec![n("c_deep.h", vec![])])])]
    );
}

#[test]
fn parse_batch_tu_dedup_is_per_tu_not_cross_tu() {
    // Same header (common.h) appears in two TUs. It is recorded
    // separately in each TU's included_files.
    let msgs = [
        "a.cpp",
        r"Note: including file: common.h",
        "b.cpp",
        r"Note: including file: common.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.translation_units.len(), 2);
    assert_eq!(r.translation_units[0].included_files, vec!["common.h"]);
    assert_eq!(r.translation_units[1].included_files, vec!["common.h"]);
}

#[test]
fn parse_marker_resets_depth_stack_so_next_tu_starts_at_depth_1() {
    // Last include of TU 1 is at depth 3. TU 2 starts at depth 1
    // — the marker must reset the depth stack, otherwise depth 1
    // would look like a backward jump (still valid) or worse, the
    // stack from TU 1 would leak into TU 2.
    let msgs = [
        "a.cpp",
        r"Note: including file: a.h",
        r"Note: including file:  a_mid.h",
        r"Note: including file:   a_leaf.h",
        "b.cpp",
        r"Note: including file: b.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.translation_units.len(), 2);
    assert_eq!(r.translation_units[1].tree, vec![n("b.h", vec![])]);
}

#[test]
fn parse_includes_before_first_marker_go_into_anonymous_tu() {
    // Pre-marker includes form a TU with source_name = None,
    // then the marker opens a named TU.
    let msgs = [
        r"Note: including file: stray.h",
        "main.cpp",
        r"Note: including file: real.h",
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.translation_units.len(), 2);
    assert_eq!(r.translation_units[0].source_name, None);
    assert_eq!(r.translation_units[0].tree, vec![n("stray.h", vec![])]);
    assert_eq!(
        r.translation_units[1].source_name.as_deref(),
        Some("main.cpp")
    );
    assert_eq!(r.translation_units[1].tree, vec![n("real.h", vec![])]);
}

#[test]
fn parse_marker_only_no_includes_yields_empty_tu() {
    // cl.exe emits the marker before every TU's includes, so a TU
    // that produced no includes still gets a (possibly empty) TU
    // entry. Tests that the parser opens the TU on the marker, not
    // lazily on the first include.
    let msgs = ["empty.cpp"];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.translation_units.len(), 1);
    let tu = &r.translation_units[0];
    assert_eq!(tu.source_name.as_deref(), Some("empty.cpp"));
    assert!(tu.tree.is_empty());
    assert!(tu.included_files.is_empty());
}

#[test]
fn parse_malformed_count_sums_across_tus() {
    let msgs = [
        "a.cpp",
        r"Note: including file: a.h",
        r"Note: including file:    skip_in_a.h", // depth 4, malformed
        "b.cpp",
        r"Note: including file: b.h",
        r"Note: including file:    skip_in_b.h", // depth 4, malformed
    ];
    let r = parse(&msgs).unwrap();
    assert_eq!(r.malformed_message_count, 2);
    assert_eq!(r.translation_units.len(), 2);
}
