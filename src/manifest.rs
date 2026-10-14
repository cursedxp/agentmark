//! The tool manifest (`tools.toml`, SPEC 7): which tools exist, what verbs, targets, sources and
//! events each one has, and what effect class it carries.
//!
//! Mistakes in the manifest are reported like mistakes in any other file (file:line, caret,
//! plain language, did-you-mean). Lines are located by scanning the text, because the manifest is
//! tiny and the `toml` crate does not give per-key positions through `toml::Table`.

use std::collections::BTreeMap;

use crate::diag::{Diagnostic, Span};
use crate::syntax::ast::Effect;

const E_TOML: &str = "E100";
const E_UNKNOWN_KEY: &str = "E101";
const E_NO_EFFECT: &str = "E102";
const E_BAD_EFFECT: &str = "E103";
const E_BAD_LIST: &str = "E104";
const E_NOT_TABLE: &str = "E105";

/// Keys a tool table accepts.
pub const TOOL_KEYS: [&str; 5] = ["verbs", "targets", "sources", "events", "effect"];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ToolDef {
    pub name: String,
    pub verbs: Vec<String>,
    /// Words that may follow the verb (`email send customer`).
    pub targets: Vec<String>,
    /// Readable collections (`crm.invoices`). A target word may also be the singular of one.
    pub sources: Vec<String>,
    /// Inbound events for `## When <tool> receives <event>`.
    pub events: Vec<String>,
    /// `Local`, `Network` or `Model`.
    pub effect: Effect,
    /// Line of the `[tool]` heading in `tools.toml` (1-based).
    pub line: u32,
}

impl ToolDef {
    /// Every word that may stand right after the verb: the declared targets, each source, and the
    /// singular of each source (OPEN_QUESTIONS Q5).
    pub fn target_words(&self) -> Vec<String> {
        let mut v = self.targets.clone();
        for s in &self.sources {
            v.push(singular(s));
            v.push(s.clone());
        }
        v
    }
}

/// Singular of a plural source word (`invoices` -> `invoice`, `entries` -> `entry`).
pub fn singular(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies") {
        if !stem.is_empty() {
            return format!("{stem}y");
        }
    }
    if let Some(stem) = word.strip_suffix('s') {
        if !stem.is_empty() && !word.ends_with("ss") && !word.ends_with("us") {
            return stem.to_string();
        }
    }
    word.to_string()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Manifest {
    /// Path shown in diagnostics; empty when there is no `tools.toml`.
    pub path: String,
    /// Tools in alphabetical order.
    pub tools: BTreeMap<String, ToolDef>,
}

impl Manifest {
    pub fn empty() -> Self {
        Manifest::default()
    }

    pub fn tool(&self, name: &str) -> Option<&ToolDef> {
        self.tools.get(name)
    }

    pub fn tool_names(&self) -> impl Iterator<Item = &str> {
        self.tools.keys().map(String::as_str)
    }

    /// Parse and validate `text`. `path` is the name shown in diagnostics.
    pub fn parse(path: &str, text: &str) -> Result<Manifest, Vec<Diagnostic>> {
        let table: toml::Table = match text.parse() {
            Ok(t) => t,
            Err(e) => {
                let line = e
                    .span()
                    .map(|r| text[..r.start.min(text.len())].matches('\n').count() as u32 + 1)
                    .unwrap_or(1);
                let msg = e.message().to_string();
                return Err(vec![Diagnostic::error(
                    E_TOML,
                    path,
                    line_span(text, line),
                    format!("This is not valid TOML: {msg}."),
                )
                .with_help(
                    "Each tool is a table: a [name] line followed by key = value lines.",
                )]);
            }
        };
        let mut diags = Vec::new();
        let mut tools = BTreeMap::new();
        for (name, value) in &table {
            let head_line = find_section(text, name).unwrap_or(1);
            let Some(t) = value.as_table() else {
                diags.push(
                    Diagnostic::error(
                        E_NOT_TABLE,
                        path,
                        key_span(text, head_line, name),
                        format!("\"{name}\" must be a tool table, written as [{name}]."),
                    )
                    .with_help("Every top-level entry of tools.toml is one tool."),
                );
                continue;
            };
            if let Some(def) = parse_tool(path, text, name, t, head_line, &mut diags) {
                tools.insert(name.clone(), def);
            }
        }
        if diags.is_empty() {
            Ok(Manifest {
                path: path.to_string(),
                tools,
            })
        } else {
            diags.sort_by_key(|d| (d.span.line, d.span.col));
            Err(diags)
        }
    }
}

