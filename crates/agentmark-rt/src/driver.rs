//! Driver strings (`- model: openai:gpt-4o`) and the router that turns them into [`ModelClient`]s.
//!
//! One parser serves the runtime, the generated programs, `agentmark check` and the binder, so
//! every tool means the same thing by `claude`, `openai:<m>`, `local:<m>` and `mock` (SPEC 19).

use crate::error::RtError;
use crate::model::{
    AnthropicModel, LocalModel, MockModel, ModelClient, ModelRequest, ModelResponse, OpenAiModel, LOCAL_BASE_URL,
    OPENAI_BASE_URL,
};
use async_trait::async_trait;
use chrono::NaiveDate;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// The Claude model used when a driver names none.
pub const DEFAULT_CLAUDE: &str = "claude-sonnet-4-5";

/// A parsed `- model:` driver string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Driver {
    /// `claude` / `claude:<model>`: the Anthropic Messages API (`None` = default model).
    Claude(Option<String>),
    /// `openai:<model>`: OpenAI Chat Completions.
    OpenAi(String),
    /// `local:<model>`: an OpenAI-compatible local server.
    Local(String),
    /// `mock` (answers from `model.json`) or `mock:<path to model.json>`.
    Mock(Option<String>),
}

/// Why a driver string was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DriverError {
    /// An enterprise route that this version does not have.
    #[error("The model provider \"{0}\" is not supported in this version.")]
    NotSupported(String),
    /// A provider name nobody knows.
    #[error("Unknown model provider \"{0}\".")]
    Unknown(String),
    /// `openai:` or `local:` without a model name.
    #[error("\"{0}:\" needs a model name, like \"{0}:{1}\".")]
    MissingModel(String, &'static str),
    /// An empty string.
    #[error("The model is empty.")]
    Empty,
}

/// The providers, for "did you mean" suggestions.
pub const PROVIDERS: [&str; 4] = ["claude", "openai", "local", "mock"];

/// What a `- model:` may say, for help lines under a rejected driver.
pub const DRIVER_HELP: &str = "A model is claude, claude:<model>, openai:<model>, local:<model> or mock.";

impl Driver {
    /// Parse `claude`, `claude:<m>`, `openai:<m>`, `local:<m>`, `mock`, `mock:<path>`.
    pub fn parse(s: &str) -> Result<Driver, DriverError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(DriverError::Empty);
        }
        let (provider, rest) = match s.split_once(':') {
            Some((p, r)) => (p, Some(r)),
            None => (s, None),
        };
        let named = |rest: Option<&str>| rest.filter(|r| !r.is_empty()).map(str::to_string);
        match provider {
            "claude" => Ok(Driver::Claude(named(rest))),
            "mock" => Ok(Driver::Mock(named(rest))),
            "openai" => named(rest).map(Driver::OpenAi).ok_or(DriverError::MissingModel("openai".into(), "gpt-4o")),
            "local" => named(rest).map(Driver::Local).ok_or(DriverError::MissingModel("local".into(), "llama3")),
            "azure" | "bedrock" => Err(DriverError::NotSupported(provider.to_string())),
            other => Err(DriverError::Unknown(other.to_string())),
        }
    }

    /// The id a request carries for this driver (`ModelRequest.model`): a Claude id, or the
    /// driver string itself for the other providers.
    pub fn request_id(&self) -> String {
        match self {
            Driver::Claude(m) => claude_id(m.as_deref()),
            Driver::OpenAi(m) => format!("openai:{m}"),
            Driver::Local(m) => format!("local:{m}"),
            Driver::Mock(None) => "mock".into(),
            Driver::Mock(Some(p)) => format!("mock:{p}"),
        }
    }
}

/// The Claude model id for `haiku`/`sonnet`/`opus`/`<id>`/none.
pub fn claude_id(m: Option<&str>) -> String {
    match m {
        None | Some("sonnet") => DEFAULT_CLAUDE.into(),
        Some("haiku") => "claude-haiku-4-5".into(),
        Some("opus") => "claude-opus-4-1".into(),
        Some(other) => other.into(),
    }
}

