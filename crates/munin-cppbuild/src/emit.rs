// Copyright (c) Michael Grier

//! Project-level JSON emitter.
//!
//! Converts an intermediate [`ProjectInvocation`] plus the CL and
//! Link task invocations that ran inside it (produced by
//! [`crate::walk::walk_projects`] / [`crate::walk::walk_cl_tasks`] /
//! [`crate::walk::walk_link_tasks`]) into the on-the-wire
//! [`crate::schema::Project`] shape.
//!
//! See `DESIGN-NOTES.md` §D-CPPSCHEMA-1 for the document shape,
//! §D-CPP-PROPSRC1 for the property-source attribution rule,
//! §D-CPP-SHOWINC for the include-tree mapping, and §D-CPP-LINK1 for
//! the link verbose parsing rules.

use std::collections::{BTreeMap, HashMap};

use crate::{
    alias::AliasTable,
    cl_cmdline::{self, ClCommandLine},
    cl_showincludes::{self, LocaleNotSupportedError, RawIncludeNode, ShowIncludes},
    link_cmdline::{self, LinkCommandLine},
    link_verbose::{self, LinkVerbose},
    path_root::to_rooted,
    schema::{
        Define, DroppedInput, IncludeNode, LinkInput, Output, Project, Root, RootedPath, Source,
    },
    walk::{CompileInvocation, LinkInvocation, ProjectInvocation},
};

/// Emit a [`crate::schema::Project`] for a single project invocation,
/// joining in every CL and Link task that ran inside it.
///
/// `cl_invs` and `link_invs` may contain invocations from other
/// projects in the same binlog — they are filtered by
/// `project_context_id` before use.
///
/// The per-project alias table is built over the **union** of every
/// path the project references (project file, CL source files,
/// `/I` include directories, every header from `/showIncludes`,
/// Link output, every Link input, and every `Unused` library), so
/// every alias surfaced in `sources[]` and `outputs[]` is also a key
/// in `alias_table`.
pub fn project_from_invocation(
    inv: &ProjectInvocation,
    cl_invs: &[CompileInvocation],
    link_invs: &[LinkInvocation],
    roots: &[Root],
) -> Project {
    let project_path = match inv.project_file.as_deref() {
        Some(p) => to_rooted(p, roots),
        None => RootedPath {
            root: None,
            path: String::new(),
        },
    };

    let cl_parses: Vec<ClParse> = cl_invs
        .iter()
        .filter(|c| c.project_context_id == inv.project_context_id)
        .map(ClParse::new)
        .collect();
    let link_parses: Vec<LinkParse> = link_invs
        .iter()
        .filter(|l| l.project_context_id == inv.project_context_id)
        .map(LinkParse::new)
        .collect();

    // ── Pass 1: collect the path universe and root each path. ──
    let mut rooted: HashMap<String, RootedPath> = HashMap::new();
    fn intern(raw: &str, roots: &[Root], rooted: &mut HashMap<String, RootedPath>) {
        if !raw.is_empty() && !rooted.contains_key(raw) {
            rooted.insert(raw.to_string(), to_rooted(raw, roots));
        }
    }

    if let Some(p) = inv.project_file.as_deref() {
        intern(p, roots, &mut rooted);
    }
    for cl in &cl_parses {
        // CPP-4.6.2 single-source bridge: emit.rs still treats one
        // CL invocation as one TU; CPP-4.6.4 will iterate every
        // source. For now collect every cmdline source into the
        // path universe so that batch-mode cmdlines don't silently
        // drop paths.
        for src in &cl.cmd.sources {
            intern(src, roots, &mut rooted);
        }
        for inc in &cl.cmd.include_paths {
            intern(inc, roots, &mut rooted);
        }
        if let Ok(si) = &cl.includes {
            for tu in &si.translation_units {
                for h in &tu.included_files {
                    intern(h, roots, &mut rooted);
                }
                walk_raw_tree(&tu.tree, &mut |p| intern(p, roots, &mut rooted));
            }
        }
    }
    for lk in &link_parses {
        if let Some(out) = lk.cmd.output.as_deref() {
            intern(out, roots, &mut rooted);
        }
        for input in &lk.verbose.inputs {
            intern(&input.path, roots, &mut rooted);
        }
        for drop in &lk.verbose.dropped {
            intern(&drop.path, roots, &mut rooted);
        }
    }

    let aliases = AliasTable::build(rooted.values().cloned());

    // ── Pass 2: build sub-objects using aliases. ────────────────
    let alias_of = |raw: &str| -> String {
        let rp = rooted
            .get(raw)
            .unwrap_or_else(|| panic!("path was not interned in pass 1: {raw}"));
        aliases
            .alias_for(rp)
            .unwrap_or_else(|| panic!("no alias for path: {raw}"))
            .to_string()
    };

    let sources: Vec<Source> = cl_parses
        .iter()
        .map(|cl| build_source(cl, &alias_of))
        .collect();
    let outputs: Vec<Output> = link_parses
        .iter()
        .map(|lk| build_output(lk, &alias_of))
        .collect();

    let alias_table = aliases.into_map();

    Project {
        project_path,
        platform: inv.platform().unwrap_or("").to_string(),
        configuration: inv.configuration().unwrap_or("").to_string(),
        global_properties: inv.to_global_properties(),
        alias_table,
        sources,
        outputs,
    }
}

