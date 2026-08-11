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

pub fn cmd_ls(path: &str) {
    let file = open_file(path);
    let reader = BufReader::new(file);
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut package_stack: Vec<String> = Vec::new();
    let mut capturing_short_name = false;
    let mut capture_depth: usize = 0;
    let mut depth: usize = 0;

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let name = local_name_str(e.local_name().as_ref());
                if name == "AR-PACKAGE" {
                    capture_depth = depth;
                } else if name == "SHORT-NAME"
                    && capture_depth > 0
                    && depth == capture_depth + 1
                {
                    capturing_short_name = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                if capturing_short_name {
                    let short_name = e.unescape().unwrap_or_default().into_owned();
                    package_stack.push(short_name);
                    println!("/{}", package_stack.join("/"));
                    capturing_short_name = false;
                    capture_depth = 0;
                }
            }
            Ok(Event::End(ref e)) => {
                let name = local_name_str(e.local_name().as_ref());
                if name == "AR-PACKAGE" {
                    package_stack.pop();
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
// Range finding
// ---------------------------------------------------------------------------

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
