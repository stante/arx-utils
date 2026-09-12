//! `arx rm`: remove `AR-PACKAGE`s or individual `ELEMENTS` from an ARXML
//! file, overwriting it in-place.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufWriter, Read, Write};

use crate::range::{collect_root_attrs, find_all_toplevel_package_ranges, find_element_ranges, PackageRange};
use crate::util::{normalise_path, open_file, write_arxml_footer, write_arxml_header};

/// Parse arguments after `rm <input>`:
/// `<pkg1> [<pkg2>...]`
pub fn parse_rm_args(args: &[String]) -> Vec<String> {
    args.iter().map(|a| normalise_path(a)).collect()
}

/// Remove the given AR-PACKAGE or ELEMENT blocks from `input`, overwriting the file in-place.
///
/// Paths that match a top-level (or nested) AR-PACKAGE are removed entirely.
/// Paths that match an element inside an `<ELEMENTS>` block (three-segment paths
/// like `Root/Components/MyComponent`) remove only that element tag.
pub fn cmd_rm(input: &str, packages: &[String]) {
    let to_remove: HashSet<&str> = packages.iter().map(|s| s.as_str()).collect();

    let root_attrs = collect_root_attrs(input);
    let all_toplevel = find_all_toplevel_package_ranges(input);

    let mut raw = Vec::new();
    open_file(input).read_to_end(&mut raw).unwrap();

    // Collect element ranges for paths that look like element references
    // (i.e. not matched by any top-level package range).
    let toplevel_paths: HashSet<&str> = all_toplevel.iter().map(|r| r.path.as_str()).collect();
    let element_targets: HashSet<&str> = to_remove
        .iter()
        .copied()
        .filter(|p| !toplevel_paths.contains(p))
        .collect();

    let element_ranges = if element_targets.is_empty() {
        vec![]
    } else {
        find_element_ranges(input, &element_targets)
    };

    let mut buf: Vec<u8> = Vec::new();
    {
        let mut out = BufWriter::new(&mut buf);
        write_arxml_header(&mut out, &root_attrs);

        let mut any_removed = false;

        for range in &all_toplevel {
            let norm = normalise_path(&range.path);
            if to_remove.contains(norm.as_str()) {
                any_removed = true;
                continue;
            }

            // Collect element ranges that fall inside this top-level package.
            let mut inner: Vec<&PackageRange> = element_ranges
                .iter()
                .filter(|er| er.start >= range.start && er.end <= range.end)
                .collect();

            if inner.is_empty() {
                // No element deletions inside this package — copy verbatim.
                out.write_all(&raw[range.start as usize..range.end as usize])
                    .unwrap();
                writeln!(out).unwrap();
            } else {
                // Copy the package bytes, skipping the element ranges.
                inner.sort_by_key(|r| r.start);
                let mut cursor = range.start as usize;
                for er in &inner {
                    if cursor < er.start as usize {
                        out.write_all(&raw[cursor..er.start as usize]).unwrap();
                    }
                    cursor = er.end as usize;
                    any_removed = true;
                }
                if cursor < range.end as usize {
                    out.write_all(&raw[cursor..range.end as usize]).unwrap();
                }
                writeln!(out).unwrap();
            }
        }

        write_arxml_footer(&mut out);

        if !any_removed {
            eprintln!("Warning: none of the specified packages or elements were found in the input.");
        }
    }

    let out_file = File::create(input).unwrap_or_else(|e| {
        eprintln!("Cannot write to file '{}': {}", input, e);
        std::process::exit(1);
    });
    BufWriter::new(out_file).write_all(&buf).unwrap();
}
