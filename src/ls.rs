//! `arx ls`: list `AR-PACKAGE`s and elements in an ARXML file, with optional
//! path filter, max-depth, exclude patterns, and type filter.

use crate::path_match::{path_under_pattern, wildcard_match};
use crate::tree::{build_tree, Tree};
use crate::util::normalise_path;

pub fn cmd_ls(
    path: &str,
    filter: Option<&str>,
    max_depth: Option<usize>,
    excludes: &[String],
    type_filter: &[String],
) {
    for line in ls_collect(path, filter, max_depth, excludes, type_filter) {
        println!("{}", line);
    }
}

/// Applies all `ls` filtering options to a [`Tree`] built by `build_tree`,
/// returning the paths that should be printed, in the same order the nodes
/// were encountered while building the tree (i.e. document order).
///
/// - `max_depth`: `None` means unlimited (the default — every matching node,
///   however deep). `Some(0)` means only the exact `filter` match itself (or
///   nothing, if there's no filter — there's no real node for the implicit
///   root). `Some(n)` means the filter match plus up to `n` levels of
///   descendants. Without a filter, depth is counted from the implicit root,
///   so `Some(1)` yields exactly the top-level AR-PACKAGEs.
/// - `type_filter`: if non-empty, only include nodes whose own XML tag
///   matches one of the given names. `AR-PACKAGE` is a valid tag name here
///   like any other — there's no automatic package/element distinction.
fn query_tree(
    tree: &Tree,
    filter: Option<&str>,
    max_depth: Option<usize>,
    excludes: &[String],
    type_filter: &[String],
) -> Vec<String> {
    let filter = filter.map(|f| normalise_path(f));
    let excludes: Vec<String> = excludes.iter().map(|e| normalise_path(e)).collect();
    // Depth of the filter path (0 = no filter, 1 = /Root, 2 = /Root/Components, ...)
    let filter_depth = filter.as_deref().map(|f| f.split('/').count()).unwrap_or(0);

    let mut results = Vec::new();

    for idx in 0..tree.nodes.len() {
        let node = &tree.nodes[idx];

        // Cheap check first: skip entirely without ever building the full
        // path if the type doesn't match.
        if !type_filter.is_empty() && !type_filter.iter().any(|t| t == &node.tag) {
            continue;
        }

        let full = tree.full_path(idx);

        if !is_within_depth(&full, filter.as_deref(), filter_depth, max_depth) {
            continue;
        }
        if is_excluded(&full, &excludes) {
            continue;
        }

        results.push(full);
    }

    results
}

/// Core logic of `ls`: returns the list of paths that would be printed.
/// Separated from `cmd_ls` so it can be called in tests without capturing
/// stdout. Internally builds a [`Tree`] of the whole file once (see
/// `build_tree`), then filters it (see [`query_tree`]).
pub fn ls_collect(
    path: &str,
    filter: Option<&str>,
    max_depth: Option<usize>,
    excludes: &[String],
    type_filter: &[String],
) -> Vec<String> {
    let tree = build_tree(path);
    query_tree(&tree, filter, max_depth, excludes, type_filter)
}

/// Returns true if `full_path` should be printed, given an optional `filter`
/// pattern (see [`path_under_pattern`]) and an optional `max_depth` relative
/// to that filter (or to the implicit root, if there's no filter):
/// - `None`: unlimited depth.
/// - `Some(0)`: only the filter match itself.
/// - `Some(n)`: the filter match plus up to `n` levels of descendants.
///
/// Mirrors `find`'s `-maxdepth`: the starting point counts as depth 0.
fn is_within_depth(
    full_path: &str,
    filter: Option<&str>,
    filter_depth: usize,
    max_depth: Option<usize>,
) -> bool {
    if let Some(f) = filter {
        if !path_under_pattern(full_path, f) {
            return false;
        }
    }
    match max_depth {
        None => true,
        Some(max) => {
            let path_depth = full_path.trim_start_matches('/').split('/').count();
            let rel_depth = path_depth.saturating_sub(filter_depth);
            rel_depth <= max
        }
    }
}

/// Returns true if `full_path` is excluded by one of the (normalised)
/// `excludes` patterns. A pattern excludes a path if it matches the path
/// itself, or any of its ancestor package paths (so excluding a package
/// also excludes everything nested underneath it). Patterns may contain
/// `*` as a wildcard matching any sequence of characters (including `/`),
/// e.g. `ComponentTypes/Dummy*`.
fn is_excluded(full_path: &str, excludes: &[String]) -> bool {
    let trimmed = full_path.trim_start_matches('/');
    let segments: Vec<&str> = trimmed.split('/').collect();
    excludes.iter().any(|pattern| {
        (1..=segments.len()).any(|i| wildcard_match(pattern, &segments[..i].join("/")))
    })
}
