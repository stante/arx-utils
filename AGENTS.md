# arx-utils — Agent Guide

## Project Overview

`arx-utils` is a collection of command-line utilities for working with **AUTOSAR ARXML files** — XML-based model files used in automotive software development. The tools operate on two structural concepts:

- **`AR-PACKAGE`** — hierarchical namespace containers, identified by slash-delimited paths like `/Root/Components`
- **`ELEMENTS`** — blocks inside packages containing typed software objects (e.g. `APPLICATION-SW-COMPONENT-TYPE`, `SENDER-RECEIVER-INTERFACE`), each identified by a `SHORT-NAME`

The core design philosophy:
- **Streaming XML** — `quick-xml` event-driven parsing, no DOM tree, scales to large files
- **Byte-range operations** — `cp` and `rm` copy or excise raw byte slices, preserving original formatting exactly
- **No re-serialisation** — output files are assembled from original input bytes

---

## Crate Structure

Single crate with one library (`src/lib.rs`) and five binaries:

```
src/
  lib.rs          — all shared logic (public API)
  bin/
    arx.rs        — dispatcher: arx <cmd> → arx-<cmd> in PATH
    arx-ls.rs     — list AR-PACKAGE / ELEMENTS paths
    arx-cp.rs     — copy AR-PACKAGE blocks into new files
    arx-rm.rs     — remove AR-PACKAGE blocks or ELEMENTS entries in-place
    arx-diff.rs   — diff AR-PACKAGE / ELEMENTS structure of two files
tests/
  integration.rs  — all tests (no unit tests inside lib.rs)
```

**Dependencies:** `quick-xml = "0.36"` (only runtime dep), `tempfile = "3"` (dev only).

---

## Commands

### `arx ls [-e] [-R] [/filter] <file.arxml>`
Lists AR-PACKAGE paths. `-e` includes ELEMENTS entries, `-R` recurses into sub-packages, `/filter` limits output to a path prefix.

### `arx cp <file.arxml> <pkg>... --into <out.arxml> [--rest <rest.arxml>]`
Copies AR-PACKAGE blocks into output files. `--into` can be repeated. `--rest` collects all unmatched top-level packages.

### `arx rm <file.arxml> <path1> [<path2>...]`
Removes AR-PACKAGE blocks or individual ELEMENTS entries in-place. Paths with depth ≥ 2 (e.g. `Root/Components/MyComp`) target elements; shorter paths target packages.

### `arx diff [-e] [--color] <file1.arxml> <file2.arxml> [/filter]`
Compares structure of two files. Order-independent (set comparison). `-e` also compares direct child tag values of matching elements. Color is auto-detected via `stdout().is_terminal()`, forced with `--color`. Exits `0` if identical, `1` if differences found.

---

## Library API

### Public Types

```rust
pub struct PackageRange { pub start: u64, pub end: u64, pub path: String }
pub struct CpGroup      { pub packages: Vec<String>, pub output: String }
pub struct Colors       { pub red, green, yellow, reset: &'static str }
pub const  COLORS_ON:  Colors  // ANSI escape codes
pub const  COLORS_OFF: Colors  // empty strings (for tests / piped output)
```

### Path Helpers

| Function | Description |
|---|---|
| `normalise_path(p)` | Strips leading `/` and whitespace — all internal paths use this form |
| `open_file(path)` | Opens file or exits with error |
| `local_name_str(bytes)` | Converts XML local name bytes to String (strips namespace prefix) |
| `write_arxml_header(out, root_attrs)` | Writes `<?xml...>`, `<AUTOSAR ...>`, `<AR-PACKAGES>` |
| `write_arxml_footer(out)` | Writes `</AR-PACKAGES>`, `</AUTOSAR>` |
| `collect_root_attrs(path)` | Extracts attributes from `<AUTOSAR>` root for header preservation |

### Command Functions

| Function | Description |
|---|---|
| `cmd_ls(path, show_elements, filter, recursive)` | Prints paths to stdout |
| `ls_collect(path, show_elements, filter, recursive) -> Vec<String>` | Core ls logic, returns paths (use this in tests) |
| `parse_cp_args(args) -> (Vec<CpGroup>, Option<String>)` | Parses cp CLI arguments |
| `cmd_cp(input, groups, rest_file)` | Copies packages to output files |
| `parse_rm_args(args) -> Vec<String>` | Normalises rm path arguments |
| `cmd_rm(input, packages)` | Removes packages/elements in-place |
| `cmd_diff(file_a, file_b, filter, colors) -> bool` | Structural diff, returns true if identical |
| `cmd_diff_extended(file_a, file_b, filter, colors) -> bool` | Structural + field-level diff |
| `collect_all_paths(path, filter) -> Vec<String>` | All AR-PACKAGE and ELEMENTS paths, sorted |
| `collect_element_fields(file, element_path) -> Vec<(String, String)>` | Direct child tags of one element |
| `collect_all_element_fields(file) -> HashMap<String, HashMap<String, String>>` | All element fields in one pass |

