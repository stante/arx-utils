use std::env;
use arxml_split::cmd_ls;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: arx-ls <file.arxml>");
        std::process::exit(1);
    }
    cmd_ls(&args[1]);
}
