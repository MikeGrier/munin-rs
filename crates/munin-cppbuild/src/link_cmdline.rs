// Copyright (c) Michael Grier

//! `link.exe` command-line tokenizer.
//!
//! Splits a `link.exe` command line into structured fields: output
//! file, library search paths, listed `.lib`/`.obj` inputs, and
//! verbatim "other" switches.
//!
//! See `DESIGN-NOTES.md` §D-CPP-LINK1 for the surrounding scope
//! (parser sits on top of this tokenizer in CPP-4.3) and the
//! shared D-CPP-CLCMDLINE1 for tokenization conventions.
//!
//! ## What this is and is not
//!
//! - **Tokenization** is shared with [`crate::cl_cmdline`]: the
//!   same `CommandLineToArgvW`-approximating splitter.
//! - **Switch recognition** covers the schema-surfaced switches —
//!   `/OUT:`, `/LIBPATH:` — and the positional `.lib` / `.obj`
//!   input list. `link.exe` switches are case-insensitive; both
//!   `/OUT:`, `-out:`, `/Out:`, etc., are recognized.
//! - **Response files** (`@file.rsp`) are **not** expanded; they
//!   land verbatim in [`LinkCommandLine::other_switches`].

use crate::cl_cmdline::{tokenize, unquote};
use crate::schema::LinkInputKind;

/// Structured view of a parsed `link.exe` command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkCommandLine {
    /// First token of the original command line (typically
    /// `link.exe` or an absolute path to it). `None` if the input
    /// was empty.
    pub executable: Option<String>,
    /// Value of the first `/OUT:<path>` switch, quotes stripped.
    /// `None` if absent.
    pub output: Option<String>,
    /// `/LIBPATH:<dir>` values in command-line order, quotes
    /// stripped.
    pub lib_paths: Vec<String>,
    /// Positional inputs whose extension is `.lib` or `.obj`
    /// (case-insensitive), in command-line order.
    pub inputs: Vec<LinkCmdInput>,
    /// Every other token (recognized switch with no schema slot,
    /// unknown positional, `@response.rsp`) preserved **verbatim**
    /// including its leading prefix and any quoting.
    pub other_switches: Vec<String>,
}

/// One positional input on the `link.exe` command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkCmdInput {
    /// Path as it appeared on the command line, with any
    /// surrounding `"…"` stripped. Bare `name.lib` references with
    /// no directory survive as-is; the linker resolves them via
    /// `/LIBPATH:` or the default library path at link time.
    pub path: String,
    /// `Lib` for `.lib`, `Obj` for `.obj` (case-insensitive on the
    /// extension).
    pub kind: LinkInputKind,
}

/// Parse a `link.exe` command line.
pub fn parse(command_line: &str) -> LinkCommandLine {
    let mut tokens = tokenize(command_line).into_iter();
    let executable = tokens.next();
    let tokens: Vec<String> = tokens.collect();

    let mut out = LinkCommandLine {
        executable,
        ..Default::default()
    };

    for tok in &tokens {
        if let Some(body) = strip_switch_prefix(tok) {
            if let Some(value) = strip_named_value(body, "OUT") {
                if out.output.is_none() {
                    out.output = Some(unquote(value));
                } else {
                    out.other_switches.push(tok.clone());
                }
                continue;
            }
            if let Some(value) = strip_named_value(body, "LIBPATH") {
                out.lib_paths.push(unquote(value));
                continue;
            }
            out.other_switches.push(tok.clone());
            continue;
        }

        // Positional. Classify by extension.
        let raw = unquote(tok);
        if let Some(kind) = input_kind_for(&raw) {
            out.inputs.push(LinkCmdInput { path: raw, kind });
        } else {
            // Unknown positional (e.g. `@response.rsp`, or a
            // positional with an unexpected extension) — preserve
            // verbatim.
            out.other_switches.push(tok.clone());
        }
    }

    out
}

/// Return the switch body if `tok` starts with `/` or `-`.
fn strip_switch_prefix(tok: &str) -> Option<&str> {
    tok.strip_prefix('/').or_else(|| tok.strip_prefix('-'))
}

/// If `body` begins (case-insensitively) with `name` followed by
/// `:`, return the value after the colon; otherwise `None`.
fn strip_named_value<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    if body.len() < name.len() + 1 {
        return None;
    }
    let (head, rest) = body.split_at(name.len());
    if !head.eq_ignore_ascii_case(name) {
        return None;
    }
    rest.strip_prefix(':')
}

/// Classify a positional argument by extension.
fn input_kind_for(path: &str) -> Option<LinkInputKind> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".lib") {
        Some(LinkInputKind::Lib)
    } else if lower.ends_with(".obj") {
        Some(LinkInputKind::Obj)
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
