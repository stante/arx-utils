use std::env;
use arx_utils::cmd_ls;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut show_elements = false;
    let mut recursive = false;
    let mut filter: Option<String> = None;
    let mut excludes: Vec<String> = Vec::new();
    let mut file: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-e" => show_elements = true,
            "-R" => recursive = true,
            "-x" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: -x requires a path argument.");
                    eprintln!("Usage: arx-ls [-e] [-R] [-x <path>]... [/filter/path] <file.arxml>");
                    std::process::exit(1);
                }
                excludes.push(args[i].clone());
            }
            arg if arg.starts_with('/') => {
                filter = Some(arg.to_string());
            }
            _ => {
                if file.is_none() {
                    file = Some(args[i].clone());
                } else {
                    eprintln!("Usage: arx-ls [-e] [-R] [-x <path>]... [/filter/path] <file.arxml>");
                    std::process::exit(1);
                }
            }
        }
        i += 1;
    }

    let path = file.unwrap_or_else(|| {
        eprintln!("Usage: arx-ls [-e] [-R] [-x <path>]... [/filter/path] <file.arxml>");
        std::process::exit(1);
    });

    cmd_ls(&path, show_elements, filter.as_deref(), recursive, &excludes);
}
