use crate::error::RtError;
use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::{json, Value};
use std::path::Path;

/// A request to a language model.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRequest {
    /// The full prompt text.
    pub prompt: String,
    /// Optional model override (provider-specific id).
    pub model: Option<String>,
    /// Optional output token limit.
    pub max_tokens: Option<u32>,
}

impl ModelRequest {
    /// A request with just a prompt.
    pub fn new(prompt: impl Into<String>) -> ModelRequest {
        ModelRequest { prompt: prompt.into(), model: None, max_tokens: None }
    }
}

/// A model reply plus, for mocks, the fixture prefix that matched.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelResponse {
    /// The generated text.
    pub text: String,
    /// The `prompt_prefix` fixture key that matched (`None` for real providers).
    pub matched_prefix: Option<String>,
}

/// One implementation per model provider; the language is unaware of which is in use.
#[async_trait]
pub trait ModelClient: Send + Sync {
    /// Run one completion.
    async fn complete(&self, req: &ModelRequest) -> Result<ModelResponse, RtError>;
}

/// Fixture-driven model: `model.json` responses keyed by prompt prefix, longest prefix wins.
#[derive(Debug, Clone, Default)]
pub struct MockModel {
    today: Option<NaiveDate>,
    responses: Vec<(String, String, Option<String>)>,
}

impl MockModel {
    /// Build from an optional fixed date and `(prompt_prefix, response)` pairs.
    pub fn new(today: Option<NaiveDate>, responses: Vec<(String, String)>) -> MockModel {
        MockModel { today, responses: responses.into_iter().map(|(p, r)| (p, r, None)).collect() }
    }

    /// Load `model.json` (`{"today": "...", "responses": [{"prompt_prefix", "response"}]}`).
    pub fn from_file(path: impl AsRef<Path>) -> Result<MockModel, RtError> {
        let path = path.as_ref();
        let err = |m: String| RtError::Fixture { path: path.display().to_string(), message: m };
        let text = std::fs::read_to_string(path).map_err(|e| err(e.to_string()))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| err(e.to_string()))?;
        let today = match v.get("today").and_then(Value::as_str) {
            Some(s) => Some(s.parse::<NaiveDate>().map_err(|e| err(format!("bad today: {e}")))?),
            None => None,
        };
        let mut responses = Vec::new();
        for r in v.get("responses").and_then(Value::as_array).into_iter().flatten() {
            let p = r.get("prompt_prefix").and_then(Value::as_str);
            let t = r.get("response").and_then(Value::as_str);
            match (p, t) {
                (Some(p), Some(t)) => responses.push((p.to_string(), t.to_string(), r.get("model").and_then(Value::as_str).map(String::from))),
                _ => return Err(err("each response needs prompt_prefix and response strings".into())),
            }
        }
        Ok(MockModel { today, responses })
    }

    /// The fixed "today" from `model.json`, if present.
    pub fn today(&self) -> Option<NaiveDate> {
        self.today
    }
}

#[async_trait]
impl ModelClient for MockModel {
    async fn complete(&self, req: &ModelRequest) -> Result<ModelResponse, RtError> {
        let mut best: Option<&(String, String, Option<String>)> = None;
        for r in &self.responses {
            // a response with a "model" only answers requests made for that model
            let model_ok = r.2.is_none() || r.2 == req.model;
            if model_ok && req.prompt.starts_with(&r.0) && best.is_none_or(|b| r.0.len() > b.0.len()) {
                best = Some(r);
            }
        }
        match best {
            Some((p, t, _)) => Ok(ModelResponse { text: t.clone(), matched_prefix: Some(p.clone()) }),
            None => Err(RtError::NoMockResponse { prompt: req.prompt.chars().take(60).collect() }),
        }
    }
}

/// Anthropic Messages API client.
#[derive(Debug, Clone)]
pub struct AnthropicModel {
    api_key: String,
    model: String,
    base_url: String,
    http: reqwest::Client,
}

