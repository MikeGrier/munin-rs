// Copyright (c) Michael Grier

//! CPP-1.5 — end-to-end check that a hand-assembled project
//! description, fed through [`to_rooted`] and
//! [`AliasTable::build`], produces the JSON shape that
//! D-CPPSCHEMA-1 specifies.

use std::collections::BTreeMap;

use munin_cppbuild::{
    AliasTable, CppBuildAnalysis, Define, GlobalProperty, Header, IncludeNode, Project,
    PropertySource, Root, SCHEMA_VERSION, Source, to_rooted,
};
use serde_json::json;

fn primary_root() -> Root {
    Root {
        name: "primary".into(),
        path: r"C:\src\product".into(),
    }
}

fn nuget_root() -> Root {
    Root {
        name: "nuget_cache".into(),
        path: r"D:\nuget".into(),
    }
}

#[test]
fn assemble_project_with_aliases_and_serialize() {
    let roots = vec![primary_root(), nuget_root()];

    // Project file and one .cpp under the primary root.
    let project_file = to_rooted(r"C:\src\product\foo\foo.vcxproj", &roots);
    let main_cpp = to_rooted(r"C:\src\product\foo\src\main.cpp", &roots);

    // Two header files with the same leaf in different intra-project
    // directories → expect last-two disambiguation.
    let common_a = to_rooted(r"C:\src\product\foo\inc\a\common.h", &roots);
    let common_b = to_rooted(r"C:\src\product\foo\inc\b\common.h", &roots);

    // A NuGet header → expect leaf alias since "thing.h" is unique.
    let nuget_h = to_rooted(r"D:\nuget\pkgA\1.0\include\thing.h", &roots);

    // Sanity: rooted forms are what we expect.
    assert_eq!(main_cpp.root, Some(0));
    assert_eq!(main_cpp.path, r"foo\src\main.cpp");
    assert_eq!(nuget_h.root, Some(1));
    assert_eq!(nuget_h.path, r"pkgA\1.0\include\thing.h");

    // Build alias table over every path that will appear in the
    // project's sources.
    let alias_input = vec![
        project_file.clone(),
        main_cpp.clone(),
        common_a.clone(),
        common_b.clone(),
        nuget_h.clone(),
    ];
    let aliases = AliasTable::build(alias_input);

    let a_main = aliases.alias_for(&main_cpp).unwrap().to_string();
    let a_common_a = aliases.alias_for(&common_a).unwrap().to_string();
    let a_common_b = aliases.alias_for(&common_b).unwrap().to_string();
    let a_thing = aliases.alias_for(&nuget_h).unwrap().to_string();
    let a_vcxproj = aliases.alias_for(&project_file).unwrap().to_string();

    assert_eq!(a_main, "main.cpp");
    assert_eq!(a_common_a, r"a\common.h");
    assert_eq!(a_common_b, r"b\common.h");
    assert_eq!(a_thing, "thing.h");
    assert_eq!(a_vcxproj, "foo.vcxproj");

    // Assemble the include tree for main.cpp:
    //   main.cpp
    //   └── a\common.h
    //       └── thing.h
    //   └── b\common.h
    let mut common_a_children = BTreeMap::new();
    common_a_children.insert(
        a_thing.clone(),
        IncludeNode {
            file: a_thing.clone(),
            children: BTreeMap::new(),
        },
    );
    let mut includes = BTreeMap::new();
    includes.insert(
        a_common_a.clone(),
        IncludeNode {
            file: a_common_a.clone(),
            children: common_a_children,
        },
    );
    includes.insert(
        a_common_b.clone(),
        IncludeNode {
            file: a_common_b.clone(),
            children: BTreeMap::new(),
        },
    );

    let source = Source {
        path: a_main.clone(),
        command_line: r"cl.exe /c /Iinc\a /Iinc\b /DFOO /DBAR=1 src\main.cpp".into(),
        include_paths: vec![],
        defines: vec![
            Define {
                name: "FOO".into(),
                value: None,
            },
            Define {
                name: "BAR".into(),
                value: Some("1".into()),
            },
        ],
        includes,
        included_files: vec![a_common_a.clone(), a_thing.clone(), a_common_b.clone()],
    };

    let project = Project {
        project_path: project_file.clone(),
        platform: "x64".into(),
        configuration: "Release".into(),
        global_properties: vec![GlobalProperty {
            name: "Configuration".into(),
            value: "Release".into(),
            source: PropertySource::CommandLine,
        }],
        alias_table: aliases.into_map(),
        sources: vec![source],
        outputs: vec![],
    };

    let doc = CppBuildAnalysis {
        schema_version: SCHEMA_VERSION,
        header: Header {
            tool_version: env!("CARGO_PKG_VERSION").into(),
            source_binlog: r"C:\fixture\foo.binlog".into(),
            roots,
        },
        projects: vec![project],
    };

    // ── Shape assertions on the emitted JSON ───────────────────
    let value = serde_json::to_value(&doc).unwrap();

    assert_eq!(value["schema_version"], json!(SCHEMA_VERSION));
    assert_eq!(value["header"]["roots"][0]["name"], json!("primary"));
    assert_eq!(value["header"]["roots"][1]["name"], json!("nuget_cache"));

    let project_v = &value["projects"][0];
    assert_eq!(
        project_v["project_path"],
        json!({ "root": 0, "path": r"foo\foo.vcxproj" })
    );

    // Alias table entries.
    let table = &project_v["alias_table"];
    assert_eq!(
        table[&a_main],
        json!({ "root": 0, "path": r"foo\src\main.cpp" })
    );
    assert_eq!(
        table[&a_thing],
        json!({ "root": 1, "path": r"pkgA\1.0\include\thing.h" })
    );
    assert_eq!(
        table[&a_common_a],
        json!({ "root": 0, "path": r"foo\inc\a\common.h" })
    );
    assert_eq!(
        table[&a_common_b],
        json!({ "root": 0, "path": r"foo\inc\b\common.h" })
    );

    // Includes tree uses the aliases.
    let src = &project_v["sources"][0];
    assert_eq!(src["path"], json!(a_main));
    assert!(src["includes"].get(&a_common_a).is_some());
    assert_eq!(
        src["includes"][&a_common_a]["children"][&a_thing]["file"],
        json!(a_thing)
    );

    // Defines round-trip with the right null-vs-string discrimination.
    assert_eq!(src["defines"][0], json!({ "name": "FOO", "value": null }));
    assert_eq!(src["defines"][1], json!({ "name": "BAR", "value": "1" }));

    // Full round-trip preserves equality.
    let back: CppBuildAnalysis = serde_json::from_value(value).unwrap();
    assert_eq!(back, doc);
}

#[test]
fn nuget_collision_falls_through_to_first_plus_leaf() {
    let roots = vec![primary_root(), nuget_root()];

    // Two NuGet packages exporting a same-named header at the same
    // sub-path → leaf collides, last-two collides, first+leaf
    // disambiguates.
    let pkga_h = to_rooted(r"D:\nuget\pkgA\1.0\include\thing.h", &roots);
    let pkgb_h = to_rooted(r"D:\nuget\pkgB\2.0\include\thing.h", &roots);

    let aliases = AliasTable::build(vec![pkga_h.clone(), pkgb_h.clone()]);
    assert_eq!(aliases.alias_for(&pkga_h), Some(r"pkgA\..\thing.h"));
    assert_eq!(aliases.alias_for(&pkgb_h), Some(r"pkgB\..\thing.h"));

    // And again the table is what the schema expects.
    let map = aliases.into_map();
    assert_eq!(map.get(r"pkgA\..\thing.h"), Some(&pkga_h));
    assert_eq!(map.get(r"pkgB\..\thing.h"), Some(&pkgb_h));
}
