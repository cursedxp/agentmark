//! The built-in library (SPEC 22.3) in one table. Sema types calls from it, codegen spells the Rust
//! call from it, `check` and the editor read the names, and `docs/reference.md` lists the same rows
//! (a test keeps the two in step). Adding a function is one row here plus its Rust function in
//! `agentmark_rt::stdlib::<module>`.
//!
//! This file is the language-neutral description; it does not depend on the checker's `Type`.

/// What a parameter accepts or a result is, in the language's own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Text,
    Int,
    Bool,
    Money,
    Date,
    /// Parameter: `int` or `money`. Result: the type of the first argument.
    Num,
    /// Parameter: `date` or `datetime`.
    DateAny,
    /// Parameter: any value.
    Any,
    /// Parameter: any list.
    AnyList,
    ListText,
    MaybeText,
    MaybeListText,
    ResultInt,
    ResultMoney,
    ResultDate,
    /// Result: `result of T`, where `T` is written after the call with `as T`.
    Decoded,
}

/// One built-in function.
#[derive(Debug, Clone, Copy)]
pub struct Func {
    pub module: &'static str,
    pub name: &'static str,
    /// Parameter names and what they take, in order.
    pub params: &'static [(&'static str, Ty)],
    pub ret: Ty,
    /// The Rust function returns `Result<_, RtError>` although the language type is not a `result`
    /// (a pattern that does not compile, amounts of different currencies, a number too large).
    pub rt_err: bool,
    pub doc: &'static str,
}

impl Func {
    /// The result is a `result of T` the program must handle with `try` or `match`.
    pub fn is_result(&self) -> bool {
        matches!(self.ret, Ty::ResultInt | Ty::ResultMoney | Ty::ResultDate | Ty::Decoded)
    }

    /// `text.split(t, sep)`.
    pub fn signature(&self) -> String {
        let ps: Vec<&str> = self.params.iter().map(|p| p.0).collect();
        format!("{}.{}({})", self.module, self.name, ps.join(", "))
    }
}

const fn f(
    module: &'static str,
    name: &'static str,
    params: &'static [(&'static str, Ty)],
    ret: Ty,
    rt_err: bool,
    doc: &'static str,
) -> Func {
    Func { module, name, params, ret, rt_err, doc }
}

use Ty::*;

/// Every function of the library, grouped by module in the order SPEC 22.3 lists them.
pub const FUNCS: &[Func] = &[
    f("text", "split", &[("t", Text), ("sep", Text)], ListText, false, "the pieces of t between sep"),
    f("text", "lines", &[("t", Text)], ListText, false, "the lines of t (no empty entry for a final newline)"),
    f("text", "trim", &[("t", Text)], Text, false, "t without white space at both ends"),
    f("text", "lower", &[("t", Text)], Text, false, "t in lower case"),
    f("text", "upper", &[("t", Text)], Text, false, "t in upper case"),
    f("text", "replace", &[("t", Text), ("from", Text), ("to", Text)], Text, false, "every from in t replaced by to"),
    f("text", "contains", &[("t", Text), ("part", Text)], Bool, false, "does t contain part?"),
    f("text", "starts_with", &[("t", Text), ("p", Text)], Bool, false, "does t start with p?"),
    f("text", "ends_with", &[("t", Text), ("p", Text)], Bool, false, "does t end with p?"),
    f("text", "length", &[("t", Text)], Int, false, "number of characters in t"),
    f("text", "pad", &[("t", Text), ("width", Int)], Text, false, "t with spaces added on the right up to width characters"),
    f("text", "repeat", &[("t", Text), ("n", Int)], Text, false, "t written n times"),
    f("regex", "matches", &[("t", Text), ("pattern", Text)], Bool, true, "does the pattern match anywhere in t?"),
    f("regex", "find", &[("t", Text), ("pattern", Text)], MaybeText, true, "the first match, or nothing"),
    f("regex", "captures", &[("t", Text), ("pattern", Text)], MaybeListText, true, "the groups of the first match (without the whole match), or nothing"),
    f("regex", "replace", &[("t", Text), ("pattern", Text), ("with", Text)], Text, true, "every match replaced; $1 is group 1"),
    f("number", "round", &[("x", Num), ("places", Int)], Num, true, "x rounded to places decimals (half away from zero)"),
    f("number", "parse", &[("t", Text)], ResultInt, false, "the whole number written in t, or an error"),
    f("number", "min", &[("a", Num), ("b", Num)], Num, true, "the smaller of a and b"),
    f("number", "max", &[("a", Num), ("b", Num)], Num, true, "the larger of a and b"),
    f("number", "abs", &[("x", Num)], Num, true, "x without its sign"),
    f("number", "format", &[("x", Int), ("places", Int)], Text, false, "x written with exactly places decimals"),
    f("money", "parse", &[("t", Text)], ResultMoney, false, "the amount written in t (like 12.50 EUR), or an error"),
    f("money", "format", &[("m", Money)], Text, false, "m written like a literal (12.50 EUR)"),
    f("date", "parse", &[("t", Text), ("pattern", Text)], ResultDate, false, "the date written in t as pattern, or an error"),
    f("date", "format", &[("d", DateAny), ("pattern", Text)], Text, true, "d written as pattern"),
    f("date", "days_between", &[("a", Date), ("b", Date)], Int, false, "days from a to b (negative when b is earlier)"),
    f("path", "join", &[("a", Text), ("b", Text)], Text, false, "a/b"),
    f("path", "name", &[("p", Text)], Text, false, "the last part of p, with its extension"),
    f("path", "stem", &[("p", Text)], Text, false, "the last part of p without its extension"),
    f("path", "extension", &[("p", Text)], Text, false, "the extension of p without the dot (empty if none)"),
    f("path", "parent", &[("p", Text)], Text, false, "p without its last part"),
    f("json", "parse", &[("t", Text)], Decoded, false, "the JSON in t read as the type after as, or an error"),
    f("json", "encode", &[("v", Any)], Text, true, "v written as compact JSON"),
    f("csv", "parse", &[("t", Text)], Decoded, false, "the CSV in t read as list of a record (header row = field names), or an error"),
    f("csv", "encode", &[("rows", AnyList)], Text, true, "a list of records written as CSV with a header row"),
];