impl AnthropicModel {
    /// Build with an explicit API key and default model id.
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> AnthropicModel {
        AnthropicModel {
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://api.anthropic.com".into(),
            http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).build().unwrap_or_default(),
        }
    }

    /// Build reading the key from `ANTHROPIC_API_KEY`.
    pub fn from_env(model: impl Into<String>) -> Result<AnthropicModel, RtError> {
        Self::from_key_result(std::env::var("ANTHROPIC_API_KEY"), model)
    }

    /// Build from the result of an env lookup (testable form of [`AnthropicModel::from_env`]).
    pub fn from_key_result(
        key: Result<String, std::env::VarError>,
        model: impl Into<String>,
    ) -> Result<AnthropicModel, RtError> {
        match key {
            Ok(k) if !k.is_empty() => Ok(AnthropicModel::new(k, model)),
            _ => Err(RtError::MissingApiKey),
        }
    }

    /// Override the API base URL (for proxies or tests).
    pub fn with_base_url(mut self, url: impl Into<String>) -> AnthropicModel {
        self.base_url = url.into();
        self
    }

    /// The JSON body sent to `/v1/messages` for a request.
    pub fn request_body(&self, req: &ModelRequest) -> Value {
        json!({
            "model": req.model.as_deref().unwrap_or(&self.model),
            "max_tokens": req.max_tokens.unwrap_or(1024),
            "messages": [{"role": "user", "content": req.prompt}],
        })
    }

    /// Concatenate the text blocks of a Messages API response.
    pub fn parse_response(v: &Value) -> Result<String, RtError> {
        let blocks = v
            .get("content")
            .and_then(Value::as_array)
            .ok_or_else(|| RtError::Model(format!("unexpected response: {v}")))?;
        Ok(blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect())
    }
}

#[async_trait]
impl ModelClient for AnthropicModel {
    async fn complete(&self, req: &ModelRequest) -> Result<ModelResponse, RtError> {
        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&self.request_body(req))
            .send()
            .await
            .map_err(|e| RtError::Model(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| RtError::Model(format!("HTTP {status}: reading body failed: {e}")))?;
        if !status.is_success() {
            return Err(RtError::Model(format!("HTTP {status}: {}", body.chars().take(500).collect::<String>())));
        }
        let v: Value = serde_json::from_str(&body).map_err(|e| RtError::Model(format!("HTTP {status}: invalid JSON: {e}")))?;
        Ok(ModelResponse { text: Self::parse_response(&v)?, matched_prefix: None })
    }
}

/// Request/response shape shared by OpenAI Chat Completions and compatible local servers.
#[derive(Debug, Clone)]
struct ChatCompletions {
    api_key: Option<String>,
    model: String,
    base_url: String,
    /// The output-limit field: OpenAI wants `max_completion_tokens`, local servers `max_tokens`.
    limit_field: &'static str,
    http: reqwest::Client,
}

