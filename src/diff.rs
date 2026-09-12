//! `arx diff`: structural (and optionally field-level) comparison of the
//! `AR-PACKAGE`/`ELEMENTS` structure of two ARXML files.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::path_match::path_under_pattern;
use crate::tree::build_tree;
use crate::util::{local_name_str, normalise_path, open_file};

pub struct Colors {
    pub red: &'static str,
    pub green: &'static str,
    pub yellow: &'static str,
    pub reset: &'static str,
}

pub const COLORS_ON: Colors = Colors {
    red:    "\x1b[31m",
    green:  "\x1b[32m",
    yellow: "\x1b[33m",
    reset:  "\x1b[0m",
};

pub const COLORS_OFF: Colors = Colors {
    red:    "",
    green:  "",
    yellow: "",
    reset:  "",
};

/// Collect all AR-PACKAGE and top-level ELEMENTS paths from an ARXML file as
/// a sorted vec, for `arx diff`'s structural comparison. Unlike `arx ls`
/// (which shows nested elements at any depth via `-t`), this stays at
/// `diff`'s documented scope: AR-PACKAGEs (any depth) plus each package's
/// direct (first-level) elements — not their internals.
/// If `filter` is given, only paths under that AR-PACKAGE prefix are returned.
pub fn collect_all_paths(path: &str, filter: Option<&str>) -> Vec<String> {
    let tree = build_tree(path);
    let filter = filter.map(|f| normalise_path(f));

    let mut paths: Vec<String> = tree
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.tag == "AR-PACKAGE"
                || matches!(node.parent, Some(p) if tree.nodes[p].tag == "AR-PACKAGE")
        })
        .map(|(idx, _)| tree.full_path(idx))
        .filter(|full| filter.as_deref().map_or(true, |f| path_under_pattern(full, f)))
        .collect();

    paths.sort();
    paths
}

/// Compare the AR-PACKAGE / ELEMENTS structure of two ARXML files and print
/// coloured `+`/`-` lines for entries that differ.
/// If `filter` is given, only paths under that AR-PACKAGE prefix are compared.
///
/// Returns `true` if the files are identical, `false` if differences were found.
pub fn cmd_diff(file_a: &str, file_b: &str, filter: Option<&str>, c: &Colors) -> bool {
    let paths_a: HashSet<String> = collect_all_paths(file_a, filter).into_iter().collect();
    let paths_b: HashSet<String> = collect_all_paths(file_b, filter).into_iter().collect();

    // Removed: in A but not in B
    let mut removed: Vec<&String> = paths_a.difference(&paths_b).collect();
    removed.sort();

    // Added: in B but not in A
    let mut added: Vec<&String> = paths_b.difference(&paths_a).collect();
    added.sort();

    if removed.is_empty() && added.is_empty() {
        return true;
    }

    println!("{}--- {}{}", c.red, file_a, c.reset);
    println!("{}+++ {}{}", c.green, file_b, c.reset);
    println!();

    for path in &removed {
        println!("{}-{} {}{}", c.red, c.reset, c.red, path);
        print!("{}", c.reset);
    }
    for path in &added {
        println!("{}+{} {}{}", c.green, c.reset, c.green, path);
        print!("{}", c.reset);
    }

    false
}

