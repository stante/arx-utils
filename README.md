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

## Installation

Requires [Rust](https://rustup.rs/).

```sh
cargo install --path .
```

This installs three binaries: `arx`, `arx-ls`, and `arx-cp`.

---

## Implementation Notes

- **Streaming XML parser** (`quick-xml`): the file is never fully loaded into memory, so it scales to large ARXML files.
- **Byte-range copying**: `arx cp` locates package blocks by byte offset in a first pass, then copies raw bytes in a second pass — the original formatting is preserved exactly.
- **Extensible dispatcher**: `arx <cmd>` simply looks up `arx-<cmd>` in `PATH`, so new subcommands can be added as standalone binaries without touching the dispatcher.
