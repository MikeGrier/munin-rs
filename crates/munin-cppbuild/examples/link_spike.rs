// Copyright (c) Michael Grier

//! CPP-4.1 spike: dump every `Link` task invocation found in one or more
//! `.binlog` files to per-task text files under an output directory.
//!
//! Usage:
//!   link_spike <output-dir> <binlog> [<binlog> ...]
//!
//! For each Link task encountered, writes a file named
//! `<output-dir>/<binlog-stem>__p<projectid>_t<taskid>.txt` containing:
//!
//!   === COMMAND LINE ===
//!   <verbatim task command line, or "(none)">
//!
//!   === MESSAGES (<n>) ===
//!   <one message per line, in stream order>
//!
//! Intended for one-off use only; not a shipping feature.

use std::fs::{File, create_dir_all};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use munin_msbuild::{BinlogEvent, BinlogIndex};

const LINK_TASK_NAME: &str = "Link";

struct LinkInvocation {
    project_context_id: i32,
    task_id: i32,
    command_line: Option<String>,
    messages: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: link_spike <output-dir> <binlog> [<binlog> ...]");
        std::process::exit(2);
    }
    let out_dir = PathBuf::from(&args[1]);
    create_dir_all(&out_dir)?;

    for binlog in &args[2..] {
        let path = PathBuf::from(binlog);
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("binlog");

        let f = File::open(&path)?;
        let index = BinlogIndex::open(f)?;

        let invocations = walk_link_tasks(&index)?;
        println!("{}: {} Link task(s)", binlog, invocations.len());

        for inv in &invocations {
            let out_path = out_dir.join(format!(
                "{}__p{}_t{}.txt",
                stem, inv.project_context_id, inv.task_id
            ));
            write_invocation(&out_path, inv)?;
        }
    }
    Ok(())
}

fn walk_link_tasks(index: &BinlogIndex) -> Result<Vec<LinkInvocation>, Box<dyn std::error::Error>> {
    let mut out: Vec<LinkInvocation> = Vec::new();
    let mut project_stack: Vec<i32> = Vec::new();
    let mut current: Option<LinkInvocation> = None;

    for (i, _meta) in index.iter_meta() {
        let event = match index.get(i)? {
            Some(ev) => ev,
            None => continue,
        };
        match event {
            BinlogEvent::ProjectStarted(ev) => {
                let id = ev
                    .fields
                    .build_event_context
                    .as_ref()
                    .map(|ctx| ctx.project_context_id)
                    .unwrap_or(0);
                project_stack.push(id);
            }
            BinlogEvent::ProjectFinished(_) => {
                project_stack.pop();
            }
            BinlogEvent::TaskStarted(ev)
                if current.is_none() && ev.task_name.as_deref() == Some(LINK_TASK_NAME) =>
            {
                let task_id = ev
                    .fields
                    .build_event_context
                    .as_ref()
                    .map(|ctx| ctx.task_id)
                    .unwrap_or(0);
                let project_context_id = project_stack.last().copied().unwrap_or(0);
                current = Some(LinkInvocation {
                    project_context_id,
                    task_id,
                    command_line: None,
                    messages: Vec::new(),
                });
            }
            BinlogEvent::TaskFinished(ev)
                if ev.task_name.as_deref() == Some(LINK_TASK_NAME) && current.is_some() =>
            {
                if let Some(c) = current.take() {
                    out.push(c);
                }
            }
            BinlogEvent::TaskCommandLine(ev) => {
                if let Some(c) = current.as_mut()
                    && c.command_line.is_none()
                {
                    c.command_line = ev.command_line.clone();
                }
            }
            BinlogEvent::Message(ev) => {
                if let Some(c) = current.as_mut()
                    && let Some(msg) = &ev.fields.message
                {
                    c.messages.push(msg.clone());
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn write_invocation(
    out_path: &Path,
    inv: &LinkInvocation,
) -> Result<(), Box<dyn std::error::Error>> {
    let f = File::create(out_path)?;
    let mut w = BufWriter::new(f);
    writeln!(w, "=== COMMAND LINE ===")?;
    match &inv.command_line {
        Some(s) => writeln!(w, "{}", s)?,
        None => writeln!(w, "(none)")?,
    }
    writeln!(w)?;
    writeln!(w, "=== MESSAGES ({}) ===", inv.messages.len())?;
    for m in &inv.messages {
        writeln!(w, "{}", m)?;
    }
    Ok(())
}
