# arx-utils

[![CI](https://github.com/stante/arx-utils/actions/workflows/ci.yml/badge.svg)](https://github.com/stante/arx-utils/actions/workflows/ci.yml)

Command-line utilities for working with AUTOSAR ARXML files.

## Tools

### `arx`

The main dispatcher. Invokes subcommands as `arx-<command>` from `PATH` — similar to how `git` works.

```
arx <command> [args...]
```

---

## Commands

### `arx ls` — List AR-Packages

Lists all `AR-PACKAGE` paths in an ARXML file.

```
arx ls [-e] [-R] [/filter/path] <file.arxml>
```

**Options:**

| Option | Description |
|---|---|
| `-e` | Also show `ELEMENTS` entries (e.g. component names) inside packages |
| `-R` | Recursive — show all descendants, not just direct children |
| `/filter/path` | Only show packages under this AUTOSAR path prefix |

**Examples:**

```sh
# List all top-level packages
arx ls model.arxml

# List all packages recursively
arx ls -R model.arxml

# List direct children of /Root/Components
arx ls /Root/Components model.arxml

# List everything under /Root/Components including elements
arx ls -R -e /Root/Components model.arxml
```

---

### `arx cp` — Copy AR-Packages

Copies one or more `AR-PACKAGE` blocks from a source ARXML file into one or more output files.

```
arx cp <file.arxml> <pkg> [<pkg> ...] --into <out.arxml>
                   [<pkg> ...] --into <out2.arxml>
                   [--rest <rest.arxml>]
```

**Options:**

| Option | Description |
|---|---|
| `--into <file>` | Output file for the preceding package(s). Can be repeated. |
| `--rest <file>` | Write all top-level packages **not** matched by any `--into` group into this file. |

Package paths can be specified with or without a leading `/`.

**Examples:**

```sh
# Copy a single package into a new file
arx cp model.arxml /Root/Components --into components.arxml

# Split multiple packages into separate files
arx cp model.arxml \
  /Root/Components --into components.arxml \
  /Root/Interfaces --into interfaces.arxml

# Split out specific packages and keep the rest
arx cp model.arxml \
  /Root/Components --into components.arxml \
  --rest remainder.arxml
```

The output files are valid ARXML files — the original `<AUTOSAR>` root element attributes (namespaces, schema locations, etc.) are preserved, and package blocks are copied byte-for-byte without re-serialisation.

---

### `arx rm` — Remove AR-Packages

Removes one or more `AR-PACKAGE` blocks or individual `ELEMENTS` entries from an ARXML file, modifying the file in-place.

```
arx rm <file.arxml> <path1> [<path2> ...]
```

Package paths can be specified with or without a leading `/`.

**Examples:**

```sh
# Remove a top-level package
arx rm model.arxml /Root/Components

# Remove multiple packages at once
arx rm model.arxml /Root/Components /Root/Interfaces

# Remove a single element inside a package
arx rm model.arxml /Root/Components/MyComponent

# Mix: remove a package and an element in one call
arx rm model.arxml /Root/Types /Root/Components/MyComponent
```

The file is overwritten in-place. Remaining packages and elements are preserved byte-for-byte and the result is a valid ARXML file.

---

### `arx diff` — Diff AR-Package / Element structure

Compares the `AR-PACKAGE` and `ELEMENTS` structure of two ARXML files.  
The comparison is order-independent — only the set of paths matters, not their position in the file.

```
arx diff <file1.arxml> <file2.arxml> [/filter/path]
```

Each difference is printed as a single coloured line:

- <span style="color:red">**-** `/path/to/removed`</span> — present in `file1.arxml`, missing in `file2.arxml`
- <span style="color:green">**+** `/path/to/added`</span> — present in `file2.arxml`, missing in `file1.arxml`

Exits with code `0` if the files are identical, `1` if differences were found.

**Options:**

| Option | Description |
|---|---|
| `/filter/path` | Only compare paths under this AR-PACKAGE prefix. Can be specified with or without a leading `/`. |

**Examples:**

```sh
# Compare full structure
arx diff baseline.arxml updated.arxml

# Compare only within /Root/Components
arx diff baseline.arxml updated.arxml /Root/Components

# Use in a script
arx diff baseline.arxml updated.arxml && echo "no structural changes"
```

---

## Installation

Requires [Rust](https://rustup.rs/).

**From GitHub:**

```sh
cargo install --git https://github.com/stante/arx-utils
```

**From a local clone:**

```sh
cargo install --path .
```

Both commands install four binaries: `arx`, `arx-ls`, `arx-cp`, `arx-rm`, and `arx-diff`.

---

## Implementation Notes

- **Streaming XML parser** (`quick-xml`): the file is never fully loaded into memory, so it scales to large ARXML files.
- **Byte-range copying**: `arx cp` locates package blocks by byte offset in a first pass, then copies raw bytes in a second pass — the original formatting is preserved exactly.
- **Extensible dispatcher**: `arx <cmd>` simply looks up `arx-<cmd>` in `PATH`, so new subcommands can be added as standalone binaries without touching the dispatcher.
