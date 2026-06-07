// Copyright (c) Michael Grier

//! `cl.exe /showIncludes` output parser.
//!
//! Reads the per-translation-unit message stream captured from a CL
//! task and reconstructs each TU's resolved-header include tree
//! plus a flat first-encounter-ordered list of every header consumed.
//!
//! See `DESIGN-NOTES.md` §D-CPP-SHOWINC-1 for the line format,
//! depth-computation rule, and the non-English-locale detection;
//! §D-CPP-SHOWINC2 for the batch-mode per-TU boundary marker rule.
//!
//! A single CL task may carry **N ≥ 1** translation units (cl.exe
//! batch mode); the parser splits the message stream into one
//! [`TuIncludes`] per TU using the bare-basename boundary markers
//! cl.exe emits between TUs.
//!
//! Lines that are not `Note: including file:` messages — compiler
//! diagnostics, build progress, file-name banners — are ignored
//! unless they match the boundary-marker pattern. When a known
//! non-English equivalent of `Note: including file:` is
//! encountered, [`parse`] returns [`LocaleNotSupportedError`] so the
//! caller can flag the binlog instead of silently emitting an empty
//! tree. English-locale `cl.exe` is the only supported tier in v1
//! (see D-CPP-5).

use std::collections::HashSet;

/// English `cl.exe` `/showIncludes` prefix. Match is **exact**
/// (case-sensitive) per cl.exe's actual emission.
const EN_PREFIX: &str = "Note: including file:";

/// Known non-English equivalents. Best-effort: a message starting
/// with one of these is unambiguously the localized form of the
/// English prefix, so we can fail loudly with
/// [`LocaleNotSupportedError`] instead of returning an empty tree.
///
/// Not exhaustive. Other locales may slip through and produce an
/// empty tree; the caller can still detect this via a zero-element
/// [`TuIncludes::included_files`] on a TU that should have had
/// includes.
const NON_ENGLISH_PREFIXES: &[(&str, &str)] = &[
    ("Remarque\u{a0}: inclusion du fichier\u{a0}:", "fr"),
    ("Remarque : inclusion du fichier :", "fr"),
    ("Hinweis: Einlesen der Datei:", "de"),
    ("Nota: incluyendo archivo:", "es"),
    ("Nota: file incluso:", "it"),
    (
        "\u{6ce8}\u{610f}: \u{5305}\u{542b}\u{6587}\u{4ef6}:",
        "zh-CN",
    ),
    (
        "\u{30e1}\u{30e2}: \u{30a4}\u{30f3}\u{30af}\u{30eb}\u{30fc}\u{30c9} \u{30d5}\u{30a1}\u{30a4}\u{30eb}:",
        "ja",
    ),
];

/// Parser result: every translation unit's includes, split by the
/// per-TU boundary markers cl.exe emits in batch mode.
///
/// See §D-CPP-SHOWINC2 for the split rule.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShowIncludes {
    /// One entry per translation unit, in cmdline order. Empty when
    /// the message stream contained neither a boundary marker nor
    /// any include line.
    pub translation_units: Vec<TuIncludes>,
    /// Count of messages that started with the include-line prefix
    /// but whose depth violated the "increases by at most 1" rule
    /// (and were therefore skipped). Summed across all TUs. Always
    /// 0 in well-formed input; non-zero indicates the source stream
    /// was reordered or truncated.
    pub malformed_message_count: usize,
}

/// Per-translation-unit include data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TuIncludes {
    /// Bare basename from the TU's boundary marker (e.g.
    /// `"AgentMonitoring.cpp"`), or `None` for includes that
    /// appeared before any marker. In well-formed batch-mode
    /// output this is always `Some`; the `None` case arises when
    /// a non-batch tool (or a test fixture) emits includes
    /// directly with no preceding marker.
    pub source_name: Option<String>,
    /// Top-level includes (depth 1) in the order they appeared.
    pub tree: Vec<RawIncludeNode>,
    /// Every resolved header path that appeared anywhere in this
    /// TU's tree, deduplicated, in first-encounter (depth-first)
    /// order. Deduplication is **per TU**, not across the whole
    /// invocation — the same header may appear in multiple TUs.
    pub included_files: Vec<String>,
}

