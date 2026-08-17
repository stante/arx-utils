use std::env;
use arx_utils::cmd_diff;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: arx-diff <file1.arxml> <file2.arxml>");
        std::process::exit(1);
    }
    let identical = cmd_diff(&args[1], &args[2]);
    if !identical {
        std::process::exit(1);
    }
}
