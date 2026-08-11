use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write};

use quick_xml::events::Event;
use quick_xml::Reader;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    match args[1].as_str() {
        "ls" => {
            if args.len() < 3 {
                eprintln!("Usage: {} ls <path-to-file.arxml>", args[0]);
                std::process::exit(1);
            }
            cmd_ls(&args[2]);
        }
        "cp" => {
            // cp <input.arxml> <pkg1> [<pkg2> ...] --into <out1.arxml>
            //                 [<pkg3> ...] --into <out2.arxml>
            //                 [--rest <rest.arxml>]
            if args.len() < 5 {
                eprintln!("Usage: {} cp <input.arxml> <pkg1> [<pkg2> ...] --into <out.arxml> [--rest <rest.arxml>]", args[0]);
                std::process::exit(1);
            }
            let input = &args[2];
            let (groups, rest_file) = parse_cp_args(&args[3..]);
            if groups.is_empty() {
                eprintln!("Error: no --into specified.");
                std::process::exit(1);
            }
            cmd_cp(input, &groups, rest_file.as_deref());
        }
        "mv" => {
            if args.len() < 5 {
                eprintln!(
                    "Usage: {} mv <input.arxml> <pkg1> [<pkg2> ...] <output.arxml>",
                    args[0]
                );
                std::process::exit(1);
            }
            let input = &args[2];
            let output = &args[args.len() - 1];
            let packages: Vec<String> = args[3..args.len() - 1]
                .iter()
                .map(|s| normalise_path(s))
                .collect();
            cmd_mv(input, &packages, output);
        }
        _ => {
            print_usage(&args[0]);
            std::process::exit(1);
        }
    }
}

fn print_usage(prog: &str) {
    eprintln!("Usage:");
    eprintln!("  {} ls <file.arxml>", prog);
    eprintln!(
        "  {} cp <file.arxml> <pkg1> [<pkg2> ...] --into <out.arxml> [<pkg3> ...] --into <out2.arxml> [--rest <rest.arxml>]",
        prog
    );
    eprintln!(
        "  {} mv <file.arxml> <pkg1> [<pkg2> ...] <output.arxml>",
        prog
    );
}

fn normalise_path(p: &str) -> String {
    p.trim().trim_start_matches('/').to_string()
}

