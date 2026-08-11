use std::env;
use std::fs::File;
use std::io::BufReader;

use quick_xml::events::Event;
use quick_xml::Reader;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args[1] != "ls" {
        eprintln!("Usage: {} ls <path-to-file.arxml>", args[0]);
        std::process::exit(1);
    }

    let path = &args[2];
    let file = File::open(path).unwrap_or_else(|e| {
        eprintln!("Error opening file '{}': {}", path, e);
        std::process::exit(1);
    });

    let reader = BufReader::new(file);
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    let mut buf = Vec::new();
    // Stack tracks the SHORT-NAME of each AR-PACKAGE ancestor
    let mut package_stack: Vec<String> = Vec::new();
    // Are we currently inside a SHORT-NAME element that belongs to an AR-PACKAGE?
    let mut capturing_short_name = false;
    // Depth of the element that triggered short-name capture (the AR-PACKAGE start tag)
    let mut capture_depth: usize = 0;
    // Current element nesting depth
    let mut depth: usize = 0;

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local_name = e.local_name();
                let name = std::str::from_utf8(local_name.as_ref()).unwrap_or("");

                if name == "AR-PACKAGE" {
                    // We expect the very next SHORT-NAME child to be the package name
                    capture_depth = depth;
                } else if name == "SHORT-NAME" && capture_depth > 0 && depth == capture_depth + 1 {
                    capturing_short_name = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                if capturing_short_name {
                    let short_name = e.unescape().unwrap_or_default().into_owned();
                    package_stack.push(short_name.clone());

                    // Build and print the full path
                    let full_path = package_stack.join("/");
                    println!("/{}", full_path);

                    capturing_short_name = false;
                    capture_depth = 0;
                }
            }
            Ok(Event::End(ref e)) => {
                let local_name = e.local_name();
                let name = std::str::from_utf8(local_name.as_ref()).unwrap_or("");

                if name == "AR-PACKAGE" {
                    package_stack.pop();
                }

                depth -= 1;
            }
            Ok(Event::Empty(_)) => {
                // self-closing tags — no depth change needed for start+end combined
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
}
