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
    responses: Vec<(String, String)>,
}

impl MockModel {
    /// Build from an optional fixed date and `(prompt_prefix, response)` pairs.
    pub fn new(today: Option<NaiveDate>, responses: Vec<(String, String)>) -> MockModel {
        MockModel { today, responses }
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
                (Some(p), Some(t)) => responses.push((p.to_string(), t.to_string())),
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
        let mut best: Option<&(String, String)> = None;
        for r in &self.responses {
            if req.prompt.starts_with(&r.0) && best.is_none_or(|b| r.0.len() > b.0.len()) {
                best = Some(r);
            }
        }
        match best {
            Some((p, t)) => Ok(ModelResponse { text: t.clone(), matched_prefix: Some(p.clone()) }),
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
            http: reqwest::Client::new(),
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
        let v: Value = resp.json().await.map_err(|e| RtError::Model(e.to_string()))?;
        if !status.is_success() {
            return Err(RtError::Model(format!("HTTP {status}: {v}")));
        }
        Ok(ModelResponse { text: Self::parse_response(&v)?, matched_prefix: None })
    }
}
