// Copyright (c) Michael Grier

//! High-level entry point: open `BinlogIndex` → produce
//! [`CppBuildAnalysis`].
//!
//! Wraps the pipeline (`walk_projects` + `walk_cl_tasks` +
//! `walk_link_tasks` + `project_from_invocation`) and constructs the
//! top-level document with its `schema_version` / `header` block.
//!
//! See `DESIGN-NOTES.md` §D-CPPSCHEMA-1 for the document shape.

use std::path::{Component, Path, PathBuf};

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
    let mut parents: Vec<PathBuf> = projects
        .iter()
        .filter_map(|p| p.project_file.as_deref())
        .filter_map(|s| Path::new(s).parent().map(Path::to_path_buf))
        .collect();
    if parents.is_empty() {
        return Ok(None);
    }
    let mut common = parents.swap_remove(0);
    for p in &parents {
        common = longest_common_prefix(&common, p);
        if common.as_os_str().is_empty() {
            return Ok(None);
        }
    }
    // Require at least one non-prefix component beyond the root
    // (a bare drive letter or UNC share is not useful).
    let component_count = common.components().count();
    let has_only_prefix_or_root = common
        .components()
        .all(|c| matches!(c, Component::Prefix(_) | Component::RootDir));
    if component_count <= 1 || has_only_prefix_or_root {
        return Ok(None);
    }
    let name = common
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "root".to_string());
    Ok(Some(Root {
        name,
        path: common.to_string_lossy().into_owned(),
    }))
}

fn longest_common_prefix(a: &Path, b: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for (ca, cb) in a.components().zip(b.components()) {
        if ca == cb {
            out.push(ca);
        } else {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests;
