// Copyright (c) Michael Grier

//! CPP-5.4: Env-var-gated corpus harness.
//!
//! When `MUNIN_CPPBUILD_TEST_BINLOGS` is set to a directory, iterate
//! every `*.binlog` file in it (non-recursively), run the analysis
//! pipeline, and assert the emitted JSON document round-trips back
//! through `serde_json::from_value::<CppBuildAnalysis>`.
//!
//! Skips cleanly (logging a single line) when the variable is unset
//! so the test is harmless in environments without a binlog corpus.

use std::{env, fs::File, io::BufReader, path::PathBuf};

use munin_cppbuild::{CppBuildAnalysis, LocaleStrategy, analyze, auto_detect_root};
use munin_msbuild::BinlogIndex;

const ENV_VAR: &str = "MUNIN_CPPBUILD_TEST_BINLOGS";

#[test]
fn corpus_binlogs_analyze_and_roundtrip() {
    let dir = match env::var(ENV_VAR) {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => {
            eprintln!("skipping: {ENV_VAR} not set");
            return;
        }
    };

    let entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .map(|x| x.eq_ignore_ascii_case("binlog"))
                    .unwrap_or(false)
        })
        .collect();

    assert!(
        !entries.is_empty(),
        "{} contained no *.binlog files",
        dir.display()
    );

    let mut failures: Vec<String> = Vec::new();
    for path in &entries {
        if let Err(e) = analyze_one(path) {
            failures.push(format!("{}: {e}", path.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} corpus binlogs failed:\n  {}",
        failures.len(),
        entries.len(),
        failures.join("\n  "),
    );
}

fn analyze_one(path: &PathBuf) -> Result<(), String> {
    let f = File::open(path).map_err(|e| format!("open: {e}"))?;
    let index = BinlogIndex::open(BufReader::new(f)).map_err(|e| format!("decode: {e}"))?;

    let mut roots = Vec::new();
    if let Some(r) = auto_detect_root(&index).map_err(|e| format!("auto_detect_root: {e}"))? {
        roots.push(r);
    }

    let source_binlog = path.display().to_string();
    let doc = analyze(&index, &source_binlog, &roots, LocaleStrategy::BestEffort)
        .map_err(|e| format!("analyze: {e}"))?;

    let value = serde_json::to_value(&doc).map_err(|e| format!("to_value: {e}"))?;
    let round: CppBuildAnalysis =
        serde_json::from_value(value).map_err(|e| format!("from_value roundtrip: {e}"))?;

    if round.schema_version != doc.schema_version {
        return Err(format!(
            "schema_version mismatch after roundtrip: {} != {}",
            round.schema_version, doc.schema_version
        ));
    }
    if round.projects.len() != doc.projects.len() {
        return Err(format!(
            "project count mismatch after roundtrip: {} != {}",
            round.projects.len(),
            doc.projects.len()
        ));
    }

    Ok(())
}