impl ChatCompletions {
    fn new(api_key: Option<String>, model: String, base_url: String, limit_field: &'static str) -> ChatCompletions {
        ChatCompletions {
            api_key,
            model,
            base_url: base_url.trim_end_matches('/').to_string(),
            limit_field,
            http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).build().unwrap_or_default(),
        }
    }

    fn request_body(&self, req: &ModelRequest) -> Value {
        let mut body = json!({
            "model": req.model.as_deref().unwrap_or(&self.model),
            "messages": [{"role": "user", "content": req.prompt}],
        });
        if let (Some(n), Some(o)) = (req.max_tokens, body.as_object_mut()) {
            o.insert(self.limit_field.into(), json!(n));
        }
        body
    }

    /// The text of the first choice (a string, or a list of `{"type":"text","text":..}` parts).
    fn parse_response(v: &Value) -> Result<String, RtError> {
        let content = v
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .ok_or_else(|| RtError::Model(format!("unexpected response: {v}")))?;
        match content {
            Value::String(s) => Ok(s.clone()),
            Value::Array(parts) => Ok(parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect()),
            _ => Err(RtError::Model(format!("the reply has no text: {v}"))),
        }
    }

    async fn complete(&self, req: &ModelRequest) -> Result<ModelResponse, RtError> {
        let mut call = self.http.post(format!("{}/chat/completions", self.base_url)).json(&self.request_body(req));
        if let Some(k) = &self.api_key {
            call = call.bearer_auth(k);
        }
        let resp = call.send().await.map_err(|e| RtError::Model(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| RtError::Model(format!("HTTP {status}: reading body failed: {e}")))?;
        if !status.is_success() {
            return Err(RtError::Model(format!("HTTP {status}: {}", body.chars().take(500).collect::<String>())));
        }
        let v: Value = serde_json::from_str(&body).map_err(|e| RtError::Model(format!("HTTP {status}: invalid JSON: {e}")))?;
        Ok(ModelResponse { text: Self::parse_response(&v)?, matched_prefix: None })
    }
}

/// OpenAI Chat Completions client (`openai:<model>`; key from `OPENAI_API_KEY`).
#[derive(Debug, Clone)]
pub struct OpenAiModel(ChatCompletions);

/// The OpenAI API base URL.
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

impl OpenAiModel {
    /// Build with an explicit API key and default model id.
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> OpenAiModel {
        OpenAiModel(ChatCompletions::new(Some(api_key.into()), model.into(), OPENAI_BASE_URL.into(), "max_completion_tokens"))
    }

    /// Build reading the key from `OPENAI_API_KEY`.
    pub fn from_env(model: impl Into<String>) -> Result<OpenAiModel, RtError> {
        match std::env::var("OPENAI_API_KEY") {
            Ok(k) if !k.is_empty() => Ok(OpenAiModel::new(k, model)),
            _ => Err(RtError::MissingKey("OPENAI_API_KEY")),
        }
    }

    /// Override the API base URL (Azure-style proxies, tests); `/chat/completions` is appended.
    pub fn with_base_url(mut self, url: impl Into<String>) -> OpenAiModel {
        self.0.base_url = url.into().trim_end_matches('/').to_string();
        self
    }

    /// The JSON body sent to `/chat/completions` for a request.
    pub fn request_body(&self, req: &ModelRequest) -> Value {
        self.0.request_body(req)
    }

    /// The text of the first choice of a Chat Completions response.
    pub fn parse_response(v: &Value) -> Result<String, RtError> {
        ChatCompletions::parse_response(v)
    }
}

#[async_trait]
impl ModelClient for OpenAiModel {
    async fn complete(&self, req: &ModelRequest) -> Result<ModelResponse, RtError> {
        self.0.complete(req).await
    }
}

/// The base URL of a local OpenAI-compatible server when none is configured (Ollama's default).
pub const LOCAL_BASE_URL: &str = "http://localhost:11434/v1";

/// A local OpenAI-compatible server (Ollama, LM Studio, vLLM): `local:<model>`.
#[derive(Debug, Clone)]
pub struct LocalModel(ChatCompletions);

impl LocalModel {
    /// Build for a base URL (ending in `/v1`; `/chat/completions` is appended) and default model.
    /// An optional bearer key is sent when given; most local servers ignore it.
    pub fn new(base_url: impl Into<String>, model: impl Into<String>, api_key: Option<String>) -> LocalModel {
        LocalModel(ChatCompletions::new(api_key.filter(|k| !k.is_empty()), model.into(), base_url.into(), "max_tokens"))
    }

    /// The JSON body sent to `/chat/completions` for a request.
    pub fn request_body(&self, req: &ModelRequest) -> Value {
        self.0.request_body(req)
    }
}

#[async_trait]
impl ModelClient for LocalModel {
    async fn complete(&self, req: &ModelRequest) -> Result<ModelResponse, RtError> {
        self.0.complete(req).await
    }
}
