// Copyright (c) Michael Grier

use super::*;
use crate::schema::LinkInputKind;

fn lib(path: &str) -> LinkCmdInput {
    LinkCmdInput {
        path: path.to_string(),
        kind: LinkInputKind::Lib,
    }
}

fn obj(path: &str) -> LinkCmdInput {
    LinkCmdInput {
        path: path.to_string(),
        kind: LinkInputKind::Obj,
    }
}

#[test]
fn parse_empty_yields_default() {
    let c = parse("");
    assert_eq!(c, LinkCommandLine::default());
}

#[test]
fn parse_executable_only() {
    let c = parse("link.exe");
    assert_eq!(c.executable.as_deref(), Some("link.exe"));
    assert!(c.output.is_none());
    assert!(c.lib_paths.is_empty());
    assert!(c.inputs.is_empty());
    assert!(c.other_switches.is_empty());
}

#[test]
fn parse_out_attached_unquoted() {
    let c = parse(r"link.exe /OUT:foo.dll a.obj");
    assert_eq!(c.output.as_deref(), Some("foo.dll"));
    assert_eq!(c.inputs, vec![obj("a.obj")]);
}

#[test]
fn parse_out_attached_quoted_path() {
    let c = parse(r#"link.exe /OUT:"C:\out\foo bar.dll" a.obj"#);
    assert_eq!(c.output.as_deref(), Some(r"C:\out\foo bar.dll"));
}

#[test]
fn parse_out_case_insensitive() {
    let c = parse(r"link.exe /out:foo.dll");
    assert_eq!(c.output.as_deref(), Some("foo.dll"));
    let c = parse(r"link.exe /Out:foo.dll");
    assert_eq!(c.output.as_deref(), Some("foo.dll"));
    let c = parse(r"link.exe -OUT:foo.dll");
    assert_eq!(c.output.as_deref(), Some("foo.dll"));
}

#[test]
fn parse_first_out_wins_duplicate_goes_to_other() {
    let c = parse(r"link.exe /OUT:a.dll /OUT:b.dll x.obj");
    assert_eq!(c.output.as_deref(), Some("a.dll"));
    assert!(c.other_switches.iter().any(|s| s == "/OUT:b.dll"));
}

#[test]
fn parse_libpath_multiple_order_preserved() {
    let c = parse(r"link.exe /LIBPATH:C:\one /LIBPATH:C:\two");
    assert_eq!(c.lib_paths, vec![r"C:\one", r"C:\two"]);
}

#[test]
fn parse_libpath_quoted_with_spaces() {
    let c = parse(r#"link.exe /LIBPATH:"C:\Program Files\Lib""#);
    assert_eq!(c.lib_paths, vec![r"C:\Program Files\Lib"]);
}

#[test]
fn parse_libpath_case_insensitive() {
    let c = parse(r"link.exe /libpath:C:\x");
    assert_eq!(c.lib_paths, vec![r"C:\x"]);
}

#[test]
fn parse_positional_lib_and_obj_classified() {
    let c = parse(r"link.exe a.obj b.lib C.LIB d.OBJ");
    assert_eq!(
        c.inputs,
        vec![obj("a.obj"), lib("b.lib"), lib("C.LIB"), obj("d.OBJ")]
    );
}

#[test]
fn parse_positional_with_paths_quoted_and_bare() {
    let c = parse(r#"link.exe foo.obj "C:\out\bar.lib" baz.lib"#);
    assert_eq!(
        c.inputs,
        vec![obj("foo.obj"), lib(r"C:\out\bar.lib"), lib("baz.lib")]
    );
}

#[test]
fn parse_unknown_extension_positional_goes_to_other() {
    let c = parse(r"link.exe foo.txt bar.obj");
    assert_eq!(c.inputs, vec![obj("bar.obj")]);
    assert!(c.other_switches.iter().any(|s| s == "foo.txt"));
}

#[test]
fn parse_response_file_preserved_verbatim() {
    let c = parse(r"link.exe @response.rsp x.obj");
    assert_eq!(c.inputs, vec![obj("x.obj")]);
    assert!(c.other_switches.iter().any(|s| s == "@response.rsp"));
}

#[test]
fn parse_other_switches_preserved() {
    let c = parse(r"link.exe /NOLOGO /INCREMENTAL:NO /WX /VERBOSE x.obj");
    let kept: Vec<&str> = c.other_switches.iter().map(String::as_str).collect();
    assert!(kept.contains(&"/NOLOGO"));
    assert!(kept.contains(&"/INCREMENTAL:NO"));
    assert!(kept.contains(&"/WX"));
    assert!(kept.contains(&"/VERBOSE"));
    assert_eq!(c.inputs, vec![obj("x.obj")]);
}

#[test]
fn parse_realistic_command_line() {
    // Trimmed from a real binlog Link task.
    let cmd = r#"F:\VC\bin\link.exe /ERRORREPORT:QUEUE /OUT:"F:\out\PreRestartState.dll" /VERBOSE /INCREMENTAL:NO /NOLOGO /LIBPATH:"F:\vcpkg\installed\x64-rel-lib\lib" /WX absl_base.lib absl_city.lib F:\src\agent\obj\PreRestartState.obj "F:\out\AgentEtw.lib""#;
    let c = parse(cmd);
    assert!(c.executable.as_deref().unwrap().ends_with("link.exe"));
    assert_eq!(c.output.as_deref(), Some(r"F:\out\PreRestartState.dll"));
    assert_eq!(c.lib_paths, vec![r"F:\vcpkg\installed\x64-rel-lib\lib"]);
    assert_eq!(
        c.inputs,
        vec![
            lib("absl_base.lib"),
            lib("absl_city.lib"),
            obj(r"F:\src\agent\obj\PreRestartState.obj"),
            lib(r"F:\out\AgentEtw.lib"),
        ]
    );
    let kept: Vec<&str> = c.other_switches.iter().map(String::as_str).collect();
    assert!(kept.contains(&"/ERRORREPORT:QUEUE"));
    assert!(kept.contains(&"/VERBOSE"));
    assert!(kept.contains(&"/INCREMENTAL:NO"));
    assert!(kept.contains(&"/NOLOGO"));
    assert!(kept.contains(&"/WX"));
}

#[test]
fn parse_dash_prefix_recognized() {
    let c = parse(r"link.exe -out:foo.dll -libpath:C:\one x.obj");
    assert_eq!(c.output.as_deref(), Some("foo.dll"));
    assert_eq!(c.lib_paths, vec![r"C:\one"]);
    assert_eq!(c.inputs, vec![obj("x.obj")]);
}

#[test]
fn parse_libpath_without_value_preserved_in_other() {
    // `/LIBPATH:` with no value — preserve verbatim rather than
    // emit an empty entry.
    let c = parse(r"link.exe /LIBPATH: x.obj");
    assert!(
        c.lib_paths.is_empty(),
        "empty-valued /LIBPATH: must not produce a lib_paths entry"
    );
    assert!(
        c.other_switches.iter().any(|s| s == "/LIBPATH:"),
        "/LIBPATH: must be preserved verbatim in other_switches"
    );
    assert_eq!(c.inputs, vec![obj("x.obj")]);
}

#[test]
fn parse_out_without_value_preserved_in_other() {
    // Bare `/OUT:` is malformed; keep verbatim, leave `output` None.
    let c = parse(r"link.exe /OUT: x.obj");
    assert!(c.output.is_none(), "empty-valued /OUT: must not set output");
    assert!(
        c.other_switches.iter().any(|s| s == "/OUT:"),
        "/OUT: must be preserved verbatim in other_switches"
    );
    assert_eq!(c.inputs, vec![obj("x.obj")]);
}
