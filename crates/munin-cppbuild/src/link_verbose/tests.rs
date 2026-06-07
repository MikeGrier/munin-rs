// Copyright (c) Michael Grier

//! Unit tests for the `/VERBOSE` parser.

use super::*;
use crate::link_cmdline;

fn parse_cmd(cmd: &str) -> LinkCommandLine {
    link_cmdline::parse(cmd)
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(|s| s.to_string()).collect()
}

#[test]
fn empty_messages_and_cmdline_yields_empty() {
    let cmd = parse_cmd("");
    let out = parse(&cmd, &[]);
    assert!(out.inputs.is_empty());
    assert!(out.dropped.is_empty());
    assert_eq!(out.synthetic_defaultlib_count, 0);
}

#[test]
fn cmdline_obj_is_always_referenced() {
    let cmd = parse_cmd("link.exe main.obj");
    let out = parse(&cmd, &[]);
    assert_eq!(out.inputs.len(), 1);
    assert_eq!(out.inputs[0].path, "main.obj");
    assert_eq!(out.inputs[0].kind, LinkInputKind::Obj);
    assert_eq!(out.inputs[0].origin, LinkInputOrigin::Direct);
    assert!(out.inputs[0].referenced);
}

#[test]
fn cmdline_lib_with_no_messages_starts_unreferenced() {
    let cmd = parse_cmd("link.exe foo.lib");
    let out = parse(&cmd, &[]);
    assert_eq!(out.inputs.len(), 1);
    assert_eq!(out.inputs[0].kind, LinkInputKind::Lib);
    assert_eq!(out.inputs[0].origin, LinkInputOrigin::Direct);
    assert!(!out.inputs[0].referenced);
}

#[test]
fn searching_with_loaded_marks_referenced() {
    let cmd = parse_cmd("link.exe foo.lib");
    let msgs = lines(
        "    Searching C:\\path\\foo.lib:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded foo.obj",
    );
    let out = parse(&cmd, &msgs);
    assert_eq!(out.inputs.len(), 1);
    assert!(out.inputs[0].referenced);
    // The cmdline bare name should be upgraded to the resolved path.
    assert_eq!(out.inputs[0].path, "C:\\path\\foo.lib");
}

#[test]
fn searching_without_loaded_remains_unreferenced() {
    let cmd = parse_cmd("link.exe foo.lib");
    let msgs = lines("    Searching C:\\path\\foo.lib:");
    let out = parse(&cmd, &msgs);
    assert_eq!(out.inputs.len(), 1);
    assert!(!out.inputs[0].referenced);
}

#[test]
fn multiple_loaded_lines_in_one_scope_stay_single_input() {
    let cmd = parse_cmd("link.exe foo.lib");
    let msgs = lines(
        "    Searching C:\\path\\foo.lib:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded a.obj\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded b.obj\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded c.obj",
    );
    let out = parse(&cmd, &msgs);
    assert_eq!(out.inputs.len(), 1);
    assert!(out.inputs[0].referenced);
}

#[test]
fn unused_libraries_section_lists_dropped_and_marks_unreferenced() {
    let cmd = parse_cmd("link.exe used.lib unused1.lib unused2.lib main.obj");
    let msgs = lines(
        "    Searching C:\\path\\used.lib:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded used.obj\n\
         Unused libraries:\n\
         \x20\x20C:\\path\\unused1.lib\n\
         \x20\x20C:\\path\\unused2.lib",
    );
    let out = parse(&cmd, &msgs);

    assert_eq!(out.dropped.len(), 2);
    assert_eq!(out.dropped[0].path, "C:\\path\\unused1.lib");
    assert_eq!(out.dropped[0].reason, "unused");
    assert_eq!(out.dropped[1].path, "C:\\path\\unused2.lib");

    let used = out
        .inputs
        .iter()
        .find(|i| i.path.ends_with("used.lib"))
        .unwrap();
    assert!(used.referenced);
    let u1 = out.inputs.iter().find(|i| i.path == "unused1.lib").unwrap();
    assert!(!u1.referenced);
    let u2 = out.inputs.iter().find(|i| i.path == "unused2.lib").unwrap();
    assert!(!u2.referenced);
    let main = out.inputs.iter().find(|i| i.path == "main.obj").unwrap();
    assert!(main.referenced);
}

#[test]
fn unused_section_is_terminated_by_non_indented_line() {
    let cmd = parse_cmd("link.exe");
    let msgs = lines(
        "Unused libraries:\n\
         \x20\x20C:\\foo\\a.lib\n\
         Finished pass 1\n\
         \x20\x20not-an-unused-entry.lib",
    );
    let out = parse(&cmd, &msgs);
    assert_eq!(out.dropped.len(), 1);
    assert_eq!(out.dropped[0].path, "C:\\foo\\a.lib");
}

#[test]
fn defaultlib_with_searching_scope_does_not_create_synthetic_input() {
    let cmd = parse_cmd("link.exe main.obj");
    let msgs = lines(
        "Processed /DEFAULTLIB:msvcprt\n\
         \x20\x20\x20\x20Searching C:\\sdk\\msvcprt.lib:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded foo.obj",
    );
    let out = parse(&cmd, &msgs);
    assert_eq!(out.synthetic_defaultlib_count, 0);
    let lib = out
        .inputs
        .iter()
        .find(|i| i.path.ends_with("msvcprt.lib"))
        .unwrap();
    assert_eq!(lib.origin, LinkInputOrigin::Searched);
    assert!(lib.referenced);
}

