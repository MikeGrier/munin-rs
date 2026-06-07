// Copyright (c) Michael Grier

//! Unit tests for [`project_from_invocation`].

use super::*;
use crate::{
    schema::{PropertySource, Root, RootedPath},
    walk::ProjectInvocation,
};

fn root(name: &str, path: &str) -> Root {
    Root {
        name: name.to_string(),
        path: path.to_string(),
    }
}

fn invocation(
    project_file: Option<&str>,
    globals: &[(&str, &str)],
    properties: &[(&str, &str)],
) -> ProjectInvocation {
    ProjectInvocation {
        project_context_id: 42,
        project_file: project_file.map(str::to_string),
        global_properties: globals
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        property_list: properties
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

#[test]
fn rooted_project_path_uses_first_matching_root() {
    let roots = vec![root("primary", r"C:\src\product")];
    let inv = invocation(
        Some(r"C:\src\product\app\app.vcxproj"),
        &[("Configuration", "Debug"), ("Platform", "x64")],
        &[],
    );

    let p = project_from_invocation(&inv, &[], &[], &roots);
    assert_eq!(p.project_path.root, Some(0));
    assert_eq!(p.project_path.path, r"app\app.vcxproj");
}

#[test]
fn project_path_outside_roots_is_absolute() {
    let roots = vec![root("primary", r"C:\src\product")];
    let inv = invocation(
        Some(r"D:\other\thing.vcxproj"),
        &[("Configuration", "Release"), ("Platform", "x64")],
        &[],
    );

    let p = project_from_invocation(&inv, &[], &[], &roots);
    assert_eq!(p.project_path.root, None);
    assert_eq!(p.project_path.path, r"D:\other\thing.vcxproj");
}

#[test]
fn missing_project_file_emits_empty_rooted_path() {
    let inv = invocation(None, &[], &[]);
    let p = project_from_invocation(&inv, &[], &[], &[]);
    assert_eq!(
        p.project_path,
        RootedPath {
            root: None,
            path: String::new()
        }
    );
    assert!(p.alias_table.is_empty() || p.alias_table.len() == 1);
}

#[test]
fn platform_and_configuration_come_from_globals() {
    let roots = vec![root("primary", r"C:\src")];
    let inv = invocation(
        Some(r"C:\src\a.vcxproj"),
        &[("Configuration", "Debug"), ("Platform", "ARM64")],
        &[],
    );

    let p = project_from_invocation(&inv, &[], &[], &roots);
    assert_eq!(p.configuration, "Debug");
    assert_eq!(p.platform, "ARM64");
}

#[test]
fn platform_and_configuration_fall_back_to_property_list() {
    let inv = invocation(
        Some("a.vcxproj"),
        &[],
        &[("Configuration", "Release"), ("Platform", "Win32")],
    );
    let p = project_from_invocation(&inv, &[], &[], &[]);
    assert_eq!(p.configuration, "Release");
    assert_eq!(p.platform, "Win32");
}

#[test]
fn missing_platform_and_configuration_become_empty_strings() {
    let inv = invocation(Some("a.vcxproj"), &[], &[]);
    let p = project_from_invocation(&inv, &[], &[], &[]);
    assert_eq!(p.platform, "");
    assert_eq!(p.configuration, "");
}

#[test]
fn global_properties_all_marked_command_line() {
    let inv = invocation(
        Some("a.vcxproj"),
        &[("Configuration", "Debug"), ("Custom", "x")],
        &[],
    );
    let p = project_from_invocation(&inv, &[], &[], &[]);
    assert_eq!(p.global_properties.len(), 2);
    for g in &p.global_properties {
        assert_eq!(g.source, PropertySource::CommandLine);
    }
}

#[test]
fn alias_table_contains_the_project_path() {
    let roots = vec![root("primary", r"C:\src")];
    let inv = invocation(
        Some(r"C:\src\app\app.vcxproj"),
        &[("Configuration", "Debug"), ("Platform", "x64")],
        &[],
    );

    let p = project_from_invocation(&inv, &[], &[], &roots);
    assert_eq!(p.alias_table.len(), 1);
    // Some alias maps to the project path.
    let project_alias = p
        .alias_table
        .iter()
        .find(|(_, v)| **v == p.project_path)
        .map(|(k, _)| k.as_str())
        .expect("project path must be in alias_table");
    assert_eq!(project_alias, "app.vcxproj");
}

#[test]
fn sources_and_outputs_are_empty_in_m2() {
    let inv = invocation(Some("x.vcxproj"), &[], &[]);
    let p = project_from_invocation(&inv, &[], &[], &[]);
    assert!(p.sources.is_empty());
    assert!(p.outputs.is_empty());
}
