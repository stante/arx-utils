use std::env;
use arxml_split::cmd_ls;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut show_elements = false;
    let mut file: Option<&str> = None;

    for arg in &args[1..] {
        match arg.as_str() {
            "-e" => show_elements = true,
            _ if file.is_none() => file = Some(arg.as_str()),
            _ => {
                eprintln!("Usage: arx-ls [-e] <file.arxml>");
                std::process::exit(1);
            }
        }
    }

    let path = file.unwrap_or_else(|| {
        eprintln!("Usage: arx-ls [-e] <file.arxml>");
        std::process::exit(1);
    });

    cmd_ls(path, show_elements);
}
