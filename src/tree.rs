//! Arena-based tree of every named `AR-PACKAGE` and every named element in an
//! ARXML file, built once via [`build_tree`] and shared by `ls` and `diff`.

use std::io::BufReader;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::util::{local_name_str, open_file};

/// One node in the [`Tree`] arena. `parent` is an index into `Tree::nodes`
/// (`None` for a top-level `AR-PACKAGE`).
///
/// There is no separate "package" vs. "element" distinction in the data
/// model — an `AR-PACKAGE` is simply a node whose own `tag` happens to be
/// `"AR-PACKAGE"`, exactly like `I-SIGNAL-TRIGGERING` or any other element
/// type. `-t AR-PACKAGE` filters to just packages the same way `-t
/// I-SIGNAL-TRIGGERING` filters to just triggerings.
pub(crate) struct Node {
    pub(crate) name: String,
    pub(crate) tag: String,
    pub(crate) parent: Option<usize>,
}

/// Arena-based tree of every named `AR-PACKAGE` and every named element (at
/// any nesting depth) found in an ARXML file. Built once via [`build_tree`]
/// in a single streaming pass; independent of any `ls` filtering options
/// (`-t`/`-d`/`-x`/filter path), which are all applied afterwards by
/// `query_tree`.
///
/// Nodes reference their parent by index instead of each storing its own
/// full path string, so a node's name is kept exactly once no matter how
/// many descendants it has — e.g. a channel with 500 signal triggerings
/// underneath doesn't duplicate its own path prefix 500 times.
pub(crate) struct Tree {
    pub(crate) nodes: Vec<Node>,
}

impl Tree {
    /// Reconstructs the full `/`-separated path of `idx` by walking up the
    /// `parent` chain. Only called (by `query_tree`) for nodes that already
    /// passed the cheap pre-filters (type), so the cost is proportional to
    /// the number of *matches*, not the size of the tree.
    pub(crate) fn full_path(&self, idx: usize) -> String {
        let mut segments = Vec::new();
        let mut cur = Some(idx);
        while let Some(i) = cur {
            segments.push(self.nodes[i].name.as_str());
            cur = self.nodes[i].parent;
        }
        segments.reverse();
        format!("/{}", segments.join("/"))
    }
}

/// Per-AR-PACKAGE parsing state used while building the [`Tree`], pushed
/// when an `AR-PACKAGE` opens and popped when it closes. Using a stack
/// (instead of shared scalars) ensures that closing a nested AR-PACKAGE
/// correctly restores the enclosing package's own ELEMENTS-tracking state,
/// regardless of whether `ELEMENTS` appears before or after the nested
/// `AR-PACKAGES` block in the XML.
struct PkgBuildFrame {
    /// Depth at which this AR-PACKAGE's own `<AR-PACKAGE>` start tag occurred.
    capture_depth: usize,
    /// Whether we're currently inside this package's own `<ELEMENTS>` block.
    in_elements: bool,
    /// Stack of currently open tags within this package's ELEMENTS subtree
    /// (mirrors XML nesting exactly, including wrapper/collection tags that
    /// never get their own SHORT-NAME, e.g. `PHYSICAL-CHANNELS`).
    elem_open: Vec<ElemBuildFrame>,
    /// Arena indices of all currently open *named* ancestors within this
    /// package's ELEMENTS subtree (unnamed wrapper tags contribute nothing).
    named_stack: Vec<usize>,
}

/// One currently open tag within an ELEMENTS subtree, during tree building.
/// Becomes `named` once a direct `SHORT-NAME` child has been seen for it.
struct ElemBuildFrame {
    depth: usize,
    tag_name: String,
    named: bool,
}

/// Streams `path` once and builds the full [`Tree`] of every `AR-PACKAGE`
/// and every named element at any depth. Always builds everything,
/// regardless of any `ls` filtering options — those are applied afterwards
/// by `query_tree`, keeping traversal and filtering fully decoupled.
pub(crate) fn build_tree(path: &str) -> Tree {
    let file = open_file(path);
    let reader = BufReader::new(file);
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut nodes: Vec<Node> = Vec::new();
    // Arena indices of all currently open AR-PACKAGEs.
    let mut package_stack: Vec<usize> = Vec::new();
    let mut capturing_short_name = false;
    let mut capturing_element_short_name = false;
    // One frame per currently-open AR-PACKAGE, so that closing a nested
    // AR-PACKAGE correctly restores the enclosing package's own ELEMENTS
    // tracking state (instead of leaking stale depths between siblings).
    let mut pkg_frames: Vec<PkgBuildFrame> = Vec::new();
    let mut depth: usize = 0;

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let name = local_name_str(e.local_name().as_ref());

                if name == "AR-PACKAGE" {
                    pkg_frames.push(PkgBuildFrame {
                        capture_depth: depth,
                        in_elements: false,
                        elem_open: Vec::new(),
                        named_stack: Vec::new(),
                    });
                } else if let Some(frame) = pkg_frames.last_mut() {
                    if !frame.in_elements {
                        if name == "SHORT-NAME" && depth == frame.capture_depth + 1 {
                            capturing_short_name = true;
                        } else if name == "ELEMENTS" && depth == frame.capture_depth + 1 {
                            frame.in_elements = true;
                            frame.elem_open.clear();
                            frame.named_stack.clear();
                        }
                    } else if name == "SHORT-NAME" {
                        // Only a direct child of the innermost open (not yet
                        // named) tag counts as that tag's own SHORT-NAME.
                        if let Some(top) = frame.elem_open.last() {
                            if !top.named && depth == top.depth + 1 {
                                capturing_element_short_name = true;
                            }
                        }
                    } else {
                        // Any other tag while inside ELEMENTS: could be a
                        // wrapper/collection tag (no own SHORT-NAME) or a
                        // typed element — we don't know yet, so just track it.
                        frame.elem_open.push(ElemBuildFrame { depth, tag_name: name, named: false });
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                if capturing_short_name {
                    let short_name = e.unescape().unwrap_or_default().into_owned();
                    capturing_short_name = false;

                    let parent = package_stack.last().copied();
                    nodes.push(Node { name: short_name, tag: "AR-PACKAGE".to_string(), parent });
                    package_stack.push(nodes.len() - 1);
                } else if capturing_element_short_name {
                    let short_name = e.unescape().unwrap_or_default().into_owned();
                    capturing_element_short_name = false;

                    if let Some(frame) = pkg_frames.last_mut() {
                        let tag_name = match frame.elem_open.last_mut() {
                            Some(top) => {
                                top.named = true;
                                top.tag_name.clone()
                            }
                            None => String::new(),
                        };

                        let parent = frame
                            .named_stack
                            .last()
                            .copied()
                            .or_else(|| package_stack.last().copied());

                        nodes.push(Node { name: short_name, tag: tag_name, parent });
                        frame.named_stack.push(nodes.len() - 1);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = local_name_str(e.local_name().as_ref());
                if name == "AR-PACKAGE" {
                    package_stack.pop();
                    pkg_frames.pop();
                } else if let Some(frame) = pkg_frames.last_mut() {
                    if frame.in_elements {
                        if name == "ELEMENTS" {
                            frame.in_elements = false;
                            frame.elem_open.clear();
                            frame.named_stack.clear();
                        } else if name != "SHORT-NAME" {
                            if let Some(popped) = frame.elem_open.pop() {
                                if popped.named {
                                    frame.named_stack.pop();
                                }
                            }
                        }
                    }
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

    Tree { nodes }
}
