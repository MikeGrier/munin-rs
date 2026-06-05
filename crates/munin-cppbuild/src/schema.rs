// Copyright (c) Michael Grier

//! Rust types for the C++ build analysis JSON schema.
//!
//! Specification: see `DESIGN-NOTES.md` §D-CPPSCHEMA-1. The shapes here
//! match the JSON exactly; serde renames keep field names in
//! `snake_case` (the convention chosen in the spec).
//!
//! `BTreeMap` is used for any map-typed field so JSON emission is
//! alphabetical by key (per the schema's determinism rules).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Schema version of the emitted JSON document. Bump when any field
/// is renamed, removed, or changes type. Readers must reject documents
/// whose `schema_version` does not match.
pub const SCHEMA_VERSION: u32 = 1;

// ── Document root ──────────────────────────────────────────────────

/// Top-level analysis document — one per binlog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CppBuildAnalysis {
    pub schema_version: u32,
    pub header: Header,
    pub projects: Vec<Project>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub tool_version: String,
    pub source_binlog: String,
    pub roots: Vec<Root>,
}

/// One entry in the document-level path-root table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    pub name: String,
    pub path: String,
}

// ── Paths ──────────────────────────────────────────────────────────

/// A path expressed relative to a [`Root`], or absolute if outside
/// every root.
///
/// `root` is the index into the document's `header.roots`; `None`
/// means the path is absolute. `path` uses the host's native
/// separator for emission but readers must accept both `\\` and `/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootedPath {
    pub root: Option<usize>,
    pub path: String,
}

// ── Project ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub project_path: RootedPath,
    pub platform: String,
    pub configuration: String,
    pub global_properties: Vec<GlobalProperty>,
    pub alias_table: BTreeMap<String, RootedPath>,
    pub sources: Vec<Source>,
    pub outputs: Vec<Output>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalProperty {
    pub name: String,
    pub value: String,
    pub source: PropertySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertySource {
    CommandLine,
    Project,
}

// ── Source (per CL translation unit) ───────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Alias of the `.cpp` / `.c` translation unit.
    pub path: String,
    pub command_line: String,
    /// Aliases of `/I` include directories, in command-line order.
    pub include_paths: Vec<String>,
    pub defines: Vec<Define>,
    /// Resolved include tree from `/showIncludes`. v1 keys are
    /// resolved-file aliases (CPP-3.4); a future revision may switch
    /// to `#include` directive text.
    pub includes: BTreeMap<String, IncludeNode>,
    /// Flat dedup of every header consumed, first-encounter order.
    pub included_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Define {
    pub name: String,
    /// `None` for bare `/Dfoo`; `Some` for `/Dfoo=bar` (including
    /// `Some("")` for `/Dfoo=`).
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncludeNode {
    /// Alias of the resolved header — duplicated from the parent's
    /// map key for self-containment.
    pub file: String,
    pub children: BTreeMap<String, IncludeNode>,
}

// ── Output (per Link invocation) ───────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    pub path: String,
    pub command_line: String,
    pub inputs: Vec<LinkInput>,
    pub dropped: Vec<DroppedInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkInput {
    pub path: String,
    pub kind: LinkInputKind,
    pub origin: LinkInputOrigin,
    pub referenced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkInputKind {
    Obj,
    Lib,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkInputOrigin {
    Direct,
    Transitive,
    Searched,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedInput {
    pub path: String,
    /// Concrete reason vocabulary is locked in D-CPP-LINK1 after the
    /// CPP-4.1 spike. Treated as free-form short tag for now.
    pub reason: String,
}

#[cfg(test)]
mod tests;
