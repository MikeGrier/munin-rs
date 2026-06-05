// Copyright (c) Michael Grier

//! CPP-3.5 integration tests: end-to-end CL task pipeline.
//!
//! These tests exercise the full chain:
//!
//! 1. Build a synthetic `.binlog` with one `.vcxproj` and one CL
//!    task carrying a realistic `cl.exe` command line and
//!    `/showIncludes` message stream.
//! 2. Open the binlog through [`munin_msbuild::BinlogIndex`].
//! 3. Walk projects and CL tasks.
//! 4. Parse the command line and the `/showIncludes` stream.
//! 5. Root each resolved header against the supplied roots and
//!    build an [`AliasTable`].
//! 6. Assert tree shape, flat-list dedup, first-encounter order,
//!    and alias coverage.

mod common;

use munin_cppbuild::{
    AliasTable, RawIncludeNode, Root, RootedPath, parse_cl_command_line, parse_cl_show_includes,
    to_rooted, walk_cl_tasks, walk_projects,
};
use munin_msbuild::BinlogIndex;

use common::{FIXTURE_PROJECT_PATH, synthetic_cl_task_binlog};

struct Derived {
    command_line: munin_cppbuild::ClCommandLine,
    includes: munin_cppbuild::ShowIncludes,
    aliases: AliasTable,
    rooted: std::collections::HashMap<String, RootedPath>,
}

fn derive(command_line: &str, messages: &[&str], roots: &[Root]) -> Derived {
    let bytes = synthetic_cl_task_binlog(command_line, messages);
    let index = BinlogIndex::open(std::io::Cursor::new(bytes)).expect("open synthetic binlog");

    let projects = walk_projects(&index).expect("walk_projects");
    assert_eq!(projects.len(), 1, "fixture has exactly one project");
    assert_eq!(
        projects[0].project_file.as_deref(),
        Some(FIXTURE_PROJECT_PATH)
    );

    let cls = walk_cl_tasks(&index).expect("walk_cl_tasks");
    assert_eq!(cls.len(), 1, "fixture has exactly one CL task");
    let cl = &cls[0];
    assert_eq!(cl.project_context_id, projects[0].project_context_id);

    let command_line = parse_cl_command_line(cl.command_line.as_deref().unwrap_or(""));
    let includes = parse_cl_show_includes(&cl.messages).expect("english-locale parse");

    // Build an alias table over: the project file + every resolved
    // header in the include tree, all keyed through the rooted
    // representation.
    let mut all_paths: Vec<String> = vec![FIXTURE_PROJECT_PATH.to_string()];
    all_paths.extend(includes.included_files.iter().cloned());

    let mut rooted: std::collections::HashMap<String, RootedPath> =
        std::collections::HashMap::new();
    for raw in &all_paths {
        rooted.insert(raw.clone(), to_rooted(raw, roots));
    }
    let aliases = AliasTable::build(rooted.values().cloned());

    Derived {
        command_line,
        includes,
        aliases,
        rooted,
    }
}

fn flatten_resolved(nodes: &[RawIncludeNode]) -> Vec<String> {
    fn rec(nodes: &[RawIncludeNode], out: &mut Vec<String>) {
        for n in nodes {
            out.push(n.resolved_path.clone());
            rec(&n.children, out);
        }
    }
    let mut out = Vec::new();
    rec(nodes, &mut out);
    out
}

fn alias_for_raw<'a>(d: &'a Derived, raw: &str) -> &'a str {
    let rp = d
        .rooted
        .get(raw)
        .unwrap_or_else(|| panic!("no rooted form recorded for {raw}"));
    d.aliases
        .alias_for(rp)
        .unwrap_or_else(|| panic!("no alias for {raw}"))
}

// ── CPP-3.5 baseline: a.cpp → a.h → b.h → c.h + duplicate ─────────

