//! Byte-range finding for `AR-PACKAGE`s and `ELEMENTS`-block elements,
//! used by `cp` and `rm` to copy/splice raw XML bytes without re-serialising.

use std::collections::HashSet;
use std::io::{BufReader, Cursor, Read};

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::util::{local_name_str, open_file};

pub struct PackageRange {
    pub start: u64,
    pub end: u64,
    pub path: String,
}

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
