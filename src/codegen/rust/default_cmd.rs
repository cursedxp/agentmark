//! The default command (SPEC 22.6, OPEN_QUESTIONS Q388): a program whose `# Run` lists exactly one
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

/// `struct CmdArgs`: the command's arguments, flattened into the program's command line.
pub fn args_struct(c: &Ctxt, cmd: &Command) -> String {
    let mut s = String::from("/// The arguments of the program's one command (`-h` lists them): positional, or `--name value`.\n#[derive(clap::Args)]\nstruct CmdArgs {\n");
    let mut index = 0;
    for a in &cmd.args {
        let id = c.n.local(&a.name);
        let flag = a.name.replace('_', "-");
        let alias = if flag == a.name { String::new() } else { format!(", alias = {}", lit(&a.name)) };
        let ty = rust_type(&a.ty, c.n);
        if is_flag(a) {
            s.push_str(&format!("    #[arg(long = {}{alias})]\n    {id}: bool,\n", lit(&flag)));
        } else if a.ty == Type::Bool {
            s.push_str(&format!("    #[arg(long = {}{alias}, action = clap::ArgAction::Set, default_value_t = true)]\n    {id}: bool,\n", lit(&flag)));
        } else {
            index += 1;
            let hyphen = if a.ty == Type::Int { ", allow_negative_numbers = true" } else { "" };
            s.push_str(&format!(
                "    #[arg(index = {index}, value_name = {}{hyphen})]\n    {id}_pos: Option<{ty}>,\n    #[arg(long = {}{alias}, value_name = {}{hyphen})]\n    {id}_opt: Option<{ty}>,\n",
                lit(&a.name.to_uppercase()),
                lit(&flag),
                lit(&a.name.to_uppercase()),
            ));
        }
    }
    s.push_str("}\n\n");
    s
}

/// `run_default`: pick each argument (named, else positional, else its default) and run the command.
pub fn run_default(c: &Ctxt, cmd: &Command, state_init: &str) -> String {
    let p = c.p;
    let mut s = String::from("/// Run the program's one command from the command line.\n#[allow(unused_variables)]\nasync fn run_default(ctx: &Ctx, a: CmdArgs) -> Result<usize, AgentError> {\n");
    let mut names: Vec<String> = Vec::new();
    for arg in &cmd.args {
        let id = c.n.local(&arg.name);
        let ty = rust_type(&arg.ty, c.n);
        if arg.ty == Type::Bool {
            s.push_str(&format!("    let {id}: bool = a.{id};\n"));
        } else {
            let b = Body::new(p, c.n, &c.o.root, "main", Kind::Unit, None);
            let fallback = match &arg.default {
                Some(d) => format!(".unwrap_or_else(|| {})", b.val(d)),
                None => format!(".unwrap_or_else(|| missing_argument({}))", lit(&arg.name)),
            };
            s.push_str(&format!("    let {id}: {ty} = a.{id}_opt.or(a.{id}_pos){fallback};\n"));
        }
        names.push(id);
    }
    let args = if names.is_empty() { String::new() } else { format!(", {}", names.join(", ")) };
    s.push_str(&format!(
        "    {}::run(ctx{args}{state_init}).await.map_err(|e| command_failed(ctx, {}, e))?;\n    Ok(0)\n}}\n\n",
        c.n.commands[0],
        lit(&cmd.name)
    ));
    s.push_str(MISSING);
    s
}

const MISSING: &str = r#"/// A command argument that was not given: the usage message of the command line, then exit.
fn missing_argument(name: &str) -> ! {
    use clap::CommandFactory;
    Cli::command()
        .error(clap::error::ErrorKind::MissingRequiredArgument, format!("the argument <{}> is required (give it as a value or as --{})", name.to_uppercase(), name.replace('_', "-")))
        .exit()
}

"#;