// ── Internals ──────────────────────────────────────────────────────

struct ClParse<'a> {
    inv: &'a CompileInvocation,
    cmd: ClCommandLine,
    includes: Result<ShowIncludes, LocaleNotSupportedError>,
}

impl<'a> ClParse<'a> {
    fn new(inv: &'a CompileInvocation) -> Self {
        let cmd = cl_cmdline::parse(inv.command_line.as_deref().unwrap_or(""));
        let includes = cl_showincludes::parse(&inv.messages);
        ClParse { inv, cmd, includes }
    }
}

struct LinkParse<'a> {
    inv: &'a LinkInvocation,
    cmd: LinkCommandLine,
    verbose: LinkVerbose,
}

impl<'a> LinkParse<'a> {
    fn new(inv: &'a LinkInvocation) -> Self {
        let cmd = link_cmdline::parse(inv.command_line.as_deref().unwrap_or(""));
        let verbose = link_verbose::parse(&cmd, &inv.messages);
        LinkParse { inv, cmd, verbose }
    }
}

fn build_source(cl: &ClParse, alias_of: &dyn Fn(&str) -> String) -> Source {
    // CPP-4.6.2 single-source bridge: take the first cmdline
    // source. Full per-TU emission lands in CPP-4.6.4.
    let path = cl
        .cmd
        .sources
        .first()
        .map(|s| alias_of(s))
        .unwrap_or_default();
    let include_paths = cl.cmd.include_paths.iter().map(|p| alias_of(p)).collect();
    let defines = cl
        .cmd
        .defines
        .iter()
        .map(|d| Define {
            name: d.name.clone(),
            value: d.value.clone(),
        })
        .collect();

    let (includes, included_files) = match &cl.includes {
        Ok(si) => si
            .translation_units
            .first()
            .map(|tu| {
                (
                    build_includes_map(&tu.tree, alias_of),
                    tu.included_files.iter().map(|p| alias_of(p)).collect(),
                )
            })
            .unwrap_or_else(|| (BTreeMap::new(), Vec::new())),
        Err(_) => (BTreeMap::new(), Vec::new()),
    };

    Source {
        path,
        command_line: cl.inv.command_line.clone().unwrap_or_default(),
        include_paths,
        defines,
        includes,
        included_files,
    }
}

fn build_includes_map(
    nodes: &[RawIncludeNode],
    alias_of: &dyn Fn(&str) -> String,
) -> BTreeMap<String, IncludeNode> {
    let mut out = BTreeMap::new();
    for n in nodes {
        let alias = alias_of(&n.resolved_path);
        let children = build_includes_map(&n.children, alias_of);
        out.insert(
            alias.clone(),
            IncludeNode {
                file: alias,
                children,
            },
        );
    }
    out
}

fn walk_raw_tree(nodes: &[RawIncludeNode], visit: &mut impl FnMut(&str)) {
    for n in nodes {
        visit(&n.resolved_path);
        walk_raw_tree(&n.children, visit);
    }
}

fn build_output(lk: &LinkParse, alias_of: &dyn Fn(&str) -> String) -> Output {
    let path = lk.cmd.output.as_deref().map(alias_of).unwrap_or_default();
    let inputs = lk
        .verbose
        .inputs
        .iter()
        .map(|i| LinkInput {
            path: alias_of(&i.path),
            kind: i.kind,
            origin: i.origin,
            referenced: i.referenced,
        })
        .collect();
    let dropped = lk
        .verbose
        .dropped
        .iter()
        .map(|d| DroppedInput {
            path: alias_of(&d.path),
            reason: d.reason.clone(),
        })
        .collect();

    Output {
        path,
        command_line: lk.inv.command_line.clone().unwrap_or_default(),
        inputs,
        dropped,
    }
}

#[cfg(test)]
mod tests;
