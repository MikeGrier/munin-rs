// Copyright (c) Michael Grier

//! Subcommand dispatch.

use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
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
    let verbose = cli.verbose;
    match cli.command {
        Command::Dump(args) => run_dump(args, verbose, sink),
        Command::Pack(args) => run_pack(args, verbose, sink),
        Command::AnalyzeCpp(args) => run_analyze_cpp(args, verbose, sink),
    }
}

/// Write a progress line for input `i` of `total` to the sink's
/// diagnostic stream. `target` is `None` for stdout output.
fn report_progress<S: OutputSink>(
    sink: &mut S,
    i: usize,
    total: usize,
    input: &Path,
    target: Option<&Path>,
) {
    let dest = match target {
        Some(p) => p.display().to_string(),
        None => "<stdout>".to_string(),
    };
    let _ = writeln!(
        sink.err(),
        "[{}/{}] {} -> {}",
        i + 1,
        total,
        input.display(),
        dest,
    );
}

fn run_dump<S: OutputSink>(args: DumpArgs, verbose: bool, sink: &mut S) -> Result<(), String> {
    let inputs = expand_inputs(&args.input)?;
    let plan = OutputPlan::resolve(&inputs, args.output.as_deref(), "jsonlog")?;
    let total = inputs.len();

    for (i, (input, target)) in inputs.iter().zip(plan.targets.iter()).enumerate() {
        if verbose {
            report_progress(sink, i, total, input, target.as_deref());
        }
        let index = load_and_redact_binlog(input, &args.redact)?;
        let mut bytes = Vec::new();
        if args.pretty {
            jsonlog::dump_index_pretty(&index, &mut bytes)
                .map_err(|e| format!("encode jsonlog: {e}"))?;
        } else {
            jsonlog::dump_index(&index, &mut bytes).map_err(|e| format!("encode jsonlog: {e}"))?;
        }
        write_output(target.as_deref(), &bytes, sink)?;
    }
    Ok(())
}

fn run_pack<S: OutputSink>(args: PackArgs, verbose: bool, sink: &mut S) -> Result<(), String> {
    let inputs = expand_inputs(&args.input)?;
    let plan = OutputPlan::resolve(&inputs, args.output.as_deref(), "binlog")?;
    let total = inputs.len();

    for (i, (input, target)) in inputs.iter().zip(plan.targets.iter()).enumerate() {
        if verbose {
            report_progress(sink, i, total, input, target.as_deref());
        }
        let index = load_and_redact_jsonlog(input, &args.redact)?;
        let mut bytes = Vec::new();
        index
            .write_binlog(&mut bytes)
            .map_err(|e| format!("encode binlog: {e}"))?;
        write_output(target.as_deref(), &bytes, sink)?;
    }
    Ok(())
}

