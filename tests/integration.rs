use std::collections::HashSet;
use std::fs;
use std::io::Write;

use arx_utils::{
    cmd_cp, cmd_diff, cmd_rm, collect_all_paths, find_element_ranges, find_package_ranges,
    ls_collect, normalise_path, parse_cp_args, parse_rm_args, CpGroup,
};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Shared ARXML fixtures
// ---------------------------------------------------------------------------

/// Minimal ARXML with a flat list of three top-level packages.
const FLAT_ARXML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<AUTOSAR xmlns="http://autosar.org/schema/r4.0">
  <AR-PACKAGES>
    <AR-PACKAGE>
      <SHORT-NAME>Alpha</SHORT-NAME>
    </AR-PACKAGE>
    <AR-PACKAGE>
      <SHORT-NAME>Beta</SHORT-NAME>
    </AR-PACKAGE>
    <AR-PACKAGE>
      <SHORT-NAME>Gamma</SHORT-NAME>
    </AR-PACKAGE>
  </AR-PACKAGES>
</AUTOSAR>
"#;

/// ARXML with nested packages and ELEMENTS entries.
const NESTED_ARXML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<AUTOSAR xmlns="http://autosar.org/schema/r4.0">
  <AR-PACKAGES>
    <AR-PACKAGE>
      <SHORT-NAME>Root</SHORT-NAME>
      <AR-PACKAGES>
        <AR-PACKAGE>
          <SHORT-NAME>Components</SHORT-NAME>
          <ELEMENTS>
            <APPLICATION-SW-COMPONENT-TYPE>
              <SHORT-NAME>MyComponent</SHORT-NAME>
            </APPLICATION-SW-COMPONENT-TYPE>
          </ELEMENTS>
        </AR-PACKAGE>
        <AR-PACKAGE>
          <SHORT-NAME>Interfaces</SHORT-NAME>
          <ELEMENTS>
            <SENDER-RECEIVER-INTERFACE>
              <SHORT-NAME>MySRInterface</SHORT-NAME>
            </SENDER-RECEIVER-INTERFACE>
          </ELEMENTS>
        </AR-PACKAGE>
      </AR-PACKAGES>
    </AR-PACKAGE>
    <AR-PACKAGE>
      <SHORT-NAME>Types</SHORT-NAME>
    </AR-PACKAGE>
  </AR-PACKAGES>
</AUTOSAR>
"#;

/// Write content to a named file inside `dir` and return the full path string.
fn write_fixture(dir: &TempDir, name: &str, content: &str) -> String {
    let path = dir.path().join(name);
    let mut f = fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    path.to_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// normalise_path
// ---------------------------------------------------------------------------

#[test]
fn normalise_path_strips_leading_slash() {
    assert_eq!(normalise_path("/Root/Components"), "Root/Components");
}

#[test]
fn normalise_path_no_slash_unchanged() {
    assert_eq!(normalise_path("Root/Components"), "Root/Components");
}

#[test]
fn normalise_path_strips_whitespace() {
    assert_eq!(normalise_path("  /Root  "), "Root");
}

#[test]
fn normalise_path_empty_string() {
    assert_eq!(normalise_path(""), "");
}

// ---------------------------------------------------------------------------
// parse_cp_args
// ---------------------------------------------------------------------------

fn s(s: &str) -> String {
    s.to_string()
}

#[test]
fn parse_cp_args_single_group() {
    let args = vec![s("/Root/Alpha"), s("--into"), s("out.arxml")];
    let (groups, rest) = parse_cp_args(&args);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].packages, vec!["Root/Alpha"]);
    assert_eq!(groups[0].output, "out.arxml");
    assert!(rest.is_none(), "expected no rest file, got {:?}", rest);
}

#[test]
fn parse_cp_args_multiple_packages_one_group() {
    let args = vec![s("/Alpha"), s("/Beta"), s("--into"), s("out.arxml")];
    let (groups, _) = parse_cp_args(&args);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].packages, vec!["Alpha", "Beta"]);
}

