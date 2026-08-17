use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write};

use quick_xml::events::Event;
use quick_xml::Reader;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct PackageRange {
    pub start: u64,
    pub end: u64,
    pub path: String,
}

/// One output group for cp: a list of package paths and the target file.
pub struct CpGroup {
    pub packages: Vec<String>,
    pub output: String,
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

pub fn normalise_path(p: &str) -> String {
    p.trim().trim_start_matches('/').to_string()
}

pub fn open_file(path: &str) -> File {
    File::open(path).unwrap_or_else(|e| {
        eprintln!("Error opening file '{}': {}", path, e);
        std::process::exit(1);
    })
}

pub fn local_name_str(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes).unwrap_or("").to_string()
}

pub fn write_arxml_header<W: Write>(out: &mut BufWriter<W>, root_attrs: &[(String, String)]) {
    writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    write!(out, "<AUTOSAR").unwrap();
    for (k, v) in root_attrs {
        write!(out, r#" {}="{}""#, k, v).unwrap();
    }
    writeln!(out, ">").unwrap();
    writeln!(out, "  <AR-PACKAGES>").unwrap();
}

pub fn write_arxml_footer<W: Write>(out: &mut BufWriter<W>) {
    writeln!(out, "  </AR-PACKAGES>").unwrap();
    writeln!(out, "</AUTOSAR>").unwrap();
}

// ---------------------------------------------------------------------------
// ls
// ---------------------------------------------------------------------------

pub fn cmd_ls(path: &str, show_elements: bool, filter: Option<&str>, recursive: bool) {
    for line in ls_collect(path, show_elements, filter, recursive) {
        println!("{}", line);
    }
}

/// Core logic of `ls`: returns the list of paths that would be printed.
/// Separated from `cmd_ls` so it can be called in tests without capturing stdout.
pub fn ls_collect(path: &str, show_elements: bool, filter: Option<&str>, recursive: bool) -> Vec<String> {
    let filter = filter.map(|f| normalise_path(f));
    // Depth of the filter path (0 = no filter, 1 = /Root, 2 = /Root/Components, ...)
    let filter_depth = filter.as_deref().map(|f| f.split('/').count()).unwrap_or(0);

    let file = open_file(path);
    let reader = BufReader::new(file);
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut package_stack: Vec<String> = Vec::new();
    let mut capturing_short_name = false;
    let mut in_elements = false;
    let mut elements_pkg_depth: usize = 0;
    let mut element_tag_depth: usize = 0;
    let mut capture_depth: usize = 0;
    let mut depth: usize = 0;
    let mut results: Vec<String> = Vec::new();

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let name = local_name_str(e.local_name().as_ref());

                if name == "AR-PACKAGE" {
                    capture_depth = depth;
                    in_elements = false;
                    element_tag_depth = 0;
                } else if name == "SHORT-NAME"
                    && capture_depth > 0
                    && depth == capture_depth + 1
                    && !in_elements
                    && element_tag_depth == 0
                {
                    capturing_short_name = true;
                } else if show_elements && name == "ELEMENTS" && capture_depth > 0 && depth == capture_depth + 1 {
                    in_elements = true;
                    elements_pkg_depth = capture_depth;
                } else if show_elements && in_elements && depth == elements_pkg_depth + 2 {
                    element_tag_depth = depth;
                } else if show_elements && in_elements && element_tag_depth > 0
                    && name == "SHORT-NAME" && depth == element_tag_depth + 1
                {
                    capturing_short_name = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                if capturing_short_name {
                    let short_name = e.unescape().unwrap_or_default().into_owned();
                    capturing_short_name = false;

                    if in_elements && element_tag_depth > 0 {
                        let full = format!("/{}/{}", package_stack.join("/"), short_name);
                        // Element is a direct child of package_stack's current package.
                        // Print it when the parent package would be visible:
                        // - in recursive mode: parent just needs to be under filter
                        // - in non-recursive mode: parent must be at exactly filter_depth
                        //   (i.e. the filtered package itself, or top-level if no filter)
                        let parent_depth = package_stack.len();
                        let element_visible = match filter.as_deref() {
                            None => {
                                if recursive { true } else { parent_depth == filter_depth }
                            }
                            Some(f) => {
                                let parent = format!("/{}", package_stack.join("/"));
                                let filter_with_slash = format!("/{}", f);
                                let under = parent == filter_with_slash
                                    || parent.starts_with(&format!("{}/", filter_with_slash));
                                if !under { false }
                                else if recursive { true }
                                else { parent_depth == filter_depth + 1 || parent == filter_with_slash }
                            }
                        };
                        if element_visible {
                            results.push(full);
                        }
                        element_tag_depth = 0;
                    } else {
                        package_stack.push(short_name);
                        let full = format!("/{}", package_stack.join("/"));
                        if should_print(&full, filter.as_deref(), filter_depth, recursive) {
                            results.push(full);
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = local_name_str(e.local_name().as_ref());
                if name == "AR-PACKAGE" {
                    package_stack.pop();
                    in_elements = false;
                    element_tag_depth = 0;
                } else if name == "ELEMENTS" {
                    in_elements = false;
                    element_tag_depth = 0;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                eprintln!("XML parse error: {}", e);
                std::process::exit(1);
            }
            _ => {}
        }
        buf.clear();
    }

    results
}

/// Returns true if `full_path` should be printed.
/// - filter: optional prefix the path must be under (or equal to)
/// - filter_depth: number of segments in the filter path
/// - recursive: if false, only print direct children (depth == filter_depth + 1)
fn should_print(full_path: &str, filter: Option<&str>, filter_depth: usize, recursive: bool) -> bool {
    // Check prefix constraint
    let under_filter = match filter {
        None => true,
        Some(f) => {
            let filter_with_slash = format!("/{}", f);
            full_path == filter_with_slash
                || full_path.starts_with(&format!("{}/", filter_with_slash))
        }
    };
    if !under_filter {
        return false;
    }
    if recursive {
        return true;
    }
    // Non-recursive: only print at exactly filter_depth + 1
    let path_depth = full_path.trim_start_matches('/').split('/').count();
    path_depth == filter_depth + 1
}

// ---------------------------------------------------------------------------
// cp
// ---------------------------------------------------------------------------

/// Parse arguments after `cp <input>`:
/// `<pkg1> [<pkg2>...] --into <out1> [<pkg3>...] --into <out2> [--rest <rest>]`
pub fn parse_cp_args(args: &[String]) -> (Vec<CpGroup>, Option<String>) {
    let mut groups: Vec<CpGroup> = Vec::new();
    let mut rest_file: Option<String> = None;
    let mut pending_pkgs: Vec<String> = Vec::new();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--into" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --into requires a filename argument.");
                    std::process::exit(1);
                }
                groups.push(CpGroup {
                    packages: pending_pkgs.drain(..).map(|s| normalise_path(&s)).collect(),
                    output: args[i].clone(),
                });
            }
            "--rest" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --rest requires a filename argument.");
                    std::process::exit(1);
                }
                rest_file = Some(args[i].clone());
            }
            pkg => {
                pending_pkgs.push(pkg.to_string());
            }
        }
        i += 1;
    }

    if !pending_pkgs.is_empty() {
        eprintln!("Error: packages {:?} have no --into target.", pending_pkgs);
        std::process::exit(1);
    }

    (groups, rest_file)
}

