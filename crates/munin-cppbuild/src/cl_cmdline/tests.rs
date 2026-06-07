// Copyright (c) Michael Grier

use super::*;

// ── tokenize ──

#[test]
fn tokenize_empty_yields_no_tokens() {
    assert!(tokenize("").is_empty());
    assert!(tokenize("   \t  ").is_empty());
}

#[test]
fn tokenize_simple_whitespace_split() {
    assert_eq!(tokenize("a b\tc"), vec!["a", "b", "c"]);
}

#[test]
fn tokenize_quoted_run_preserves_internal_whitespace() {
    assert_eq!(
        tokenize(r#"cmd "a b c" tail"#),
        vec!["cmd", "a b c", "tail"]
    );
}

#[test]
fn tokenize_quoted_run_glues_to_prefix() {
    assert_eq!(
        tokenize(r#"/I"C:\path with spaces" /c"#),
        vec![r"/IC:\path with spaces", "/c"]
    );
}

#[test]
fn tokenize_doubled_quote_inside_quoted_run_is_literal_quote() {
    assert_eq!(tokenize(r#""he said ""hi""""#), vec![r#"he said "hi""#]);
}

#[test]
fn tokenize_backslashes_are_literal() {
    assert_eq!(
        tokenize(r#"C:\foo\bar.exe arg"#),
        vec![r"C:\foo\bar.exe", "arg"]
    );
}

// ── strip_switch_prefix ──

#[test]
fn strip_switch_prefix_accepts_slash_and_dash() {
    assert_eq!(strip_switch_prefix("/Foo"), Some("Foo"));
    assert_eq!(strip_switch_prefix("-Foo"), Some("Foo"));
    assert_eq!(strip_switch_prefix("Foo"), None);
    assert_eq!(strip_switch_prefix(""), None);
}

// ── parse: include paths ──

#[test]
fn parse_extracts_attached_include_path() {
    let cl = parse(r"CL.exe /IC:\inc\foo");
    assert_eq!(cl.executable.as_deref(), Some("CL.exe"));
    assert_eq!(cl.include_paths, vec![r"C:\inc\foo"]);
    assert!(cl.other_switches.is_empty());
}

#[test]
fn parse_extracts_separated_include_path() {
    let cl = parse(r"CL.exe /I C:\inc\foo");
    assert_eq!(cl.include_paths, vec![r"C:\inc\foo"]);
}

#[test]
fn parse_extracts_quoted_include_path_with_spaces() {
    let cl = parse(r#"CL.exe /I"C:\Program Files\inc""#);
    assert_eq!(cl.include_paths, vec![r"C:\Program Files\inc"]);
}

#[test]
fn parse_preserves_include_order() {
    let cl = parse(r"CL.exe /Ia /Ib /Ic");
    assert_eq!(cl.include_paths, vec!["a", "b", "c"]);
}

#[test]
fn parse_dash_i_form_is_also_accepted() {
    let cl = parse(r"CL.exe -Ia -I b");
    assert_eq!(cl.include_paths, vec!["a", "b"]);
}

// ── parse: defines ──

#[test]
fn parse_extracts_bare_define() {
    let cl = parse("CL.exe /DNDEBUG");
    assert_eq!(
        cl.defines,
        vec![Define {
            name: "NDEBUG".into(),
            value: None
        }]
    );
}

#[test]
fn parse_extracts_define_with_value() {
    let cl = parse("CL.exe /DFOO=bar");
    assert_eq!(
        cl.defines,
        vec![Define {
            name: "FOO".into(),
            value: Some("bar".into())
        }]
    );
}

#[test]
fn parse_extracts_define_with_empty_value() {
    let cl = parse("CL.exe /DFOO=");
    assert_eq!(
        cl.defines,
        vec![Define {
            name: "FOO".into(),
            value: Some(String::new())
        }]
    );
}

#[test]
fn parse_extracts_separated_define() {
    let cl = parse("CL.exe /D FOO=bar");
    assert_eq!(
        cl.defines,
        vec![Define {
            name: "FOO".into(),
            value: Some("bar".into())
        }]
    );
}

#[test]
fn parse_extracts_quoted_define_value_with_spaces() {
    let cl = parse(r#"CL.exe /D_MY="value with spaces""#);
    assert_eq!(
        cl.defines,
        vec![Define {
            name: "_MY".into(),
            value: Some("value with spaces".into())
        }]
    );
}

#[test]
fn parse_preserves_define_order() {
    let cl = parse("CL.exe /DA /DB=1 /DC=2");
    let names: Vec<&str> = cl.defines.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, vec!["A", "B", "C"]);
}

// ── parse: source file ──

#[test]
fn parse_extracts_cpp_source_file() {
    let cl = parse("CL.exe /c main.cpp");
    assert_eq!(cl.sources, vec!["main.cpp"]);
    assert_eq!(cl.other_switches, vec!["/c"]);
}

#[test]
fn parse_extracts_source_by_extension_case_insensitive() {
    for ext in [".CPP", ".Cxx", ".cc", ".C", ".c++"] {
        let name = format!("src{ext}");
        let cl = parse(&format!("CL.exe {name}"));
        assert_eq!(cl.sources, vec![name.clone()], "ext {ext}");
    }
}

#[test]
fn parse_quoted_source_path_unquoted() {
    let cl = parse(r#"CL.exe "C:\some dir\main.cpp""#);
    assert_eq!(cl.sources, vec![r"C:\some dir\main.cpp"]);
}

#[test]
fn parse_batch_mode_collects_all_sources_in_order() {
    // cl.exe batch mode: N sources on one cmdline. All are
    // retained in cmdline order; none are demoted.
    let cl = parse("CL.exe a.cpp b.cpp c.cpp");
    assert_eq!(cl.sources, vec!["a.cpp", "b.cpp", "c.cpp"]);
    assert!(
        cl.other_switches.is_empty(),
        "sources must not leak into other_switches"
    );
}

#[test]
fn parse_batch_mode_sources_interleaved_with_switches() {
    let cl = parse("CL.exe /c a.cpp /W4 b.cpp /nologo c.cpp");
    assert_eq!(cl.sources, vec!["a.cpp", "b.cpp", "c.cpp"]);
    assert_eq!(cl.other_switches, vec!["/c", "/W4", "/nologo"]);
}

#[test]
fn parse_no_source_yields_empty_sources_vec() {
    let cl = parse("CL.exe /c /nologo");
    assert!(cl.sources.is_empty());
}

// ── parse: other switches ──

#[test]
fn parse_preserves_unknown_switches_verbatim() {
    let cl = parse(r#"CL.exe /nologo /W4 /Fo"obj\" /EHsc"#);
    assert_eq!(
        cl.other_switches,
        vec!["/nologo", "/W4", r"/Foobj\", "/EHsc"]
    );
    assert!(cl.include_paths.is_empty());
    assert!(cl.defines.is_empty());
}

#[test]
fn parse_preserves_response_file_directive_verbatim() {
    let cl = parse("CL.exe @response.rsp");
    assert_eq!(cl.other_switches, vec!["@response.rsp"]);
    assert!(cl.include_paths.is_empty());
    assert!(cl.defines.is_empty());
}

// ── parse: empty / degenerate ──

#[test]
fn parse_empty_yields_default_with_no_executable() {
    let cl = parse("");
    assert_eq!(cl, ClCommandLine::default());
}

#[test]
fn parse_executable_only() {
    let cl = parse("CL.exe");
    assert_eq!(cl.executable.as_deref(), Some("CL.exe"));
    assert!(cl.sources.is_empty());
    assert!(cl.include_paths.is_empty());
    assert!(cl.defines.is_empty());
    assert!(cl.other_switches.is_empty());
}

#[test]
fn parse_trailing_separated_switch_without_value_preserved_in_other_switches() {
    // `/I` at end with no value: we cannot record an include path,
    // but we also don't silently lose the token — it falls through
    // to other_switches so the analysis can flag it.
    let cl = parse("CL.exe /I");
    assert!(cl.include_paths.is_empty());
    assert_eq!(cl.other_switches, vec!["/I"]);
}

// ── realistic-shaped command line ──

#[test]
fn parse_realistic_msbuild_cl_command_line() {
    let line = r#""C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.40.33807\bin\HostX64\x64\CL.exe" /c /I"C:\proj\include" /I"C:\proj\other inc" /DWIN32 /D_DEBUG /D_CONSOLE /EHsc /W4 /nologo /showIncludes /Fo"obj\main.obj" main.cpp"#;
    let cl = parse(line);
    assert!(cl.executable.as_deref().unwrap().ends_with(r"CL.exe"));
    assert_eq!(
        cl.include_paths,
        vec![r"C:\proj\include", r"C:\proj\other inc"]
    );
    let define_names: Vec<&str> = cl.defines.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(define_names, vec!["WIN32", "_DEBUG", "_CONSOLE"]);
    assert_eq!(cl.sources, vec!["main.cpp"]);
    assert!(cl.other_switches.iter().any(|s| s == "/c"));
    assert!(cl.other_switches.iter().any(|s| s == "/EHsc"));
    assert!(cl.other_switches.iter().any(|s| s == "/W4"));
    assert!(cl.other_switches.iter().any(|s| s == "/nologo"));
    assert!(cl.other_switches.iter().any(|s| s == "/showIncludes"));
    assert!(cl.other_switches.iter().any(|s| s == r"/Foobj\main.obj"));
}

// ── lowercase-prefix switches must NOT match /D or /I ──

#[test]
fn parse_lowercase_diagnostics_switch_is_not_a_define() {
    // Real-world regression: `/diagnostics:classic` (lowercase d)
    // is an MSVC switch that controls diagnostic format. It must
    // be preserved verbatim in other_switches, not parsed as a
    // define named `iagnostics:classic`.
    let cl = parse("CL.exe /diagnostics:classic main.cpp");
    assert!(cl.defines.is_empty(), "/diagnostics is not a define");
    assert!(
        cl.other_switches
            .iter()
            .any(|s| s == "/diagnostics:classic"),
        "/diagnostics:classic must be preserved verbatim"
    );
    assert_eq!(cl.sources, vec!["main.cpp"]);
}

#[test]
fn parse_lowercase_d1_d2_switches_are_not_defines() {
    // `/d1*` and `/d2*` are internal MSVC switches (e.g.
    // `/d1reportTime`, `/d2Zi+`); they must not be parsed as
    // defines.
    let cl = parse("CL.exe /d1reportTime /d2Zi+ main.cpp");
    assert!(cl.defines.is_empty());
    assert!(cl.other_switches.iter().any(|s| s == "/d1reportTime"));
    assert!(cl.other_switches.iter().any(|s| s == "/d2Zi+"));
}

#[test]
fn parse_lowercase_i_prefix_switch_is_not_an_include_path() {
    // No lowercase-`i` cl.exe switch in common use today, but the
    // same case-sensitivity rule must hold for `/I` as for `/D`.
    let cl = parse("CL.exe /ignoreMe main.cpp");
    assert!(cl.include_paths.is_empty());
    assert!(cl.other_switches.iter().any(|s| s == "/ignoreMe"));
}
