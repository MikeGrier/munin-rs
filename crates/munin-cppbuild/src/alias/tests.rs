// Copyright (c) Michael Grier

use super::*;

fn rp(root: Option<usize>, path: &str) -> RootedPath {
    RootedPath {
        root,
        path: path.into(),
    }
}

#[test]
fn empty_input_yields_empty_table() {
    let t = AliasTable::build(std::iter::empty());
    assert!(t.is_empty());
    assert_eq!(t.len(), 0);
}

#[test]
fn all_unique_leaves_use_leaf_names() {
    let paths = vec![
        rp(Some(0), r"src\a.cpp"),
        rp(Some(0), r"include\b.h"),
        rp(Some(0), r"deep\nested\c.h"),
    ];
    let t = AliasTable::build(paths.clone());

    assert_eq!(t.alias_for(&paths[0]), Some("a.cpp"));
    assert_eq!(t.alias_for(&paths[1]), Some("b.h"));
    assert_eq!(t.alias_for(&paths[2]), Some("c.h"));
}

#[test]
fn colliding_leaves_disambiguate_with_last_two() {
    let paths = vec![
        rp(Some(0), r"src\foo\common.h"),
        rp(Some(0), r"src\bar\common.h"),
    ];
    let t = AliasTable::build(paths.clone());

    assert_eq!(t.alias_for(&paths[0]), Some(r"foo\common.h"));
    assert_eq!(t.alias_for(&paths[1]), Some(r"bar\common.h"));
}

#[test]
fn fallback_to_first_and_leaf_when_last_two_also_collide() {
    // Two NuGet-style paths where leaf collides AND the last two
    // segments collide (both end in include\thing.h), but the first
    // segment differs (the package names).
    let paths = vec![
        rp(Some(1), r"pkgA\1.0\include\thing.h"),
        rp(Some(1), r"pkgB\2.0\include\thing.h"),
    ];
    let t = AliasTable::build(paths.clone());

    assert_eq!(t.alias_for(&paths[0]), Some(r"pkgA\..\thing.h"));
    assert_eq!(t.alias_for(&paths[1]), Some(r"pkgB\..\thing.h"));
}

#[test]
fn fallback_to_numbered_when_all_three_collide() {
    // Same root, same relative path inside different version dirs of
    // the same package: first segment, last two, and leaf all collide.
    let paths = vec![
        rp(Some(0), r"pkg\1.0\include\thing.h"),
        rp(Some(0), r"pkg\2.0\include\thing.h"),
        rp(Some(0), r"pkg\3.0\include\thing.h"),
    ];
    let t = AliasTable::build(paths.clone());

    // Numbering is deterministic — sorted by RootedPath, so
    // ascending version order: 1.0 → #1, 2.0 → #2, 3.0 → #3.
    assert_eq!(t.alias_for(&paths[0]), Some("thing.h#1"));
    assert_eq!(t.alias_for(&paths[1]), Some("thing.h#2"));
    assert_eq!(t.alias_for(&paths[2]), Some("thing.h#3"));
}

#[test]
fn unique_leaf_path_is_unaffected_by_collisions_elsewhere() {
    let paths = vec![
        rp(Some(0), r"src\foo\common.h"),
        rp(Some(0), r"src\bar\common.h"),
        rp(Some(0), r"src\unique.cpp"),
    ];
    let t = AliasTable::build(paths.clone());

    assert_eq!(t.alias_for(&paths[0]), Some(r"foo\common.h"));
    assert_eq!(t.alias_for(&paths[1]), Some(r"bar\common.h"));
    assert_eq!(t.alias_for(&paths[2]), Some("unique.cpp"));
}

