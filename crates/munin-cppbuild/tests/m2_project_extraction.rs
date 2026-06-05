// Copyright (c) Michael Grier

//! M2 pipeline integration test.
//!
//! Wires together `BinlogIndex::open` → `walk_projects` →
//! `project_from_invocation` against the synthetic fixture from
//! `tests/common/mod.rs` and asserts the per-project metadata
//! (project path, platform, configuration, global properties).
//!
//! This is the end-to-end coverage for milestone 2; CPP-3 and CPP-4
//! will extend the fixture with CL / Link tasks and assert sources /
//! outputs.

mod common;

use std::io::Cursor;

use munin_cppbuild::{
    Project, ProjectInvocation, Root, project_from_invocation, schema::PropertySource,
    walk_projects,
};
use munin_msbuild::BinlogIndex;

use crate::common::{FIXTURE_PROJECT_PATH, synthetic_vcxproj_binlog};

/// Run the full M2 pipeline against the synthetic fixture.
fn run_pipeline(roots: &[Root]) -> (Vec<ProjectInvocation>, Vec<Project>) {
    let bytes = synthetic_vcxproj_binlog();
    let index = BinlogIndex::open(Cursor::new(&bytes)).expect("fixture should open");
    let invocations = walk_projects(&index).expect("walk should succeed");
    let projects: Vec<Project> = invocations
        .iter()
        .map(|inv| project_from_invocation(inv, roots))
        .collect();
    (invocations, projects)
}

#[test]
fn fixture_yields_two_project_invocations() {
    let (invocations, projects) = run_pipeline(&[]);
    assert_eq!(invocations.len(), 2);
    assert_eq!(projects.len(), 2);
}

#[test]
fn project_invocations_have_distinct_context_ids() {
    let (invocations, _) = run_pipeline(&[]);
    assert_eq!(invocations[0].project_context_id, 10);
    assert_eq!(invocations[1].project_context_id, 11);
}

#[test]
fn project_invocations_report_the_same_project_file() {
    let (invocations, _) = run_pipeline(&[]);
    assert_eq!(
        invocations[0].project_file.as_deref(),
        Some(FIXTURE_PROJECT_PATH)
    );
    assert_eq!(
        invocations[1].project_file.as_deref(),
        Some(FIXTURE_PROJECT_PATH)
    );
}

#[test]
fn debug_and_release_configurations_extracted() {
    let (_, projects) = run_pipeline(&[]);
    assert_eq!(projects[0].configuration, "Debug");
    assert_eq!(projects[0].platform, "x64");
    assert_eq!(projects[1].configuration, "Release");
    assert_eq!(projects[1].platform, "x64");
}

#[test]
fn project_path_resolves_against_supplied_root() {
    let roots = vec![Root {
        name: "primary".to_string(),
        path: r"C:\src\product".to_string(),
    }];
    let (_, projects) = run_pipeline(&roots);

    for p in &projects {
        assert_eq!(p.project_path.root, Some(0));
        assert_eq!(p.project_path.path, r"app\app.vcxproj");
    }
}

#[test]
fn project_path_is_absolute_when_no_root_matches() {
    let (_, projects) = run_pipeline(&[]);
    for p in &projects {
        assert_eq!(p.project_path.root, None);
        assert_eq!(p.project_path.path, FIXTURE_PROJECT_PATH);
    }
}

#[test]
fn global_properties_are_emitted_with_command_line_source() {
    let (_, projects) = run_pipeline(&[]);
    for p in &projects {
        assert_eq!(p.global_properties.len(), 2);
        for g in &p.global_properties {
            assert_eq!(g.source, PropertySource::CommandLine);
            assert!(g.name == "Configuration" || g.name == "Platform");
        }
    }
}

#[test]
fn alias_table_is_populated_with_project_alias() {
    let roots = vec![Root {
        name: "primary".to_string(),
        path: r"C:\src\product".to_string(),
    }];
    let (_, projects) = run_pipeline(&roots);

    for p in &projects {
        assert!(p.alias_table.contains_key("app.vcxproj"));
        let entry = &p.alias_table["app.vcxproj"];
        assert_eq!(entry.root, Some(0));
        assert_eq!(entry.path, r"app\app.vcxproj");
    }
}

#[test]
fn sources_and_outputs_are_empty_for_m2_pipeline() {
    let (_, projects) = run_pipeline(&[]);
    for p in &projects {
        assert!(p.sources.is_empty());
        assert!(p.outputs.is_empty());
    }
}

#[test]
fn m2_project_serializes_as_valid_json() {
    let (_, projects) = run_pipeline(&[]);
    let json = serde_json::to_string_pretty(&projects).expect("serialize");
    // Round-trip via Value to validate the document is well-formed.
    let _round_trip: serde_json::Value = serde_json::from_str(&json).expect("re-parse");
    assert!(json.contains("\"Configuration\""));
    assert!(json.contains("\"Platform\""));
    assert!(json.contains("\"command_line\""));
}
