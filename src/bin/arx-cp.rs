use std::env;
use arxml_split::{cmd_cp, parse_cp_args};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "Usage: arx-cp <file.arxml> <pkg1> [<pkg2> ...] --into <out.arxml> [--rest <rest.arxml>]"
        );
        std::process::exit(1);
    }
    let input = &args[1];
    let (groups, rest_file) = parse_cp_args(&args[2..]);
    if groups.is_empty() {
        eprintln!("Error: no --into specified.");
        std::process::exit(1);
    }
    cmd_cp(input, &groups, rest_file.as_deref());
}