/// The request id for a driver string that already passed [`Driver::parse`] (unparsable input is
/// returned unchanged).
pub fn request_model_id(driver: &str) -> String {
    Driver::parse(driver).map(|d| d.request_id()).unwrap_or_else(|_| driver.to_string())
}

/// What the router needs from `agentmark.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderConfig {
    /// `[models]`: environment name -> driver string.
    pub environments: BTreeMap<String, String>,
    /// `[models.local] base_url`.
    pub local_base_url: Option<String>,
}

impl ProviderConfig {
    /// Build from literal tables (generated programs bake their `agentmark.toml` in).
    pub fn new(environments: &[(&str, &str)], local_base_url: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            environments: environments.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            local_base_url: local_base_url.map(String::from),
        }
    }
}

type Built = Arc<dyn ModelClient>;

/// Chooses a provider per request and builds each client once.
///
/// Precedence of the model a program uses: `AGENTMARK_MODEL` (a driver string), then the
/// `[models]` entry named by `AGENTMARK_ENV`, then each agent's own `- model:` (Q240).
pub struct Router {
    config: ProviderConfig,
    getenv: Arc<dyn Fn(&str) -> Option<String> + Send + Sync>,
    forced: Option<Driver>,
    forced_client: Option<Built>,
    mock_today: Option<NaiveDate>,
    built: Mutex<BTreeMap<String, Built>>,
}

impl std::fmt::Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Router").field("forced", &self.forced).finish()
    }
}

fn nonempty(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.is_empty())
}

impl Router {
    /// A router reading the process environment.
    pub fn from_env(config: ProviderConfig) -> Result<Router, RtError> {
        Router::new(config, |k| std::env::var(k).ok())
    }

    /// A router with its own environment lookup (tests).
    pub fn new(config: ProviderConfig, getenv: impl Fn(&str) -> Option<String> + Send + Sync + 'static) -> Result<Router, RtError> {
        let getenv: Arc<dyn Fn(&str) -> Option<String> + Send + Sync> = Arc::new(getenv);
        let spec = match nonempty(getenv("AGENTMARK_MODEL")) {
            Some(s) => Some(s),
            None => match nonempty(getenv("AGENTMARK_ENV")) {
                None => None,
                Some(env) => match config.environments.get(&env) {
                    Some(d) => Some(d.clone()),
                    // production is the default environment: unmapped means "agents decide"
                    None if env == "production" || config.environments.is_empty() => None,
                    None => {
                        let known: Vec<&str> = config.environments.keys().map(String::as_str).collect();
                        return Err(RtError::Model(format!(
                            "AGENTMARK_ENV=\"{env}\" is not in [models] of agentmark.toml (it has: {})",
                            known.join(", ")
                        )));
                    }
                },
            },
        };
        let forced = match spec {
            Some(s) => Some(Driver::parse(&s).map_err(|e| RtError::Model(format!("model \"{s}\": {e}")))?),
            None => None,
        };
        let mut r = Router { config, getenv, forced, forced_client: None, mock_today: None, built: Mutex::new(BTreeMap::new()) };
        if let Some(d) = r.forced.clone() {
            // fail at start-up, not at the first question, for a mock whose file cannot be read
            let c = r.build(&d)?;
            r.forced_client = Some(c);
            if let Driver::Mock(_) = d {
                r.mock_today = r.mock(&d)?.today();
            }
        }
        Ok(r)
    }

    /// Build the client of a driver now, so a missing key or a bad file is reported before the first question.
    pub fn prepare(&self, driver: &Driver) -> Result<(), RtError> {
        self.build(driver).map(|_| ())
    }

    /// The driver forced by `AGENTMARK_MODEL` / `AGENTMARK_ENV`, if any.
    pub fn forced(&self) -> Option<&Driver> {
        self.forced.as_ref()
    }

    /// The `today` of a forced mock model (so the binder can use its fixed date).
    pub fn mock_today(&self) -> Option<NaiveDate> {
        self.mock_today
    }

    fn mock(&self, d: &Driver) -> Result<MockModel, RtError> {
        let Driver::Mock(path) = d else { return Err(RtError::Model("not a mock driver".into())) };
        let file = match path {
            Some(p) => p.clone(),
            None => {
                let dir = nonempty((self.getenv)("AGENTMARK_FIXTURES")).unwrap_or_else(|| ".".into());
                format!("{}/model.json", dir.trim_end_matches('/'))
            }
        };
        MockModel::from_file(&file)
    }