/// Raw include-tree node at the parser layer.
///
/// Resolved-path strings are stored verbatim as `cl.exe` emitted
/// them — no normalization, no aliasing. The aliasing layer
/// (CPP-1.4) converts these into the schema-level alias-keyed tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawIncludeNode {
    pub resolved_path: String,
    pub children: Vec<RawIncludeNode>,
}

/// Returned by [`parse`] when the input stream is in a non-English
/// `cl.exe` locale we recognize but do not support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleNotSupportedError {
    /// ISO 639-1 (or BCP-47) tag of the detected locale.
    pub locale: String,
    /// Verbatim copy of the first message that triggered the
    /// detection — useful for diagnostics and for adding new locale
    /// patterns later.
    pub sample_message: String,
}

impl std::fmt::Display for LocaleNotSupportedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cl.exe /showIncludes output is in non-English locale '{}'; only English is supported in v1 (sample: {:?})",
            self.locale, self.sample_message
        )
    }
}

impl std::error::Error for LocaleNotSupportedError {}

/// Parse a sequence of CL task messages into per-TU include trees.
///
/// `messages` is the raw `Message` event stream captured under a
/// single CL task invocation (see `walk::CompileInvocation::messages`).
/// Order is significant: the depth-stack is rebuilt as we go, and
/// boundary markers segment the stream into one TU per source file
/// in cmdline order.
///
/// Boundary detection: a message line is a boundary marker iff it
/// matches `^[A-Za-z0-9_.+-]+\.(c|cc|cpp|cxx|c\+\+)$` (case-
/// insensitive). See §D-CPP-SHOWINC2.
pub fn parse<S: AsRef<str>>(messages: &[S]) -> Result<ShowIncludes, LocaleNotSupportedError> {
    // First sweep: bail out loudly if we see a known non-English
    // prefix anywhere in the stream.
    for msg in messages {
        let text = msg.as_ref();
        for (prefix, locale) in NON_ENGLISH_PREFIXES {
            if text.starts_with(prefix) {
                return Err(LocaleNotSupportedError {
                    locale: (*locale).to_string(),
                    sample_message: text.to_string(),
                });
            }
        }
    }

    let mut out = ShowIncludes::default();
    // Indices into the current TU's tree identifying the lineage
    // of "current parent at each depth". `path[i]` is the index of
    // the depth-(i+1) node on the current ancestor chain.
    let mut path: Vec<usize> = Vec::new();
    // Index into `out.translation_units` of the currently-open TU.
    // `None` means no TU has been opened yet (no boundary marker
    // and no include line has been seen).
    let mut current_tu: Option<usize> = None;

    for msg in messages {
        let text = msg.as_ref();

        if let Some(basename) = boundary_marker(text) {
            // Open a new TU. Reset the depth-tracking stack.
            out.translation_units.push(TuIncludes {
                source_name: Some(basename.to_string()),
                ..TuIncludes::default()
            });
            current_tu = Some(out.translation_units.len() - 1);
            path.clear();
            continue;
        }

        let Some((depth, resolved)) = parse_include_line(text) else {
            continue;
        };

        // Depth must be in 1..=path.len()+1. Greater than that means
        // a level was skipped (malformed); record and drop.
        if depth == 0 || depth > path.len() + 1 {
            out.malformed_message_count += 1;
            continue;
        }

        // If no TU has been opened yet, open an anonymous one so
        // pre-marker include lines are still captured.
        let tu_idx = match current_tu {
            Some(i) => i,
            None => {
                out.translation_units.push(TuIncludes::default());
                let i = out.translation_units.len() - 1;
                current_tu = Some(i);
                i
            }
        };
        let tu = &mut out.translation_units[tu_idx];

        path.truncate(depth - 1);
        let node = RawIncludeNode {
            resolved_path: resolved.to_string(),
            children: Vec::new(),
        };

        if depth == 1 {
            tu.tree.push(node);
            path.push(tu.tree.len() - 1);
        } else {
            let parent_children = walk_to_children(&mut tu.tree, &path);
            parent_children.push(node);
            path.push(parent_children.len() - 1);
        }
    }

    // Per-TU flat dedup, depth-first first-encounter order.
    fn dfs(nodes: &[RawIncludeNode], seen: &mut HashSet<String>, flat: &mut Vec<String>) {
        for n in nodes {
            if seen.insert(n.resolved_path.clone()) {
                flat.push(n.resolved_path.clone());
            }
            dfs(&n.children, seen, flat);
        }
    }
    for tu in &mut out.translation_units {
        let mut seen: HashSet<String> = HashSet::new();
        let mut flat: Vec<String> = Vec::new();
        dfs(&tu.tree, &mut seen, &mut flat);
        tu.included_files = flat;
    }

    Ok(out)
}

