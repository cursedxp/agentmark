use crate::error::RtError;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Tool backend used by generated code (mock or real). Tracing is done by `Ctx`, not here.
#[async_trait]
pub trait Tools: Send + Sync {
    /// Run `tool.verb` with JSON args (e.g. `{"source":"invoices","id":"INV-1"}`); returns JSON.
    async fn call(&self, tool: &str, verb: &str, args: &Value) -> Result<Value, RtError>;
}

/// Fixture-backed tools: loads `<tool>.<source>.json`; `list`/`get` read it, other verbs return null.
#[derive(Debug, Clone, Default)]
pub struct MockTools {
    sources: HashMap<(String, String), Value>,
}

impl MockTools {
    /// Tools with no fixtures loaded.
    pub fn empty() -> MockTools {
        MockTools::default()
    }

    /// Load every `<tool>.<source>.json` file in a fixture folder.
    pub fn load(dir: impl AsRef<Path>) -> Result<MockTools, RtError> {
        let mut sources = HashMap::new();
        for entry in std::fs::read_dir(dir.as_ref())? {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            let parts: Vec<&str> = name.split('.').collect();
            if parts.len() != 3 || parts[2] != "json" {
                continue;
            }
            let err = |m: String| RtError::Fixture { path: path.display().to_string(), message: m };
            let text = std::fs::read_to_string(&path).map_err(|e| err(e.to_string()))?;
            let v: Value = serde_json::from_str(&text).map_err(|e| err(e.to_string()))?;
            sources.insert((parts[0].to_string(), parts[1].to_string()), v);
        }
        Ok(MockTools { sources })
    }

    fn source(&self, tool: &str, args: &Value) -> Result<&Value, RtError> {
        let src = args
            .get("source")
            .and_then(Value::as_str)
            .ok_or_else(|| RtError::Tool(format!("{tool}: missing \"source\" argument")))?;
        self.sources
            .get(&(tool.to_string(), src.to_string()))
            .ok_or_else(|| RtError::MissingFixture { tool: tool.into(), source_name: src.into() })
    }
}

#[async_trait]
impl Tools for MockTools {
    async fn call(&self, tool: &str, verb: &str, args: &Value) -> Result<Value, RtError> {
        match verb {
            "list" => Ok(self.source(tool, args)?.clone()),
            "get" => {
                let data = self.source(tool, args)?;
                let id = args.get("id");
                Ok(data
                    .as_array()
                    .and_then(|a| a.iter().find(|r| r.get("id") == id))
                    .cloned()
                    .unwrap_or(Value::Null))
            }
            _ => Ok(Value::Null),
        }
    }
}
