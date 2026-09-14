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

/// One currently open tag, during tree building. Every start tag other than
/// `SHORT-NAME` gets a frame — `AR-PACKAGE`, `AR-PACKAGES`, `ELEMENTS`, and
/// any element or wrapper/collection tag alike, with no special case for any
/// particular tag name. A frame becomes `named` once a direct `SHORT-NAME`
/// child has been seen for it.
struct OpenFrame {
    depth: usize,
    tag_name: String,
    named: bool,
}

/// Streams `path` once and builds the full [`Tree`] of every `AR-PACKAGE`
/// and every named element at any depth. Always builds everything,
/// regardless of any `ls` filtering options — those are applied afterwards
/// by `query_tree`, keeping traversal and filtering fully decoupled.
///
/// Every open tag is tracked uniformly on a single stack, regardless of its
/// name: a tag becomes a node the moment a direct `SHORT-NAME` child is
/// seen, and a new node's parent is simply the nearest still-open named
/// ancestor. There is no special case for `AR-PACKAGE` or `ELEMENTS` — this
/// is what lets siblings of the same tag name nest arbitrarily (a package
/// with both its own `ELEMENTS` and nested `AR-PACKAGES`, an
/// `APPLICATION-RECORD-DATA-TYPE`'s own `ELEMENTS` inside its package's
/// `ELEMENTS`, ...) without one closing tag being mistaken for another.
pub(crate) fn build_tree(path: &str) -> Tree {
    let file = open_file(path);
    let reader = BufReader::new(file);
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut nodes: Vec<Node> = Vec::new();
    // Stack of every currently open tag (mirrors XML nesting exactly),
    // including wrapper/collection tags that never get their own
    // SHORT-NAME (e.g. AR-PACKAGES, ELEMENTS, PHYSICAL-CHANNELS).
    let mut open_stack: Vec<OpenFrame> = Vec::new();
    // Arena indices of all currently open *named* ancestors (unnamed
    // wrapper tags contribute nothing), used to find each new node's parent.
    let mut named_stack: Vec<usize> = Vec::new();
    let mut capturing_short_name = false;
    let mut depth: usize = 0;

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local_name = e.local_name();

                if local_name.as_ref() == b"SHORT-NAME" {
                    // Only a direct child of the innermost open (not yet
                    // named) tag counts as that tag's own SHORT-NAME.
                    if let Some(top) = open_stack.last() {
                        if !top.named && depth == top.depth + 1 {
                            capturing_short_name = true;
                        }
                    }
                } else {
                    // Only allocate a String for tags we actually keep
                    // around on the stack — SHORT-NAME itself (handled
                    // above) and plain closing tags (below) never need one.
                    let tag_name = local_name_str(local_name.as_ref());
                    open_stack.push(OpenFrame { depth, tag_name, named: false });
                }
            }
            Ok(Event::Text(ref e)) => {
                if capturing_short_name {
                    capturing_short_name = false;
                    let short_name = e.unescape().unwrap_or_default().into_owned();

                    if let Some(top) = open_stack.last_mut() {
                        top.named = true;
                        let parent = named_stack.last().copied();
                        // tag_name is never read again after this, so move
                        // it out instead of cloning.
                        let tag = std::mem::take(&mut top.tag_name);
                        nodes.push(Node { name: short_name, tag, parent });
                        named_stack.push(nodes.len() - 1);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                // A byte comparison here (instead of decoding to a String
                // first) is exact for ASCII tag names like SHORT-NAME, and
                // the result is only ever compared, never stored.
                if e.local_name().as_ref() != b"SHORT-NAME" {
                    if let Some(popped) = open_stack.pop() {
                        if popped.named {
                            named_stack.pop();
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
