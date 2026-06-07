// Copyright (c) Michael Grier

//! `link.exe /VERBOSE` output parser.
//!
//! Reads the per-Link-task message stream captured from a Link task
//! (see [`crate::walk::LinkInvocation::messages`]) together with the
//! parsed command line (see [`crate::link_cmdline::LinkCommandLine`])
//! and produces a structured view of:
//!
//! - **Inputs** — every `.obj` / `.lib` the linker considered, with
//!   `origin` (direct command-line vs. searched via `/DEFAULTLIB`
//!   or `/LIBPATH`) and a `referenced` bit inferred from whether
//!   any member was `Loaded` from the library.
//! - **Dropped** — every entry in the `Unused libraries:` section,
//!   tagged with `reason = "unused"`.
//!
//! See `DESIGN-NOTES.md` §D-CPP-LINK1 for the line classes and the
//! reference-inference rule.
//!
//! Paths are kept verbatim at this layer; rooting and aliasing are
//! the emitter's job (CPP-4.4).

use crate::link_cmdline::LinkCommandLine;
use crate::schema::{LinkInputKind, LinkInputOrigin};

/// Parser result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkVerbose {
    /// Every input the linker considered, in stable order: first
    /// the command-line inputs (in command-line order), then any
    /// libraries discovered via `/DEFAULTLIB` or by being searched
    /// without being explicitly named (`origin = Searched`).
    pub inputs: Vec<RawLinkInput>,
    /// Every entry in the `Unused libraries:` section, in the
    /// order they appeared.
    pub dropped: Vec<RawDroppedInput>,
    /// Count of `/DEFAULTLIB:<name>` directives observed without a
    /// matching `Searching` scope. These produced synthetic
    /// `defaultlib:<name>` inputs.
    pub synthetic_defaultlib_count: usize,
}

/// One entry in [`LinkVerbose::inputs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLinkInput {
    /// Path verbatim from the command line, or the resolved path
    /// from a `Searching <path>:` scope, or `defaultlib:<name>` for
    /// a `/DEFAULTLIB` directive whose target the linker never
    /// reported searching.
    pub path: String,
    pub kind: LinkInputKind,
    pub origin: LinkInputOrigin,
    /// `true` if at least one `Loaded` line appeared inside this
    /// input's `Searching <path>:` scope. Always `true` for `.obj`
    /// inputs (the linker does not list objs as unused).
    pub referenced: bool,
}

/// One entry in [`LinkVerbose::dropped`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDroppedInput {
    /// Path verbatim from the `Unused libraries:` section.
    pub path: String,
    /// Currently always `"unused"` (the only v1 reason vocabulary).
    pub reason: String,
}

const UNUSED_REASON: &str = "unused";

const PASS_START: &str = "Starting pass ";
const PASS_END: &str = "Finished pass ";
const DEFAULTLIB_PREFIX: &str = "Processed /DEFAULTLIB:";
const SEARCHING_LIBRARIES: &str = "Searching libraries";
const FINISHED_SEARCHING: &str = "Finished searching libraries";
const UNUSED_HEADER: &str = "Unused libraries:";
const SEARCHING_LIB_PREFIX: &str = "    Searching "; // 4 spaces
const LOADED_PREFIX: &str = "        Loaded "; // 8 spaces
const UNUSED_ENTRY_PREFIX: &str = "  "; // 2 spaces

