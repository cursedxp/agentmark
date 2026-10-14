//! Code actions: the mechanical fixes of the lint rules, "Did you mean" replacements, and the
//! optional model-backed actions.
//!
//! Which lint rules have a mechanical fix (Q255):
//!
//! | rule | action |
//! |---|---|
//! | `magic-value` | extract a `# Constant` and use its name at every place the literal is written |
//! | `unnamed-guard` | insert a `> TODO: ...` comment line above the guard |
//! | `prompt-in-loop` | insert `- guard: x is not empty` (or `x matches Record`) after the `ask model` line |
//! | `missing-doc` | insert a `> TODO: describe ...` line above the heading |
//! | `long-file`, `duplicate-rule`, `vague-name` | none: splitting, merging and naming need a decision only the author can make |

use std::collections::HashMap;
use std::sync::Arc;

use agentmark::diag::Diagnostic;
use agentmark::syntax::parser::parse_file;
use agentmark_rt::{ModelClient, ModelRequest};
use lsp_types::{CodeAction, CodeActionKind, Command, Position, Range, TextEdit, Uri, WorkspaceEdit};
use serde_json::{json, Value};

use crate::diagnostics;
use crate::index::decl_name;
use crate::text::{end_position, line_text, position};
use crate::uri::uri_from_path;
use crate::workspace::Analysis;

/// The prompt of "explain this block" starts with this text (fixtures key on prompt prefixes).
pub const EXPLAIN_PROMPT: &str = "Explain this AgentMark block in plain language for an operations manager.";
/// The prompt of "suggest a guard" starts with this text.
pub const GUARD_PROMPT: &str = "Suggest one guard line for this AgentMark statement.";
pub const EXPLAIN_COMMAND: &str = "agentmark.explainBlock";
pub const KIND_EXPLAIN: &str = "refactor.agentmark.explain";
pub const KIND_GUARD: &str = "refactor.agentmark.guard";

fn edit(uri: &Uri, edits: Vec<TextEdit>) -> WorkspaceEdit {
    WorkspaceEdit { changes: Some(HashMap::from([(uri.clone(), edits)])), ..Default::default() }
}

fn intersects(a: Range, b: Range) -> bool {
    (a.start.line, a.start.character) <= (b.end.line, b.end.character)
        && (b.start.line, b.start.character) <= (a.end.line, a.end.character)
}

/// A whole line (without its line ending) as a range.
fn line_range(text: &str, line0: usize) -> Range {
    Range::new(position(text, line0, 0), position(text, line0, usize::MAX / 2))
}

fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// Insert a line of text before line `line0`.
fn insert_before(line0: usize, content: &str) -> TextEdit {
    TextEdit::new(Range::new(Position::new(line0 as u32, 0), Position::new(line0 as u32, 0)), format!("{content}\n"))
}

/// Insert a line of text after line `line0`.
fn insert_after(text: &str, line0: usize, content: &str) -> TextEdit {
    if line0 + 1 < text.split('\n').count() {
        insert_before(line0 + 1, content)
    } else {
        let end = end_position(text);
        TextEdit::new(Range::new(end, end), format!("\n{content}"))
    }
}

/// A fix: title, the plain edits, and (for fixes that leave a blank to fill in) the same edits with
/// a snippet placeholder, used when the client supports snippet text edits.
struct Fix {
    title: String,
    edits: Vec<TextEdit>,
    snippet: Option<Vec<TextEdit>>,
}

fn plain(title: String, edits: Vec<TextEdit>) -> Option<Fix> {
    Some(Fix { title, edits, snippet: None })
}

