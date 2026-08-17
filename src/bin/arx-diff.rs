use std::env;
use arx_utils::{cmd_diff, cmd_diff_extended};

fn main() {
    let args: Vec<String> = env::args().collect();

    // arx-diff [-e] <file1> <file2> [/filter/path]
    let extended = args.iter().any(|a| a == "-e");
    let positional: Vec<&String> = args[1..].iter().filter(|a| *a != "-e").collect();

    if positional.len() < 2 || positional.len() > 3 {
        eprintln!("Usage: arx-diff [-e] <file1.arxml> <file2.arxml> [/filter/path]");
        std::process::exit(1);
    }

    let file1 = positional[0].as_str();
    let file2 = positional[1].as_str();
    let filter = positional.get(2).map(|s| s.as_str());

    let identical = if extended {
        cmd_diff_extended(file1, file2, filter)
    } else {
        cmd_diff(file1, file2, filter)
    };

    if !identical {
        std::process::exit(1);
    }
}
