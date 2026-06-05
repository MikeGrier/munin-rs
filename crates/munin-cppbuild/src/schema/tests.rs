// Copyright (c) Michael Grier

use super::*;
use serde_json::json;

#[test]
fn schema_version_is_one() {
    assert_eq!(SCHEMA_VERSION, 1);
}

#[test]
fn empty_document_round_trips() {
    let doc = CppBuildAnalysis {
        schema_version: SCHEMA_VERSION,
        header: Header {
            tool_version: "0.0.0-test".into(),
            source_binlog: r"C:\fixture.binlog".into(),
            roots: vec![],
        },
        projects: vec![],
    };

    let json = serde_json::to_string(&doc).unwrap();
    let back: CppBuildAnalysis = serde_json::from_str(&json).unwrap();
    assert_eq!(doc, back);
}

#[test]
fn rooted_path_absolute_vs_relative() {
    let rel = RootedPath {
        root: Some(0),
        path: r"src\foo.cpp".into(),
    };
    let abs = RootedPath {
        root: None,
        path: r"D:\external\bar.h".into(),
    };

    let rel_v = serde_json::to_value(&rel).unwrap();
    let abs_v = serde_json::to_value(&abs).unwrap();

    assert_eq!(rel_v, json!({ "root": 0, "path": r"src\foo.cpp" }));
    assert_eq!(abs_v, json!({ "root": null, "path": r"D:\external\bar.h" }));
}

#[test]
fn defines_serialize_with_null_for_bare() {
    let bare = Define {
        name: "FOO".into(),
        value: None,
    };
    let kv = Define {
        name: "BAR".into(),
        value: Some("1".into()),
    };

    assert_eq!(
        serde_json::to_value(&bare).unwrap(),
        json!({ "name": "FOO", "value": null })
    );
    assert_eq!(
        serde_json::to_value(&kv).unwrap(),
        json!({ "name": "BAR", "value": "1" })
    );
}

#[test]
fn property_source_enum_is_snake_case() {
    assert_eq!(
        serde_json::to_string(&PropertySource::CommandLine).unwrap(),
        "\"command_line\""
    );
    assert_eq!(
        serde_json::to_string(&PropertySource::Project).unwrap(),
        "\"project\""
    );
}

#[test]
fn link_input_kind_and_origin_are_snake_case() {
    assert_eq!(
        serde_json::to_string(&LinkInputKind::Obj).unwrap(),
        "\"obj\""
    );
    assert_eq!(
        serde_json::to_string(&LinkInputKind::Lib).unwrap(),
        "\"lib\""
    );
    assert_eq!(
        serde_json::to_string(&LinkInputOrigin::Direct).unwrap(),
        "\"direct\""
    );
    assert_eq!(
        serde_json::to_string(&LinkInputOrigin::Transitive).unwrap(),
        "\"transitive\""
    );
    assert_eq!(
        serde_json::to_string(&LinkInputOrigin::Searched).unwrap(),
        "\"searched\""
    );
}

#[test]
fn include_tree_serializes_recursively() {
    let mut grandchild = BTreeMap::new();
    grandchild.insert(
        "c.h".into(),
        IncludeNode {
            file: "c.h".into(),
            children: BTreeMap::new(),
        },
    );
    let mut child = BTreeMap::new();
    child.insert(
        "b.h".into(),
        IncludeNode {
            file: "b.h".into(),
            children: grandchild,
        },
    );
    let root = IncludeNode {
        file: "a.h".into(),
        children: child,
    };

    let v = serde_json::to_value(&root).unwrap();
    assert_eq!(
        v,
        json!({
            "file": "a.h",
            "children": {
                "b.h": {
                    "file": "b.h",
                    "children": {
                        "c.h": { "file": "c.h", "children": {} }
                    }
                }
            }
        })
    );
}

#[test]
fn full_minimal_project_round_trips() {
    let mut alias_table = BTreeMap::new();
    alias_table.insert(
        "foo.cpp".into(),
        RootedPath {
            root: Some(0),
            path: r"src\foo.cpp".into(),
        },
    );
    alias_table.insert(
        "foo.vcxproj".into(),
        RootedPath {
            root: Some(0),
            path: r"src\foo.vcxproj".into(),
        },
    );

    let project = Project {
        project_path: RootedPath {
            root: Some(0),
            path: r"src\foo.vcxproj".into(),
        },
        platform: "x64".into(),
        configuration: "Release".into(),
        global_properties: vec![GlobalProperty {
            name: "Configuration".into(),
            value: "Release".into(),
            source: PropertySource::CommandLine,
        }],
        alias_table,
        sources: vec![Source {
            path: "foo.cpp".into(),
            command_line: "cl.exe /c foo.cpp".into(),
            include_paths: vec![],
            defines: vec![],
            includes: BTreeMap::new(),
            included_files: vec![],
        }],
        outputs: vec![],
    };

    let doc = CppBuildAnalysis {
        schema_version: SCHEMA_VERSION,
        header: Header {
            tool_version: "0.0.0-test".into(),
            source_binlog: r"C:\foo.binlog".into(),
            roots: vec![Root {
                name: "primary".into(),
                path: r"C:\src\product".into(),
            }],
        },
        projects: vec![project],
    };

    let json = serde_json::to_string_pretty(&doc).unwrap();
    let back: CppBuildAnalysis = serde_json::from_str(&json).unwrap();
    assert_eq!(doc, back);
}