/// Parse a Link task message stream against its command line.
pub fn parse(cmdline: &LinkCommandLine, messages: &[String]) -> LinkVerbose {
    let mut out = LinkVerbose::default();

    // Seed inputs from the command line, in order.
    for input in &cmdline.inputs {
        let referenced = matches!(input.kind, LinkInputKind::Obj);
        out.inputs.push(RawLinkInput {
            path: input.path.clone(),
            kind: input.kind,
            origin: LinkInputOrigin::Direct,
            referenced,
        });
    }

    // Walk messages, tracking the open Searching scope and the
    // Unused-libraries section state.
    let mut scopes: Vec<Scope> = Vec::new();
    let mut current_scope: Option<usize> = None;
    let mut in_unused = false;
    let mut defaultlibs: Vec<String> = Vec::new();

    for line in messages {
        // Section-terminator detection: any line that isn't a
        // 2-space-indented entry closes the Unused-libraries block.
        if in_unused && !is_unused_entry(line) {
            in_unused = false;
        }

        if let Some(rest) = line.strip_prefix(SEARCHING_LIB_PREFIX) {
            // Open a new Searching scope. The path is everything
            // between the prefix and the trailing colon.
            if let Some(path) = rest.strip_suffix(':') {
                scopes.push(Scope {
                    path: path.to_string(),
                    had_loaded: false,
                });
                current_scope = Some(scopes.len() - 1);
            }
            continue;
        }
        if line.starts_with(LOADED_PREFIX) {
            if let Some(idx) = current_scope {
                scopes[idx].had_loaded = true;
            }
            continue;
        }
        if line == FINISHED_SEARCHING || line == SEARCHING_LIBRARIES {
            current_scope = None;
            continue;
        }
        if line == UNUSED_HEADER {
            in_unused = true;
            current_scope = None;
            continue;
        }
        if in_unused
            && let Some(path) = line.strip_prefix(UNUSED_ENTRY_PREFIX)
            && !path.starts_with(' ')
        {
            out.dropped.push(RawDroppedInput {
                path: path.to_string(),
                reason: UNUSED_REASON.to_string(),
            });
            continue;
        }
        if let Some(name) = line.strip_prefix(DEFAULTLIB_PREFIX) {
            defaultlibs.push(name.to_string());
            continue;
        }
        if line.starts_with(PASS_START) || line.starts_with(PASS_END) {
            current_scope = None;
            continue;
        }
        // Everything else is `Other` per D-CPP-LINK1; ignored.
    }

    // Reconcile scopes with seeded inputs.
    //
    // For each scope: if its path matches a seeded input (full path
    // case-insensitive OR same basename case-insensitive), update
    // that input's `referenced` to `had_loaded || existing`. If no
    // match, append a new Searched input with the scope's path.
    for scope in &scopes {
        if let Some(idx) = find_matching_input_idx(&out.inputs, &scope.path) {
            if scope.had_loaded {
                out.inputs[idx].referenced = true;
            }
            // If this seeded input was Direct with no path-only name
            // (e.g. `absl_base.lib`), upgrade the stored path to the
            // resolved path the linker reported. This keeps the
            // emitter's rooting accurate.
            if !out.inputs[idx].path.contains(['\\', '/']) {
                out.inputs[idx].path = scope.path.clone();
            }
        } else {
            out.inputs.push(RawLinkInput {
                path: scope.path.clone(),
                kind: LinkInputKind::Lib,
                origin: LinkInputOrigin::Searched,
                referenced: scope.had_loaded,
            });
        }
    }

    // Apply Unused-libraries: section as the authoritative
    // "not referenced" signal — overrides scope inference if they
    // disagree.
    for dropped in &out.dropped {
        if let Some(idx) = find_matching_input_idx(&out.inputs, &dropped.path) {
            out.inputs[idx].referenced = false;
        }
    }

    // Add synthetic inputs for any /DEFAULTLIB:<name> that did not
    // correspond to an observed Searching scope.
    for name in &defaultlibs {
        if !defaultlib_has_scope(&scopes, name) {
            let path = format!("defaultlib:{}", name);
            if find_matching_input_idx(&out.inputs, &path).is_none() {
                out.inputs.push(RawLinkInput {
                    path,
                    kind: LinkInputKind::Lib,
                    origin: LinkInputOrigin::Searched,
                    referenced: false,
                });
                out.synthetic_defaultlib_count += 1;
            }
        }
    }

    out
}

struct Scope {
    path: String,
    had_loaded: bool,
}

fn is_unused_entry(line: &str) -> bool {
    // 2-space indent, third char not a space (so 4-space-indented
    // Searching lines don't get mistaken for unused entries).
    let bytes = line.as_bytes();
    bytes.len() >= 3 && bytes[0] == b' ' && bytes[1] == b' ' && bytes[2] != b' '
}

fn find_matching_input_idx(inputs: &[RawLinkInput], path: &str) -> Option<usize> {
    let target_full = path.to_ascii_lowercase();
    let target_base = path_basename(path).to_ascii_lowercase();
    inputs.iter().position(|i| {
        let full = i.path.to_ascii_lowercase();
        if full == target_full {
            return true;
        }
        path_basename(&i.path).to_ascii_lowercase() == target_base
    })
}

fn path_basename(p: &str) -> &str {
    let bytes = p.as_bytes();
    let mut split = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\\' || b == b'/' {
            split = i + 1;
        }
    }
    &p[split..]
}

fn defaultlib_has_scope(scopes: &[Scope], name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let with_ext = if lower.ends_with(".lib") {
        lower.clone()
    } else {
        format!("{lower}.lib")
    };
    scopes.iter().any(|s| {
        let base = path_basename(&s.path).to_ascii_lowercase();
        base == lower || base == with_ext
    })
}

#[cfg(test)]
mod tests;
