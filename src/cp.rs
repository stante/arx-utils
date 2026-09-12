//! `arx cp`: copy one or more `AR-PACKAGE`s out of an ARXML file into new
//! output file(s), optionally writing everything else to a `--rest` file.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};

use crate::range::{find_all_toplevel_package_ranges, find_package_ranges, collect_root_attrs, PackageRange};
use crate::util::{normalise_path, open_file, write_arxml_footer, write_arxml_header};

/// One output group for cp: a list of package paths and the target file.
pub struct CpGroup {
    pub packages: Vec<String>,
    pub output: String,
}

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
