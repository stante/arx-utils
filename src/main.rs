use std::collections::HashSet;
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
            // split <input.arxml> <pkg1> [<pkg2> ...] <output.arxml>
            if args.len() < 5 {
                eprintln!(
                    "Usage: {} cp <input.arxml> <pkg1> [<pkg2> ...] <output.arxml>",
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
            cmd_split(input, &packages, output);
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
        "  {} cp <file.arxml> <pkg1> [<pkg2> ...] <output.arxml>",
        prog
    );
}

fn normalise_path(p: &str) -> String {
    p.trim().trim_start_matches('/').to_string()
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
// split
// ---------------------------------------------------------------------------

struct PackageRange {
    start: u64,
    end: u64,
}

fn cmd_split(input: &str, packages: &[String], output: &str) {
    let targets: HashSet<&str> = packages.iter().map(|s| s.as_str()).collect();

    let root_attrs = collect_root_attrs(input);
    let ranges = find_package_ranges(input, &targets);

    if ranges.is_empty() {
        eprintln!("No matching packages found.");
        std::process::exit(1);
    }

    let out_file = File::create(output).unwrap_or_else(|e| {
        eprintln!("Cannot create output file '{}': {}", output, e);
        std::process::exit(1);
    });
    let mut out = BufWriter::new(out_file);

    writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    write!(out, "<AUTOSAR").unwrap();
    for (k, v) in &root_attrs {
        write!(out, r#" {}="{}""#, k, v).unwrap();
    }
    writeln!(out, ">").unwrap();
    writeln!(out, "  <AR-PACKAGES>").unwrap();

    let mut src = File::open(input).unwrap();
    for range in &ranges {
        src.seek(SeekFrom::Start(range.start)).unwrap();
        let len = (range.end - range.start) as usize;
        let mut block = vec![0u8; len];
        src.read_exact(&mut block).unwrap();
        out.write_all(&block).unwrap();
        writeln!(out).unwrap();
    }

    writeln!(out, "  </AR-PACKAGES>").unwrap();
    writeln!(out, "</AUTOSAR>").unwrap();

    println!("Written to '{}'", output);
}

fn find_package_ranges(path: &str, targets: &HashSet<&str>) -> Vec<PackageRange> {
    // Read the entire file into memory for precise byte-offset tracking.
    // quick-xml's buffer_position() is exact when using Cursor<&[u8]>.
    let mut raw = Vec::new();
    open_file(path).read_to_end(&mut raw).unwrap();
    let cursor = Cursor::new(&raw);
    let mut xml = Reader::from_reader(cursor);
    xml.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut pkg_stack: Vec<String> = Vec::new();
    let mut depth: usize = 0;

    // Short-name capture
    let mut read_short_name = false;
    let mut sn_for_depth: usize = 0; // depth of the AR-PACKAGE whose SHORT-NAME we want

    // Byte-range capture
    // Stack of (pkg-stack-len-when-started, start_byte) for nested targets
    // In practice we only capture the outermost matching level.
    let mut capture: Option<(usize, u64)> = None; // (pkg_stack.len at capture start, start_byte)

    // We need pos_before each event to record where AR-PACKAGE starts.
    // quick-xml buffer_position() returns position *after* the event.
    let mut pos_before: u64;
    // Stack: for each AR-PACKAGE start event we push its pos_before so we
    // can look it up when we later decide to capture.
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
                } else if name == "SHORT-NAME"
                    && sn_for_depth > 0
                    && depth == sn_for_depth + 1
                {
                    read_short_name = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                if read_short_name {
                    let raw = e.unescape().unwrap_or_default();
                    let short_name_trimmed = raw.trim();
                    if short_name_trimmed.is_empty() {
                        // whitespace-only text before actual content — skip
                        buf.clear();
                        continue;
                    }
                    read_short_name = false;
                    sn_for_depth = 0;
                    let short_name = short_name_trimmed.to_string();
                    pkg_stack.push(short_name.clone());

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
