// Copyright (c) Michael Grier

//! CPP-4.5 integration test: end-to-end Link task pipeline with
//! explicit verification that **unused `AdditionalDependencies`**
//! libraries are surfaced.
//!
//! Builds a synthetic binlog containing one project, one CL task,
//! and one Link task. The Link task's command line lists three
//! libraries plus an `.obj`; the verbose message stream marks two of
//! those libraries as unused via the `Unused libraries:` block.
//! The test asserts that:
//!
//! 1. `outputs[0].dropped[]` contains both unused libraries with
//!    `reason == "unused"`.
//! 2. `outputs[0].inputs[]` carries `referenced = true` for the
//!    used lib and `referenced = false` for the two unused libs.
//! 3. The `.obj` input is `referenced = true`.
//! 4. Every path appearing in `inputs[].path`, `dropped[].path`,
//!    and `path` resolves through `alias_table` to a `RootedPath`.

mod common;

use munin_cppbuild::{
    Root, project_from_invocation, schema::LinkInputKind, walk_cl_tasks, walk_link_tasks,
    walk_projects,
};
use munin_msbuild::BinlogIndex;

use crate::common::synthetic_cl_link_binlog;

#[test]
fn unused_additional_libraries_are_surfaced_in_dropped_and_inputs() {
    let roots = vec![Root {
        name: "src".into(),
        path: r"C:\src\product".into(),
    }];

    let cl_cmd = r#"CL.exe /c /I"C:\src\product\include" /DWIN32 /showIncludes main.cpp"#;
    let cl_msgs = [r"Note: including file: C:\src\product\include\main.h"];

    // Link command line names three libraries (used, unused1,
    // unused2) and one object (main.obj). The unused-libs block
    // below marks unused1 / unused2 as never referenced.
    let link_cmd = concat!(
        "link.exe ",
        "/OUT:\"C:\\src\\product\\bin\\app.exe\" ",
        "/LIBPATH:\"C:\\src\\product\\libs\" ",
        "C:\\src\\product\\libs\\used.lib ",
        "C:\\src\\product\\libs\\unused1.lib ",
        "C:\\src\\product\\libs\\unused2.lib ",
        "C:\\src\\product\\obj\\main.obj"
    );
    let link_msgs = [
        r"Starting pass 1",
        r"Processed /DEFAULTLIB:msvcrt",
        r"    Searching C:\src\product\libs\used.lib:",
        r"        Loaded used_member.obj",
        r"    Searching C:\src\product\libs\unused1.lib:",
        r"    Searching C:\src\product\libs\unused2.lib:",
        r"Finished pass 1",
        r"Unused libraries:",
        r"  C:\src\product\libs\unused1.lib",
        r"  C:\src\product\libs\unused2.lib",
    ];

    let bytes = synthetic_cl_link_binlog(cl_cmd, &cl_msgs, link_cmd, &link_msgs);
    let index = BinlogIndex::open(std::io::Cursor::new(bytes)).expect("open synthetic binlog");

    let projects = walk_projects(&index).expect("walk_projects");
    let cls = walk_cl_tasks(&index).expect("walk_cl_tasks");
    let links = walk_link_tasks(&index).expect("walk_link_tasks");
    assert_eq!(projects.len(), 1);
    assert_eq!(cls.len(), 1);
    assert_eq!(links.len(), 1);

    let project = project_from_invocation(&projects[0], &cls, &links, &roots);

    // ── Outputs basic shape ────────────────────────────────────
    assert_eq!(project.outputs.len(), 1, "one Link task → one output");
    let out = &project.outputs[0];

    // `outputs[0].path` is an alias. Resolve it through the alias
    // table to verify it points at app.exe under the `src` root.
    let out_rp = project
        .alias_table
        .get(&out.path)
        .expect("output path alias resolves");
    assert_eq!(out_rp.root, Some(0));
    assert_eq!(out_rp.path, r"bin\app.exe");

    // ── Dropped libs ───────────────────────────────────────────
    assert_eq!(
        out.dropped.len(),
        2,
        "Unused libraries: block has two entries"
    );
    let dropped_paths: Vec<&str> = out
        .dropped
        .iter()
        .map(|d| {
            let rp = project
                .alias_table
                .get(&d.path)
                .expect("dropped alias resolves");
            rp.path.as_str()
        })
        .collect();
    assert!(
        dropped_paths.iter().any(|p| p.ends_with(r"unused1.lib")),
        "unused1.lib must appear in dropped[]: got {:?}",
        dropped_paths
    );
    assert!(
        dropped_paths.iter().any(|p| p.ends_with(r"unused2.lib")),
        "unused2.lib must appear in dropped[]: got {:?}",
        dropped_paths
    );
    for d in &out.dropped {
        assert_eq!(d.reason, "unused");
    }

    // ── Inputs reference inference ─────────────────────────────
    let find_input = |suffix: &str| {
        out.inputs
            .iter()
            .find(|i| {
                let rp = project
                    .alias_table
                    .get(&i.path)
                    .expect("input alias resolves");
                rp.path.ends_with(suffix)
            })
            .unwrap_or_else(|| panic!("missing input ending with {suffix}: {:?}", out.inputs))
    };

    let used = find_input("used.lib");
    assert_eq!(used.kind, LinkInputKind::Lib);
    assert!(used.referenced, "used.lib must be referenced");

    let unused1 = find_input("unused1.lib");
    assert_eq!(unused1.kind, LinkInputKind::Lib);
    assert!(
        !unused1.referenced,
        "unused1.lib must NOT be referenced — it was in Unused libraries:"
    );

    let unused2 = find_input("unused2.lib");
    assert!(
        !unused2.referenced,
        "unused2.lib must NOT be referenced — it was in Unused libraries:"
    );

    let main_obj = find_input("main.obj");
    assert_eq!(main_obj.kind, LinkInputKind::Obj);
    assert!(main_obj.referenced, ".obj inputs are always referenced");

    // ── Every alias in outputs is also a key in alias_table ────
    for input in &out.inputs {
        assert!(
            project.alias_table.contains_key(&input.path),
            "input alias {} not in alias_table",
            input.path
        );
    }
    for drop in &out.dropped {
        assert!(
            project.alias_table.contains_key(&drop.path),
            "dropped alias {} not in alias_table",
            drop.path
        );
    }
    assert!(project.alias_table.contains_key(&out.path));
}