#[test]
fn parse_cp_args_multiple_groups() {
    let args = vec![
        s("/Alpha"), s("--into"), s("a.arxml"),
        s("/Beta"),  s("--into"), s("b.arxml"),
    ];
    let (groups, rest) = parse_cp_args(&args);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].packages, vec!["Alpha"]);
    assert_eq!(groups[0].output, "a.arxml");
    assert_eq!(groups[1].packages, vec!["Beta"]);
    assert_eq!(groups[1].output, "b.arxml");
    assert!(rest.is_none());
}

#[test]
fn parse_cp_args_with_rest() {
    let args = vec![
        s("/Alpha"), s("--into"), s("a.arxml"),
        s("--rest"), s("remainder.arxml"),
    ];
    let (groups, rest) = parse_cp_args(&args);
    assert_eq!(groups.len(), 1);
    assert_eq!(rest, Some(s("remainder.arxml")));
}

#[test]
fn parse_cp_args_normalises_paths() {
    // Paths with and without leading slash should both be normalised.
    let args = vec![s("Root/Alpha"), s("--into"), s("out.arxml")];
    let (groups, _) = parse_cp_args(&args);
    assert_eq!(groups[0].packages, vec!["Root/Alpha"]);
}

// ---------------------------------------------------------------------------
// ls_collect
// ---------------------------------------------------------------------------

#[test]
fn ls_flat_no_filter_no_recursive() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "flat.arxml", FLAT_ARXML);

    let result = ls_collect(&path, false, None, false);
    assert_eq!(result, vec!["/Alpha", "/Beta", "/Gamma"]);
}

#[test]
fn ls_nested_no_filter_non_recursive_shows_only_toplevel() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let result = ls_collect(&path, false, None, false);
    // Without -R, only direct children of the root are shown.
    assert_eq!(result, vec!["/Root", "/Types"]);
}

#[test]
fn ls_nested_recursive_shows_all_packages() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let result = ls_collect(&path, false, None, true);
    assert_eq!(result, vec!["/Root", "/Root/Components", "/Root/Interfaces", "/Types"]);
}

#[test]
fn ls_filter_shows_direct_children_only() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let result = ls_collect(&path, false, Some("/Root"), false);
    assert_eq!(result, vec!["/Root/Components", "/Root/Interfaces"]);
}

#[test]
fn ls_filter_recursive_shows_root_and_all_descendants() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    // With -R and filter /Root, the filter path itself is included together with
    // all of its descendants.
    let result = ls_collect(&path, false, Some("/Root"), true);
    assert_eq!(result, vec!["/Root", "/Root/Components", "/Root/Interfaces"]);
}

#[test]
fn ls_filter_non_matching_returns_empty() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let result = ls_collect(&path, false, Some("/DoesNotExist"), true);
    assert!(result.is_empty());
}

#[test]
fn ls_show_elements_includes_element_names() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    // -e with filter /Root/Components, non-recursive: should show the component name.
    let result = ls_collect(&path, true, Some("/Root/Components"), false);
    assert!(result.contains(&"/Root/Components/MyComponent".to_string()),
        "Expected /Root/Components/MyComponent in {:?}", result);
}

#[test]
fn ls_show_elements_recursive_includes_all_elements() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let result = ls_collect(&path, true, None, true);
    assert!(result.contains(&"/Root/Components/MyComponent".to_string()));
    assert!(result.contains(&"/Root/Interfaces/MySRInterface".to_string()));
}

// ---------------------------------------------------------------------------
// find_package_ranges
// ---------------------------------------------------------------------------

#[test]
fn find_package_ranges_finds_toplevel_package() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "flat.arxml", FLAT_ARXML);

    let targets: HashSet<&str> = ["Alpha"].iter().cloned().collect();
    let ranges = find_package_ranges(&path, &targets);

    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].path, "Alpha");

    // Verify the extracted bytes are valid XML containing the right SHORT-NAME.
    let raw = fs::read(&path).unwrap();
    let block = std::str::from_utf8(&raw[ranges[0].start as usize..ranges[0].end as usize]).unwrap();
    assert!(block.contains("<SHORT-NAME>Alpha</SHORT-NAME>"), "block: {}", block);
    assert!(block.trim_start().starts_with("<AR-PACKAGE>"), "block: {}", block);
    assert!(block.trim_end().ends_with("</AR-PACKAGE>"), "block: {}", block);
}

