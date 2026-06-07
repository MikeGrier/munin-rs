// Copyright (c) Michael Grier

//! End-to-end M4 demo: open a `.binlog`, run the full project /
//! CL / Link pipeline through `project_from_invocation`, and print
//! every project's outputs — with special attention to the
//! `Unused libraries:` block surfaced as `dropped[reason=unused]`.
//!
//! Usage:
//!
//! ```text
//! cargo run -p munin-cppbuild --example link_pipeline_demo -- <binlog>...
//! ```
//!
//! For each binlog, prints one block per project:
//!
//! ```text
//! === <binlog-stem> ===
//! project: <rooted-path>  platform=<...>  config=<...>
//!   output: <alias>  (-> <rooted-path>)
//!     inputs:   <referenced count>/<total> referenced
//!     dropped:  <count> unused libraries
//!       - <alias>  (-> <rooted-path>)
//!       ...
//! ```

use std::path::Path;

use munin_cppbuild::{
    project_from_invocation, schema::Project, walk_cl_tasks, walk_link_tasks, walk_projects,
};
use munin_msbuild::BinlogIndex;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "usage: link_pipeline_demo <binlog>...\n\n\
             For each binlog, runs the full M4 pipeline and prints\n\
             every Output, highlighting the Unused-libraries set."
        );
        std::process::exit(2);
    }

    for binlog in &args {
        if let Err(e) = process_one(binlog) {
            eprintln!("error processing {binlog}: {e}");
        }
    }
}

fn process_one(binlog: &str) -> Result<(), Box<dyn std::error::Error>> {
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

    // No roots — emit absolute paths.
    let emitted: Vec<Project> = projects
        .iter()
        .map(|inv| project_from_invocation(inv, &cls, &links, &[]))
        .collect();

    for p in &emitted {
        let project_disp = if let Some(idx) = p.project_path.root {
            format!("[root #{idx}] {}", p.project_path.path)
        } else {
            p.project_path.path.clone()
        };
        println!(
            "project: {project_disp}  platform={}  config={}",
            p.platform, p.configuration
        );

        for out in &p.outputs {
            let out_target = p
                .alias_table
                .get(&out.path)
                .map(|rp| rp.path.as_str())
                .unwrap_or(out.path.as_str());
            println!("  output: {}  (-> {})", out.path, out_target);

            let total = out.inputs.len();
            let referenced = out.inputs.iter().filter(|i| i.referenced).count();
            println!("    inputs:   {referenced}/{total} referenced");

            if out.dropped.is_empty() {
                println!("    dropped:  none");
            } else {
                println!("    dropped:  {} unused libraries", out.dropped.len());
                for d in &out.dropped {
                    let resolved = p
                        .alias_table
                        .get(&d.path)
                        .map(|rp| rp.path.as_str())
                        .unwrap_or(d.path.as_str());
                    println!("      - {}  (-> {})  reason={}", d.path, resolved, d.reason);
                }
            }
        }
    }
    println!();
    Ok(())
}
