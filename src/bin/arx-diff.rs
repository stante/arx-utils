use std::env;
use arx_utils::cmd_diff;

fn main() {
    let args: Vec<String> = env::args().collect();

    // arx-diff <file1> <file2> [/filter/path]
    if args.len() < 3 || args.len() > 4 {
        eprintln!("Usage: arx-diff <file1.arxml> <file2.arxml> [/filter/path]");
        std::process::exit(1);
    }

    let filter = args.get(3).map(|s| s.as_str());
    let identical = cmd_diff(&args[1], &args[2], filter);
    if !identical {
        std::process::exit(1);
    }
}