/// Parse the arguments after `cp <input>`:
/// Returns (groups, rest_file) where groups is a list of (packages, output_file).
fn parse_cp_args(args: &[String]) -> (Vec<(Vec<String>, String)>, Option<String>) {
    let mut groups: Vec<(Vec<String>, String)> = Vec::new();
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
                let out = args[i].clone();
                groups.push((pending_pkgs.drain(..).map(|s| normalise_path(&s)).collect(), out));
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

// ---------------------------------------------------------------------------
// ls
// ---------------------------------------------------------------------------

fn cmd_ls(path: &str) {
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

struct PackageRange {
    start: u64,
    end: u64,
    /// normalised path e.g. "Root/DataTypes"
    path: String,
}

fn cmd_cp(input: &str, groups: &[(Vec<String>, String)], rest_file: Option<&str>) {
    // Collect all targets across all groups
    let all_targets: HashSet<&str> = groups
        .iter()
        .flat_map(|(pkgs, _)| pkgs.iter().map(|s| s.as_str()))
        .collect();

    let root_attrs = collect_root_attrs(input);
    let all_ranges = find_package_ranges(input, &all_targets);

    if all_ranges.is_empty() {
        eprintln!("No matching packages found.");
        std::process::exit(1);
    }

    // Build a map: path -> range index for quick lookup
    let range_by_path: HashMap<&str, &PackageRange> = all_ranges
        .iter()
        .map(|r| (r.path.as_str(), r))
        .collect();

    let mut src = File::open(input).unwrap();

    // Write each --into group
    for (pkgs, out_path) in groups {
        let out_file = File::create(out_path).unwrap_or_else(|e| {
            eprintln!("Cannot create output file '{}': {}", out_path, e);
            std::process::exit(1);
        });
        let mut out = BufWriter::new(out_file);
        write_arxml_header(&mut out, &root_attrs);

        for pkg in pkgs {
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
        println!("Written to '{}'", out_path);
    }

    // Write --rest if requested
    if let Some(rest_path) = rest_file {
        // Collect all matched ranges sorted by start position
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

        // Find all top-level AR-PACKAGE ranges and exclude matched ones
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

fn write_arxml_header<W: Write>(out: &mut BufWriter<W>, root_attrs: &[(String, String)]) {
    writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    write!(out, "<AUTOSAR").unwrap();
    for (k, v) in root_attrs {
        write!(out, r#" {}="{}""#, k, v).unwrap();
    }
    writeln!(out, ">").unwrap();
    writeln!(out, "  <AR-PACKAGES>").unwrap();
}

fn write_arxml_footer<W: Write>(out: &mut BufWriter<W>) {
    writeln!(out, "  </AR-PACKAGES>").unwrap();
    writeln!(out, "</AUTOSAR>").unwrap();
}

// ---------------------------------------------------------------------------
// mv
// ---------------------------------------------------------------------------

fn cmd_mv(input: &str, packages: &[String], output: &str) {
    let targets: HashSet<&str> = packages.iter().map(|s| s.as_str()).collect();

    let root_attrs = collect_root_attrs(input);
    let ranges = find_package_ranges(input, &targets);

    if ranges.is_empty() {
        eprintln!("No matching packages found.");
        std::process::exit(1);
    }

    // --- write output file ---
    {
        let out_file = File::create(output).unwrap_or_else(|e| {
            eprintln!("Cannot create output file '{}': {}", output, e);
            std::process::exit(1);
        });
        let mut out = BufWriter::new(out_file);
        write_arxml_header(&mut out, &root_attrs);

        let mut src = File::open(input).unwrap();
        for range in &ranges {
            src.seek(SeekFrom::Start(range.start)).unwrap();
            let len = (range.end - range.start) as usize;
            let mut block = vec![0u8; len];
            src.read_exact(&mut block).unwrap();
            out.write_all(&block).unwrap();
            writeln!(out).unwrap();
        }

        write_arxml_footer(&mut out);
    }
    println!("Written to '{}'", output);

    // --- rewrite input file, skipping moved ranges ---
    let mut raw = Vec::new();
    open_file(input).read_to_end(&mut raw).unwrap();

    let tmp_path = format!("{}.tmp", input);
    {
        let tmp_file = File::create(&tmp_path).unwrap_or_else(|e| {
            eprintln!("Cannot create temp file '{}': {}", tmp_path, e);
            std::process::exit(1);
        });
        let mut tmp = BufWriter::new(tmp_file);

        let mut pos: u64 = 0;
        for range in &ranges {
            if range.start > pos {
                tmp.write_all(&raw[pos as usize..range.start as usize])
                    .unwrap();
            }
            pos = range.end;
        }
        if (pos as usize) < raw.len() {
            tmp.write_all(&raw[pos as usize..]).unwrap();
        }
    }

    std::fs::rename(&tmp_path, input).unwrap_or_else(|e| {
        eprintln!("Failed to overwrite input file: {}", e);
        std::process::exit(1);
    });
    println!("Removed packages from '{}'", input);
}

// ---------------------------------------------------------------------------
// Range finding
// ---------------------------------------------------------------------------

fn find_package_ranges(path: &str, targets: &HashSet<&str>) -> Vec<PackageRange> {
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
                            let path_str = pkg_stack.join("/");
                            ranges.push(PackageRange {
                                start,
                                end: pos_after,
                                path: path_str,
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

/// Find byte ranges for ALL top-level AR-PACKAGEs (direct children of AR-PACKAGES under AUTOSAR).
fn find_all_toplevel_package_ranges(path: &str) -> Vec<PackageRange> {
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

    // We capture every AR-PACKAGE whose depth == toplevel_depth
    // toplevel_depth is the depth of AR-PACKAGE children directly under AR-PACKAGES/AUTOSAR
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

                    // Record toplevel depth on first AR-PACKAGE seen
                    if toplevel_depth.is_none() {
                        toplevel_depth = Some(depth);
                    }

                    // Start capturing if this is a top-level package and not already capturing
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

fn collect_root_attrs(path: &str) -> Vec<(String, String)> {
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

fn open_file(path: &str) -> File {
    File::open(path).unwrap_or_else(|e| {
        eprintln!("Error opening file '{}': {}", path, e);
        std::process::exit(1);
    })
}

fn local_name_str(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes).unwrap_or("").to_string()
}
