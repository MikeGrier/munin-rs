// Copyright (c) Michael Grier

//! Subcommand dispatch.

use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::Path,
};

use munin_msbuild::{jsonlog, BinlogIndex};

use crate::{
    cli::{Cli, Command, DumpArgs, PackArgs, RedactArgs},
    output::OutputSink,
    redact_args,
};

/// Top-level dispatch. Errors are returned as human-readable strings
/// (the binary's `main` prints them to stderr).
pub fn dispatch<S: OutputSink>(cli: Cli, sink: &mut S) -> Result<(), String> {
    match cli.command {
        Command::Dump(args) => run_dump(args, sink),
        Command::Pack(args) => run_pack(args, sink),
    }
}

fn run_dump<S: OutputSink>(args: DumpArgs, sink: &mut S) -> Result<(), String> {
    let index = load_and_redact_binlog(&args.input, &args.redact)?;
    write_jsonlog(&index, args.output.as_deref(), args.pretty, sink)
}

fn run_pack<S: OutputSink>(args: PackArgs, sink: &mut S) -> Result<(), String> {
    let index = load_and_redact_jsonlog(&args.input, &args.redact)?;
    write_binlog(&index, args.output.as_deref(), sink)
}

fn load_and_redact_binlog(input: &Path, redact: &RedactArgs) -> Result<BinlogIndex, String> {
    let f = File::open(input).map_err(|e| format!("open {}: {e}", input.display()))?;
    let mut index = BinlogIndex::open(BufReader::new(f))
        .map_err(|e| format!("read binlog {}: {e}", input.display()))?;
    if redact_args::is_active(redact) {
        let r = redact_args::build_redactor(redact)?;
        r.apply(&mut index);
    }
    Ok(index)
}

fn load_and_redact_jsonlog(input: &Path, redact: &RedactArgs) -> Result<BinlogIndex, String> {
    let f = File::open(input).map_err(|e| format!("open {}: {e}", input.display()))?;
    let mut index = BinlogIndex::open_json(BufReader::new(f))
        .map_err(|e| format!("read jsonlog {}: {e}", input.display()))?;
    if redact_args::is_active(redact) {
        let r = redact_args::build_redactor(redact)?;
        r.apply(&mut index);
    }
    Ok(index)
}

fn write_jsonlog<S: OutputSink>(
    index: &BinlogIndex,
    output: Option<&Path>,
    pretty: bool,
    sink: &mut S,
) -> Result<(), String> {
    let mut bytes = Vec::new();
    if pretty {
        jsonlog::dump_index_pretty(index, &mut bytes)
            .map_err(|e| format!("encode jsonlog: {e}"))?;
    } else {
        jsonlog::dump_index(index, &mut bytes).map_err(|e| format!("encode jsonlog: {e}"))?;
    }
    write_output(output, &bytes, sink)
}

fn write_binlog<S: OutputSink>(
    index: &BinlogIndex,
    output: Option<&Path>,
    sink: &mut S,
) -> Result<(), String> {
    let mut bytes = Vec::new();
    index
        .write_binlog(&mut bytes)
        .map_err(|e| format!("encode binlog: {e}"))?;
    write_output(output, &bytes, sink)
}

fn write_output<S: OutputSink>(
    output: Option<&Path>,
    bytes: &[u8],
    sink: &mut S,
) -> Result<(), String> {
    match output {
        Some(path) => {
            let f = File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
            let mut w = BufWriter::new(f);
            w.write_all(bytes)
                .map_err(|e| format!("write {}: {e}", path.display()))?;
            w.flush()
                .map_err(|e| format!("flush {}: {e}", path.display()))?;
            Ok(())
        }
        None => sink
            .out()
            .write_all(bytes)
            .map_err(|e| format!("write stdout: {e}")),
    }
}