    /// The base URL of the local server: `AGENTMARK_LOCAL_BASE_URL`, `[models.local]`, Ollama's.
    pub fn local_base_url(&self) -> String {
        nonempty((self.getenv)("AGENTMARK_LOCAL_BASE_URL"))
            .or_else(|| self.config.local_base_url.clone())
            .unwrap_or_else(|| LOCAL_BASE_URL.into())
    }

    fn build(&self, d: &Driver) -> Result<Built, RtError> {
        let key = d.request_id();
        let cached = self.built.lock().map_err(|_| RtError::Model("model cache poisoned".into()))?.get(&key).cloned();
        if let Some(c) = cached {
            return Ok(c);
        }
        let client: Built = match d {
            Driver::Claude(m) => {
                let id = claude_id(m.as_deref());
                let key = (self.getenv)("ANTHROPIC_API_KEY").filter(|k| !k.is_empty()).ok_or(RtError::MissingApiKey)?;
                let mut c = AnthropicModel::new(key, id);
                if let Some(u) = nonempty((self.getenv)("ANTHROPIC_BASE_URL")) {
                    c = c.with_base_url(u);
                }
                Arc::new(c)
            }
            Driver::OpenAi(m) => {
                let key = nonempty((self.getenv)("OPENAI_API_KEY")).ok_or(RtError::MissingKey("OPENAI_API_KEY"))?;
                let base = nonempty((self.getenv)("OPENAI_BASE_URL")).unwrap_or_else(|| OPENAI_BASE_URL.into());
                Arc::new(OpenAiModel::new(key, m.clone()).with_base_url(base))
            }
            Driver::Local(m) => Arc::new(LocalModel::new(self.local_base_url(), m.clone(), (self.getenv)("LOCAL_API_KEY"))),
            Driver::Mock(_) => Arc::new(self.mock(d)?),
        };
        self.built.lock().map_err(|_| RtError::Model("model cache poisoned".into()))?.insert(key, client.clone());
        Ok(client)
    }

    /// The driver of a request that no override replaces.
    fn driver_of(req: &ModelRequest) -> Result<Driver, RtError> {
        match req.model.as_deref() {
            None => Ok(Driver::Claude(None)),
            Some(m) if ["openai:", "local:", "mock"].iter().any(|p| m.starts_with(p)) => {
                Driver::parse(m).map_err(|e| RtError::Model(format!("model \"{m}\": {e}")))
            }
            Some(id) => Ok(Driver::Claude(Some(id.to_string()))),
        }
    }
}

#[async_trait]
impl ModelClient for Router {
    async fn complete(&self, req: &ModelRequest) -> Result<ModelResponse, RtError> {
        match (&self.forced, &self.forced_client) {
            (Some(d), Some(c)) => match d {
                // a mock keeps the agent's model id, so fixtures can tell the agents apart
                Driver::Mock(_) => c.complete(req).await,
                // a real provider answers for every agent with the forced model
                _ => {
                    let mut forced = req.clone();
                    forced.model = Some(match (d, Router::driver_of(req)?) {
                        (Driver::OpenAi(m) | Driver::Local(m), _) => m.clone(),
                        // plain `claude` leaves an agent's own Claude model (haiku, opus) alone
                        (Driver::Claude(None), Driver::Claude(Some(own))) => own,
                        (Driver::Claude(m), _) => claude_id(m.as_deref()),
                        (Driver::Mock(_), _) => unreachable!(),
                    });
                    c.complete(&forced).await
                }
            },
            _ => {
                let d = Router::driver_of(req)?;
                let c = self.build(&d)?;
                let mut routed = req.clone();
                routed.model = match &d {
                    Driver::OpenAi(m) | Driver::Local(m) => Some(m.clone()),
                    Driver::Claude(m) => Some(claude_id(m.as_deref())),
                    Driver::Mock(_) => req.model.clone(),
                };
                c.complete(&routed).await
            }
        }
    }
}
