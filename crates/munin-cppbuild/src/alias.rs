// Copyright (c) Michael Grier

//! Alias-table construction (D-CPP-ALIAS-1).
//!
//! Given the set of [`RootedPath`]s that appear inside a single
//! project invocation, produce a deterministic alias for each one.
//!
//! ## Decision — D-CPP-ALIAS-1
//!
//! For each path, candidate aliases are tried in order:
//!
//! 1. **Leaf only.** If the leaf name (last segment) is unique among
//!    all paths in the project, the alias is the leaf.
//! 2. **Last two segments** joined by `\`. For paths whose leaf
//!    collided in step 1, if their last-two-segment string is unique
//!    among the still-colliding set, that becomes the alias.
//! 3. **First segment + leaf** joined by `\..\`. For paths still
//!    colliding after step 2, if `<first-segment>\..\<leaf>` is
//!    unique among the still-colliding set, that becomes the alias.
//!    (This form is intentionally evocative for paths like NuGet
//!    package layouts: the first segment is typically the package
//!    name.)
//! 4. **Leaf with counter** `<leaf>#n`. For any paths still
//!    colliding after step 3, assign `<leaf>#1`, `<leaf>#2`, …
//!    deterministically by sorting the colliding paths.
//!
//! Segments are derived from [`RootedPath::path`] split on `\`
//! (forward slashes are already normalized by `path_root`). The
//! root index / root name is **not** part of the segment list — two
//! paths with identical relative components but different roots will
//! collide and fall through to the `#n` form.
//!
//! ## Determinism
//!
//! All grouping and tie-breaks use `BTreeMap` or sorted iteration on
//! [`RootedPath`]'s derived `Ord`. The same input set always
//! produces the same alias table.

use std::collections::BTreeMap;

use crate::schema::RootedPath;

/// Per-project mapping `alias → RootedPath` plus the reverse lookup.
///
/// The forward map is exactly the value emitted as
/// `Project::alias_table` in the JSON schema (D-CPPSCHEMA-1).
#[derive(Debug, Clone, Default)]
pub struct AliasTable {
    forward: BTreeMap<String, RootedPath>,
    reverse: BTreeMap<RootedPath, String>,
}

impl AliasTable {
    /// Build an alias table from the distinct paths referenced by a
    /// project. Duplicate inputs are deduplicated; insertion order
    /// of the input does not affect the result.
    pub fn build<I>(paths: I) -> Self
    where
        I: IntoIterator<Item = RootedPath>,
    {
        // Deduplicate and sort for deterministic processing.
        let mut distinct: Vec<RootedPath> = paths.into_iter().collect();
        distinct.sort();
        distinct.dedup();

        let mut assigned: BTreeMap<RootedPath, String> = BTreeMap::new();

        // ── Pass 1: leaf unique? ───────────────────────────────
        let mut by_leaf: BTreeMap<String, Vec<&RootedPath>> = BTreeMap::new();
        for p in &distinct {
            by_leaf.entry(leaf_of(p)).or_default().push(p);
        }
        let mut still_colliding: Vec<&RootedPath> = Vec::new();
        for (leaf, group) in &by_leaf {
            if group.len() == 1 {
                assigned.insert((*group[0]).clone(), leaf.clone());
            } else {
                still_colliding.extend(group.iter().copied());
            }
        }

        // ── Pass 2: last-two-segments unique? ──────────────────
        let remaining = std::mem::take(&mut still_colliding);
        let mut by_last_two: BTreeMap<String, Vec<&RootedPath>> = BTreeMap::new();
        for p in &remaining {
            by_last_two
                .entry(last_two_segments(p))
                .or_default()
                .push(*p);
        }
        for (key, group) in &by_last_two {
            if group.len() == 1 {
                assigned.insert((*group[0]).clone(), key.clone());
            } else {
                still_colliding.extend(group.iter().copied());
            }
        }

        // ── Pass 3: first-segment + leaf unique? ───────────────
        let remaining = std::mem::take(&mut still_colliding);
        let mut by_first_leaf: BTreeMap<String, Vec<&RootedPath>> = BTreeMap::new();
        for p in &remaining {
            by_first_leaf
                .entry(first_segment_and_leaf(p))
                .or_default()
                .push(*p);
        }
        for (key, group) in &by_first_leaf {
            if group.len() == 1 {
                assigned.insert((*group[0]).clone(), key.clone());
            } else {
                still_colliding.extend(group.iter().copied());
            }
        }

        // ── Pass 4: <leaf>#n fallback. ─────────────────────────
        let mut by_leaf_fallback: BTreeMap<String, Vec<&RootedPath>> = BTreeMap::new();
        for p in &still_colliding {
            by_leaf_fallback.entry(leaf_of(p)).or_default().push(*p);
        }
        for (leaf, mut group) in by_leaf_fallback {
            // Group came from BTreeMap iteration order; sort once
            // more on RootedPath for deterministic #n assignment.
            group.sort();
            for (i, p) in group.iter().enumerate() {
                let alias = format!("{}#{}", leaf, i + 1);
                assigned.insert((*p).clone(), alias);
            }
        }

        let mut forward: BTreeMap<String, RootedPath> = BTreeMap::new();
        for (path, alias) in &assigned {
            forward.insert(alias.clone(), path.clone());
        }

        AliasTable {
            forward,
            reverse: assigned,
        }
    }

    /// Look up the alias for a previously-built path.
    pub fn alias_for(&self, path: &RootedPath) -> Option<&str> {
        self.reverse.get(path).map(|s| s.as_str())
    }

    /// The number of distinct paths in the table.
    pub fn len(&self) -> usize {
        self.forward.len()
    }

    /// True iff the table is empty.
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    /// Consume self and return the alias → path map as it will be
    /// emitted in `Project::alias_table`.
    pub fn into_map(self) -> BTreeMap<String, RootedPath> {
        self.forward
    }

    /// Borrow the alias → path map without consuming.
    pub fn as_map(&self) -> &BTreeMap<String, RootedPath> {
        &self.forward
    }
}

// ── Segment helpers ────────────────────────────────────────────────

fn segments(p: &RootedPath) -> Vec<&str> {
    p.path.split('\\').filter(|s| !s.is_empty()).collect()
}

fn leaf_of(p: &RootedPath) -> String {
    segments(p).last().copied().unwrap_or("").to_string()
}

fn last_two_segments(p: &RootedPath) -> String {
    let segs = segments(p);
    match segs.len() {
        0 => String::new(),
        1 => segs[0].to_string(),
        _ => format!("{}\\{}", segs[segs.len() - 2], segs[segs.len() - 1]),
    }
}

fn first_segment_and_leaf(p: &RootedPath) -> String {
    let segs = segments(p);
    match segs.len() {
        0 => String::new(),
        1 => segs[0].to_string(),
        _ => format!("{}\\..\\{}", segs[0], segs[segs.len() - 1]),
    }
}

#[cfg(test)]
mod tests;