/// The mechanical fix of a lint finding. The facts come from `Diagnostic.data` (set by `lint`),
/// never from the wording of the message.
fn lint_fix(a: &Analysis, text: &str, d: &Diagnostic) -> Option<Fix> {
    let none = Value::Null;
    let data = d.data.as_ref().unwrap_or(&none);
    let line0 = d.span.line.saturating_sub(1) as usize;
    let line = line_text(text, line0);
    match d.code {
        "unnamed-guard" => {
            let ind = indent_of(line);
            Some(Fix {
                title: "Add a comment above this guard".to_string(),
                edits: vec![insert_before(line0, &format!("{ind}> TODO: say what this guard protects against."))],
                snippet: Some(vec![insert_before(line0, &format!("{ind}> ${{1:say what this guard protects against}}"))]),
            })
        }
        "missing-doc" => {
            let kind = data.get("kind")?.as_str()?;
            Some(Fix {
                title: format!("Add a description above this {kind}"),
                edits: vec![insert_before(line0, &format!("> TODO: describe what this {kind} does."))],
                snippet: Some(vec![insert_before(line0, &format!("> ${{1:describe what this {kind} does}}"))]),
            })
        }
        "prompt-in-loop" => {
            let var = data.get("var")?.as_str()?;
            let record = line.rfind('"').and_then(|i| {
                let rest = line[i + 1..].trim_start();
                let rest = rest.strip_prefix("as ")?;
                let w = rest.split(|c: char| c.is_whitespace() || c == '-').next()?;
                (!w.is_empty()).then(|| w.to_string())
            });
            let guard = match record {
                Some(r) => format!("{var} matches {r}"),
                None => format!("{var} is not empty"),
            };
            plain(format!("Add a guard on \"{var}\""), vec![insert_after(text, line0, &format!("{}- guard: {guard}", indent_of(line)))])
        }
        "magic-value" => magic_value_fix(a, text, d),
        _ => None,
    }
}

fn taken_names(a: &Analysis) -> Vec<String> {
    a.project.files.iter().flat_map(|f| f.ast.decls.iter()).filter_map(|d| decl_name(d).map(|(_, n, _)| n.name.clone())).collect()
}

/// Replace `lit` by `with` where it stands outside (`inside = false`) or inside (`inside = true`)
/// a string literal. Returns `None` when nothing changed.
fn replace_literal(line: &str, lit: &str, with: &str, inside: bool) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let lit_chars: Vec<char> = lit.chars().collect();
    let (mut out, mut i, mut in_str, mut changed) = (String::new(), 0, false, false);
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && in_str && i + 1 < chars.len() {
            out.push(c);
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
        }
        let word = |ch: char| ch.is_alphanumeric() || ch == '_' || ch == '.';
        let before_ok = inside || i == 0 || !word(chars[i - 1]);
        let after = i + lit_chars.len();
        let after_ok = inside || after >= chars.len() || !word(chars[after]);
        if in_str == inside && chars[i..].starts_with(&lit_chars) && before_ok && after_ok {
            out.push_str(with);
            i = after;
            changed = true;
            continue;
        }
        out.push(c);
        i += 1;
    }
    changed.then_some(out)
}

fn magic_value_fix(a: &Analysis, text: &str, d: &Diagnostic) -> Option<Fix> {
    let data = d.data.as_ref()?;
    let kind = data.get("kind")?.as_str()?;
    let key = data.get("key")?.as_str()?.to_string();
    let mut lines: Vec<usize> = data
        .get("lines")?
        .as_array()?
        .iter()
        .filter_map(|n| n.as_u64())
        .map(|n| n as usize - 1)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    let first = line_text(text, d.span.line as usize - 1);
    let literal: String = first.chars().skip(d.span.col as usize - 1).take(d.span.len as usize).collect();
    let (base, value, inside, with_name): (&str, String, bool, fn(&str) -> String) = match kind {
        "URL" => ("fixed_url", format!("\"{key}\""), true, |n| format!("{{{n}}}")),
        "duration" => ("fixed_duration", literal.clone(), false, |n| n.to_string()),
        _ => ("fixed_amount", literal.clone(), false, |n| n.to_string()),
    };
    let needle = if kind == "URL" { key.clone() } else { literal };
    let taken = taken_names(a);
    let mut name = base.to_string();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("{base}_{n}");
        n += 1;
    }
    let mut edits = Vec::new();
    for l in lines {
        let old = line_text(text, l);
        if let Some(new) = replace_literal(old, &needle, &with_name(&name), inside) {
            edits.push(TextEdit::new(line_range(text, l), new));
        }
    }
    if edits.is_empty() {
        return None;
    }
    let sep = if text.ends_with('\n') { "\n" } else { "\n\n" };
    let end = end_position(text);
    edits.push(TextEdit::new(Range::new(end, end), format!("{sep}# Constant: {name}\n- value: {value}\n")));
    plain(format!("Extract the {kind} into the constant \"{name}\""), edits)
}

/// Marks (in `CodeAction.data`, only until the server serializes it) an action whose edit has
/// snippet placeholders.
pub const SNIPPET_MARK: &str = "agentmarkSnippet";