#[test]
fn find_package_ranges_finds_multiple_packages() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "flat.arxml", FLAT_ARXML);

    let targets: HashSet<&str> = ["Alpha", "Gamma"].iter().cloned().collect();
    let ranges = find_package_ranges(&path, &targets);

    assert_eq!(ranges.len(), 2);
    let paths: HashSet<&str> = ranges.iter().map(|r| r.path.as_str()).collect();
    assert!(paths.contains("Alpha"));
    assert!(paths.contains("Gamma"));
}

#[test]
fn find_package_ranges_nested_package() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let targets: HashSet<&str> = ["Root/Components"].iter().cloned().collect();
    let ranges = find_package_ranges(&path, &targets);

    assert_eq!(ranges.len(), 1);
    let raw = fs::read(&path).unwrap();
    let block = std::str::from_utf8(&raw[ranges[0].start as usize..ranges[0].end as usize]).unwrap();
    assert!(block.contains("MyComponent"));
}

#[test]
fn find_package_ranges_unknown_package_returns_empty() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "flat.arxml", FLAT_ARXML);

    let targets: HashSet<&str> = ["DoesNotExist"].iter().cloned().collect();
    let ranges = find_package_ranges(&path, &targets);
    assert!(ranges.is_empty());
}

// ---------------------------------------------------------------------------
// cmd_cp
// ---------------------------------------------------------------------------

/// Parse an ARXML output file and return the SHORT-NAMEs of its top-level AR-PACKAGEs.
fn toplevel_package_names(path: &str) -> Vec<String> {
    ls_collect(path, false, None, false)
        .into_iter()
        .map(|p| p.trim_start_matches('/').to_string())
        .collect()
}

#[test]
fn cmd_cp_single_package_produces_valid_arxml() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(&dir, "flat.arxml", FLAT_ARXML);
    let output = dir.path().join("out.arxml").to_str().unwrap().to_string();

    let groups = vec![CpGroup {
        packages: vec!["Alpha".to_string()],
        output: output.clone(),
    }];
    cmd_cp(&input, &groups, None);

    // Output must be valid enough for ls_collect to parse.
    let names = toplevel_package_names(&output);
    assert_eq!(names, vec!["Alpha"]);
}

#[test]
fn cmd_cp_multiple_packages_into_one_file() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(&dir, "flat.arxml", FLAT_ARXML);
    let output = dir.path().join("out.arxml").to_str().unwrap().to_string();

    let groups = vec![CpGroup {
        packages: vec!["Alpha".to_string(), "Gamma".to_string()],
        output: output.clone(),
    }];
    cmd_cp(&input, &groups, None);

    let names = toplevel_package_names(&output);
    assert_eq!(names, vec!["Alpha", "Gamma"]);
}

#[test]
fn cmd_cp_split_into_two_files() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(&dir, "flat.arxml", FLAT_ARXML);
    let out_a = dir.path().join("a.arxml").to_str().unwrap().to_string();
    let out_b = dir.path().join("b.arxml").to_str().unwrap().to_string();

    let groups = vec![
        CpGroup { packages: vec!["Alpha".to_string()], output: out_a.clone() },
        CpGroup { packages: vec!["Beta".to_string()],  output: out_b.clone() },
    ];
    cmd_cp(&input, &groups, None);

    assert_eq!(toplevel_package_names(&out_a), vec!["Alpha"]);
    assert_eq!(toplevel_package_names(&out_b), vec!["Beta"]);
}

#[test]
fn cmd_cp_rest_file_contains_unmatched_packages() {
    let dir = TempDir::new().unwrap();
    let input = write_fixture(&dir, "flat.arxml", FLAT_ARXML);
    let out_a = dir.path().join("a.arxml").to_str().unwrap().to_string();
    let rest  = dir.path().join("rest.arxml").to_str().unwrap().to_string();

    let groups = vec![CpGroup {
        packages: vec!["Alpha".to_string()],
        output: out_a.clone(),
    }];
    cmd_cp(&input, &groups, Some(&rest));

    // Alpha was extracted; Beta and Gamma should be in the rest file.
    let rest_names = toplevel_package_names(&rest);
    assert!(!rest_names.contains(&"Alpha".to_string()), "Alpha must not appear in rest");
    assert!(rest_names.contains(&"Beta".to_string()));
    assert!(rest_names.contains(&"Gamma".to_string()));
}