pub fn cmd_cp(input: &str, groups: &[CpGroup], rest_file: Option<&str>) {
    let all_targets: HashSet<&str> = groups
        .iter()
        .flat_map(|g| g.packages.iter().map(|s| s.as_str()))
        .collect();

    let root_attrs = collect_root_attrs(input);
    let all_ranges = find_package_ranges(input, &all_targets);

    if all_ranges.is_empty() {
        eprintln!("No matching packages found.");
        std::process::exit(1);
    }

    let range_by_path: HashMap<&str, &PackageRange> =
        all_ranges.iter().map(|r| (r.path.as_str(), r)).collect();

    let mut src = File::open(input).unwrap();

    for group in groups {
        let out_file = File::create(&group.output).unwrap_or_else(|e| {
            eprintln!("Cannot create output file '{}': {}", group.output, e);
            std::process::exit(1);
        });
        let mut out = BufWriter::new(out_file);
        write_arxml_header(&mut out, &root_attrs);

        for pkg in &group.packages {
            if let Some(range) = range_by_path.get(pkg.as_str()) {
                src.seek(SeekFrom::Start(range.start)).unwrap();
                let len = (range.end - range.start) as usize;
                let mut block = vec![0u8; len];
                src.read_exact(&mut block).unwrap();
                out.write_all(&block).unwrap();
                writeln!(out).unwrap();
            } else {
                eprintln!("Warning: package '{}' not found in input.", pkg);
            }
        }

        write_arxml_footer(&mut out);
        println!("Written to '{}'", group.output);
    }

    if let Some(rest_path) = rest_file {
        let mut matched: Vec<&PackageRange> = all_ranges.iter().collect();
        matched.sort_by_key(|r| r.start);

        let mut raw = Vec::new();
        open_file(input).read_to_end(&mut raw).unwrap();

        let out_file = File::create(rest_path).unwrap_or_else(|e| {
            eprintln!("Cannot create rest file '{}': {}", rest_path, e);
            std::process::exit(1);
        });
        let mut out = BufWriter::new(out_file);
        write_arxml_header(&mut out, &root_attrs);

        let all_toplevel = find_all_toplevel_package_ranges(input);
        for range in &all_toplevel {
            let is_matched = matched.iter().any(|m| m.start == range.start);
            if !is_matched {
                out.write_all(&raw[range.start as usize..range.end as usize])
                    .unwrap();
                writeln!(out).unwrap();
            }
        }

        write_arxml_footer(&mut out);
        println!("Written rest to '{}'", rest_path);
    }
}

