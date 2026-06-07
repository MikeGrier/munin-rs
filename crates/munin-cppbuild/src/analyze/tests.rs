// Copyright (c) Michael Grier

use super::*;

#[test]
fn longest_common_prefix_of_identical_paths_is_the_path() {
    let a = r"C:\src\product\app";
    let b = r"C:\src\product\app";
    assert_eq!(longest_common_prefix(a, b), r"C:\src\product\app");
}

#[test]
fn longest_common_prefix_of_sibling_dirs_is_their_parent() {
    let a = r"C:\src\product\app\foo.vcxproj";
    let b = r"C:\src\product\lib\bar.vcxproj";
    assert_eq!(longest_common_prefix(a, b), r"C:\src\product");
}

#[test]
fn longest_common_prefix_of_different_drives_is_empty() {
    let a = r"C:\src\product";
    let b = r"D:\src\product";
    assert_eq!(longest_common_prefix(a, b), "");
}

#[test]
fn longest_common_prefix_stops_at_first_diverging_component() {
    let a = r"C:\src\product\app";
    let b = r"C:\src\other\app";
    assert_eq!(longest_common_prefix(a, b), r"C:\src");
}

#[test]
fn longest_common_prefix_is_case_insensitive_for_ascii_components() {
    // Windows paths that differ only by ASCII case must share a
    // common prefix; case-sensitive comparison would incorrectly
    // produce an empty result.
    let a = r"C:\Src\Product\app";
    let b = r"c:\src\product\lib";
    assert_eq!(longest_common_prefix(a, b), r"C:\Src\Product");
}

#[test]
fn longest_common_prefix_accepts_forward_slashes() {
    let a = "C:/src/product/app";
    let b = r"C:\src\product\lib";
    assert_eq!(longest_common_prefix(a, b), r"C:\src\product");
}
