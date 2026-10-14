//! The `agentmark` command line (SPEC 9, 18). Argument parsing lives here; each command is a
//! function in a submodule that prints its report and returns the process exit code.
//!
//! Output conventions: diagnostics go to stderr in the SPEC 10 format, reports to stdout. Exit
//! code 0 = fine, 1 = the project has problems, 2 = the command itself could not run (bad
//! arguments, unreadable path, no model).

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod bind;
mod check;
mod docgen;
mod files;
mod fmtlint;

/// AgentMark: write agents, services and commands in Markdown.
#[derive(Parser, Debug)]
#[command(name = "agentmark", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Check a file or project: errors, effect summary, inferred types, what to approve.
    Check {
        /// A `.agent.md` file or a project folder.
        path: String,
        /// Print one JSON document instead of text (schema: docs/check-json.md).
        #[arg(long)]
        json: bool,
        /// Do not fail because of libraries nobody approved yet (development only).
        #[arg(long)]
        allow_unverified: bool,
    },
    /// Rewrite files into the one canonical style.
    Fmt {
        /// A `.agent.md` file or a folder.
        path: String,
        /// Change nothing; list the files that are not canonical and exit 1 if there are any.
        #[arg(long)]
        check: bool,
    },
    /// Run the style rules; rules listed under `[lint] deny` in agentmark.toml are errors.
    Lint {
        /// A `.agent.md` file or a project folder.
        path: String,
    },
    /// Print the documentation page of a project, or the cheat sheet of a bound library.
    Doc {
        /// A project folder or file; or a library name (needs `bindings/<name>.agent.md`).
        /// Default: the current folder.
        target: Option<String>,
        /// Project folder to look for `bindings/` when `target` is a library name.
        #[arg(long, default_value = ".")]
        project: String,
    },
    /// Create or refresh the binding of a library (`bindings/<crate>.agent.md`).
    Bind {
        /// The library (crate) name.
        #[arg(required_unless_present = "all")]
        name: Option<String>,
        /// Regenerate even when the binding is up to date (overwrites an approved one).
        #[arg(long)]
        force: bool,
        /// Refresh every `bindings/*.agent.md` of the project.
        #[arg(long)]
        all: bool,
        /// Project folder whose `bindings/` is used.
        #[arg(long, default_value = ".")]
        project: String,
    },
}

/// Parse `args` (the first is the program name) and run the command.
pub fn run<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => {
            let _ = e.print();
            return ExitCode::from(if e.use_stderr() { 2 } else { 0 });
        }
    };
    let code = match cli.command {
        Command::Check { path, json, allow_unverified } => check::run(&path, json, allow_unverified),
        Command::Fmt { path, check } => fmtlint::fmt(&path, check),
        Command::Lint { path } => fmtlint::lint(&path),
        Command::Doc { target, project } => docgen::run(target.as_deref(), &project),
        Command::Bind { name, force, all, project } => bind::run(name.as_deref(), force, all, &project),
    };
    ExitCode::from(code)
}

/// Print an error of the command itself (not of the user's program) and give exit code 2.
pub(crate) fn fail(message: impl std::fmt::Display) -> u8 {
    eprintln!("agentmark: {message}");
    2
}

/// Does `path` exist on disk?
pub(crate) fn exists(path: &str) -> bool {
    Path::new(path).exists()
}
