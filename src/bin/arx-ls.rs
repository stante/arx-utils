use std::env;
use arxml_split::cmd_ls;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut show_elements = false;
    let mut filter: Option<String> = None;
    let mut file: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-e" => show_elements = true,
            arg if arg.starts_with('/') => {
                // Could be a filter path or the file — paths start with '/', files typically don't
                // but we treat leading '/' as a package filter
                filter = Some(arg.to_string());
            }
            _ => {
                if file.is_none() {
                    file = Some(args[i].clone());
                } else {
                    eprintln!("Usage: arx-ls [-e [/filter/path]] <file.arxml>");
                    std::process::exit(1);
                }
            }
        }
        i += 1;
    }

    let path = file.unwrap_or_else(|| {
        eprintln!("Usage: arx-ls [-e [/filter/path]] <file.arxml>");
        std::process::exit(1);
    });

    cmd_ls(&path, show_elements, filter.as_deref());
}
