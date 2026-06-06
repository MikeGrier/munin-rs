// Copyright (c) Michael Grier

//! CPP-4.6.5 integration test: end-to-end CL batch-mode pipeline.
//!
//! A single `cl.exe` invocation that compiles three sources
//! (`a.cpp`, `b.cpp`, `c.cpp`) — exactly the shape MSBuild emits
//! when the CL task receives multiple `<ClCompile Include="…"/>`
//! items in one batch.
//!
//! cl.exe prints a bare-basename TU-boundary marker before each
//! TU's `/showIncludes` output (D-CPP-SHOWINC2). The pipeline
//! must split the messages on those markers and emit three
//! distinct [`Source`] entries — one per cmdline source — with no
//! header cross-contamination between TUs.

mod common;

use munin_cppbuild::{
    Root, project_from_invocation, walk_cl_tasks, walk_link_tasks, walk_projects,
};
use munin_msbuild::BinlogIndex;

use crate::common::synthetic_cl_task_binlog;

#[test]
fn cl_batch_mode_emits_one_source_per_cmdline_source_with_no_cross_contamination() {
    let roots = vec![Root {
        name: "src".into(),
        path: r"C:\src\product".into(),
    }];

    // One CL invocation, three source files. Shared include path
    // and a single shared define. Compiler is invoked once.
    let cl_cmd = concat!(
        "CL.exe /c ",
        "/I\"C:\\src\\product\\include\" ",
        "/DWIN32 /showIncludes ",
        "a.cpp b.cpp c.cpp"
    );

    // Per-TU header sets. Each TU includes a common header plus
    // a TU-unique one — the unique header must only appear in
    // that TU's resulting Source, never in the other two.
    let cl_msgs = [
        // MSBuild noise that precedes the first marker in real
        // batch-mode output; must not be mistaken for a marker.
        r#"All source files are not up-to-date: missing command TLog "{0}"."#,
        // ── a.cpp ──────────────────────────────────────────
        r"a.cpp",
        r"Note: including file: C:\src\product\include\common.h",
        r"Note: including file: C:\src\product\include\a_only.h",
        // ── b.cpp ──────────────────────────────────────────
        r"b.cpp",
        r"Note: including file: C:\src\product\include\common.h",
        r"Note: including file: C:\src\product\include\b_only.h",
        // ── c.cpp ──────────────────────────────────────────
        r"c.cpp",
        r"Note: including file: C:\src\product\include\common.h",
        r"Note: including file: C:\src\product\include\c_only.h",
    ];

    let bytes = synthetic_cl_task_binlog(cl_cmd, &cl_msgs);
    let index = BinlogIndex::open(std::io::Cursor::new(bytes)).expect("open synthetic binlog");

    let projects = walk_projects(&index).expect("walk_projects");
    let cls = walk_cl_tasks(&index).expect("walk_cl_tasks");
    let links = walk_link_tasks(&index).expect("walk_link_tasks");
    assert_eq!(projects.len(), 1);
    assert_eq!(cls.len(), 1, "one CL task (one cl.exe invocation)");
    assert!(links.is_empty());

    let project = project_from_invocation(&projects[0], &cls, &links, &roots);

    // ── Three Sources, in cmdline order ────────────────────────
    assert_eq!(
        project.sources.len(),
        3,
        "one Source per cmdline source (a.cpp, b.cpp, c.cpp)"
    );

    let resolve = |alias: &str| -> String {
        let rp = project
            .alias_table
            .get(alias)
            .unwrap_or_else(|| panic!("alias {alias} resolves"));
        match rp.root {
            Some(0) => format!(r"<src>\{}", rp.path),
            _ => rp.path.clone(),
        }
    };

    let src_paths: Vec<String> = project.sources.iter().map(|s| resolve(&s.path)).collect();
    assert_eq!(
        src_paths,
        vec![
            "a.cpp".to_string(),
            "b.cpp".to_string(),
            "c.cpp".to_string()
        ],
        "Sources appear in cmdline order"
    );

    // ── Per-TU header attribution ──────────────────────────────
    // Each Source's `included_files` must contain exactly that TU's
    // headers (common.h + the TU-unique header) and nothing else.
    let expected_unique = ["a_only.h", "b_only.h", "c_only.h"];

    for (i, src) in project.sources.iter().enumerate() {
        let header_paths: Vec<String> = src.included_files.iter().map(|a| resolve(a)).collect();
        assert_eq!(
            header_paths,
            vec![
                r"<src>\include\common.h".to_string(),
                format!(r"<src>\include\{}", expected_unique[i]),
            ],
            "Source {i} should carry only its own TU's headers"
        );

        // Same content via the include tree's top-level entries
        // (BTreeMap iteration → alphabetical).
        let mut tree_paths: Vec<String> = src.includes.keys().map(|a| resolve(a)).collect();
        tree_paths.sort();
        let mut expected_sorted = vec![
            r"<src>\include\common.h".to_string(),
            format!(r"<src>\include\{}", expected_unique[i]),
        ];
        expected_sorted.sort();
        assert_eq!(
            tree_paths, expected_sorted,
            "Source {i} include tree should match included_files (sorted)"
        );
    }

    // ── No header cross-contamination ──────────────────────────
    // A TU-unique header must never appear in any other Source.
    for (i, src) in project.sources.iter().enumerate() {
        let resolved: Vec<String> = src.included_files.iter().map(|a| resolve(a)).collect();
        for (j, other_unique) in expected_unique.iter().enumerate() {
            if i == j {
                continue;
            }
            let needle = format!(r"<src>\include\{other_unique}");
            assert!(
                !resolved.contains(&needle),
                "Source {i} ({}) must NOT contain {needle}",
                src_paths[i],
            );
        }
    }

    // ── Cmdline metadata shared across all Sources ─────────────
    // Each Source carries the full CL command line and the same
    // include_paths / defines (this is one invocation).
    for src in &project.sources {
        assert_eq!(src.command_line, cl_cmd);
        assert_eq!(src.include_paths.len(), 1);
        assert_eq!(resolve(&src.include_paths[0]), r"<src>\include");
        assert_eq!(src.defines.len(), 1);
        assert_eq!(src.defines[0].name, "WIN32");
        assert!(src.defines[0].value.is_none());
    }
}
