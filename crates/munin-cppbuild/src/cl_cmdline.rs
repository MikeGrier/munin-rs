// Copyright (c) Michael Grier

//! `cl.exe` command-line tokenizer.
//!
//! Splits a `cl.exe` command line into structured fields: include
//! paths, defines, source file, and verbatim "other" switches.
//!
//! See `DESIGN-NOTES.md` §D-CPP-CLCMDLINE1 for the scope and the
//! response-file limitation.
//!
//! ## What this is and is not
//!
//! - **Tokenization** approximates `CommandLineToArgvW` for the
//!   inputs MSBuild actually emits: double-quoted runs are one
//!   token, backslashes are literal (typical Windows paths), and
//!   `""` inside a quoted run is a literal `"`.
//! - **Switch recognition** covers exactly the switches the schema
//!   surfaces (`/I`, `/D`) plus extraction of source files by
//!   extension. Every other switch is preserved verbatim in
//!   [`ClCommandLine::other_switches`].
//! - **Batch mode.** `cl.exe` accepts N source files on one
//!   command line (N ≥ 1). All matching positional tokens are
//!   collected, in cmdline order, into
//!   [`ClCommandLine::sources`]; see D-CPP-SHOWINC2.
//! - **Response files** (`@file.rsp`) are **not** expanded. They are
//!   recorded verbatim in `other_switches` so the analysis can flag
//!   them but the file's contents are not re-parsed in v1.

use crate::schema::Define;

/// Structured view of a parsed `cl.exe` command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClCommandLine {
    /// First token of the original command line (typically
    /// `CL.exe` or an absolute path to it).  `None` if the input
    /// was empty.
    pub executable: Option<String>,
    /// Source files passed as positional arguments (case-insensitive
    /// match against `.cpp`, `.cxx`, `.cc`, `.c`, `.c++`), in
    /// command-line order. Empty when the cmdline only contains
    /// switches (rare; usually a response-file invocation).
    ///
    /// See D-CPP-SHOWINC2 for batch-mode semantics.
    pub sources: Vec<String>,
    /// `/I` include paths in command-line order, with `/I` stripped
    /// and surrounding quotes removed.
    pub include_paths: Vec<String>,
    /// `/D` preprocessor defines in command-line order.
    pub defines: Vec<Define>,
    /// Every other token (recognized switch with no schema slot,
    /// response-file directive, unknown positional) preserved
    /// **verbatim** including its leading prefix and any quoting.
    pub other_switches: Vec<String>,
}

/// Parse a `cl.exe` command line.
///
/// The first token of `command_line` is captured as
/// [`ClCommandLine::executable`] and not otherwise processed. The
/// remaining tokens are classified according to the rules in the
/// module docs.
pub fn parse(command_line: &str) -> ClCommandLine {
    let mut tokens = tokenize(command_line).into_iter();
    let executable = tokens.next();
    let tokens: Vec<String> = tokens.collect();

    let mut out = ClCommandLine {
        executable,
        ..Default::default()
    };

    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];

        // Switch?  Both `/` and `-` are recognized as switch prefixes
        // by cl.exe.
        if let Some(body) = strip_switch_prefix(tok) {
            // `/I` include path
            if let Some(value) = strip_letter(body, 'I') {
                if value.is_empty() {
                    // Separated form: `/I path`
                    if let Some(next) = tokens.get(i + 1) {
                        out.include_paths.push(unquote(next));
                        i += 2;
                        continue;
                    }
                } else {
                    out.include_paths.push(unquote(value));
                    i += 1;
                    continue;
                }
            }

            // `/D` define
            if let Some(value) = strip_letter(body, 'D') {
                if value.is_empty() {
                    // Separated form: `/D NAME[=VALUE]`
                    if let Some(next) = tokens.get(i + 1) {
                        out.defines.push(parse_define(&unquote(next)));
                        i += 2;
                        continue;
                    }
                } else {
                    out.defines.push(parse_define(&unquote(value)));
                    i += 1;
                    continue;
                }
            }

            // Unrecognized switch — preserve verbatim.
            out.other_switches.push(tok.clone());
            i += 1;
            continue;
        }

        // Positional. Collect source files in cmdline order;
        // route everything else to other_switches verbatim.
        if is_source_extension(tok) {
            out.sources.push(unquote(tok));
        } else {
            out.other_switches.push(tok.clone());
        }
        i += 1;
    }

    out
}

/// Return the switch body if `tok` starts with `/` or `-`.
fn strip_switch_prefix(tok: &str) -> Option<&str> {
    tok.strip_prefix('/').or_else(|| tok.strip_prefix('-'))
}

/// If `body` begins with `letter` (case-sensitive), return the
/// remainder; otherwise `None`. `cl.exe` switches are
/// case-insensitive, but `/I` and `/D` are the canonical forms; we
/// match both cases to be safe.
fn strip_letter(body: &str, letter: char) -> Option<&str> {
    body.strip_prefix(letter)
        .or_else(|| body.strip_prefix(letter.to_ascii_lowercase()))
}

/// Split `NAME[=VALUE]`.
fn parse_define(s: &str) -> Define {
    match s.split_once('=') {
        Some((name, value)) => Define {
            name: name.to_string(),
            value: Some(value.to_string()),
        },
        None => Define {
            name: s.to_string(),
            value: None,
        },
    }
}

/// Recognize a C/C++ source file extension, case-insensitive.
fn is_source_extension(path: &str) -> bool {
    let lower = unquote(path).to_ascii_lowercase();
    [".cpp", ".cxx", ".cc", ".c", ".c++"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// Strip a single pair of surrounding double-quotes if present.
/// Internal `""` sequences (escaped quotes) are not collapsed here —
/// the tokenizer already handled them.
pub(crate) fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Split a command line into tokens.
///
/// Whitespace separates tokens at top level. A double-quote toggles
/// "in-quotes" mode; whitespace inside quotes does not split. A
/// literal double-quote inside a quoted run is written as `""`
/// (CommandLineToArgvW convention). Backslashes are treated as
/// literal characters so Windows paths (`C:\foo\bar`) round-trip
/// verbatim — this differs from CommandLineToArgvW's
/// backslash-before-quote rule, which MSBuild-emitted CL command
/// lines do not exercise in practice.
pub(crate) fn tokenize(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut have_token = false;
    let chars: Vec<char> = s.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_quotes {
            if c == '"' {
                // `""` inside quotes = literal `"`.
                if chars.get(i + 1) == Some(&'"') {
                    cur.push('"');
                    i += 2;
                    continue;
                }
                in_quotes = false;
                i += 1;
                continue;
            }
            cur.push(c);
            i += 1;
        } else if c.is_whitespace() {
            if have_token {
                out.push(std::mem::take(&mut cur));
                have_token = false;
            }
            i += 1;
        } else if c == '"' {
            in_quotes = true;
            have_token = true;
            i += 1;
        } else {
            cur.push(c);
            have_token = true;
            i += 1;
        }
    }
    if have_token {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests;