#[test]
fn cmd_cp_rest_file_is_complete_partition() {
    // Every top-level package must end up in exactly one output file.
    let dir = TempDir::new().unwrap();
    let input = write_fixture(&dir, "flat.arxml", FLAT_ARXML);
    let out_a = dir.path().join("a.arxml").to_str().unwrap().to_string();
    let out_b = dir.path().join("b.arxml").to_str().unwrap().to_string();
    let rest  = dir.path().join("rest.arxml").to_str().unwrap().to_string();

    let groups = vec![
        CpGroup { packages: vec!["Alpha".to_string()], output: out_a.clone() },
        CpGroup { packages: vec!["Beta".to_string()],  output: out_b.clone() },
    ];
    cmd_cp(&input, &groups, Some(&rest));

    let mut all: Vec<String> = Vec::new();
    all.extend(toplevel_package_names(&out_a));
    all.extend(toplevel_package_names(&out_b));
    all.extend(toplevel_package_names(&rest));
    all.sort();

    assert_eq!(all, vec!["Alpha", "Beta", "Gamma"]);
}

#[test]
fn cmd_cp_preserves_nested_content() {
    // Copying a top-level package that contains sub-packages must preserve them.
    let dir = TempDir::new().unwrap();
    let input = write_fixture(&dir, "nested.arxml", NESTED_ARXML);
    let output = dir.path().join("out.arxml").to_str().unwrap().to_string();

    let groups = vec![CpGroup {
        packages: vec!["Root".to_string()],
        output: output.clone(),
    }];
    cmd_cp(&input, &groups, None);

    // Sub-packages of Root must still be present.
    let names = ls_collect(&output, false, None, true);
    assert!(names.contains(&"/Root/Components".to_string()));
    assert!(names.contains(&"/Root/Interfaces".to_string()));
}

// ---------------------------------------------------------------------------
// parse_rm_args
// ---------------------------------------------------------------------------

#[test]
fn parse_rm_args_single_package() {
    let args = vec![s("/Alpha")];
    let pkgs = parse_rm_args(&args);
    assert_eq!(pkgs, vec!["Alpha"]);
}

#[test]
fn parse_rm_args_multiple_packages() {
    let args = vec![s("/Alpha"), s("/Beta")];
    let pkgs = parse_rm_args(&args);
    assert_eq!(pkgs, vec!["Alpha", "Beta"]);
}

#[test]
fn parse_rm_args_normalises_paths() {
    let args = vec![s("Root/Components"), s("/Root/Interfaces")];
    let pkgs = parse_rm_args(&args);
    assert_eq!(pkgs, vec!["Root/Components", "Root/Interfaces"]);
}

// ---------------------------------------------------------------------------
// cmd_rm
// ---------------------------------------------------------------------------

#[test]
fn cmd_rm_removes_single_package_in_place() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "flat.arxml", FLAT_ARXML);

    cmd_rm(&path, &[s("Alpha")]);

    let names = toplevel_package_names(&path);
    assert!(!names.contains(&"Alpha".to_string()), "Alpha should have been removed");
    assert!(names.contains(&"Beta".to_string()));
    assert!(names.contains(&"Gamma".to_string()));
}

#[test]
fn cmd_rm_removes_multiple_packages_in_place() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "flat.arxml", FLAT_ARXML);

    cmd_rm(&path, &[s("Alpha"), s("Beta")]);

    let names = toplevel_package_names(&path);
    assert!(!names.contains(&"Alpha".to_string()));
    assert!(!names.contains(&"Beta".to_string()));
    assert_eq!(names, vec!["Gamma"]);
}

#[test]
fn cmd_rm_result_is_valid_arxml() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "flat.arxml", FLAT_ARXML);

    cmd_rm(&path, &[s("Beta")]);

    // ls_collect must parse the result without errors
    let names = toplevel_package_names(&path);
    assert_eq!(names, vec!["Alpha", "Gamma"]);
}