#[test]
fn baseline_chain_with_duplicate_include() {
    let roots = vec![Root {
        name: "primary".into(),
        path: r"C:\proj".into(),
    }];

    let cmd = r#"CL.exe /c /I"C:\proj\include" /DUNICODE /DWIN32 /showIncludes a.cpp"#;
    // Tree shape:
    //   a.h
    //     b.h
    //       c.h
    //   d.h
    //     b.h        (duplicate — appears in tree again under d.h)
    let msgs = [
        r"Note: including file: C:\proj\include\a.h",
        r"Note: including file:  C:\proj\include\b.h",
        r"Note: including file:   C:\proj\include\c.h",
        r"Note: including file: C:\proj\include\d.h",
        r"Note: including file:  C:\proj\include\b.h",
    ];

    let d = derive(cmd, &msgs, &roots);

    // Command-line extraction.
    assert_eq!(d.command_line.source.as_deref(), Some("a.cpp"));
    assert_eq!(d.command_line.include_paths, vec![r"C:\proj\include"]);
    let define_names: Vec<&str> = d
        .command_line
        .defines
        .iter()
        .map(|x| x.name.as_str())
        .collect();
    assert_eq!(define_names, vec!["UNICODE", "WIN32"]);
    assert!(
        d.command_line
            .other_switches
            .iter()
            .any(|s| s == "/showIncludes")
    );

    // Include tree shape.
    assert_eq!(d.includes.tree.len(), 2);
    assert_eq!(d.includes.tree[0].resolved_path, r"C:\proj\include\a.h");
    assert_eq!(d.includes.tree[0].children.len(), 1);
    assert_eq!(
        d.includes.tree[0].children[0].resolved_path,
        r"C:\proj\include\b.h"
    );
    assert_eq!(d.includes.tree[0].children[0].children.len(), 1);
    assert_eq!(
        d.includes.tree[0].children[0].children[0].resolved_path,
        r"C:\proj\include\c.h"
    );
    assert_eq!(d.includes.tree[1].resolved_path, r"C:\proj\include\d.h");
    assert_eq!(d.includes.tree[1].children.len(), 1);
    assert_eq!(
        d.includes.tree[1].children[0].resolved_path,
        r"C:\proj\include\b.h"
    );

    // Duplicate b.h appears twice in the raw tree…
    let resolved_all = flatten_resolved(&d.includes.tree);
    let b_in_tree = resolved_all.iter().filter(|p| p.ends_with("\\b.h")).count();
    assert_eq!(b_in_tree, 2);

    // …but exactly once in the flat list, in first-encounter order.
    assert_eq!(
        d.includes.included_files,
        vec![
            r"C:\proj\include\a.h",
            r"C:\proj\include\b.h",
            r"C:\proj\include\c.h",
            r"C:\proj\include\d.h",
        ]
    );

    // Alias table covers every resolved header plus the project
    // file. Each path resolves to a unique alias.
    for path in &d.includes.included_files {
        let _ = alias_for_raw(&d, path);
    }
    let _ = alias_for_raw(&d, FIXTURE_PROJECT_PATH);

    let alias_set: std::collections::HashSet<&str> = d
        .includes
        .included_files
        .iter()
        .map(|p| alias_for_raw(&d, p))
        .collect();
    assert_eq!(
        alias_set.len(),
        d.includes.included_files.len(),
        "aliases must be unique across included files"
    );
    // Unique leaves → leaf-name aliases per D-CPP-ALIAS-1.
    assert_eq!(alias_for_raw(&d, r"C:\proj\include\a.h"), "a.h");
    assert_eq!(alias_for_raw(&d, r"C:\proj\include\b.h"), "b.h");
    assert_eq!(alias_for_raw(&d, r"C:\proj\include\c.h"), "c.h");
    assert_eq!(alias_for_raw(&d, r"C:\proj\include\d.h"), "d.h");
}

// ── CPP-3.5 stdlib-style: deep transitive chain with duplicates ───

