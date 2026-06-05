// Copyright (c) Michael Grier

//! Project-level JSON emitter.
//!
//! Converts an intermediate [`ProjectInvocation`] (produced by
//! [`crate::walk::walk_projects`]) into the on-the-wire
//! [`schema::Project`] shape. M2 emits empty `sources` and `outputs`
//! vectors; CL and Link records are joined into the project in later
//! milestones.
//!
//! See `DESIGN-NOTES.md` §D-CPPSCHEMA-1 for the document shape and
//! §D-CPP-PROPSRC1 for the property-source attribution rule.

use crate::{
    alias::AliasTable,
    path_root::to_rooted,
    schema::{Project, Root, RootedPath},
    walk::ProjectInvocation,
};

/// Emit a [`schema::Project`] from a [`ProjectInvocation`] using
/// `roots` for path canonicalization.
///
/// M2 contract:
///
/// - `project_path` is the project file path resolved through
///   [`to_rooted`]. If the invocation reported no project file, an
///   empty absolute `RootedPath` is emitted (`root: None, path: ""`).
/// - `platform` and `configuration` are taken from the invocation
///   (globals first, then property-list fallback). Missing values
///   become empty strings — the schema requires the fields but they
///   are documentation-only when the binlog did not record them.
/// - `global_properties` is the result of
///   [`ProjectInvocation::to_global_properties`] (all entries marked
///   `command_line`; see D-CPP-PROPSRC1).
/// - `alias_table` is built over the single rooted project path so the
///   per-project alias scope is non-empty even before sources / outputs
///   are attached.
/// - `sources` and `outputs` are empty (CPP-3.x / CPP-4.x will fill
///   them).
pub fn project_from_invocation(inv: &ProjectInvocation, roots: &[Root]) -> Project {
    let project_path = match inv.project_file.as_deref() {
        Some(p) => to_rooted(p, roots),
        None => RootedPath {
            root: None,
            path: String::new(),
        },
    };

    let alias_table = AliasTable::build(std::iter::once(project_path.clone())).into_map();

    Project {
        project_path,
        platform: inv.platform().unwrap_or("").to_string(),
        configuration: inv.configuration().unwrap_or("").to_string(),
        global_properties: inv.to_global_properties(),
        alias_table,
        sources: Vec::new(),
        outputs: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