fn run_analyze_cpp<S: OutputSink>(
    args: AnalyzeCppArgs,
    verbose: bool,
    sink: &mut S,
) -> Result<(), String> {
    let inputs = expand_inputs(&args.input)?;
    let plan = OutputPlan::resolve(&inputs, args.output.as_deref(), "cpp.json")?;
    let total = inputs.len();

    let explicit_roots: Vec<munin_cppbuild::Root> = args
        .root
        .iter()
        .map(|s| parse_root_arg(s))
        .collect::<Result<Vec<_>, String>>()?;

    let strategy = if args.locale_strict {
        munin_cppbuild::LocaleStrategy::Strict
    } else {
        munin_cppbuild::LocaleStrategy::BestEffort
    };

    for (i, (input, target)) in inputs.iter().zip(plan.targets.iter()).enumerate() {
        if verbose {
            report_progress(sink, i, total, input, target.as_deref());
        }
        let f = File::open(input).map_err(|e| format!("open {}: {e}", input.display()))?;
        let index = BinlogIndex::open(BufReader::new(f))
            .map_err(|e| format!("read binlog {}: {e}", input.display()))?;

        let mut roots = explicit_roots.clone();
        if args.auto_root
            && let Some(r) = munin_cppbuild::auto_detect_root(&index)
                .map_err(|e| format!("auto-detect root: {e}"))?
        {
            roots.push(r);
        }

        let source_binlog = input.display().to_string();
        let doc = munin_cppbuild::analyze(&index, &source_binlog, &roots, strategy)
            .map_err(|e| format!("analyze {}: {e}", input.display()))?;

        let mut bytes = if args.pretty {
            serde_json::to_vec_pretty(&doc)
        } else {
            serde_json::to_vec(&doc)
        }
        .map_err(|e| format!("encode CppBuildAnalysis: {e}"))?;
        bytes.push(b'\n');

        write_output(target.as_deref(), &bytes, sink)?;
    }
    Ok(())
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
    // Use a platform-independent splitter so Windows paths
    // (`C:\src\product`) yield the expected leaf name when running
    // the CLI on Linux as well.
    let trimmed = s.trim_end_matches(['\\', '/']);
    let leaf_start = trimmed.rfind(['\\', '/']).map(|i| i + 1).unwrap_or(0);
    let name = trimmed[leaf_start..].to_string();
    if name.is_empty() {
        return Err(format!(
            "--root {s}: cannot derive a name from path with no leaf"
        ));
    }
    Ok(munin_cppbuild::Root {
        name,
        path: s.to_string(),
    })
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

/// Expand each input pattern into one or more file paths.
///
/// Patterns containing `*`, `?`, or `[` are passed to the `glob`
/// crate (which supports `**` for recursive descent); other inputs
/// are treated as literal paths and must exist. A glob that matches
/// no files is an error.
fn expand_inputs(patterns: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for pat in patterns {
        if pat.contains('*') || pat.contains('?') || pat.contains('[') {
            let matches = glob::glob(pat).map_err(|e| format!("invalid glob {pat:?}: {e}"))?;
            let before = out.len();
            for m in matches {
                let p = m.map_err(|e| format!("glob {pat:?}: {e}"))?;
                if p.is_file() {
                    out.push(p);
                }
            }
            if out.len() == before {
                return Err(format!("glob {pat:?} matched no files"));
            }
        } else {
            let p = PathBuf::from(pat);
            if !p.exists() {
                return Err(format!("input {} does not exist", p.display()));
            }
            out.push(p);
        }
    }
    Ok(out)
}

/// Resolves the per-input output target for a batch of inputs.
#[derive(Debug)]
struct OutputPlan {
    /// One entry per input. `None` means stdout (only possible when
    /// `inputs.len() == 1` and no `--output` was given).
    targets: Vec<Option<PathBuf>>,
}

impl OutputPlan {
    fn resolve(
        inputs: &[PathBuf],
        output: Option<&Path>,
        out_extension: &str,
    ) -> Result<Self, String> {
        match (inputs.len(), output) {
            (0, _) => Err("no input files".to_string()),
            (1, out) => Ok(Self {
                targets: vec![out.map(PathBuf::from)],
            }),
            (_, None) => Ok(Self {
                targets: inputs
                    .iter()
                    .map(|p| Some(p.with_extension(out_extension)))
                    .collect(),
            }),
            (_, Some(dir)) => {
                if !dir.is_dir() {
                    return Err(format!(
                        "--output {} must be an existing directory when multiple inputs are given",
                        dir.display()
                    ));
                }
                let mut targets = Vec::with_capacity(inputs.len());
                for input in inputs {
                    let name = input.file_stem().ok_or_else(|| {
                        format!("cannot derive output name from {}", input.display())
                    })?;
                    let mut fname = PathBuf::from(name);
                    fname.set_extension(out_extension);
                    targets.push(Some(dir.join(fname)));
                }
                Ok(Self { targets })
            }
        }
    }
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
    use std::fs;
    use tempfile::tempdir;

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

    #[test]
    fn expand_inputs_literal_path_passthrough() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("a.binlog");
        fs::write(&p, b"x").unwrap();
        let out = expand_inputs(&[p.display().to_string()]).unwrap();
        assert_eq!(out, vec![p]);
    }

    #[test]
    fn expand_inputs_literal_missing_path_errors() {
        let err = expand_inputs(&["definitely_missing_file.binlog".to_string()]).unwrap_err();
        assert!(err.contains("does not exist"), "got: {err}");
    }

    #[test]
    fn expand_inputs_glob_matches_files() {
        let tmp = tempdir().unwrap();
        for name in ["a.binlog", "b.binlog", "c.txt"] {
            fs::write(tmp.path().join(name), b"x").unwrap();
        }
        let pat = tmp.path().join("*.binlog").display().to_string();
        let mut out = expand_inputs(&[pat]).unwrap();
        out.sort();
        assert_eq!(out.len(), 2);
        assert!(out[0].ends_with("a.binlog"));
        assert!(out[1].ends_with("b.binlog"));
    }

    #[test]
    fn expand_inputs_recursive_glob_descends() {
        let tmp = tempdir().unwrap();
        let sub = tmp.path().join("deep").join("nested");
        fs::create_dir_all(&sub).unwrap();
        fs::write(tmp.path().join("top.binlog"), b"x").unwrap();
        fs::write(sub.join("inner.binlog"), b"x").unwrap();
        let pat = format!("{}/**/*.binlog", tmp.path().display());
        let out = expand_inputs(&[pat]).unwrap();
        assert_eq!(out.len(), 2, "got: {out:?}");
    }

    #[test]
    fn expand_inputs_unmatched_glob_errors() {
        let tmp = tempdir().unwrap();
        let pat = tmp.path().join("*.nope").display().to_string();
        let err = expand_inputs(&[pat]).unwrap_err();
        assert!(err.contains("matched no files"), "got: {err}");
    }

    #[test]
    fn output_plan_single_input_no_output_is_stdout() {
        let plan = OutputPlan::resolve(&[PathBuf::from("a.binlog")], None, "jsonlog").unwrap();
        assert_eq!(plan.targets, vec![None]);
    }

    #[test]
    fn output_plan_single_input_with_output_uses_path() {
        let plan = OutputPlan::resolve(
            &[PathBuf::from("a.binlog")],
            Some(Path::new("out.jsonlog")),
            "jsonlog",
        )
        .unwrap();
        assert_eq!(plan.targets, vec![Some(PathBuf::from("out.jsonlog"))]);
    }

    #[test]
    fn output_plan_multi_input_no_output_writes_alongside() {
        let inputs = vec![PathBuf::from("a.binlog"), PathBuf::from("dir/b.binlog")];
        let plan = OutputPlan::resolve(&inputs, None, "jsonlog").unwrap();
        assert_eq!(
            plan.targets,
            vec![
                Some(PathBuf::from("a.jsonlog")),
                Some(PathBuf::from("dir/b.jsonlog")),
            ]
        );
    }

    #[test]
    fn output_plan_multi_input_with_dir_routes_to_dir() {
        let tmp = tempdir().unwrap();
        let inputs = vec![PathBuf::from("a.binlog"), PathBuf::from("sub/b.binlog")];
        let plan = OutputPlan::resolve(&inputs, Some(tmp.path()), "cpp.json").unwrap();
        assert_eq!(
            plan.targets,
            vec![
                Some(tmp.path().join("a.cpp.json")),
                Some(tmp.path().join("b.cpp.json")),
            ]
        );
    }

    #[test]
    fn output_plan_multi_input_with_file_output_errors() {
        let tmp = tempdir().unwrap();
        let not_a_dir = tmp.path().join("out.json");
        fs::write(&not_a_dir, b"x").unwrap();
        let inputs = vec![PathBuf::from("a.binlog"), PathBuf::from("b.binlog")];
        let err = OutputPlan::resolve(&inputs, Some(&not_a_dir), "cpp.json").unwrap_err();
        assert!(err.contains("must be an existing directory"), "got: {err}");
    }
}