fn parse_tool(
    path: &str,
    text: &str,
    name: &str,
    t: &toml::Table,
    head_line: u32,
    diags: &mut Vec<Diagnostic>,
) -> Option<ToolDef> {
    let before = diags.len();
    let mut lists: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut effect = None;
    for (key, value) in t {
        let line = find_key(text, head_line, key).unwrap_or(head_line);
        if !TOOL_KEYS.contains(&key.as_str()) {
            diags.push(
                Diagnostic::error(
                    E_UNKNOWN_KEY,
                    path,
                    key_span(text, line, key),
                    format!("Unknown key \"{key}\" for tool \"{name}\"."),
                )
                .suggesting(key, TOOL_KEYS)
                .with_help(format!("A tool accepts: {}.", TOOL_KEYS.join(", "))),
            );
            continue;
        }
        if key == "effect" {
            let Some(s) = value.as_str() else {
                diags.push(bad_effect(path, text, line, key, &value.to_string()));
                continue;
            };
            effect = match s {
                "local" => Some(Effect::Local),
                "network" => Some(Effect::Network),
                "model" => Some(Effect::Model),
                other => {
                    diags.push(bad_effect(path, text, line, key, other));
                    None
                }
            };
        } else {
            let items = value.as_array().and_then(|a| {
                a.iter()
                    .map(|v| v.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()
            });
            match items {
                Some(v) => {
                    lists.insert(TOOL_KEYS.iter().find(|k| **k == key).copied().unwrap(), v);
                }
                None => diags.push(
                    Diagnostic::error(
                        E_BAD_LIST,
                        path,
                        key_span(text, line, key),
                        format!("\"{key}\" of tool \"{name}\" must be a list of words."),
                    )
                    .with_help(format!("Write it like: {key} = [\"one\", \"two\"]")),
                ),
            }
        }
    }
    if !t.contains_key("effect") {
        diags.push(
            Diagnostic::error(
                E_NO_EFFECT,
                path,
                key_span(text, head_line, name),
                format!("The tool \"{name}\" does not say what effect it has."),
            )
            .with_help("Add effect = \"local\" (touches this machine) or \"network\" (talks to the outside)."),
        );
    }
    if diags.len() > before {
        return None;
    }
    Some(ToolDef {
        name: name.to_string(),
        verbs: lists.remove("verbs").unwrap_or_default(),
        targets: lists.remove("targets").unwrap_or_default(),
        sources: lists.remove("sources").unwrap_or_default(),
        events: lists.remove("events").unwrap_or_default(),
        effect: effect?,
        line: head_line,
    })
}

fn bad_effect(path: &str, text: &str, line: u32, key: &str, got: &str) -> Diagnostic {
    let mut d = Diagnostic::error(
        E_BAD_EFFECT,
        path,
        value_span(text, line, key),
        format!("\"{got}\" is not an effect class for a tool."),
    )
    .suggesting(got, ["local", "network", "model"]);
    d.help = Some(
        "A tool's effect is \"local\", \"network\" or \"model\" (a tool is never pure).".into(),
    );
    d
}

fn nth_line(text: &str, line: u32) -> &str {
    text.lines()
        .nth(line.saturating_sub(1) as usize)
        .unwrap_or("")
}

fn line_span(text: &str, line: u32) -> Span {
    let l = nth_line(text, line);
    let indent = l.chars().take_while(|c| c.is_whitespace()).count();
    let len = l.trim().chars().count().max(1);
    Span::new(line, indent as u32 + 1, len as u32)
}

/// Line of `[name]` (or `["name"]`).
fn find_section(text: &str, name: &str) -> Option<u32> {
    text.lines().enumerate().find_map(|(i, l)| {
        let t = l.trim();
        let inner = t.strip_prefix('[')?.strip_suffix(']')?.trim();
        (inner == name || inner.trim_matches('"') == name).then_some(i as u32 + 1)
    })
}

/// Line of `key =` inside the section starting at `from`.
fn find_key(text: &str, from: u32, key: &str) -> Option<u32> {
    for (i, l) in text.lines().enumerate().skip(from as usize) {
        let t = l.trim();
        if t.starts_with('[') {
            return None;
        }
        if let Some(rest) = t.strip_prefix(key) {
            if rest.trim_start().starts_with('=') {
                return Some(i as u32 + 1);
            }
        }
    }
    None
}

/// Span of `word` on `line` (falls back to the whole line).
fn key_span(text: &str, line: u32, word: &str) -> Span {
    let l = nth_line(text, line);
    match l.find(word) {
        Some(byte) => Span::new(
            line,
            l[..byte].chars().count() as u32 + 1,
            word.chars().count() as u32,
        ),
        None => line_span(text, line),
    }
}

/// Span of the text after `=` on the line of `key`.
fn value_span(text: &str, line: u32, key: &str) -> Span {
    let l = nth_line(text, line);
    let Some(eq) = l.find('=') else {
        return key_span(text, line, key);
    };
    let after = &l[eq + 1..];
    let lead = after.chars().take_while(|c| c.is_whitespace()).count();
    let value = after.trim();
    let col = l[..eq].chars().count() + 1 + lead + 1;
    Span::new(line, col as u32, value.chars().count().max(1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[email]
verbs = ["send"]
targets = ["customer", "address"]
effect = "network"

[crm]
verbs = ["list", "get", "update"]
sources = ["invoices", "leads"]
events = ["lead"]
effect = "network"

[log]
verbs = []
effect = "local"
"#;

    fn render(text: &str) -> String {
        match Manifest::parse("tools.toml", text) {
            Ok(_) => panic!("expected errors"),
            Err(d) => crate::diag::render_all(&d, text),
        }
    }

    #[test]
    fn parses_the_example_manifest() {
        let m = Manifest::parse("tools.toml", GOOD).unwrap();
        assert_eq!(m.tool_names().collect::<Vec<_>>(), ["crm", "email", "log"]);
        let crm = m.tool("crm").unwrap();
        assert_eq!(crm.verbs, ["list", "get", "update"]);
        assert_eq!(crm.effect, Effect::Network);
        assert_eq!(m.tool("log").unwrap().effect, Effect::Local);
        assert!(crm.target_words().contains(&"invoice".to_string()));
    }

    #[test]
    fn singular_handles_plurals() {
        assert_eq!(singular("invoices"), "invoice");
        assert_eq!(singular("entries"), "entry");
        assert_eq!(singular("status"), "status");
        assert_eq!(singular("class"), "class");
    }

    #[test]
    fn invalid_toml_names_the_line() {
        let out = render("[email]\nverbs = [\"send\"\n");
        assert!(out.starts_with("tools.toml:"), "{out}");
        assert!(out.contains("This is not valid TOML"), "{out}");
    }

    #[test]
    fn unknown_key_suggests_the_right_one() {
        let out = render("[email]\nverb = [\"send\"]\neffect = \"network\"\n");
        assert_eq!(
            out,
            "tools.toml:2\n  verb = [\"send\"]\n  ^^^^\n  Unknown key \"verb\" for tool \"email\". Did you mean \"verbs\"?\n  A tool accepts: verbs, targets, sources, events, effect."
        );
    }

    #[test]
    fn missing_effect_is_an_error() {
        let out = render("[email]\nverbs = [\"send\"]\n");
        assert!(out.contains("does not say what effect it has"), "{out}");
        assert!(out.starts_with("tools.toml:1\n  [email]\n"), "{out}");
    }

    #[test]
    fn bad_effect_value_is_an_error_with_suggestion() {
        let out = render("[email]\nverbs = []\neffect = \"netwrk\"\n");
        assert!(
            out.contains("\"netwrk\" is not an effect class for a tool. Did you mean \"network\"?"),
            "{out}"
        );
        assert!(out.contains("tools.toml:3"), "{out}");
    }

    #[test]
    fn pure_is_not_a_tool_effect() {
        let out = render("[calc]\nverbs = []\neffect = \"pure\"\n");
        assert!(out.contains("never pure"), "{out}");
    }

    #[test]
    fn lists_must_be_lists_of_words() {
        let out = render("[email]\nverbs = \"send\"\neffect = \"network\"\n");
        assert!(
            out.contains("\"verbs\" of tool \"email\" must be a list of words."),
            "{out}"
        );
    }

    #[test]
    fn top_level_value_that_is_not_a_table_is_an_error() {
        let out = render("email = \"yes\"\n");
        assert!(out.contains("\"email\" must be a tool table"), "{out}");
    }
}