// ---------------------------------------------------------------------------
// rm
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// diff
// ---------------------------------------------------------------------------

const COLOR_RED: &str = "\x1b[31m";
const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_RESET: &str = "\x1b[0m";

/// Collect all AR-PACKAGE and ELEMENTS paths from an ARXML file as a sorted vec.
/// If `filter` is given, only paths under that AR-PACKAGE prefix are returned.
pub fn collect_all_paths(path: &str, filter: Option<&str>) -> Vec<String> {
    let mut paths = ls_collect(path, true, filter, true);
    paths.sort();
    paths
}

/// Compare the AR-PACKAGE / ELEMENTS structure of two ARXML files and print
/// coloured `+`/`-` lines for entries that differ.
/// If `filter` is given, only paths under that AR-PACKAGE prefix are compared.
///
/// Returns `true` if the files are identical, `false` if differences were found.
pub fn cmd_diff(file_a: &str, file_b: &str, filter: Option<&str>) -> bool {
    let paths_a: std::collections::HashSet<String> =
        collect_all_paths(file_a, filter).into_iter().collect();
    let paths_b: std::collections::HashSet<String> =
        collect_all_paths(file_b, filter).into_iter().collect();

    // Removed: in A but not in B
    let mut removed: Vec<&String> = paths_a.difference(&paths_b).collect();
    removed.sort();

    // Added: in B but not in A
    let mut added: Vec<&String> = paths_b.difference(&paths_a).collect();
    added.sort();

    if removed.is_empty() && added.is_empty() {
        return true;
    }

    println!("{}--- {}{}", COLOR_RED, file_a, COLOR_RESET);
    println!("{}+++ {}{}", COLOR_GREEN, file_b, COLOR_RESET);
    println!();

    for path in &removed {
        println!("{}-{} {}{}", COLOR_RED, COLOR_RESET, COLOR_RED, path);
        print!("{}", COLOR_RESET);
    }
    for path in &added {
        println!("{}+{} {}{}", COLOR_GREEN, COLOR_RESET, COLOR_GREEN, path);
        print!("{}", COLOR_RESET);
    }

    false
}

// ---------------------------------------------------------------------------
// Range finding
// ---------------------------------------------------------------------------

