use std::env;
use arxml_split::{cmd_mv, normalise_path};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: arx-mv <file.arxml> <pkg1> [<pkg2> ...] <output.arxml>");
        std::process::exit(1);
    }
    let input = &args[1];
    let output = &args[args.len() - 1];
    let packages: Vec<String> = args[2..args.len() - 1]
        .iter()
        .map(|s| normalise_path(s))
        .collect();
    cmd_mv(input, &packages, output);
}
