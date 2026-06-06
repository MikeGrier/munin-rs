// Copyright (c) Michael Grier

//! End-to-end M3 demo: open a `.binlog`, run every CL task through
//! the full pipeline (`cl_cmdline` + `cl_showincludes`), and print
//! a per-translation-unit summary — source file, include-path
//! count, define count, and the resolved include tree shape.
//!
//! Usage:
//!
//! ```text
//! cargo run -p munin-cppbuild --example cl_pipeline_demo -- [--verbose] <binlog>...
//! ```
//!
//! By default emits one line per CL task:
//!
//! ```text
//! === <binlog-stem> ===
//! cl: <alias>  includes=<n>  defines=<n>  top-level-hdrs=<n>  total-hdrs=<n>  max-depth=<n>
//! ```
//!
//! With `--verbose`, also prints the first few top-level included
//! headers per TU.

use std::path::Path;

use munin_cppbuild::{
    project_from_invocation, schema::Project, walk_cl_tasks, walk_link_tasks, walk_projects,
};
use munin_msbuild::BinlogIndex;

fn main() {
    let mut verbose = false;
    let mut binlogs: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        if arg == "--verbose" || arg == "-v" {
            verbose = true;
        } else {
            binlogs.push(arg);
        }
    }
    if binlogs.is_empty() {
        eprintln!(
            "usage: cl_pipeline_demo [--verbose] <binlog>...\n\n\
             For each binlog, runs the full M3 pipeline and prints\n\
             a per-CL-task summary of /showIncludes results."
        );
        std::process::exit(2);
    }

    let mut total_projects = 0usize;
    let mut total_cl_tasks = 0usize;
    let mut total_tus_with_includes = 0usize;
    let mut total_headers = 0usize;
    let mut grand_max_depth = 0usize;

    for binlog in &binlogs {
        match process_one(binlog, verbose) {
            Ok(stats) => {
                total_projects += stats.projects;
                total_cl_tasks += stats.cl_tasks;
                total_tus_with_includes += stats.tus_with_includes;
                total_headers += stats.headers_total;
                grand_max_depth = grand_max_depth.max(stats.max_depth);
            }
            Err(e) => eprintln!("error processing {binlog}: {e}"),
        }
    }

    println!("--- summary ---");
    println!("binlogs:                {}", binlogs.len());
    println!("projects total:         {total_projects}");
    println!("CL tasks total:         {total_cl_tasks}");
    println!("TUs with includes:      {total_tus_with_includes}");
    println!("headers (all TUs):      {total_headers}");
    println!("max include depth:      {grand_max_depth}");
}

struct Stats {
    projects: usize,
    cl_tasks: usize,
    tus_with_includes: usize,
    headers_total: usize,
    max_depth: usize,
}

fn process_one(binlog: &str, verbose: bool) -> Result<Stats, Box<dyn std::error::Error>> {
    let stem = Path::new(binlog)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| binlog.to_string());
    println!("=== {stem} ===");

    let file = std::fs::File::open(binlog)?;
    let index = BinlogIndex::open(file)?;

    let projects = walk_projects(&index)?;
    let cls = walk_cl_tasks(&index)?;
    let links = walk_link_tasks(&index)?;

    let emitted: Vec<Project> = projects
        .iter()
        .map(|inv| project_from_invocation(inv, &cls, &links, &[]))
        .collect();

    let mut stats = Stats {
        projects: emitted.len(),
        cl_tasks: 0,
        tus_with_includes: 0,
        headers_total: 0,
        max_depth: 0,
    };

    for p in &emitted {
        if p.sources.is_empty() {
            continue;
        }
        let project_disp = if let Some(idx) = p.project_path.root {
            format!("[root #{idx}] {}", p.project_path.path)
        } else {
            p.project_path.path.clone()
        };
        println!(
            "project: {project_disp}  platform={}  config={}  sources={}",
            p.platform,
            p.configuration,
            p.sources.len()
        );

        for src in &p.sources {
            stats.cl_tasks += 1;
            let depth = max_depth(&src.includes, 1);
            let total_hdrs = src.included_files.len();
            if total_hdrs > 0 {
                stats.tus_with_includes += 1;
                stats.headers_total += total_hdrs;
                stats.max_depth = stats.max_depth.max(depth);
            }
            println!(
                "  cl: {}  includes={}  defines={}  top-hdrs={}  total-hdrs={}  max-depth={}",
                src.path,
                src.include_paths.len(),
                src.defines.len(),
                src.includes.len(),
                total_hdrs,
                depth
            );
            if verbose && !src.includes.is_empty() {
                for (shown, alias) in src.includes.keys().enumerate() {
                    if shown >= 5 {
                        println!("      ... ({} more)", src.includes.len() - shown);
                        break;
                    }
                    let resolved = p
                        .alias_table
                        .get(alias)
                        .map(|rp| rp.path.as_str())
                        .unwrap_or(alias);
                    println!("      - {alias}  (-> {resolved})");
                }
            }
        }
    }
    println!();
    Ok(stats)
}

fn max_depth(
    map: &std::collections::BTreeMap<String, munin_cppbuild::schema::IncludeNode>,
    current: usize,
) -> usize {
    let mut best = if map.is_empty() { 0 } else { current };
    for node in map.values() {
        let child = max_depth(&node.children, current + 1);
        if child > best {
            best = child;
        }
    }
    best
}
