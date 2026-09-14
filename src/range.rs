//! Byte-range finding for `AR-PACKAGE`s and `ELEMENTS`-block elements,
//! used by `cp` and `rm` to copy/splice raw XML bytes without re-serialising.
//!
//! All three lookups below are simple filters over the same [`Tree`] built
//! once by `build_tree` (see [`crate::tree`]) — traversal happens exactly
//! once per file, regardless of how many of these are called.

use std::collections::HashSet;
use std::io::BufReader;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::tree::build_tree;
use crate::util::{local_name_str, open_file};

pub struct PackageRange {
    pub start: u64,
    pub end: u64,
    pub path: String,
}

/// Find byte ranges of top-level elements inside `<ELEMENTS>` blocks (direct
/// children of a package's `ELEMENTS`, e.g. `Root/Components/MyComponent`)
/// whose full path matches one of the `targets`.
pub fn find_element_ranges(path: &str, targets: &HashSet<&str>) -> Vec<PackageRange> {
    let tree = build_tree(path);

    tree.nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.tag != "AR-PACKAGE"
                && matches!(node.parent, Some(p) if tree.nodes[p].tag == "AR-PACKAGE")
        })
        .filter_map(|(idx, node)| {
            let full = tree.full_path(idx);
            let trimmed = full.trim_start_matches('/');
            targets.contains(trimmed).then(|| PackageRange {
                start: node.start,
                end: node.end,
                path: trimmed.to_string(),
            })
        })
        .collect()
}

/// Find byte ranges of `AR-PACKAGE`s (at any nesting depth) whose full path
/// matches one of the `targets`.
pub fn find_package_ranges(path: &str, targets: &HashSet<&str>) -> Vec<PackageRange> {
    let tree = build_tree(path);

    tree.nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.tag == "AR-PACKAGE")
        .filter_map(|(idx, node)| {
            let full = tree.full_path(idx);
            let trimmed = full.trim_start_matches('/');
            targets.contains(trimmed).then(|| PackageRange {
                start: node.start,
                end: node.end,
                path: trimmed.to_string(),
            })
        })
        .collect()
}

/// Find byte ranges of every top-level (root) `AR-PACKAGE`, regardless of name.
pub fn find_all_toplevel_package_ranges(path: &str) -> Vec<PackageRange> {
    let tree = build_tree(path);

    tree.nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.tag == "AR-PACKAGE" && node.parent.is_none())
        .map(|(idx, node)| PackageRange {
            start: node.start,
            end: node.end,
            path: tree.full_path(idx).trim_start_matches('/').to_string(),
        })
        .collect()
}

pub fn collect_root_attrs(path: &str) -> Vec<(String, String)> {
    let file = open_file(path);
    let reader = BufReader::new(file);
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut attrs = Vec::new();

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = local_name_str(e.local_name().as_ref());
                if name == "AUTOSAR" {
                    for attr in e.attributes().flatten() {
                        let key = std::str::from_utf8(attr.key.as_ref())
                            .unwrap_or("")
                            .to_string();
                        let val = attr.unescape_value().unwrap_or_default().into_owned();
                        attrs.push((key, val));
                    }
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    attrs
}