/// The modules, in the order of SPEC 22.3.
pub const MODULES: &[&str] = &["text", "regex", "number", "money", "date", "path", "json", "csv"];

/// The verbs of the built-in `file` tool with what each takes (SPEC 22.3).
pub const FILE_VERBS: &[FileVerb] = &[
    FileVerb { verb: "read", shape: "file read <path> into <name>", extra: Extra::None, result: Some(Text), can_fail: true },
    FileVerb { verb: "write", shape: "file write <path>: <text>", extra: Extra::Content, result: None, can_fail: false },
    FileVerb { verb: "append", shape: "file append <path>: <line>", extra: Extra::Content, result: None, can_fail: false },
    FileVerb { verb: "list", shape: "file list <folder> into <names>", extra: Extra::None, result: Some(ListText), can_fail: false },
    FileVerb { verb: "rename", shape: "file rename <old> to <new>", extra: Extra::To, result: None, can_fail: false },
    FileVerb { verb: "exists", shape: "file exists <path> into <present>", extra: Extra::None, result: Some(Bool), can_fail: false },
    FileVerb { verb: "delete", shape: "file delete <path>", extra: Extra::None, result: None, can_fail: false },
];

/// What follows the path of a `file` statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extra {
    None,
    /// `: <text>`
    Content,
    /// `to <text>`
    To,
}

/// One verb of the `file` tool.
#[derive(Debug, Clone, Copy)]
pub struct FileVerb {
    pub verb: &'static str,
    pub shape: &'static str,
    pub extra: Extra,
    /// What `-> name` holds; `None`: the verb gives nothing back.
    pub result: Option<Ty>,
    /// The call is a `result` (use `try`).
    pub can_fail: bool,
}

/// Is `name` one of the library's modules?
pub fn is_module(name: &str) -> bool {
    MODULES.contains(&name)
}

/// A function by module and name.
pub fn lookup(module: &str, name: &str) -> Option<&'static Func> {
    FUNCS.iter().find(|f| f.module == module && f.name == name)
}

/// The names of a module's functions, in table order.
pub fn functions_of(module: &str) -> impl Iterator<Item = &'static Func> + '_ {
    FUNCS.iter().filter(move |f| f.module == module)
}

/// A verb of the `file` tool.
pub fn file_verb(verb: &str) -> Option<&'static FileVerb> {
    FILE_VERBS.iter().find(|v| v.verb == verb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_belongs_to_a_module_and_is_unique() {
        for f in FUNCS {
            assert!(is_module(f.module), "{}", f.signature());
        }
        for (i, a) in FUNCS.iter().enumerate() {
            assert!(FUNCS[i + 1..].iter().all(|b| (a.module, a.name) != (b.module, b.name)), "{}", a.signature());
        }
        for m in MODULES {
            assert!(functions_of(m).next().is_some(), "{m}");
        }
    }
}
