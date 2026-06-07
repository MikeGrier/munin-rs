// Copyright (c) Michael Grier

//! High-level entry point: open `BinlogIndex` → produce
//! [`CppBuildAnalysis`].
//!
//! Wraps the pipeline (`walk_projects` + `walk_cl_tasks` +
//! `walk_link_tasks` + `project_from_invocation`) and constructs the
//! top-level document with its `schema_version` / `header` block.
//!
//! See `DESIGN-NOTES.md` §D-CPPSCHEMA-1 for the document shape.

use munin_msbuild::BinlogIndex;

use crate::{
    cl_showincludes,
    emit::project_from_invocation,
    schema::{CppBuildAnalysis, Header, Root, SCHEMA_VERSION},
    walk::{walk_cl_tasks, walk_link_tasks, walk_projects},
};

/// How to handle CL `/showIncludes` streams whose locale is not
/// English (the only locale [`cl_showincludes::parse`] understands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LocaleStrategy {
    /// Silently produce an empty include tree for any TU whose
    /// locale could not be parsed. Header counts will appear low
    /// or zero but the analysis still completes. This matches the
    /// historical behavior of [`project_from_invocation`].
    #[default]
    BestEffort,
    /// Fail [`analyze`] with [`AnalyzeError::LocaleNotSupported`]
    /// at the first non-English CL task encountered.
    Strict,
}

/// Errors returned by [`analyze`].
#[derive(Debug)]
pub enum AnalyzeError {
    /// A walker over the binlog's events failed.
    Walk(Box<dyn std::error::Error + Send + Sync>),
    /// A CL task's `/showIncludes` stream was in an unsupported
    /// locale and [`LocaleStrategy::Strict`] was requested. The
    /// inner error carries the offending sample line.
    LocaleNotSupported {
        project_file: Option<String>,
        cause: cl_showincludes::LocaleNotSupportedError,
    },
}

impl std::fmt::Display for AnalyzeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalyzeError::Walk(e) => write!(f, "walk binlog: {e}"),
            AnalyzeError::LocaleNotSupported {
                project_file,
                cause,
            } => {
                if let Some(p) = project_file {
                    write!(f, "/showIncludes locale not supported in {p}: {cause}")
                } else {
                    write!(f, "/showIncludes locale not supported: {cause}")
                }
            }
        }
    }
}

impl std::error::Error for AnalyzeError {}

/// Analyze a parsed binlog and produce the top-level
/// [`CppBuildAnalysis`] document.
///
/// - `source_binlog` is recorded verbatim into `header.source_binlog`;
///   the CLI passes the input path as supplied by the user.
/// - `roots` is the document's path-root table (also recorded into
///   `header.roots`). Pass an empty slice to disable rooting.
/// - `locale_strategy` controls behavior on non-English locales —
///   see [`LocaleStrategy`].
pub fn analyze(
    index: &BinlogIndex,
    source_binlog: &str,
    roots: &[Root],
    locale_strategy: LocaleStrategy,
) -> Result<CppBuildAnalysis, AnalyzeError> {
    let projects = walk_projects(index).map_err(|e| AnalyzeError::Walk(Box::new(e)))?;
    let cls = walk_cl_tasks(index).map_err(|e| AnalyzeError::Walk(Box::new(e)))?;
    let links = walk_link_tasks(index).map_err(|e| AnalyzeError::Walk(Box::new(e)))?;

    if locale_strategy == LocaleStrategy::Strict {
        for cl in &cls {
            if let Err(cause) = cl_showincludes::parse(&cl.messages) {
                let project_file = projects
                    .iter()
                    .find(|p| p.project_context_id == cl.project_context_id)
                    .and_then(|p| p.project_file.clone());
                return Err(AnalyzeError::LocaleNotSupported {
                    project_file,
                    cause,
                });
            }
        }
    }

    let built = projects
        .iter()
        .map(|p| project_from_invocation(p, &cls, &links, roots))
        .collect();

    Ok(CppBuildAnalysis {
        schema_version: SCHEMA_VERSION,
        header: Header {
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            source_binlog: source_binlog.to_string(),
            roots: roots.to_vec(),
        },
        projects: built,
    })
}

/// Discover a single path-root candidate from the projects in
/// `index`: the longest common parent directory of every
/// `project_file` recorded in the binlog.
///
/// Returns `None` when there are no projects, when project paths
/// span multiple drives / UNC shares, or when the only common
/// prefix is a drive letter (`C:\`) — too coarse to be useful as a
/// root.
///
/// The returned root is named after the leaf directory of the
/// common ancestor (e.g. `C:\src\product` → `name = "product"`).
pub fn auto_detect_root(index: &BinlogIndex) -> Result<Option<Root>, AnalyzeError> {
    let projects = walk_projects(index).map_err(|e| AnalyzeError::Walk(Box::new(e)))?;
    let parents: Vec<String> = projects
        .iter()
        .filter_map(|p| p.project_file.as_deref())
        .filter_map(parent_dir)
        .collect();
    if parents.is_empty() {
        return Ok(None);
    }
    let mut common = parents[0].clone();
    for p in parents.iter().skip(1) {
        common = longest_common_prefix(&common, p);
        if common.is_empty() {
            return Ok(None);
        }
    }
    // Require at least one named directory beyond a drive letter or
    // UNC root marker (a bare drive letter or `\\` is not useful).
    let comps: Vec<&str> = split_components(&common).into_iter().collect();
    let named: Vec<&str> = comps.iter().copied().filter(|c| !c.is_empty()).collect();
    if named.is_empty() || (named.len() == 1 && is_drive_letter(named[0])) {
        return Ok(None);
    }
    let name = named.last().copied().unwrap_or("root").to_string();
    Ok(Some(Root { name, path: common }))
}

/// Split a (possibly Windows-style) path into components on either
/// `\` or `/`. The result preserves empty components produced by
/// leading separators so that UNC paths (`\\server\share`) can still
/// be recognized by callers.
fn split_components(path: &str) -> Vec<&str> {
    path.split(['\\', '/']).collect()
}

/// Return `true` iff `s` is a Windows drive-letter prefix (e.g.
/// `C:`).
fn is_drive_letter(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Return the parent directory of `path` as a string, treating `\`
/// and `/` as separators regardless of host platform.
fn parent_dir(path: &str) -> Option<String> {
    let last = path.rfind(['\\', '/'])?;
    Some(path[..last].to_string())
}

/// Compute the longest common path prefix of `a` and `b`, comparing
/// components case-insensitively for ASCII (matching Windows volume
/// / filesystem semantics for ASCII paths). Either `\` or `/` is
/// accepted as a separator on input; the result is rejoined with
/// `\` for Windows-native output.
fn longest_common_prefix(a: &str, b: &str) -> String {
    let ac = split_components(a);
    let bc = split_components(b);
    let mut common: Vec<&str> = Vec::new();
    for (ca, cb) in ac.iter().zip(bc.iter()) {
        if ca.eq_ignore_ascii_case(cb) {
            common.push(ca);
        } else {
            break;
        }
    }
    common.join("\\")
}

#[cfg(test)]
mod tests;