/// If `line` is a TU boundary marker (a bare source-file basename
/// at column 0 with a recognized C/C++ source extension), return
/// the basename. Otherwise `None`. See §D-CPP-SHOWINC2.
///
/// Match is whole-string and case-insensitive on the extension.
/// The basename body accepts ASCII alphanumerics plus `_ . + -`
/// which covers all real-world MSBuild-emitted source names.
fn boundary_marker(line: &str) -> Option<&str> {
    // No leading whitespace; no embedded spaces.
    if line.is_empty() || line.starts_with(char::is_whitespace) {
        return None;
    }
    if line.bytes().any(|b| b == b' ' || b == b'\t') {
        return None;
    }
    // Must not be an include line (defensive; include lines start
    // with `Note: ` which contains a space, so already filtered).
    if line.starts_with(EN_PREFIX) {
        return None;
    }
    // Validate basename body characters.
    if !line
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'+' | b'-'))
    {
        return None;
    }
    // Must end with a known source extension (case-insensitive).
    let lower = line.to_ascii_lowercase();
    const EXTS: &[&str] = &[".cpp", ".cxx", ".cc", ".c", ".c++"];
    if !EXTS.iter().any(|e| lower.ends_with(e)) {
        return None;
    }
    // Must have a non-empty stem (rejects bare `.cpp`).
    let stem_len = EXTS
        .iter()
        .find_map(|e| lower.ends_with(e).then(|| line.len() - e.len()))
        .unwrap_or(0);
    if stem_len == 0 {
        return None;
    }
    Some(line)
}

/// If `line` is an English `Note: including file:` message, return
/// `(depth, resolved_path)`. Depth is the count of leading spaces
/// after the prefix's trailing colon; `cl.exe` emits at least one
/// space, so depth is always `>= 1` for a well-formed line.
fn parse_include_line(line: &str) -> Option<(usize, &str)> {
    let rest = line.strip_prefix(EN_PREFIX)?;
    // Count leading ASCII spaces.
    let depth = rest.bytes().take_while(|&b| b == b' ').count();
    if depth == 0 {
        return None;
    }
    let path = &rest[depth..];
    if path.is_empty() {
        return None;
    }
    Some((depth, path))
}

/// Walk `path` (a chain of child indices) into `roots` and return a
/// mutable reference to the deepest node's children vec. `path` must
/// be non-empty; callers handle the empty (depth-1) case separately.
fn walk_to_children<'a>(
    roots: &'a mut [RawIncludeNode],
    path: &[usize],
) -> &'a mut Vec<RawIncludeNode> {
    let mut current = &mut roots[path[0]];
    for &idx in &path[1..] {
        current = &mut current.children[idx];
    }
    &mut current.children
}

#[cfg(test)]
mod tests;
