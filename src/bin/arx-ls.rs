use std::env;
use arx_utils::{cmd_ls, parse_ls_args};

fn main() {
    let args: Vec<String> = env::args().collect();
    let (file, filter, max_depth, excludes, type_filter) = parse_ls_args(&args[1..]);
    cmd_ls(&file, filter.as_deref(), max_depth, &excludes, &type_filter);
}
