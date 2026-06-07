// Copyright (c) Michael Grier

use super::*;

fn root(name: &str, path: &str) -> Root {
    Root {
        name: name.into(),
        path: path.into(),
    }
}

#[test]
fn path_inside_root_becomes_relative() {
    let roots = vec![root("primary", r"C:\src\product")];
    let r = to_rooted(r"C:\src\product\foo\bar.cpp", &roots);
    assert_eq!(r.root, Some(0));
    assert_eq!(r.path, r"foo\bar.cpp");
}

#[test]
fn path_equal_to_root_returns_empty_relative() {
    let roots = vec![root("primary", r"C:\src\product")];
    let r = to_rooted(r"C:\src\product", &roots);
    assert_eq!(r.root, Some(0));
    assert_eq!(r.path, "");
}

#[test]
fn path_outside_all_roots_is_absolute() {
    let roots = vec![root("primary", r"C:\src\product")];
    let r = to_rooted(r"D:\external\bar.h", &roots);
    assert_eq!(r.root, None);
    assert_eq!(r.path, r"D:\external\bar.h");
}

#[test]
fn first_matching_root_wins_when_multiple_match() {
    // The second root is a subdirectory of the first, but the first
    // is supplied first, so it wins.
    let roots = vec![root("outer", r"C:\src"), root("inner", r"C:\src\product")];
    let r = to_rooted(r"C:\src\product\foo.cpp", &roots);
    assert_eq!(r.root, Some(0));
    assert_eq!(r.path, r"product\foo.cpp");
}

#[test]
fn multiple_roots_separate_drives() {
    let roots = vec![
        root("primary", r"C:\src\product"),
        root("nuget_cache", r"D:\nuget"),
    ];
    let r1 = to_rooted(r"C:\src\product\foo.cpp", &roots);
    assert_eq!(r1.root, Some(0));
    assert_eq!(r1.path, r"foo.cpp");

    let r2 = to_rooted(r"D:\nuget\pkg\1.0\include\thing.h", &roots);
    assert_eq!(r2.root, Some(1));
    assert_eq!(r2.path, r"pkg\1.0\include\thing.h");
}

#[test]
fn case_insensitive_ascii_match() {
    let roots = vec![root("primary", r"C:\Src\Product")];
    let r = to_rooted(r"c:\SRC\product\foo.cpp", &roots);
    assert_eq!(r.root, Some(0));
    assert_eq!(r.path, r"foo.cpp");
}

#[test]
fn forward_slashes_normalized_in_input() {
    let roots = vec![root("primary", r"C:\src\product")];
    let r = to_rooted("C:/src/product/foo/bar.cpp", &roots);
    assert_eq!(r.root, Some(0));
    assert_eq!(r.path, r"foo\bar.cpp");
}

#[test]
fn forward_slashes_normalized_in_root() {
    let roots = vec![root("primary", "C:/src/product")];
    let r = to_rooted(r"C:\src\product\foo.cpp", &roots);
    assert_eq!(r.root, Some(0));
    assert_eq!(r.path, r"foo.cpp");
}

#[test]
fn trailing_separator_on_root_ignored() {
    let roots = vec![root("primary", r"C:\src\product\")];
    let r = to_rooted(r"C:\src\product\foo.cpp", &roots);
    assert_eq!(r.root, Some(0));
    assert_eq!(r.path, r"foo.cpp");
}

#[test]
fn prefix_must_end_at_component_boundary() {
    // C:\src\foo must NOT match C:\src\foobar\...
    let roots = vec![root("primary", r"C:\src\foo")];
    let r = to_rooted(r"C:\src\foobar\baz.cpp", &roots);
    assert_eq!(r.root, None);
    assert_eq!(r.path, r"C:\src\foobar\baz.cpp");
}

#[test]
fn no_roots_supplied_returns_absolute_normalized() {
    let r = to_rooted("C:/src/foo.cpp", &[]);
    assert_eq!(r.root, None);
    assert_eq!(r.path, r"C:\src\foo.cpp");
}

#[test]
fn deterministic_repeated_calls() {
    let roots = vec![root("a", r"C:\a"), root("b", r"C:\b"), root("c", r"C:\c")];
    let p = r"C:\b\sub\file.h";
    let r1 = to_rooted(p, &roots);
    let r2 = to_rooted(p, &roots);
    let r3 = to_rooted(p, &roots);
    assert_eq!(r1, r2);
    assert_eq!(r2, r3);
}