#[test]
fn cmd_rm_with_leading_slash_normalised() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "flat.arxml", FLAT_ARXML);

    // parse_rm_args normalises the leading slash before cmd_rm is called
    let pkgs = parse_rm_args(&[s("/Gamma")]);
    cmd_rm(&path, &pkgs);

    let names = toplevel_package_names(&path);
    assert!(!names.contains(&"Gamma".to_string()));
}

#[test]
fn cmd_rm_preserves_remaining_package_content() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    cmd_rm(&path, &[s("Types")]);

    let names = ls_collect(&path, false, None, true);
    assert!(names.contains(&"/Root".to_string()));
    assert!(names.contains(&"/Root/Components".to_string()));
    assert!(names.contains(&"/Root/Interfaces".to_string()));
    assert!(!names.contains(&"/Types".to_string()));
}

// ---------------------------------------------------------------------------
// find_element_ranges
// ---------------------------------------------------------------------------

#[test]
fn find_element_ranges_finds_element_by_full_path() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let targets: HashSet<&str> = ["Root/Components/MyComponent"].iter().cloned().collect();
    let ranges = find_element_ranges(&path, &targets);

    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].path, "Root/Components/MyComponent");

    let raw = fs::read(&path).unwrap();
    let block = std::str::from_utf8(&raw[ranges[0].start as usize..ranges[0].end as usize]).unwrap();
    assert!(block.contains("MyComponent"), "block: {}", block);
    assert!(block.contains("APPLICATION-SW-COMPONENT-TYPE"), "block: {}", block);
}

#[test]
fn find_element_ranges_unknown_element_returns_empty() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let targets: HashSet<&str> = ["Root/Components/DoesNotExist"].iter().cloned().collect();
    let ranges = find_element_ranges(&path, &targets);
    assert!(ranges.is_empty());
}

// ---------------------------------------------------------------------------
// cmd_rm with elements
// ---------------------------------------------------------------------------

#[test]
fn cmd_rm_removes_element_from_package() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let pkgs = parse_rm_args(&[s("Root/Components/MyComponent")]);
    cmd_rm(&path, &pkgs);

    // Package must still exist
    let names = ls_collect(&path, false, None, true);
    assert!(names.contains(&"/Root/Components".to_string()));

    // Element must be gone
    let elements = ls_collect(&path, true, Some("/Root/Components"), false);
    assert!(!elements.contains(&"/Root/Components/MyComponent".to_string()),
        "MyComponent should have been removed, got: {:?}", elements);
}

#[test]
fn cmd_rm_element_leaves_sibling_elements_intact() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    // Remove MyComponent from Components, MySRInterface in Interfaces must survive
    let pkgs = parse_rm_args(&[s("Root/Components/MyComponent")]);
    cmd_rm(&path, &pkgs);

    let elements = ls_collect(&path, true, Some("/Root/Interfaces"), false);
    assert!(elements.contains(&"/Root/Interfaces/MySRInterface".to_string()),
        "MySRInterface should still be present, got: {:?}", elements);
}

#[test]
fn cmd_rm_element_result_is_valid_arxml() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let pkgs = parse_rm_args(&[s("Root/Interfaces/MySRInterface")]);
    cmd_rm(&path, &pkgs);

    // ls_collect must be able to parse the result without errors
    let names = ls_collect(&path, false, None, true);
    assert!(names.contains(&"/Root".to_string()));
    assert!(names.contains(&"/Root/Interfaces".to_string()));
}

// ---------------------------------------------------------------------------
// collect_all_paths / cmd_diff
// ---------------------------------------------------------------------------

/// FLAT_ARXML with only Alpha and Beta (Gamma removed) — used as "file B" in diff tests.
const FLAT_ARXML_AB: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<AUTOSAR xmlns="http://autosar.org/schema/r4.0">
  <AR-PACKAGES>
    <AR-PACKAGE>
      <SHORT-NAME>Alpha</SHORT-NAME>
    </AR-PACKAGE>
    <AR-PACKAGE>
      <SHORT-NAME>Beta</SHORT-NAME>
    </AR-PACKAGE>
  </AR-PACKAGES>
</AUTOSAR>
"#;