/// For a given element path (e.g. `Root/Components/MyComponent`), collect the
/// direct child tags and their text content from within the `<ELEMENTS>` block.
///
/// Returns a `Vec<(tag_name, text_value)>` in document order.
/// Tags that have no direct text content (only child elements) get an empty string.
pub fn collect_element_fields(file: &str, element_path: &str) -> Vec<(String, String)> {
    let norm = normalise_path(element_path);
    // Split into package path and element short-name
    // e.g. "Root/Components/MyComponent" -> pkg="Root/Components", name="MyComponent"
    let (pkg_path, element_name) = match norm.rfind('/') {
        Some(pos) => (&norm[..pos], &norm[pos + 1..]),
        None => return vec![],
    };

    let mut raw = Vec::new();
    open_file(file).read_to_end(&mut raw).unwrap();
    let cursor = Cursor::new(&raw);
    let mut xml = Reader::from_reader(cursor);
    xml.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut pkg_stack: Vec<String> = Vec::new();
    let mut depth: usize = 0;

    let mut in_elements = false;
    let mut elements_depth: usize = 0;
    let mut element_tag_depth: usize = 0; // depth of the direct child of <ELEMENTS>
    let mut in_target_element = false;    // inside the element we are looking for
    let mut target_found = false;
    let mut field_depth: usize = 0;       // depth of a direct child tag of the element
    let mut current_field: Option<String> = None;
    let mut results: Vec<(String, String)> = Vec::new();

    let mut read_short_name = false;

    loop {
        let event = xml.read_event_into(&mut buf);
        match event {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let name = local_name_str(e.local_name().as_ref());

                if in_target_element {
                    if field_depth == 0 && depth == element_tag_depth + 1 {
                        // Direct child tag of the target element
                        field_depth = depth;
                        current_field = Some(name.clone());
                    }
                    // Deeper nesting — ignore for now (no text capture)
                } else if name == "AR-PACKAGE" && !in_elements {
                    // will get SHORT-NAME next
                } else if name == "SHORT-NAME" && !in_elements && !in_target_element {
                    read_short_name = true;
                } else if name == "ELEMENTS" && !in_elements {
                    // Check if current package matches pkg_path
                    let current = pkg_stack.join("/");
                    if current == pkg_path {
                        in_elements = true;
                        elements_depth = depth;
                    }
                } else if in_elements && depth == elements_depth + 1 && element_tag_depth == 0 {
                    element_tag_depth = depth;
                } else if in_elements && element_tag_depth > 0 && !in_target_element
                    && name == "SHORT-NAME" && depth == element_tag_depth + 1
                {
                    read_short_name = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    buf.clear();
                    continue;
                }
                if read_short_name {
                    read_short_name = false;
                    if in_elements && element_tag_depth > 0 && !in_target_element {
                        if trimmed == element_name {
                            in_target_element = true;
                            target_found = true;
                            // SHORT-NAME is itself a field — record it now
                            results.push(("SHORT-NAME".to_string(), trimmed.to_string()));
                        }
                    } else if in_target_element && field_depth > 0 && depth == field_depth {
                        // SHORT-NAME read as a field inside the target element
                        results.push(("SHORT-NAME".to_string(), trimmed.to_string()));
                        current_field = None;
                        field_depth = 0;
                    } else if !in_elements {
                        pkg_stack.push(trimmed.to_string());
                    }
                } else if in_target_element && field_depth > 0 && depth == field_depth {
                    if let Some(ref tag) = current_field {
                        results.push((tag.clone(), trimmed.to_string()));
                    }
                    current_field = None;
                    field_depth = 0;
                }
            }
            Ok(Event::End(ref e)) => {
                let name = local_name_str(e.local_name().as_ref());

                if in_target_element {
                    if field_depth > 0 && depth == field_depth {
                        // Closing a direct child tag — if no text was captured, record empty
                        if let Some(ref tag) = current_field {
                            results.push((tag.clone(), String::new()));
                        }
                        current_field = None;
                        field_depth = 0;
                    } else if depth == element_tag_depth {
                        // Closing the element itself — done
                        break;
                    }
                } else if in_elements && element_tag_depth > 0 && depth == element_tag_depth {
                    element_tag_depth = 0;
                } else if name == "ELEMENTS" && in_elements && depth == elements_depth {
                    in_elements = false;
                    elements_depth = 0;
                    if target_found { break; }
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

    results
}

/// Collect direct child tag values for **all** elements in `<ELEMENTS>` blocks
/// in a single streaming pass over the file.
///
/// Returns `HashMap<normalised_element_path, HashMap<tag_name, text_value>>`.
pub fn collect_all_element_fields(file: &str) -> HashMap<String, HashMap<String, String>> {
    let mut raw = Vec::new();
    open_file(file).read_to_end(&mut raw).unwrap();
    let cursor = Cursor::new(&raw);
    let mut xml = Reader::from_reader(cursor);
    xml.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut pkg_stack: Vec<String> = Vec::new();
    let mut depth: usize = 0;

    let mut in_elements = false;
    let mut elements_depth: usize = 0;
    let mut element_tag_depth: usize = 0;
    let mut element_path: Option<String> = None; // full path of current element
    let mut field_depth: usize = 0;
    let mut current_field: Option<String> = None;
    let mut current_fields: HashMap<String, String> = HashMap::new();

    let mut read_short_name = false;
    let mut sn_is_element = false; // true when reading SHORT-NAME of an element tag

    let mut result: HashMap<String, HashMap<String, String>> = HashMap::new();

    loop {
        let event = xml.read_event_into(&mut buf);
        match event {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let name = local_name_str(e.local_name().as_ref());

                if let Some(ref _ep) = element_path {
                    // Inside an element
                    if field_depth == 0 && depth == element_tag_depth + 1 {
                        field_depth = depth;
                        current_field = Some(name.clone());
                        if name == "SHORT-NAME" {
                            read_short_name = true;
                            sn_is_element = false; // treated as regular field here
                        }
                    }
                    // deeper nesting — ignore text
                } else if in_elements && depth == elements_depth + 1 && element_tag_depth == 0 {
                    element_tag_depth = depth;
                    current_fields = HashMap::new();
                } else if in_elements && element_tag_depth > 0 && element_path.is_none()
                    && name == "SHORT-NAME" && depth == element_tag_depth + 1
                {
                    read_short_name = true;
                    sn_is_element = true;
                } else if name == "ELEMENTS" && !in_elements {
                    in_elements = true;
                    elements_depth = depth;
                } else if name == "SHORT-NAME" && !in_elements {
                    read_short_name = true;
                    sn_is_element = false;
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    buf.clear();
                    continue;
                }

                if read_short_name {
                    read_short_name = false;
                    if sn_is_element {
                        // SHORT-NAME of an element tag — build its full path
                        let pkg = pkg_stack.join("/");
                        let full = format!("{}/{}", pkg, trimmed);
                        element_path = Some(full.clone());
                        current_fields.insert("SHORT-NAME".to_string(), trimmed.to_string());
                    } else if element_path.is_some() && field_depth > 0 && depth == field_depth {
                        // SHORT-NAME as a regular field inside the element (shouldn't happen
                        // normally since sn_is_element=false only for pkg SHORT-NAMEs, but
                        // guard anyway)
                        current_fields.insert("SHORT-NAME".to_string(), trimmed.to_string());
                        current_field = None;
                        field_depth = 0;
                    } else if !in_elements {
                        pkg_stack.push(trimmed.to_string());
                    }
                } else if element_path.is_some() && field_depth > 0 && depth == field_depth {
                    if let Some(ref tag) = current_field {
                        current_fields.insert(tag.clone(), trimmed.to_string());
                    }
                    current_field = None;
                    field_depth = 0;
                }
            }
            Ok(Event::End(ref e)) => {
                let name = local_name_str(e.local_name().as_ref());

                if element_path.is_some() {
                    if field_depth > 0 && depth == field_depth {
                        // Close a direct child tag with no text — record empty
                        if let Some(ref tag) = current_field {
                            current_fields.entry(tag.clone()).or_insert_with(String::new);
                        }
                        current_field = None;
                        field_depth = 0;
                    } else if depth == element_tag_depth {
                        // Closing the element itself — store and reset
                        let path = element_path.take().unwrap();
                        result.insert(path, std::mem::take(&mut current_fields));
                        element_tag_depth = 0;
                    }
                } else if in_elements && element_tag_depth > 0 && depth == element_tag_depth {
                    // Element with no SHORT-NAME found — skip
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

    result
}

/// Extended diff: in addition to added/removed paths, compare the direct child
/// tags of elements that exist in both files and report changed field values.
pub fn cmd_diff_extended(file_a: &str, file_b: &str, filter: Option<&str>, c: &Colors) -> bool {
    let paths_a: HashSet<String> = collect_all_paths(file_a, filter).into_iter().collect();
    let paths_b: HashSet<String> = collect_all_paths(file_b, filter).into_iter().collect();

    let mut removed: Vec<&String> = paths_a.difference(&paths_b).collect();
    removed.sort();
    let mut added: Vec<&String> = paths_b.difference(&paths_a).collect();
    added.sort();

    // Paths present in both — check field-level differences using a single pass each.
    let common: HashSet<&String> = paths_a.intersection(&paths_b).collect();

    let all_fields_a = collect_all_element_fields(file_a);
    let all_fields_b = collect_all_element_fields(file_b);

    let mut field_diffs: Vec<(String, Vec<(String, String, String)>)> = Vec::new();
    let mut common_sorted: Vec<&&String> = common.iter().collect();
    common_sorted.sort();

    for path in common_sorted {
        let norm = normalise_path(path);
        let fields_a = all_fields_a.get(&norm).cloned().unwrap_or_default();
        let fields_b = all_fields_b.get(&norm).cloned().unwrap_or_default();

        if fields_a.is_empty() && fields_b.is_empty() {
            continue;
        }

        let all_tags: HashSet<&String> = fields_a.keys().chain(fields_b.keys()).collect();
        let mut changes: Vec<(String, String, String)> = Vec::new();
        let mut tags_sorted: Vec<&&String> = all_tags.iter().collect();
        tags_sorted.sort();

        for tag in tags_sorted {
            let val_a = fields_a.get(*tag).map(|s| s.as_str()).unwrap_or("");
            let val_b = fields_b.get(*tag).map(|s| s.as_str()).unwrap_or("");
            if val_a != val_b {
                changes.push(((*tag).clone(), val_a.to_string(), val_b.to_string()));
            }
        }
        if !changes.is_empty() {
            field_diffs.push(((*path).clone(), changes));
        }
    }

    let identical = removed.is_empty() && added.is_empty() && field_diffs.is_empty();
    if identical {
        return true;
    }

    println!("{}--- {}{}", c.red, file_a, c.reset);
    println!("{}+++ {}{}", c.green, file_b, c.reset);
    println!();

    for path in &removed {
        println!("{}-{} {}{}", c.red, c.reset, c.red, path);
        print!("{}", c.reset);
    }
    for path in &added {
        println!("{}+{} {}{}", c.green, c.reset, c.green, path);
        print!("{}", c.reset);
    }
    for (path, changes) in &field_diffs {
        println!("{}~{} {}{}", c.yellow, c.reset, c.yellow, path);
        print!("{}", c.reset);
        for (tag, old, new) in changes {
            if old.is_empty() {
                println!("{}  + {}: {}{}", c.green, tag, new, c.reset);
            } else if new.is_empty() {
                println!("{}  - {}: {}{}", c.red, tag, old, c.reset);
            } else {
                println!("{}  - {}: {}{}", c.red, tag, old, c.reset);
                println!("{}  + {}: {}{}", c.green, tag, new, c.reset);
            }
        }
    }

    false
}
