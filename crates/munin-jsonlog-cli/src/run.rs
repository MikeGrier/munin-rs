// Copyright (c) Michael Grier

//! Subcommand dispatch.

use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::Path,
};

use munin_msbuild::{BinlogIndex, jsonlog};

use crate::{
    cli::{AnalyzeCppArgs, Cli, Command, DumpArgs, PackArgs, RedactArgs},
    output::OutputSink,
    redact_args,
};

/// Top-level dispatch. Errors are returned as human-readable strings
/// (the binary's `main` prints them to stderr).
pub fn dispatch<S: OutputSink>(cli: Cli, sink: &mut S) -> Result<(), String> {
    match cli.command {
        Command::Dump(args) => run_dump(args, sink),
        Command::Pack(args) => run_pack(args, sink),
        Command::AnalyzeCpp(args) => run_analyze_cpp(args, sink),
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

fn run_analyze_cpp<S: OutputSink>(args: AnalyzeCppArgs, sink: &mut S) -> Result<(), String> {
    let f = File::open(&args.input).map_err(|e| format!("open {}: {e}", args.input.display()))?;
    let index = BinlogIndex::open(BufReader::new(f))
        .map_err(|e| format!("read binlog {}: {e}", args.input.display()))?;

    let mut roots: Vec<munin_cppbuild::Root> = args
        .root
        .iter()
        .map(|s| parse_root_arg(s))
        .collect::<Result<Vec<_>, String>>()?;

    if args.auto_root
        && let Some(r) =
            munin_cppbuild::auto_detect_root(&index).map_err(|e| format!("auto-detect root: {e}"))?
    {
        roots.push(r);
    }

    let strategy = if args.locale_strict {
        munin_cppbuild::LocaleStrategy::Strict
    } else {
        munin_cppbuild::LocaleStrategy::BestEffort
    };

    let source_binlog = args.input.display().to_string();
    let doc = munin_cppbuild::analyze(&index, &source_binlog, &roots, strategy)
        .map_err(|e| format!("analyze {}: {e}", args.input.display()))?;

    let mut bytes = if args.pretty {
        serde_json::to_vec_pretty(&doc)
    } else {
        serde_json::to_vec(&doc)
    }
    .map_err(|e| format!("encode CppBuildAnalysis: {e}"))?;
    bytes.push(b'\n');

    write_output(args.output.as_deref(), &bytes, sink)
}

/// Parse `--root` argument: accepts `NAME=PATH` or bare `PATH`.
/// For the bare form, name is the leaf component of the path.
fn parse_root_arg(s: &str) -> Result<munin_cppbuild::Root, String> {
    if let Some((name, path)) = s.split_once('=') {
        if name.is_empty() {
            return Err(format!("--root {s}: name before '=' must not be empty"));
        }
        if path.is_empty() {
            return Err(format!("--root {s}: path after '=' must not be empty"));
        }
        return Ok(munin_cppbuild::Root {
            name: name.to_string(),
            path: path.to_string(),
        });
    }
    let path = std::path::Path::new(s);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| format!("--root {s}: cannot derive a name from path with no leaf"))?;
    Ok(munin_cppbuild::Root {
        name,
        path: s.to_string(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_root_arg_name_equals_path() {
        let r = parse_root_arg("product=C:\\src\\product").unwrap();
        assert_eq!(r.name, "product");
        assert_eq!(r.path, "C:\\src\\product");
    }

    #[test]
    fn parse_root_arg_bare_path_uses_leaf_name() {
        let r = parse_root_arg("C:\\src\\product").unwrap();
        assert_eq!(r.name, "product");
        assert_eq!(r.path, "C:\\src\\product");
    }

    #[test]
    fn parse_root_arg_empty_name_errors() {
        let err = parse_root_arg("=C:\\path").unwrap_err();
        assert!(err.contains("name before '=' must not be empty"));
    }

    #[test]
    fn parse_root_arg_empty_path_errors() {
        let err = parse_root_arg("name=").unwrap_err();
        assert!(err.contains("path after '=' must not be empty"));
    }
}
