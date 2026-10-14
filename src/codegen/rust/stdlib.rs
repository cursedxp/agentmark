//! Rust spelling of the built-in library (SPEC 22.3): `agentmark_rt::stdlib::<module>::<fn>(…)`.
//! The table of functions is `crate::stdlib`; `body.rs` renders the arguments (it knows how to borrow
//! and clone) and this file puts the call together.

use crate::stdlib::{Func, Ty};

/// How a parameter is passed to the Rust function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// `&str`: a literal stays a literal, anything else is borrowed.
    Text,
    /// By reference (`&value`): any value, any list, a date or datetime.
    Ref,
    /// By value.
    Value,
}

/// How the parameter `ty` is passed.
pub fn pass(ty: Ty) -> Pass {
    match ty {
        Ty::Text => Pass::Text,
        Ty::Any | Ty::AnyList | Ty::DateAny => Pass::Ref,
        _ => Pass::Value,
    }
}

/// The call text. `args` are the rendered arguments in order; `num` is the Rust type of the numeric
/// argument for the generic number functions (`i64` or `Money`); `decode` is the Rust type the
/// result is read as (the record, or the element of a list for `csv`). `question` adds the `?`
/// that turns a Rust `Result` into the value (a `try` statement) — otherwise a `result of T`
/// stays a Rust `Result`.
pub fn call(f: &Func, args: &[String], num: Option<&str>, decode: Option<&str>, question: bool) -> String {
    let turbofish = match (f.module, f.ret, decode, num) {
        ("json", _, Some(t), _) | ("csv", _, Some(t), _) => format!("::<{t}>"),
        (_, _, _, Some(n)) if f.params.iter().any(|p| p.1 == Ty::Num) => format!("::<{n}>"),
        _ => String::new(),
    };
    let mut s = format!("agentmark_rt::stdlib::{}::{}{turbofish}({})", f.module, f.name, args.join(", "));
    if (f.rt_err && !f.is_result()) || (f.is_result() && question) {
        s.push('?');
    }
    s
}
