use std::env;
use arx_utils::{cmd_rm, parse_rm_args};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: arx-rm <file.arxml> <pkg1> [<pkg2> ...]");
        std::process::exit(1);
    }
    let input = &args[1];
    let packages = parse_rm_args(&args[2..]);
    if packages.is_empty() {
        eprintln!("Error: no packages specified.");
        std::process::exit(1);
    }
    cmd_rm(input, &packages);
}
