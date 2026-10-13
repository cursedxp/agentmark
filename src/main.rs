//! Thin entry point. The full CLI (`check`, `build`, ...) arrives in step 7; until then the binary
//! parses the files it is given and prints diagnostics in the SPEC 10 format.

use std::process::ExitCode;

use agentmark::diag::render_all;
use agentmark::syntax::parser::parse_file;

fn main() -> ExitCode {
    let mut failed = false;
    for path in std::env::args().skip(1) {
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{path}: cannot read file: {e}");
                failed = true;
                continue;
            }
        };
        match parse_file(&path, &src) {
            Ok(file) => println!("{path}: {} declarations", file.decls.len()),
            Err(diags) => {
                eprintln!("{}", render_all(&diags, &src));
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