/// Find byte ranges of individual elements inside `<ELEMENTS>` blocks whose
/// full path (`Package/SubPkg/ShortName`) matches one of the `targets`.
///
/// The returned `PackageRange.path` is the full slash-separated path of the element.
pub fn find_element_ranges(path: &str, targets: &HashSet<&str>) -> Vec<PackageRange> {
    let mut raw = Vec::new();
    open_file(path).read_to_end(&mut raw).unwrap();
    let cursor = Cursor::new(&raw);
    let mut xml = Reader::from_reader(cursor);
    xml.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut pkg_stack: Vec<String> = Vec::new();
    let mut depth: usize = 0;

    let mut in_elements = false;
    let mut elements_depth: usize = 0;
    let mut element_tag_depth: usize = 0;
    let mut element_start: u64 = 0;

    let mut read_short_name = false;
    let mut sn_context: &str = "pkg"; // "pkg" | "element"

    let mut ranges: Vec<PackageRange> = Vec::new();
    let mut pos_before: u64;

    loop {
        pos_before = xml.buffer_position() as u64;
        let event = xml.read_event_into(&mut buf);
        let pos_after = xml.buffer_position() as u64;

        match event {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let name = local_name_str(e.local_name().as_ref());

                if name == "AR-PACKAGE" {
                    // nothing special yet; SHORT-NAME reading set below
                } else if name == "ELEMENTS" && !in_elements {
                    in_elements = true;
                    elements_depth = depth;
                } else if in_elements && depth == elements_depth + 1 && element_tag_depth == 0 {
                    element_tag_depth = depth;
                    element_start = pos_before;
                }

                if name == "SHORT-NAME" {
                    if in_elements && element_tag_depth > 0 && depth == element_tag_depth + 1 {
                        read_short_name = true;
                        sn_context = "element";
                    } else if !in_elements {
                        read_short_name = true;
                        sn_context = "pkg";
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                if read_short_name {
                    let raw_text = e.unescape().unwrap_or_default();
                    let trimmed = raw_text.trim();
                    if trimmed.is_empty() {
                        buf.clear();
                        continue;
                    }
                    read_short_name = false;
                    pkg_stack.push(trimmed.to_string());
                    let _ = sn_context;
                }
            }
            Ok(Event::End(ref e)) => {
                let name = local_name_str(e.local_name().as_ref());

                if in_elements && element_tag_depth > 0 && depth == element_tag_depth {
                    // Closing tag of an element — finalise range if it matches
                    if let Some(short_name) = pkg_stack.last().cloned() {
                        let parent = if pkg_stack.len() >= 2 {
                            pkg_stack[..pkg_stack.len() - 1].join("/")
                        } else {
                            String::new()
                        };
                        let full = format!("{}/{}", parent, short_name);
                        if targets.contains(full.as_str()) {
                            ranges.push(PackageRange {
                                start: element_start,
                                end: pos_after,
                                path: full,
                            });
                        }
                        pkg_stack.pop();
                    }
                    element_tag_depth = 0;
                } else if name == "ELEMENTS" && in_elements && depth == elements_depth {
                    in_elements = false;
                    elements_depth = 0;
                } else if name == "AR-PACKAGE" && !in_elements {
                    pkg_stack.pop();
                }

                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                eprintln!("XML parse error: {}", e);
                std::process::exit(1);
            }
            _ => {}
        }
        buf.clear();
    }

    ranges
}

pub fn find_package_ranges(path: &str, targets: &HashSet<&str>) -> Vec<PackageRange> {
    let mut raw = Vec::new();
    open_file(path).read_to_end(&mut raw).unwrap();
    let cursor = Cursor::new(&raw);
    let mut xml = Reader::from_reader(cursor);
    xml.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut pkg_stack: Vec<String> = Vec::new();
    let mut depth: usize = 0;

    let mut read_short_name = false;
    let mut sn_for_depth: usize = 0;
    let mut capture: Option<(usize, u64)> = None;
    let mut pos_before: u64;
    let mut ar_pkg_start_positions: Vec<u64> = Vec::new();
    let mut ranges: Vec<PackageRange> = Vec::new();

    loop {
        pos_before = xml.buffer_position() as u64;
        let event = xml.read_event_into(&mut buf);
        let pos_after = xml.buffer_position() as u64;

        match event {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let name = local_name_str(e.local_name().as_ref());
                if name == "AR-PACKAGE" {
                    ar_pkg_start_positions.push(pos_before);
                    sn_for_depth = depth;
                } else if name == "SHORT-NAME" && sn_for_depth > 0 && depth == sn_for_depth + 1 {
                    read_short_name = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                if read_short_name {
                    let raw_text = e.unescape().unwrap_or_default();
                    let trimmed = raw_text.trim();
                    if trimmed.is_empty() {
                        buf.clear();
                        continue;
                    }
                    read_short_name = false;
                    sn_for_depth = 0;
                    pkg_stack.push(trimmed.to_string());

                    if capture.is_none() {
                        let current_path = pkg_stack.join("/");
                        if targets.contains(current_path.as_str()) {
                            let start = *ar_pkg_start_positions.last().unwrap();
                            capture = Some((pkg_stack.len(), start));
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = local_name_str(e.local_name().as_ref());
                if name == "AR-PACKAGE" {
                    ar_pkg_start_positions.pop();
                    if let Some((cap_len, start)) = capture {
                        if pkg_stack.len() == cap_len {
                            ranges.push(PackageRange {
                                start,
                                end: pos_after,
                                path: pkg_stack.join("/"),
                            });
                            capture = None;
                        }
                    }
                    pkg_stack.pop();
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                eprintln!("XML parse error: {}", e);
                std::process::exit(1);
            }
            _ => {}
        }
        buf.clear();
    }

    ranges
}

pub fn find_all_toplevel_package_ranges(path: &str) -> Vec<PackageRange> {
    let mut raw = Vec::new();
    open_file(path).read_to_end(&mut raw).unwrap();
    let cursor = Cursor::new(&raw);
    let mut xml = Reader::from_reader(cursor);
    xml.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut depth: usize = 0;
    let mut pkg_stack: Vec<String> = Vec::new();

    let mut read_short_name = false;
    let mut sn_for_depth: usize = 0;
    let mut toplevel_depth: Option<usize> = None;
    let mut capture: Option<(usize, u64)> = None;
    let mut ar_pkg_start_positions: Vec<u64> = Vec::new();
    let mut ranges: Vec<PackageRange> = Vec::new();
    let mut pos_before: u64;

    loop {
        pos_before = xml.buffer_position() as u64;
        let event = xml.read_event_into(&mut buf);
        let pos_after = xml.buffer_position() as u64;

        match event {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let name = local_name_str(e.local_name().as_ref());
                if name == "AR-PACKAGE" {
                    ar_pkg_start_positions.push(pos_before);
                    sn_for_depth = depth;
                    if toplevel_depth.is_none() {
                        toplevel_depth = Some(depth);
                    }
                    if capture.is_none() && Some(depth) == toplevel_depth {
                        capture = Some((depth, pos_before));
                    }
                } else if name == "SHORT-NAME" && sn_for_depth > 0 && depth == sn_for_depth + 1 {
                    read_short_name = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                if read_short_name {
                    let raw_text = e.unescape().unwrap_or_default();
                    let trimmed = raw_text.trim();
                    if trimmed.is_empty() {
                        buf.clear();
                        continue;
                    }
                    read_short_name = false;
                    sn_for_depth = 0;
                    pkg_stack.push(trimmed.to_string());
                }
            }
            Ok(Event::End(ref e)) => {
                let name = local_name_str(e.local_name().as_ref());
                if name == "AR-PACKAGE" {
                    ar_pkg_start_positions.pop();
                    if let Some((cap_depth, start)) = capture {
                        if depth == cap_depth {
                            ranges.push(PackageRange {
                                start,
                                end: pos_after,
                                path: pkg_stack.join("/"),
                            });
                            capture = None;
                        }
                    }
                    pkg_stack.pop();
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                eprintln!("XML parse error: {}", e);
                std::process::exit(1);
            }
            _ => {}
        }
        buf.clear();
    }

    ranges
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
