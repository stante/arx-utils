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

### `arx ls` — List AR-Packages and elements

Lists all `AR-PACKAGE` paths and named elements in an ARXML file. There is no
`AR-PACKAGE`-vs-element distinction in the output — a package is simply a
node whose own tag happens to be `AR-PACKAGE`, exactly like
`I-SIGNAL-TRIGGERING` or any other element type. By default, `ls` shows
**everything**: all packages and all named elements, at any nesting depth
(cluster → variant → channel → triggering, however deep).

```
arx ls [-d <n>|--max-depth <n>] [-t <type>]... [-x <path>]... [/filter/path] <file.arxml>
```

**Options:**

| Option | Description |
|---|---|
| `-d <n>` / `--max-depth <n>` | Limit output to `n` levels below the `/filter/path` match (or below the implicit root, if no filter). `0` = only the match itself, `1` = match + direct children, etc. Mirrors `find -maxdepth`. Omit for unlimited depth (the default) |
| `-t <type>` | Only show nodes whose own XML tag matches this type name (e.g. `AR-PACKAGE`, `I-SIGNAL-TRIGGERING`). Repeatable (OR-matched). Non-matching ancestors are still traversed (silently) to reach matching descendants |
| `-x <path>` | Exclude this path (package or element, and everything under it) from the output. Supports `*` as a wildcard and may be repeated |
| `/filter/path` | Only show nodes under this path prefix — packages or, at any depth, named elements (e.g. `/Root/Cluster/Variant1/Channel1`) |

`-t AR-PACKAGE` is how you get a "packages only" listing (the old default
before elements were unified into the same tree).

`/filter/path` isn't limited to `AR-PACKAGE` boundaries — it may point at any
named element, however deep. Unlike `-x`, its wildcard segments (`*`) don't
span `/` — `/Root/*/Channel1` matches `Channel1` under any direct child of
`Root`, not arbitrarily deep descendants.

`-x` patterns *do* span `/`, so a pattern doesn't have to name an exact
node — e.g. `/ComponentTypes/Dummy*` excludes every package under
`/ComponentTypes` whose name starts with `Dummy`, along with all of their
descendants. Quote the pattern (e.g. `'/ComponentTypes/Dummy*'`) on shells
that would otherwise expand `*` themselves.

Wrapper/collection tags that have no `SHORT-NAME` of their own (e.g.
`ETHERNET-CLUSTER-VARIANTS`, `PHYSICAL-CHANNELS`, `I-SIGNAL-TRIGGERINGS`) are
traversed but never appear as path segments — only nodes that actually have
their own `SHORT-NAME` show up in the output.

**Examples:**

```sh
# List everything: every package and every element, at any depth
arx ls model.arxml

# Packages only, at any depth (old plain "ls -R" behaviour)
arx ls -t AR-PACKAGE model.arxml

# Top-level packages only (old plain "ls" default, before elements existed)
arx ls -d 1 model.arxml

# List direct children of /Root/Components (packages and/or elements)
arx ls -d 1 /Root/Components model.arxml

# List every I-SIGNAL-TRIGGERING anywhere in the file, however deeply nested
arx ls -t I-SIGNAL-TRIGGERING model.arxml

# List everything, but skip /Root/Components entirely
arx ls -x /Root/Components model.arxml

# Exclude all packages under /ComponentTypes whose name starts with "Dummy"
arx ls -x /ComponentTypes/Dummy* model.arxml

# List every I-SIGNAL-TRIGGERING under one specific channel — the filter
# already reaches into the element hierarchy, no extra flag needed
arx ls -t I-SIGNAL-TRIGGERING /Root/Cluster/Variant1/Channel1 model.arxml
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
arx diff [-e] <file1.arxml> <file2.arxml> [/filter/path]
```

Each difference is printed as a single coloured line:

- <span style="color:red">**-** `/path/to/removed`</span> — present in `file1.arxml`, missing in `file2.arxml`
- <span style="color:green">**+** `/path/to/added`</span> — present in `file2.arxml`, missing in `file1.arxml`

With `-e`, elements that exist in both files are also compared field by field:

- <span style="color:#cc0">**~** `/path/to/element`</span> — element exists in both files but fields differ
  - <span style="color:red">**- CATEGORY:** `APPLICATION`</span>
  - <span style="color:green">**+ CATEGORY:** `COMPOSITION`</span>

Exits with code `0` if the files are identical, `1` if differences were found.

**Options:**

| Option | Description |
|---|---|
| `-e` | Extended mode: also compare direct child tag values of elements present in both files. |
| `--color` | Force coloured output even when piping to a file or another program. By default colour is only enabled when writing to a terminal. |
| `/filter/path` | Only compare paths under this AR-PACKAGE prefix. Can be specified with or without a leading `/`. |

**Examples:**

```sh
# Compare full structure
arx diff baseline.arxml updated.arxml

# Extended: also compare field values of matching elements
arx diff -e baseline.arxml updated.arxml

# Compare only within /Root/Components
arx diff baseline.arxml updated.arxml /Root/Components

# Extended + filter
arx diff -e baseline.arxml updated.arxml /Root/Components

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
