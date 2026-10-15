//! The default command (SPEC 22.6, OPEN_QUESTIONS Q388): a program whose `- start:` lists exactly one
//! `# Command` (and nothing else) is started as `my-tool <args>`, without the command's name.
//! Each argument is positional (declared order) or `--name value`; a `bool` argument is the flag
//! `--name`. Dashes and underscores in the flag name are interchangeable.

use super::body::{lit, Body, Kind};
use super::files::Ctxt;
use super::ty::rust_type;
use crate::ir::*;

/// The command that is the program's default, if any.
pub fn default_command<'a>(c: &Ctxt<'a>) -> Option<(usize, &'a Command)> {
    let p = c.p;
    match (p.commands.as_slice(), p.run.as_slice()) {
        ([cmd], [only]) if &cmd.name == only && p.entries.is_empty() && p.services.is_empty() => Some((0, cmd)),
        _ => None,
    }
}

fn is_flag(a: &Param) -> bool {
    a.ty == Type::Bool && !matches!(a.default.as_ref().map(|d| &d.kind), Some(ExprKind::Bool(true)))
}

/// `struct CmdArgs`: the command's arguments, flattened into the program's command line. A value
/// argument is `--name value` or one of the trailing positional values, which are handed to the
/// arguments not given by name in declared order (`run_default` does that and checks them).
pub fn args_struct(c: &Ctxt, cmd: &Command) -> String {
    let mut s = String::from("/// The arguments of the program's one command (`-h` lists them): positional, or `--name value`.\n#[derive(clap::Args)]\nstruct CmdArgs {\n");
    for a in &cmd.args {
        let id = c.n.local(&a.name);
        let flag = a.name.replace('_', "-");
        let alias = if flag == a.name { String::new() } else { format!(", alias = {}", lit(&a.name)) };
        if is_flag(a) {
            s.push_str(&format!("    #[arg(long = {}{alias})]\n    {id}: bool,\n", lit(&flag)));
        } else if a.ty == Type::Bool {
            s.push_str(&format!("    #[arg(long = {}{alias}, action = clap::ArgAction::Set, default_value_t = true)]\n    {id}: bool,\n", lit(&flag)));
        } else {
            let hyphen = if a.ty == Type::Int { ", allow_negative_numbers = true" } else { "" };
            s.push_str(&format!("    #[arg(long = {}{alias}, value_name = {}{hyphen})]\n    {id}: Option<String>,\n", lit(&flag), lit(&a.name.to_uppercase())));
        }
    }
    let names: Vec<String> = cmd.args.iter().filter(|a| a.ty != Type::Bool).map(|a| a.name.to_uppercase()).collect();
    s.push_str(&format!(
        "    /// {}\n    #[arg(value_name = \"VALUE\", allow_negative_numbers = true)]\n    __rest: Vec<String>,\n}}\n\n",
        if names.is_empty() { "No positional values".to_string() } else { format!("Values in the order {}, for the ones not given by name", names.join(" ")) }
    ));
    s
}

/// `run_default`: pick each argument (named, else the next positional value, else its default) and run the command.
pub fn run_default(c: &Ctxt, cmd: &Command, state_init: &str) -> String {
    let p = c.p;
    let mut s = String::from("/// Run the program's one command from the command line.\n#[allow(unused_variables)]\nasync fn run_default(ctx: &Ctx, __args: CmdArgs) -> Result<usize, AgentError> {\n    let mut __rest = __args.__rest.into_iter();\n");
    let mut names: Vec<String> = Vec::new();
    for arg in &cmd.args {
        let id = c.n.local(&arg.name);
        let ty = rust_type(&arg.ty, c.n);
        if arg.ty == Type::Bool {
            s.push_str(&format!("    let {id}: bool = __args.{id};\n"));
        } else {
            let b = Body::new(p, c.n, &c.o.root, "main", Kind::Unit, None);
            let fallback = match &arg.default {
                Some(d) => b.val(d).to_string(),
                None => format!("usage_error(\"the argument <{}> is required (give it as a value or as --{})\")", arg.name.to_uppercase(), arg.name.replace('_', "-")),
            };
            // the text of the command line, read as the argument's type
            let conv = match &arg.ty {
                Type::Text => "Some(t.clone())",
                Type::Bytes => "Some(t.clone().into_bytes())",
                Type::Int | Type::Money | Type::Date | Type::DateTime => "t.parse().ok()",
                _ => "serde_json::from_str(&t).ok()",
            };
            s.push_str(&format!(
                "    let {id}: {ty} = match __args.{id}.or_else(|| __rest.next()) {{\n        Some(t) => ({conv}).unwrap_or_else(|| usage_error(&format!(\"{{t:?}} is not a valid value for <{}>\"))),\n        None => {fallback},\n    }};\n",
                arg.name.to_uppercase()
            ));
        }
        names.push(id);
    }
    s.push_str("    if let Some(extra) = __rest.next() {\n        usage_error(&format!(\"unexpected value {extra:?}: every argument is already given (an argument given by name does not take a positional value too)\"));\n    }\n");
    let args = if names.is_empty() { String::new() } else { format!(", {}", names.join(", ")) };
    s.push_str(&format!(
        "    {}::run(ctx{args}{state_init}).await.map_err(|e| command_failed(ctx, {}, e))?;\n    Ok(0)\n}}\n\n",
        c.n.commands[0],
        lit(&cmd.name)
    ));
    s.push_str(USAGE_ERROR);
    s
}

const USAGE_ERROR: &str = r#"/// A command line the command cannot use: the usage message of the command line, then exit.
fn usage_error(message: &str) -> ! {
    use clap::CommandFactory;
    Cli::command().error(clap::error::ErrorKind::InvalidValue, message).exit()
}

"#;
