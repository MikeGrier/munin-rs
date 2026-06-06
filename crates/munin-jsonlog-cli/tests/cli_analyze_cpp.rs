// Copyright (c) Michael Grier

//! CPP-5.3: CLI roundtrip test for `munin-jsonlog analyze-cpp`.
//!
//! Builds a synthetic M3 `.binlog` (one `.vcxproj` with a single CL
//! task and `/showIncludes` output), runs the CLI on it, and asserts
//! the emitted `CppBuildAnalysis` JSON document matches expectations.

use std::{fs, path::PathBuf};

use assert_cmd::Command;
use munin_cppbuild::testkit::synthetic_cl_task_binlog;
use tempfile::tempdir;

const CL_COMMAND: &str =
    r#"cl.exe /c /showIncludes /DDEBUG=1 /IC:\sdk\include C:\src\product\app\main.cpp"#;

const CL_MESSAGES: &[&str] = &[
    "main.cpp",
    "Note: including file: C:\\sdk\\include\\sdk.h",
    "Note: including file:  C:\\sdk\\include\\sdk_detail.h",
];

fn run_analyze_cpp(extra: &[&str]) -> (PathBuf, serde_json::Value, tempfile::TempDir) {
    let tmp = tempdir().expect("tempdir");
    let binlog_path = tmp.path().join("synth.binlog");
    let json_path = tmp.path().join("synth.cpp.json");

    fs::write(
        &binlog_path,
        synthetic_cl_task_binlog(CL_COMMAND, CL_MESSAGES),
    )
    .expect("write synthetic binlog");

    let mut cmd = Command::cargo_bin("munin-jsonlog").expect("locate munin-jsonlog binary");
    cmd.arg("analyze-cpp")
        .arg(&binlog_path)
        .arg("-o")
        .arg(&json_path);
    for a in extra {
        cmd.arg(a);
    }
    cmd.assert().success();

    let bytes = fs::read(&json_path).expect("read analyze-cpp output");
    let doc: serde_json::Value = serde_json::from_slice(&bytes).expect("output is valid JSON");
    (binlog_path, doc, tmp)
}

#[test]
fn analyze_cpp_emits_expected_document() {
    let (binlog_path, doc, _tmp) = run_analyze_cpp(&["--auto-root", "--pretty"]);

    assert_eq!(doc["schema_version"], 1, "schema_version must be 1");

    let header = &doc["header"];
    assert_eq!(
        header["source_binlog"].as_str().unwrap(),
        binlog_path.display().to_string(),
        "source_binlog must echo the --input path"
    );

    let roots = header["roots"].as_array().expect("roots is an array");
    assert!(
        !roots.is_empty(),
        "--auto-root should produce at least one root"
    );

    let projects = doc["projects"].as_array().expect("projects is an array");
    assert_eq!(projects.len(), 1, "synthetic binlog has one .vcxproj");
    let project = &projects[0];

    let project_path = &project["project_path"];
    assert!(
        project_path["path"].is_string(),
        "project_path.path must be a string; got {project_path}"
    );

    let sources = project["sources"].as_array().expect("sources is an array");
    assert_eq!(sources.len(), 1, "expected one CL translation unit");
    let source = &sources[0];

    assert!(
        source["command_line"]
            .as_str()
            .unwrap()
            .contains("main.cpp"),
        "command_line must contain the .cpp path"
    );

    let defines = source["defines"].as_array().expect("defines array");
    assert!(
        defines
            .iter()
            .any(|d| d["name"] == "DEBUG" && d["value"] == "1"),
        "expected DEBUG=1 define; got {defines:?}"
    );
}

#[test]
fn analyze_cpp_writes_to_stdout_when_no_output_flag() {
    let tmp = tempdir().expect("tempdir");
    let binlog_path = tmp.path().join("synth.binlog");

    fs::write(
        &binlog_path,
        synthetic_cl_task_binlog(CL_COMMAND, CL_MESSAGES),
    )
    .expect("write synthetic binlog");

    let out = Command::cargo_bin("munin-jsonlog")
        .expect("locate munin-jsonlog binary")
        .arg("analyze-cpp")
        .arg(&binlog_path)
        .output()
        .expect("run munin-jsonlog");

    assert!(
        out.status.success(),
        "analyze-cpp failed: {}",
        String::from_utf8_lossy(&out.stderr),
    );

    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is valid JSON");
    assert_eq!(doc["schema_version"], 1);
}

#[test]
fn analyze_cpp_named_root_appears_in_header() {
    let (_, doc, _tmp) = run_analyze_cpp(&["--root", r"product=C:\src\product"]);

    let roots = doc["header"]["roots"].as_array().expect("roots array");
    assert_eq!(roots.len(), 1, "exactly one root expected");
    assert_eq!(roots[0]["name"], "product");
    assert_eq!(roots[0]["path"], r"C:\src\product");
}

#[test]
fn analyze_cpp_glob_expands_recursively_and_writes_per_input() {
    let tmp = tempdir().expect("tempdir");
    let out_dir = tmp.path().join("out");
    fs::create_dir(&out_dir).unwrap();

    // Two binlogs at different depths under tmp.
    let top_dir = tmp.path().join("top");
    let nested_dir = tmp.path().join("deep").join("nested");
    fs::create_dir_all(&top_dir).unwrap();
    fs::create_dir_all(&nested_dir).unwrap();
    let top = top_dir.join("alpha.binlog");
    let nested = nested_dir.join("beta.binlog");
    fs::write(&top, synthetic_cl_task_binlog(CL_COMMAND, CL_MESSAGES)).unwrap();
    fs::write(&nested, synthetic_cl_task_binlog(CL_COMMAND, CL_MESSAGES)).unwrap();

    let pattern = format!("{}/**/*.binlog", tmp.path().display());

    Command::cargo_bin("munin-jsonlog")
        .expect("locate munin-jsonlog binary")
        .arg("analyze-cpp")
        .arg(&pattern)
        .arg("-o")
        .arg(&out_dir)
        .assert()
        .success();

    let alpha_out = out_dir.join("alpha.cpp.json");
    let beta_out = out_dir.join("beta.cpp.json");
    assert!(alpha_out.is_file(), "missing {}", alpha_out.display());
    assert!(beta_out.is_file(), "missing {}", beta_out.display());

    for p in [alpha_out, beta_out] {
        let doc: serde_json::Value =
            serde_json::from_slice(&fs::read(&p).unwrap()).expect("valid JSON");
        assert_eq!(doc["schema_version"], 1, "{}: bad schema", p.display());
    }
}
