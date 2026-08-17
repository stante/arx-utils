use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let subcommand = &args[1];
    let sub_args = &args[2..];

    // Build the binary name: arx-<subcommand>
    let binary = format!("arx-{}", subcommand);

    let status = Command::new(&binary)
        .args(sub_args)
        .status()
        .unwrap_or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("arx: unknown command '{}' (binary '{}' not found in PATH)", subcommand, binary);
            } else {
                eprintln!("arx: failed to run '{}': {}", binary, e);
            }
            std::process::exit(1);
        });

    std::process::exit(status.code().unwrap_or(1));
}

fn print_usage() {
    eprintln!("Usage: arx <command> [args...]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  ls  <file.arxml>");
    eprintln!("  cp  <file.arxml> <pkg1> [<pkg2> ...] --into <out.arxml> [--rest <rest.arxml>]");
    eprintln!("  rm    <file.arxml> <path1> [<path2> ...]");
    eprintln!("  diff  <file1.arxml> <file2.arxml>");
}
