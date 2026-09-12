use std::fs::File;
use std::io::{BufWriter, Write};

pub fn normalise_path(p: &str) -> String {
    p.trim().trim_start_matches('/').to_string()
}

pub(crate) fn open_file(path: &str) -> File {
    File::open(path).unwrap_or_else(|e| {
        eprintln!("Error opening file '{}': {}", path, e);
        std::process::exit(1);
    })
}

pub(crate) fn local_name_str(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes).unwrap_or("").to_string()
}

pub(crate) fn write_arxml_header<W: Write>(out: &mut BufWriter<W>, root_attrs: &[(String, String)]) {
    writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    write!(out, "<AUTOSAR").unwrap();
    for (k, v) in root_attrs {
        write!(out, r#" {}="{}""#, k, v).unwrap();
    }
    writeln!(out, ">").unwrap();
    writeln!(out, "  <AR-PACKAGES>").unwrap();
}

pub(crate) fn write_arxml_footer<W: Write>(out: &mut BufWriter<W>) {
    writeln!(out, "  </AR-PACKAGES>").unwrap();
    writeln!(out, "</AUTOSAR>").unwrap();
}