#[test]
fn mixed_collision_classes() {
    // - "uniq.h" only appears once → leaf alias.
    // - "a.h" appears in two distinct dirs → last-two disambiguates.
    // - "b.h" appears under pkgA and pkgB with same last-two
    //   "include\b.h" → first+leaf disambiguates.
    let paths = vec![
        rp(Some(0), r"only\uniq.h"),
        rp(Some(0), r"x\a.h"),
        rp(Some(0), r"y\a.h"),
        rp(Some(0), r"pkgA\v1\include\b.h"),
        rp(Some(0), r"pkgB\v1\include\b.h"),
    ];
    let t = AliasTable::build(paths.clone());

    assert_eq!(t.alias_for(&paths[0]), Some("uniq.h"));
    assert_eq!(t.alias_for(&paths[1]), Some(r"x\a.h"));
    assert_eq!(t.alias_for(&paths[2]), Some(r"y\a.h"));
    assert_eq!(t.alias_for(&paths[3]), Some(r"pkgA\..\b.h"));
    assert_eq!(t.alias_for(&paths[4]), Some(r"pkgB\..\b.h"));
}

#[test]
fn deduplicates_repeated_input() {
    let paths = vec![
        rp(Some(0), r"src\foo.cpp"),
        rp(Some(0), r"src\foo.cpp"),
        rp(Some(0), r"src\foo.cpp"),
    ];
    let t = AliasTable::build(paths.clone());
    assert_eq!(t.len(), 1);
    assert_eq!(t.alias_for(&paths[0]), Some("foo.cpp"));
}

#[test]
fn determinism_across_input_orderings() {
    let mut a = vec![
        rp(Some(0), r"x\a.h"),
        rp(Some(0), r"y\a.h"),
        rp(Some(0), r"z\b.h"),
    ];
    let mut b = a.clone();
    b.reverse();

    let table_a = AliasTable::build(a.clone()).into_map();
    let table_b = AliasTable::build(b).into_map();
    assert_eq!(table_a, table_b);

    a.swap(0, 2);
    let table_c = AliasTable::build(a).into_map();
    assert_eq!(table_a, table_c);
}

#[test]
fn different_roots_with_same_relative_path_collide_to_numbered() {
    // Same `path` string under two roots: by design the alias
    // scheme operates on the relative path only and does not
    // disambiguate via root, so these collide all the way to #n.
    let paths = vec![rp(Some(0), r"pkg\thing.h"), rp(Some(1), r"pkg\thing.h")];
    let t = AliasTable::build(paths.clone());

    assert_eq!(t.alias_for(&paths[0]), Some("thing.h#1"));
    assert_eq!(t.alias_for(&paths[1]), Some("thing.h#2"));
}

#[test]
fn absolute_path_uses_drive_token_as_first_segment() {
    // `path_root` emits absolute paths with the drive letter as the
    // leading segment (e.g. "D:\external\foo.h" → segments
    // ["D:", "external", "foo.h"]).
    let paths = vec![
        rp(None, r"D:\external\foo.h"),
        rp(None, r"E:\external\foo.h"),
    ];
    let t = AliasTable::build(paths.clone());

    // Leaves collide → last-two:
    //   "external\foo.h" — also collides → first+leaf:
    //   "D:\..\foo.h" vs "E:\..\foo.h" — unique.
    assert_eq!(t.alias_for(&paths[0]), Some(r"D:\..\foo.h"));
    assert_eq!(t.alias_for(&paths[1]), Some(r"E:\..\foo.h"));
}

#[test]
fn forward_map_matches_reverse_map() {
    let paths = vec![
        rp(Some(0), r"x\a.h"),
        rp(Some(0), r"y\a.h"),
        rp(Some(0), r"z\unique.h"),
    ];
    let t = AliasTable::build(paths.clone());
    for p in &paths {
        let alias = t.alias_for(p).unwrap();
        assert_eq!(t.as_map().get(alias), Some(p));
    }
}

#[test]
fn single_segment_path_uses_leaf_or_numbered() {
    let paths = vec![rp(None, "loose.h"), rp(Some(0), "loose.h")];
    let t = AliasTable::build(paths.clone());

    // last-two, first+leaf all reduce to the leaf — fall through
    // to #n.
    assert_eq!(t.alias_for(&paths[0]), Some("loose.h#1"));
    assert_eq!(t.alias_for(&paths[1]), Some("loose.h#2"));
}