### Range Finders (byte-offset scanning)

| Function | Description |
|---|---|
| `find_package_ranges(path, targets) -> Vec<PackageRange>` | Byte ranges of specific AR-PACKAGEs |
| `find_all_toplevel_package_ranges(path) -> Vec<PackageRange>` | Byte ranges of all top-level packages |
| `find_element_ranges(path, targets) -> Vec<PackageRange>` | Byte ranges of specific ELEMENTS entries |

---

## Code Conventions

### Path Convention
- Paths are stored and looked up **without** leading `/` (normalised form via `normalise_path()`)
- Paths are printed **with** leading `/` using `format!("/{}", path)`
- All `HashSet` lookups, `HashMap` keys, and `PackageRange.path` use the normalised form

### Argument Parsing
No CLI framework — all args are parsed manually from `std::env::args()`. Each command follows a strict split:
1. `parse_<cmd>_args(args: &[String]) -> <type>` — pure parsing, no I/O, lives in `lib.rs`
2. `cmd_<cmd>(...)` — execution with I/O, lives in `lib.rs`
3. `main()` in the binary is a thin wrapper (10–20 lines): validate, parse, execute

### Error Handling
No `Result` types are returned from public functions — all errors are terminal:
```rust
.unwrap_or_else(|e| { eprintln!("Error: {}", e); std::process::exit(1); })
```
Non-fatal issues use `eprintln!("Warning: ...")` without exiting.

### XML Parsing Pattern
All parsers share the same event-loop skeleton:
```rust
let mut xml = Reader::from_reader(reader);
xml.config_mut().trim_text(true);  // false for byte-range finders
let mut buf = Vec::new();
loop {
    match xml.read_event_into(&mut buf) {
        Ok(Event::Start(ref e)) => { depth += 1; ... }
        Ok(Event::Text(ref e))  => { ... }
        Ok(Event::End(ref e))   => { ... depth -= 1; }
        Ok(Event::Eof) => break,
        Err(e) => { eprintln!("XML parse error: {}", e); std::process::exit(1); }
        _ => {}
    }
    buf.clear();
}
```

- `trim_text(true)` for semantic parsing (path/name collection)
- `trim_text(false)` for byte-range finding (positional accuracy)
- Depth tracked as `usize`, incremented on `Start`, decremented on `End`
- Package path tracked via a `Vec<String>` stack (`pkg_stack`) pushed/popped with `AR-PACKAGE` open/close
- Tag local names via `local_name_str(e.local_name().as_ref())`

### Byte-Range Two-Pass Pattern
Used in `cmd_cp` and `cmd_rm`:
1. **Scan pass**: stream XML, record `(start, end)` byte offsets via `xml.buffer_position()`
2. **Write pass**: open raw bytes, copy slices verbatim — no re-serialisation

---

## Tests

All tests live in `tests/integration.rs`. No unit tests inside `lib.rs`.

### Fixtures (inline `const &str`)

| Constant | Description |
|---|---|
| `FLAT_ARXML` | Three flat top-level packages: Alpha, Beta, Gamma |
| `FLAT_ARXML_AB` | Like FLAT but only Alpha and Beta |
| `NESTED_ARXML` | Root → {Components (MyComponent), Interfaces (MySRInterface)}, Types |
| `NESTED_ARXML_MODIFIED` | Like NESTED but NewComp added, MySRInterface removed |
| `ELEMENTS_ARXML_A/B` | Packages with CATEGORY/DESC fields for extended diff tests |

### Test Helpers

```rust
fn write_fixture(dir: &TempDir, name: &str, content: &str) -> String
// Writes fixture to temp file, returns absolute path

fn toplevel_package_names(path: &str) -> Vec<String>
// Calls ls_collect, strips leading slash — primary assertion helper

fn s(s: &str) -> String
// Shorthand for .to_string()
```

### Test Patterns
- Every test creates a `TempDir::new().unwrap()` for isolation
- `ls_collect` is the primary assertion tool — if it parses the output, it is valid ARXML
- Diff tests pass `&arx_utils::COLORS_OFF` to eliminate ANSI codes
- Byte-range correctness verified via `fs::read()` + raw byte slice assertions

### Adding a New Command
1. Add `parse_<cmd>_args` and `cmd_<cmd>` to `src/lib.rs`
2. Create `src/bin/arx-<cmd>.rs` with a thin `main()` wrapper
3. Add `[[bin]]` entry to `Cargo.toml`
4. Add the command to `print_usage()` in `src/bin/arx.rs`
5. Add integration tests in `tests/integration.rs` using the existing fixture + TempDir pattern
6. Update `README.md`
