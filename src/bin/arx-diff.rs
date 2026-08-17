use std::env;
use std::io::IsTerminal;
use arx_utils::{cmd_diff, cmd_diff_extended, COLORS_ON, COLORS_OFF};

fn main() {
    let args: Vec<String> = env::args().collect();

    // arx-diff [-e] [--color] <file1> <file2> [/filter/path]
    let extended  = args.iter().any(|a| a == "-e");
    let force_color = args.iter().any(|a| a == "--color");
    let positional: Vec<&String> = args[1..]
        .iter()
        .filter(|a| *a != "-e" && *a != "--color")
        .collect();

    if positional.len() < 2 || positional.len() > 3 {
        eprintln!("Usage: arx-diff [-e] [--color] <file1.arxml> <file2.arxml> [/filter/path]");
        std::process::exit(1);
    }

    let file1  = positional[0].as_str();
    let file2  = positional[1].as_str();
    let filter = positional.get(2).map(|s| s.as_str());

    let use_color = force_color || std::io::stdout().is_terminal();
    let colors = if use_color { &COLORS_ON } else { &COLORS_OFF };

    let identical = if extended {
        cmd_diff_extended(file1, file2, filter, colors)
    } else {
        cmd_diff(file1, file2, filter, colors)
    };

    if !identical {
        std::process::exit(1);
    }
}
