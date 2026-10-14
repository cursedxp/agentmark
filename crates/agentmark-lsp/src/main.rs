//! `agentmark-lsp`: speaks the Language Server Protocol over stdin/stdout.

fn main() -> std::process::ExitCode {
    let (conn, io) = lsp_server::Connection::stdio();
    let result = agentmark_lsp::serve_from_env(conn);
    if let Err(e) = io.join() {
        eprintln!("agentmark-lsp: {e}");
    }
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("agentmark-lsp: {e}");
            std::process::ExitCode::from(1)
        }
    }
}