/// NESTED_ARXML variant: Root/Components has an extra element NewComp, MySRInterface removed.
const NESTED_ARXML_MODIFIED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<AUTOSAR xmlns="http://autosar.org/schema/r4.0">
  <AR-PACKAGES>
    <AR-PACKAGE>
      <SHORT-NAME>Root</SHORT-NAME>
      <AR-PACKAGES>
        <AR-PACKAGE>
          <SHORT-NAME>Components</SHORT-NAME>
          <ELEMENTS>
            <APPLICATION-SW-COMPONENT-TYPE>
              <SHORT-NAME>MyComponent</SHORT-NAME>
            </APPLICATION-SW-COMPONENT-TYPE>
            <APPLICATION-SW-COMPONENT-TYPE>
              <SHORT-NAME>NewComp</SHORT-NAME>
            </APPLICATION-SW-COMPONENT-TYPE>
          </ELEMENTS>
        </AR-PACKAGE>
        <AR-PACKAGE>
          <SHORT-NAME>Interfaces</SHORT-NAME>
        </AR-PACKAGE>
      </AR-PACKAGES>
    </AR-PACKAGE>
    <AR-PACKAGE>
      <SHORT-NAME>Types</SHORT-NAME>
    </AR-PACKAGE>
  </AR-PACKAGES>
</AUTOSAR>
"#;

#[test]
fn collect_all_paths_returns_packages_and_elements() {
    let dir = TempDir::new().unwrap();
    let path = write_fixture(&dir, "nested.arxml", NESTED_ARXML);

    let paths = collect_all_paths(&path);
    assert!(paths.contains(&"/Root".to_string()));
    assert!(paths.contains(&"/Root/Components".to_string()));
    assert!(paths.contains(&"/Root/Components/MyComponent".to_string()));
    assert!(paths.contains(&"/Root/Interfaces/MySRInterface".to_string()));
    assert!(paths.contains(&"/Types".to_string()));
}

#[test]
fn cmd_diff_identical_files_returns_true() {
    let dir = TempDir::new().unwrap();
    let a = write_fixture(&dir, "a.arxml", FLAT_ARXML);
    let b = write_fixture(&dir, "b.arxml", FLAT_ARXML);

    assert!(cmd_diff(&a, &b));
}

#[test]
fn cmd_diff_detects_removed_package() {
    let dir = TempDir::new().unwrap();
    let a = write_fixture(&dir, "a.arxml", FLAT_ARXML);   // Alpha, Beta, Gamma
    let b = write_fixture(&dir, "b.arxml", FLAT_ARXML_AB); // Alpha, Beta only

    assert!(!cmd_diff(&a, &b), "files differ, should return false");
}

#[test]
fn cmd_diff_detects_added_package() {
    let dir = TempDir::new().unwrap();
    let a = write_fixture(&dir, "a.arxml", FLAT_ARXML_AB); // Alpha, Beta
    let b = write_fixture(&dir, "b.arxml", FLAT_ARXML);    // Alpha, Beta, Gamma

    assert!(!cmd_diff(&a, &b));
}

#[test]
fn cmd_diff_detects_element_changes() {
    let dir = TempDir::new().unwrap();
    let a = write_fixture(&dir, "a.arxml", NESTED_ARXML);
    let b = write_fixture(&dir, "b.arxml", NESTED_ARXML_MODIFIED);

    // MySRInterface removed, NewComp added — files differ
    assert!(!cmd_diff(&a, &b));
}

#[test]
fn cmd_diff_order_independent() {
    // Same packages in different order must be considered identical.
    let reversed = r#"<?xml version="1.0" encoding="UTF-8"?>
<AUTOSAR xmlns="http://autosar.org/schema/r4.0">
  <AR-PACKAGES>
    <AR-PACKAGE>
      <SHORT-NAME>Gamma</SHORT-NAME>
    </AR-PACKAGE>
    <AR-PACKAGE>
      <SHORT-NAME>Beta</SHORT-NAME>
    </AR-PACKAGE>
    <AR-PACKAGE>
      <SHORT-NAME>Alpha</SHORT-NAME>
    </AR-PACKAGE>
  </AR-PACKAGES>
</AUTOSAR>
"#;
    let dir = TempDir::new().unwrap();
    let a = write_fixture(&dir, "a.arxml", FLAT_ARXML);
    let b = write_fixture(&dir, "b.arxml", reversed);

    assert!(cmd_diff(&a, &b), "same packages in different order should be identical");
}