/// The edit as `documentChanges`; [`mark_snippets`] tags its edits as snippets on the way out.
fn snippet_edit(uri: &Uri, edits: Vec<TextEdit>) -> WorkspaceEdit {
    use lsp_types::{DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier, TextDocumentEdit};
    WorkspaceEdit {
        document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
            text_document: OptionalVersionedTextDocumentIdentifier { uri: uri.clone(), version: None },
            edits: edits.into_iter().map(OneOf::Left).collect(),
        }])),
        ..Default::default()
    }
}

/// The snippet text-edit extension (`insertTextFormat: 2` on an edit, as rust-analyzer does it):
/// tag the edits of the actions marked with [`SNIPPET_MARK`] and drop the mark.
pub fn mark_snippets(action: &mut Value) {
    let marked = action.get("data").and_then(|d| d.get(SNIPPET_MARK)).is_some();
    if !marked {
        return;
    }
    action.as_object_mut().map(|o| o.remove("data"));
    if let Some(changes) = action.pointer_mut("/edit/documentChanges").and_then(Value::as_array_mut) {
        for c in changes {
            if let Some(edits) = c.get_mut("edits").and_then(Value::as_array_mut) {
                for e in edits {
                    e["insertTextFormat"] = json!(2);
                }
            }
        }
    }
}

/// Quick fixes for the findings that touch `range` of the document `path`.
pub fn quick_fixes(a: &Analysis, path: &str, text: &str, range: Range, snippets: bool) -> Vec<CodeAction> {
    let uri = uri_from_path(path);
    let mut out = Vec::new();
    for d in a.lint.iter().filter(|d| d.file == path) {
        let r = diagnostics::range(a, d);
        if !intersects(r, range) {
            continue;
        }
        if let Some(fix) = lint_fix(a, text, d) {
            let (edit, data) = match (&fix.snippet, snippets) {
                (Some(sn), true) => (snippet_edit(&uri, sn.clone()), Some(json!({ SNIPPET_MARK: true }))),
                _ => (edit(&uri, fix.edits), None),
            };
            out.push(CodeAction {
                title: fix.title,
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostics::to_lsp(a, d, "agentmark-lint")]),
                edit: Some(edit),
                data,
                ..Default::default()
            });
        }
    }
    for d in a.compiler.iter().filter(|d| d.file == path) {
        let (Some(c), r) = (&d.did_you_mean, diagnostics::range(a, d)) else { continue };
        if !intersects(r, range) {
            continue;
        }
        out.push(CodeAction {
            title: format!("Change to \"{c}\""),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostics::to_lsp(a, d, "agentmark")]),
            edit: Some(edit(&uri, vec![TextEdit::new(r, c.clone())])),
            is_preferred: Some(true),
            ..Default::default()
        });
    }
    out
}

// ---------------------------------------------------------------------------------- model actions

/// The model, and a runtime to wait for it on.
pub struct ModelRunner {
    client: Arc<dyn ModelClient>,
    rt: tokio::runtime::Runtime,
}

impl ModelRunner {
    pub fn new(client: Arc<dyn ModelClient>) -> std::io::Result<ModelRunner> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        Ok(ModelRunner { client, rt })
    }

    pub fn ask(&self, prompt: String) -> Result<String, String> {
        self.rt
            .block_on(self.client.complete(&ModelRequest::new(prompt)))
            .map(|r| r.text)
            .map_err(|e| e.to_string())
    }
}

/// The lines of the block around `line0`: from its heading to the line before the next heading.
pub fn block_lines(text: &str, line0: usize) -> Option<(usize, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let start = (0..=line0.min(lines.len().saturating_sub(1))).rev().find(|&l| lines[l].starts_with('#'))?;
    let end = (start + 1..lines.len()).find(|&l| lines[l].starts_with('#')).unwrap_or(lines.len());
    Some((start, end))
}

