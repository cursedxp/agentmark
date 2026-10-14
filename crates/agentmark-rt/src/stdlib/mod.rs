//! The built-in library of SPEC 22.3: pure modules (`text`, `regex`, `number`, `money`, `date`,
//! `path`, `json`, `csv`) that generated code calls as `agentmark_rt::stdlib::<module>::<fn>`, and
//! the `file` tool ([`file`]). A function the language types as `result of T` returns
//! `Result<T, RtError>` with a plain-language message; the others return a value (or a `Result`
//! only for a pattern that is not valid, which the compiler also checks when it is a literal).

pub mod csv;
pub mod date;
pub mod file;
pub mod json;
pub mod money;
pub mod number;
pub mod path;
pub mod regex;
pub mod text;

/// Reads all of standard input (`- read input -> text`).
pub fn read_input() -> Result<String, crate::RtError> {
    use std::io::Read;
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s)?;
    Ok(s)
}
