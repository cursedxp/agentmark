//! Thin entry point: everything lives in [`agentmark::cli`].

fn main() -> std::process::ExitCode {
    agentmark::cli::run(std::env::args_os())
}