#[test]
fn realistic_stdlib_chain_iostream_style() {
    let roots = vec![
        Root {
            name: "vc".into(),
            path: r"C:\VC\include".into(),
        },
        Root {
            name: "src".into(),
            path: r"C:\src".into(),
        },
    ];

    // Simulate a typical `#include <iostream>` from MSVC's STL.
    // The chain mimics actual cl.exe output: dozens of CRT headers
    // pulled in via a 6-level-deep nesting, with several duplicates
    // (e.g. <yvals.h>, <vcruntime.h>) showing up under multiple
    // branches.
    let cmd = r#""C:\BuildTools\VC\Tools\MSVC\14.40.33807\bin\HostX64\x64\CL.exe" /c /I"C:\src\include" /DUNICODE /D_UNICODE /DNDEBUG /EHsc /MD /W4 /nologo /showIncludes main.cpp"#;

    let msgs = [
        // <iostream> → <istream> → <ostream> → <ios> → deep CRT chain.
        r"Note: including file: C:\VC\include\iostream",
        r"Note: including file:  C:\VC\include\istream",
        r"Note: including file:   C:\VC\include\ostream",
        r"Note: including file:    C:\VC\include\ios",
        r"Note: including file:     C:\VC\include\xlocnum",
        r"Note: including file:      C:\VC\include\climits",
        r"Note: including file:       C:\VC\include\yvals.h",
        r"Note: including file:        C:\VC\include\vcruntime.h",
        r"Note: including file:         C:\VC\include\sal.h",
        r"Note: including file:        C:\VC\include\crtdefs.h",
        r"Note: including file:         C:\VC\include\corecrt.h",
        r"Note: including file:          C:\VC\include\sal.h", // dup
        r"Note: including file:       C:\VC\include\limits.h",
        r"Note: including file:      C:\VC\include\cstdio",
        r"Note: including file:       C:\VC\include\yvals_core.h",
        r"Note: including file:        C:\VC\include\vcruntime.h", // dup
        r"Note: including file:       C:\VC\include\stdio.h",
        r"Note: including file:     C:\VC\include\xiosbase",
        r"Note: including file:      C:\VC\include\xlocale",
        r"Note: including file:       C:\VC\include\cstring",
        r"Note: including file:        C:\VC\include\string.h",
        r"Note: including file:        C:\VC\include\vcruntime_string.h",
        r"Note: including file:     C:\VC\include\streambuf",
        r"Note: including file:    C:\VC\include\system_error",
        r"Note: including file:     C:\VC\include\__msvc_system_error_abi.hpp",
        r"Note: including file:    C:\VC\include\stdexcept",
        r"Note: including file:     C:\VC\include\exception",
        // Second top-level: project header that also pulls in some
        // CRT pieces (vcruntime.h shows up a 3rd structural time).
        r"Note: including file: C:\src\include\app.h",
        r"Note: including file:  C:\VC\include\string",
        r"Note: including file:   C:\VC\include\xstring",
        r"Note: including file:    C:\VC\include\vcruntime.h", // dup
        r"Note: including file:  C:\VC\include\vector",
        // Third top-level with no nesting.
        r"Note: including file: C:\src\include\app_config.h",
    ];

    let d = derive(cmd, &msgs, &roots);

    // Command-line surface.
    assert_eq!(d.command_line.source.as_deref(), Some("main.cpp"));
    assert_eq!(d.command_line.include_paths, vec![r"C:\src\include"]);
    let define_names: Vec<&str> = d
        .command_line
        .defines
        .iter()
        .map(|x| x.name.as_str())
        .collect();
    assert_eq!(define_names, vec!["UNICODE", "_UNICODE", "NDEBUG"]);

    // Top-level shape: three depth-1 includes in source order.
    assert_eq!(d.includes.tree.len(), 3);
    let top: Vec<&str> = d
        .includes
        .tree
        .iter()
        .map(|n| n.resolved_path.as_str())
        .collect();
    assert_eq!(
        top,
        vec![
            r"C:\VC\include\iostream",
            r"C:\src\include\app.h",
            r"C:\src\include\app_config.h",
        ]
    );

    // The raw tree preserves duplicates structurally.
    let resolved_all = flatten_resolved(&d.includes.tree);
    let vcrt_in_tree = resolved_all
        .iter()
        .filter(|p| p.ends_with("\\vcruntime.h"))
        .count();
    assert_eq!(
        vcrt_in_tree, 3,
        "vcruntime.h appears once under yvals.h, once under \
         cstdio→yvals_core.h, and once under app.h→string→xstring \
         (3 structural occurrences)"
    );
    let sal_in_tree = resolved_all
        .iter()
        .filter(|p| p.ends_with("\\sal.h"))
        .count();
    assert_eq!(
        sal_in_tree, 2,
        "sal.h appears once via vcruntime, once via corecrt"
    );

    // The flat list deduplicates and is in first-encounter order.
    let flat = &d.includes.included_files;
    let unique_count = flat.iter().collect::<std::collections::HashSet<_>>().len();
    assert_eq!(flat.len(), unique_count, "flat list must be deduplicated");

    // First three entries match the depth-first first-encounter
    // walk: iostream, istream, ostream.
    assert_eq!(flat[0], r"C:\VC\include\iostream");
    assert_eq!(flat[1], r"C:\VC\include\istream");
    assert_eq!(flat[2], r"C:\VC\include\ostream");
    // Last entry is the trailing top-level include.
    assert_eq!(flat.last().unwrap(), r"C:\src\include\app_config.h");

    // No malformed lines in this well-formed input.
    assert_eq!(d.includes.malformed_message_count, 0);

    // Alias coverage: every resolved header has a unique alias.
    let alias_set: std::collections::HashSet<&str> =
        flat.iter().map(|p| alias_for_raw(&d, p)).collect();
    assert_eq!(
        alias_set.len(),
        flat.len(),
        "aliases must be unique across {} resolved headers",
        flat.len()
    );

    // Spot-check a couple of aliases. Unique leaves get leaf names.
    assert_eq!(alias_for_raw(&d, r"C:\VC\include\iostream"), "iostream");
    assert_eq!(
        alias_for_raw(&d, r"C:\VC\include\vcruntime.h"),
        "vcruntime.h"
    );
    assert_eq!(alias_for_raw(&d, r"C:\src\include\app.h"), "app.h");

    // Sanity: this fixture's flat-list size is in the dozens, which
    // is the realistic-stdlib scale CPP-3.5 calls for.
    assert!(flat.len() >= 25, "flat list has {} headers", flat.len());
}
