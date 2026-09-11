use std::env;
use arx_utils::cmd_ls;

const USAGE: &str =
    "Usage: arx-ls [-d <n>|--max-depth <n>] [-t <type>]... [-x <path>]... [/filter/path] <file.arxml>";

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut max_depth: Option<usize> = None;
    let mut filter: Option<String> = None;
    let mut excludes: Vec<String> = Vec::new();
    let mut type_filter: Vec<String> = Vec::new();
    let mut file: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-d" | "--max-depth" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: {} requires a number argument.", args[i - 1]);
                    eprintln!("{}", USAGE);
                    std::process::exit(1);
                }
                max_depth = Some(args[i].parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("Error: invalid --max-depth value '{}'.", args[i]);
                    std::process::exit(1);
                }));
            }
            "-x" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: -x requires a path argument.");
                    eprintln!("{}", USAGE);
                    std::process::exit(1);
                }
                excludes.push(args[i].clone());
            }
            "-t" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: -t requires a type name argument.");
                    eprintln!("{}", USAGE);
                    std::process::exit(1);
                }
                type_filter.push(args[i].clone());
            }
            arg if arg.starts_with('/') => {
                filter = Some(arg.to_string());
            }
            _ => {
                if file.is_none() {
                    file = Some(args[i].clone());
                } else {
                    eprintln!("{}", USAGE);
                    std::process::exit(1);
                }
            }
        }
        i += 1;
    }

    let path = file.unwrap_or_else(|| {
        eprintln!("{}", USAGE);
        std::process::exit(1);
    });

    cmd_ls(&path, filter.as_deref(), max_depth, &excludes, &type_filter);
}