/// The model-backed actions offered at `line0`. Only called when `[lsp] model_actions` is on.
pub fn model_actions(uri: &Uri, text: &str, line0: usize) -> Vec<CodeAction> {
    let mut out = Vec::new();
    if block_lines(text, line0).is_some() {
        out.push(CodeAction {
            title: "Explain this block (uses the model)".to_string(),
            kind: Some(CodeActionKind::new(KIND_EXPLAIN)),
            command: Some(Command {
                title: "Explain this block".to_string(),
                command: EXPLAIN_COMMAND.to_string(),
                arguments: Some(vec![json!(uri.as_str()), json!(line0)]),
            }),
            ..Default::default()
        });
    }
    if line_text(text, line0).trim_start().trim_start_matches("- ").starts_with("ask model") {
        out.push(CodeAction {
            title: "Suggest a guard for this answer (uses the model)".to_string(),
            kind: Some(CodeActionKind::new(KIND_GUARD)),
            data: Some(json!({ "action": "suggestGuard", "uri": uri.as_str(), "line": line0 })),
            ..Default::default()
        });
    }
    out
}

/// Ask the model to explain the block around `line0`; `effects` is the checker's summary of it.
pub fn explain(runner: &ModelRunner, text: &str, line0: usize, effects: Option<&str>) -> Result<String, String> {
    let (s, e) = block_lines(text, line0).ok_or("There is no block at this position.")?;
    let block = text.split('\n').skip(s).take(e - s).collect::<Vec<_>>().join("\n");
    let eff = effects.map(|e| format!("\nThe compiler says it can touch: {e}\n")).unwrap_or_default();
    runner.ask(format!("{EXPLAIN_PROMPT}{eff}\n```\n{}\n```", block.trim_end()))
}

/// Ask the model for a guard on the `ask model` line `line0`; returns the edit that inserts it.
/// The answer is untrusted: it must parse in place or nothing is offered.
pub fn suggest_guard(runner: &ModelRunner, path: &str, text: &str, line0: usize) -> Result<Vec<TextEdit>, String> {
    let line = line_text(text, line0);
    if !line.trim_start().trim_start_matches("- ").starts_with("ask model") {
        return Err("This is not an \"ask model\" line.".into());
    }
    let (s, e) = block_lines(text, line0).unwrap_or((line0, line0 + 1));
    let block = text.split('\n').skip(s).take(e - s).collect::<Vec<_>>().join("\n");
    let answer = runner.ask(format!("{GUARD_PROMPT} Answer with the guard line only.\n\nThe statement:\n{line}\n\nIts block:\n```\n{}\n```", block.trim_end()))?;
    let guard = clean_guard(&answer).ok_or("The model did not answer with a guard.")?;
    let edit = insert_after(text, line0, &format!("{}- {guard}", indent_of(line)));
    let mut applied = apply(text, std::slice::from_ref(&edit));
    if !applied.ends_with('\n') && text.ends_with('\n') {
        applied.push('\n');
    }
    if parse_file(path, &applied).is_err() {
        return Err(format!("The model's guard \"{guard}\" does not parse, so it was not inserted."));
    }
    Ok(vec![edit])
}

fn clean_guard(answer: &str) -> Option<String> {
    let l = answer
        .lines()
        .map(|l| l.trim().trim_matches('`').trim())
        .find(|l| !l.is_empty() && !l.starts_with("```"))?;
    let l = l.strip_prefix("- ").unwrap_or(l).trim();
    let l = if l.starts_with("guard:") { l.to_string() } else { format!("guard: {l}") };
    (l.len() > "guard:".len() + 1).then_some(l)
}

/// Apply non-overlapping edits to `text` (also used by the tests).
pub fn apply(text: &str, edits: &[TextEdit]) -> String {
    let offset = |p: Position| -> usize {
        let mut off = 0;
        for (i, l) in text.split('\n').enumerate() {
            if i == p.line as usize {
                let mut units = 0;
                for (bi, c) in l.char_indices() {
                    if units >= p.character {
                        return off + bi;
                    }
                    units += c.len_utf16() as u32;
                }
                return off + l.len();
            }
            off += l.len() + 1;
        }
        text.len()
    };
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| offset(e.range.start));
    let mut out = String::new();
    let mut at = 0;
    for e in sorted {
        let (s, t) = (offset(e.range.start), offset(e.range.end));
        out.push_str(&text[at..s]);
        out.push_str(&e.new_text);
        at = t;
    }
    out.push_str(&text[at..]);
    out
}

/// JSON of the `data` field of a resolve request into (uri, line).
pub fn resolve_args(data: &Value) -> Option<(String, usize)> {
    Some((data.get("uri")?.as_str()?.to_string(), data.get("line")?.as_u64()? as usize))
}
