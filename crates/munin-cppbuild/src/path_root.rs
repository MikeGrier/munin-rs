// Copyright (c) Michael Grier

//! Multi-root path canonicalization (D-CPP-PATHROOT-1).
//!
//! Given an absolute path and an ordered list of root directories,
//! produce a [`RootedPath`]: either `(root_index, relative_path)` for
//! the first root that contains the path, or `(None, absolute_path)`
//! when the path is outside every root.
//!
//! ## Decision — D-CPP-PATHROOT-1
//!
//! 1. **Ordered first-match.** Roots are scanned in supplied order.
//!    A path inside two roots is attributed to the first match. This
//!    is observable in the document (`root` index), so the caller's
//!    root order is part of the contract.
//! 2. **Separator normalization.** `/` is treated as equivalent to
//!    `\` for both root paths and input paths. The emitted relative
//!    or absolute string always uses `\` (Windows-native; this tool
//!    targets MSBuild binlogs).
//! 3. **Case-insensitive ASCII fold.** Root containment is checked
//!    with ASCII case-folding (matching Windows volume / filesystem
//!    semantics for ASCII paths). Non-ASCII bytes compare
//!    case-sensitively. This is a documented limitation; in practice
//!    Windows build paths are overwhelmingly ASCII.
//! 4. **Component-boundary match.** A root prefix matches only if it
//!    ends at a path-component boundary in the input. Root
//!    `C:\src\foo` does not match input `C:\src\foobar\baz.cpp`.
//! 5. **Trailing separators on roots are ignored** (`C:\src\` and
//!    `C:\src` are equivalent root specifications).
//! 6. **Path equals root exactly** → relative `""`.

use std::path::Path;

use crate::schema::{Root, RootedPath};

/// Canonicalize `path` against `roots`, returning a [`RootedPath`].
///
/// `path` should be absolute; relative inputs are not rejected but
/// will only match a root when the literal prefix happens to match.
pub fn to_rooted(path: impl AsRef<Path>, roots: &[Root]) -> RootedPath {
    let path = path.as_ref();
    let path_str = path.to_string_lossy();
    let normalized = normalize_separators(&path_str);

    for (idx, root) in roots.iter().enumerate() {
        let root_norm = normalize_separators(&root.path);
        if let Some(rel) = strip_root_prefix_ci(&normalized, &root_norm) {
            return RootedPath {
                root: Some(idx),
                path: rel,
            };
        }
    }

    RootedPath {
        root: None,
        path: normalized,
    }
}

/// Replace every `/` with `\` and collapse no other structure.
fn normalize_separators(s: &str) -> String {
    s.replace('/', "\\")
}

/// Return the portion of `haystack` after `needle`, with the leading
/// separator stripped, iff `needle` matches at a component boundary
/// using ASCII case-folding. Trailing separators on `needle` are
/// ignored.
fn strip_root_prefix_ci(haystack: &str, needle: &str) -> Option<String> {
    let needle = needle.trim_end_matches('\\');
    let n = needle.len();

    if n == 0 {
        // An empty root would match everything; treat as "no match"
        // to avoid surprises. Callers should not pass empty roots.
        return None;
    }
    if haystack.len() < n {
        return None;
    }
    if !haystack.is_char_boundary(n) {
        return None;
    }
    let (head, tail) = haystack.split_at(n);
    if !head.eq_ignore_ascii_case(needle) {
        return None;
    }
    if tail.is_empty() {
        return Some(String::new());
    }
    // Component boundary: next byte must be a separator.
    if tail.as_bytes()[0] != b'\\' {
        return None;
    }
    Some(tail[1..].to_string())
}

#[cfg(test)]
mod tests;
