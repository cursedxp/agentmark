use crate::money::MoneyError;

/// Errors raised by the runtime.
#[derive(Debug, thiserror::Error)]
pub enum RtError {
    /// No `model.json` response has a prefix of the given prompt.
    #[error("no mock model response matches prompt starting {prompt:?}")]
    NoMockResponse {
        /// The first characters of the unmatched prompt.
        prompt: String,
    },
    /// A fixture file `<tool>.<source>.json` was not found.
    #[error("missing fixture for {tool}.{source_name}")]
    MissingFixture {
        /// Tool name.
        tool: String,
        /// Source name.
        source_name: String,
    },
    /// A fixture or config file could not be read or parsed.
    #[error("fixture error in {path}: {message}")]
    Fixture {
        /// File path.
        path: String,
        /// What went wrong.
        message: String,
    },
    /// A model provider call failed.
    #[error("model error: {0}")]
    Model(String),
    /// The `ANTHROPIC_API_KEY` environment variable is missing.
    #[error("environment variable ANTHROPIC_API_KEY is not set")]
    MissingApiKey,
    /// The environment variable holding a provider's API key is missing.
    #[error("environment variable {0} is not set")]
    MissingKey(&'static str),
    /// A tool call was malformed.
    #[error("tool error: {0}")]
    Tool(String),
    /// The trace could not be written (I/O failure or poisoned lock).
    #[error("trace write failed: {0}")]
    Trace(String),
    /// A schedule was built with invalid values.
    #[error("invalid schedule: {0}")]
    Schedule(String),
    /// Money arithmetic or parsing failed.
    #[error(transparent)]
    Money(#[from] MoneyError),
    /// An I/O error (trace file etc.).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
