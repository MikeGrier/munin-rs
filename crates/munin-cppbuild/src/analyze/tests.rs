// Copyright (c) Michael Grier

use std::path::PathBuf;

use super::*;

#[test]
fn longest_common_prefix_of_identical_paths_is_the_path() {
    let a = PathBuf::from(r"C:\src\product\app");
    let b = PathBuf::from(r"C:\src\product\app");
    assert_eq!(
        longest_common_prefix(&a, &b),
        PathBuf::from(r"C:\src\product\app")
    );
}

#[test]
fn longest_common_prefix_of_sibling_dirs_is_their_parent() {
    let a = PathBuf::from(r"C:\src\product\app\foo.vcxproj");
    let b = PathBuf::from(r"C:\src\product\lib\bar.vcxproj");
    assert_eq!(
        longest_common_prefix(&a, &b),
        PathBuf::from(r"C:\src\product")
    );
}

#[test]
fn longest_common_prefix_of_different_drives_is_empty() {
    let a = PathBuf::from(r"C:\src\product");
    let b = PathBuf::from(r"D:\src\product");
    assert_eq!(longest_common_prefix(&a, &b), PathBuf::from(""));
}

#[test]
fn longest_common_prefix_stops_at_first_diverging_component() {
    let a = PathBuf::from(r"C:\src\product\app");
    let b = PathBuf::from(r"C:\src\other\app");
    assert_eq!(longest_common_prefix(&a, &b), PathBuf::from(r"C:\src"));
}