#[test]
fn link_output_records_command_line_verbatim() {
    let link_cmd = "link.exe /OUT:foo.exe main.obj";
    let bytes = synthetic_cl_link_binlog("CL.exe /c x.cpp", &[], link_cmd, &[]);
    let index = BinlogIndex::open(std::io::Cursor::new(bytes)).expect("open");
    let projects = walk_projects(&index).unwrap();
    let cls = walk_cl_tasks(&index).unwrap();
    let links = walk_link_tasks(&index).unwrap();
    let project = project_from_invocation(&projects[0], &cls, &links, &[]);

    assert_eq!(project.outputs[0].command_line, link_cmd);
}

#[test]
fn defaultlib_without_searching_scope_surfaces_as_synthetic_input() {
    let link_cmd = "link.exe /OUT:foo.exe main.obj";
    let link_msgs = [r"Processed /DEFAULTLIB:ghost"];
    let bytes = synthetic_cl_link_binlog("CL.exe /c x.cpp", &[], link_cmd, &link_msgs);
    let index = BinlogIndex::open(std::io::Cursor::new(bytes)).expect("open");
    let projects = walk_projects(&index).unwrap();
    let cls = walk_cl_tasks(&index).unwrap();
    let links = walk_link_tasks(&index).unwrap();
    let project = project_from_invocation(&projects[0], &cls, &links, &[]);

    let out = &project.outputs[0];
    let aliases = &project.alias_table;
    let ghost = out
        .inputs
        .iter()
        .find(|i| {
            aliases
                .get(&i.path)
                .map(|rp| rp.path == "defaultlib:ghost")
                .unwrap_or(false)
        })
        .expect("synthetic defaultlib:ghost input must be present");
    assert!(!ghost.referenced);
}