#[test]
fn defaultlib_without_searching_scope_creates_synthetic_input() {
    let cmd = parse_cmd("link.exe main.obj");
    let msgs = lines("Processed /DEFAULTLIB:ghost");
    let out = parse(&cmd, &msgs);
    assert_eq!(out.synthetic_defaultlib_count, 1);
    let lib = out
        .inputs
        .iter()
        .find(|i| i.path == "defaultlib:ghost")
        .unwrap();
    assert_eq!(lib.kind, LinkInputKind::Lib);
    assert_eq!(lib.origin, LinkInputOrigin::Searched);
    assert!(!lib.referenced);
}

#[test]
fn defaultlib_already_with_lib_suffix_matches_basename() {
    let cmd = parse_cmd("link.exe main.obj");
    let msgs = lines(
        "Processed /DEFAULTLIB:foo.lib\n\
         \x20\x20\x20\x20Searching C:\\x\\foo.lib:",
    );
    let out = parse(&cmd, &msgs);
    assert_eq!(out.synthetic_defaultlib_count, 0);
}

#[test]
fn case_insensitive_basename_matching_for_cmdline_inputs() {
    let cmd = parse_cmd("link.exe FOO.LIB");
    let msgs = lines(
        "    Searching C:\\path\\foo.lib:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded x.obj",
    );
    let out = parse(&cmd, &msgs);
    assert_eq!(out.inputs.len(), 1);
    assert!(out.inputs[0].referenced);
}

#[test]
fn searched_lib_not_on_cmdline_appears_as_searched_origin() {
    let cmd = parse_cmd("link.exe main.obj");
    let msgs = lines(
        "    Searching C:\\sdk\\extra.lib:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded e.obj",
    );
    let out = parse(&cmd, &msgs);
    let extra = out
        .inputs
        .iter()
        .find(|i| i.path.ends_with("extra.lib"))
        .unwrap();
    assert_eq!(extra.origin, LinkInputOrigin::Searched);
    assert_eq!(extra.kind, LinkInputKind::Lib);
    assert!(extra.referenced);
}

#[test]
fn unused_entry_does_not_get_confused_with_searching_line() {
    let cmd = parse_cmd("link.exe");
    // 4-space indented lines should never be classified as unused
    // entries even if Unused header preceded them.
    let msgs = lines(
        "Unused libraries:\n\
         \x20\x20\x20\x20Searching C:\\sdk\\x.lib:",
    );
    let out = parse(&cmd, &msgs);
    assert!(out.dropped.is_empty());
}

#[test]
fn cmdline_lib_in_unused_section_overrides_scope_inference() {
    // Pathological case: linker reports a Loaded under foo.lib but
    // also lists it as unused. The Unused list is authoritative.
    let cmd = parse_cmd("link.exe foo.lib");
    let msgs = lines(
        "    Searching C:\\path\\foo.lib:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded x.obj\n\
         Unused libraries:\n\
         \x20\x20C:\\path\\foo.lib",
    );
    let out = parse(&cmd, &msgs);
    assert_eq!(out.inputs.len(), 1);
    assert!(!out.inputs[0].referenced);
    assert_eq!(out.dropped.len(), 1);
}

#[test]
fn pass_boundaries_reset_current_scope() {
    let cmd = parse_cmd("link.exe");
    let msgs = lines(
        "    Searching C:\\a.lib:\n\
         Finished pass 1\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded after-pass.obj",
    );
    let out = parse(&cmd, &msgs);
    // The Loaded after pass end must not retroactively reference a.lib.
    let a = out.inputs.iter().find(|i| i.path == "C:\\a.lib").unwrap();
    assert!(!a.referenced);
}

#[test]
fn realistic_sample_unused_libs_block() {
    let cmd = parse_cmd("link.exe /OUT:foo.dll a.lib b.lib c.lib main.obj");
    let msgs = lines(
        "Starting pass 1\n\
         Processed /DEFAULTLIB:msvcrt\n\
         \x20\x20\x20\x20Searching C:\\sdk\\msvcrt.lib:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded crt0.obj\n\
         \x20\x20\x20\x20Searching D:\\repo\\a.lib:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20Loaded a1.obj\n\
         \x20\x20\x20\x20Searching D:\\repo\\b.lib:\n\
         \x20\x20\x20\x20Searching D:\\repo\\c.lib:\n\
         Finished pass 1\n\
         Unused libraries:\n\
         \x20\x20D:\\repo\\b.lib\n\
         \x20\x20D:\\repo\\c.lib",
    );
    let out = parse(&cmd, &msgs);

    // Dropped libs.
    assert_eq!(out.dropped.len(), 2);
    assert_eq!(out.dropped[0].path, "D:\\repo\\b.lib");
    assert_eq!(out.dropped[1].path, "D:\\repo\\c.lib");

    let a = out
        .inputs
        .iter()
        .find(|i| i.path.ends_with("a.lib"))
        .unwrap();
    assert_eq!(a.origin, LinkInputOrigin::Direct);
    assert!(a.referenced);

    let b = out
        .inputs
        .iter()
        .find(|i| i.path.ends_with("b.lib"))
        .unwrap();
    assert!(!b.referenced);
    let c = out
        .inputs
        .iter()
        .find(|i| i.path.ends_with("c.lib"))
        .unwrap();
    assert!(!c.referenced);

    let crt = out
        .inputs
        .iter()
        .find(|i| i.path.ends_with("msvcrt.lib"))
        .unwrap();
    assert_eq!(crt.origin, LinkInputOrigin::Searched);
    assert!(crt.referenced);
    assert_eq!(out.synthetic_defaultlib_count, 0);

    let main = out.inputs.iter().find(|i| i.path == "main.obj").unwrap();
    assert_eq!(main.kind, LinkInputKind::Obj);
    assert!(main.referenced);
}
