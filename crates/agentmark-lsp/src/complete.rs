//! Completion: records, rules, tools and their verbs, bindings, constants, and fields after `x.`.
//!
//! The context is read from the text left of the cursor (the buffer rarely parses while the author
//! is typing); names and types come from the project's current ASTs and from the last good check
//! (Q252).

use agentmark::project::Project;
use agentmark::sema::{self, Checked, Kind, Type};
use agentmark::syntax::ast::{Decl, Trigger};
use lsp_types::{CompletionItem, CompletionItemKind, Documentation, Position};

use crate::index::{block_param, is_ident_char, prefixes, var_type, visible, walk_fields};
use crate::text::{char_pos, line_text};
use crate::workspace::{Analysis, Docs, OverlayFs, Doc};

const BUILTIN_TYPES: [&str; 8] = ["text", "int", "money", "date", "datetime", "duration", "bool", "bytes"];
const STATEMENTS: [&str; 12] = [
    "guard:", "set ", "ask model", "for each ", "if ", "else:", "stop", "do ", "try ", "return ", "parallel:", "respond ",
];
const HEADINGS: [&str; 9] = [
    "Agent: ", "Record: ", "Rule: ", "Function: ", "Constant: ", "Import: ", "Service: ", "Command: ", "Binding: ",
];
const BLOCKS: [&str; 5] = ["Every ", "Every day at ", "When ", "On start", "Do "];

fn item(label: impl Into<String>, kind: CompletionItemKind, detail: impl Into<Option<String>>, doc: Option<&String>) -> CompletionItem {
    CompletionItem {
        label: label.into(),
        kind: Some(kind),
        detail: detail.into(),
        documentation: doc.map(|d| Documentation::String(d.clone())),
        ..Default::default()
    }
}

fn kind_of(k: Kind) -> CompletionItemKind {
    match k {
        Kind::Record => CompletionItemKind::STRUCT,
        Kind::Enum => CompletionItemKind::ENUM,
        Kind::TypeAlias => CompletionItemKind::TYPE_PARAMETER,
        Kind::Rule | Kind::Function | Kind::RustBlock => CompletionItemKind::FUNCTION,
        Kind::Constant => CompletionItemKind::CONSTANT,
        Kind::Binding => CompletionItemKind::MODULE,
        Kind::Agent | Kind::Service | Kind::Command => CompletionItemKind::CLASS,
    }
}

/// Is the cursor inside the text of a string (not inside a `{...}` hole)? Returns the depth of
/// open holes when it is inside a string at all.
fn string_state(prefix: &str) -> Option<usize> {
    let (mut in_str, mut depth, mut esc) = (false, 0usize, false);
    for c in prefix.chars() {
        if esc {
            esc = false;
            continue;
        }
        match c {
            '\\' if in_str => esc = true,
            '"' if depth == 0 => in_str = !in_str,
            '{' if in_str => depth += 1,
            '}' if in_str => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    in_str.then_some(depth)
}

struct Enclosing {
    decl: Option<String>,
    in_block: bool,
}

fn enclosing(text: &str, line: usize) -> Enclosing {
    let mut decl = None;
    let mut in_block = false;
    for l in (0..line).rev() {
        let t = line_text(text, l);
        if t.starts_with("## ") && decl.is_none() && !in_block {
            in_block = true;
        }
        if let Some(rest) = t.strip_prefix("# ") {
            decl = rest.split(':').next().map(|w| w.trim().to_lowercase());
            break;
        }
    }
    Enclosing { decl, in_block }
}

fn matches_partial(label: &str, partial: &str) -> bool {
    partial.is_empty() || label.to_lowercase().starts_with(&partial.to_lowercase())
}

pub fn complete(a: &Analysis, docs: &Docs, path: &str, text: &str, pos: Position) -> Vec<CompletionItem> {
    let (line, col) = char_pos(text, pos);
    let line_s = line_text(text, line);
    let prefix: String = line_s.chars().take(col).collect();
    let depth = string_state(&prefix);
    if depth == Some(0) {
        return Vec::new(); // plain text of a string
    }
    let trimmed = prefix.trim_start();
    let enc = enclosing(text, line);

    // the dotted token left of the cursor
    let chars: Vec<char> = prefix.chars().collect();
    let mut start = chars.len();
    while start > 0 && (is_ident_char(chars[start - 1]) || chars[start - 1] == '.') {
        start -= 1;
    }
    let token: String = chars[start..].iter().collect();
    let mut segs: Vec<&str> = token.split('.').collect();
    let partial = segs.pop().unwrap_or("").to_string();

    let mut items: Vec<CompletionItem> = Vec::new();
    if trimmed.starts_with('#') {
        items = heading_items(trimmed);
    } else if !segs.is_empty() && segs[0].chars().next().is_some_and(|c| c.is_alphabetic()) {
        let dot_col = chars.len() - partial.chars().count() - 1;
        items = field_items(a, docs, path, text, line, start, dot_col, col, &segs);
    } else {
        let before: String = prefix.chars().take(chars.len() - partial.chars().count()).collect();
        let before = before.trim_end();
        let stmt = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        let stmt = stmt.strip_prefix("try ").unwrap_or(stmt);
        let words_before: Vec<&str> = stmt.split_whitespace().collect();
        let ends_space = prefix.ends_with(' ');
        let rest_words = if ends_space { words_before.len() } else { words_before.len().saturating_sub(1) };

        let in_import = trimmed.starts_with("# Import");
        if !in_import && type_context(before, &enc, trimmed) {
            items = type_items(a, path, before);
        } else if stmt.starts_with("tools:") {
            items = tool_items(a, path);
        } else if rest_words == 1 && stmt.starts_with("do ") {
            items = action_items(a, path);
        } else if rest_words == 2 && stmt.starts_with("for each ") {
            items = rule_items(a, path);
        } else if rest_words >= 1 && !stmt.contains(':') && tool_statement(a, path, words_before.first().copied().unwrap_or("")) {
            items = verb_items(a, path, words_before.first().copied().unwrap_or(""), rest_words);
        } else if rest_words == 0 && enc.in_block {
            items = statement_items(a, path);
        } else if rest_words == 0 && !enc.in_block && trimmed.starts_with("- ") {
            items = property_items(enc.decl.as_deref());
        } else if expr_context(before, depth) {
            items = expression_items(a, path, line);
        }
    }
    items.retain(|i| matches_partial(&i.label, &partial));
    items
}

fn heading_items(trimmed: &str) -> Vec<CompletionItem> {
    let (list, rest): (&[&str], &str) = if let Some(r) = trimmed.strip_prefix("## ") {
        (&BLOCKS, r)
    } else if let Some(r) = trimmed.strip_prefix("# ") {
        if r.contains(':') {
            return Vec::new();
        }
        (&HEADINGS, r)
    } else {
        return Vec::new();
    };
    list.iter()
        .filter(|h| matches_partial(h, rest))
        .map(|h| item(*h, CompletionItemKind::KEYWORD, None, None))
        .collect()
}

fn type_context(before: &str, enc: &Enclosing, trimmed: &str) -> bool {
    let last = before.rsplit(|c: char| c.is_whitespace()).next().unwrap_or("");
    match last {
        "as" | "matches" | "for:" => true,
        "of" | "maybe" => true,
        l if l.ends_with(':') => {
            // `- amount: ` in a record, `- in: invoice: ` and `- out: ` in a function or Rust block
            let one_colon = before.matches(':').count() == 1;
            let in_record = enc.decl.as_deref() == Some("record") && trimmed.starts_with("- ") && one_colon;
            let in_signature = trimmed.starts_with("- in:") || trimmed.starts_with("- out:");
            let signature_slot = l == "out:" || (trimmed.starts_with("- in:") && before.matches(':').count() == 2);
            in_record || (in_signature && signature_slot)
        }
        _ => false,
    }
}

fn type_items(a: &Analysis, path: &str, before: &str) -> Vec<CompletionItem> {
    let last = before.rsplit(|c: char| c.is_whitespace()).next().unwrap_or("");
    let mut out: Vec<CompletionItem> = visible(a, path)
        .iter()
        .filter(|v| matches!(v.kind, Kind::Record | Kind::Enum | Kind::TypeAlias))
        .map(|v| {
            let kind = match v.kind {
                Kind::Enum => CompletionItemKind::ENUM,
                Kind::TypeAlias => CompletionItemKind::TYPE_PARAMETER,
                _ => CompletionItemKind::STRUCT,
            };
            item(v.name.clone(), kind, Some(v.kind.noun().to_string()), v.doc.as_ref())
        })
        .collect();
    if !matches!(last, "as" | "matches" | "for:") {
        for t in BUILTIN_TYPES {
            out.push(item(t, CompletionItemKind::TYPE_PARAMETER, None, None));
        }
        for t in ["list of ", "maybe ", "result of "] {
            out.push(item(t, CompletionItemKind::KEYWORD, None, None));
        }
    }
    out
}

fn binding_names(a: &Analysis) -> Vec<(String, Vec<String>)> {
    let mut v = Vec::new();
    for f in &a.project.files {
        for d in &f.ast.decls {
            if let Decl::Binding(b) = d {
                // methods of a handle are called on a variable (`- hasher update data`), not on the binding
                let is_method = |i: &agentmark::syntax::ast::BindingItem| i.params.first().is_some_and(|p| p.name.name == "self");
                v.push((b.name.name.clone(), b.items.iter().filter(|i| !is_method(i)).map(|i| i.name.name.clone()).collect()));
            }
        }
    }
    v
}

fn tool_items(a: &Analysis, _path: &str) -> Vec<CompletionItem> {
    let mut out: Vec<CompletionItem> = a
        .project
        .manifest
        .tools
        .values()
        .map(|t| item(t.name.clone(), CompletionItemKind::MODULE, Some(format!("tool, {}", agentmark::sema::effects::effect_name(t.effect))), None))
        .collect();
    out.push(item("file", CompletionItemKind::MODULE, Some("built-in tool, local".to_string()), None));
    for (b, _) in binding_names(a) {
        out.push(item(b, CompletionItemKind::MODULE, Some("binding".to_string()), None));
    }
    out
}

fn tool_statement(a: &Analysis, _path: &str, word: &str) -> bool {
    let word = word.trim_end_matches(':');
    word == "file" || a.project.manifest.tool(word).is_some() || binding_names(a).iter().any(|(b, _)| b == word)
}

fn verb_items(a: &Analysis, _path: &str, tool: &str, rest_words: usize) -> Vec<CompletionItem> {
    if tool == "file" && rest_words == 1 {
        return agentmark::stdlib::FILE_VERBS
            .iter()
            .map(|v| item(v.verb, CompletionItemKind::METHOD, Some(v.shape.to_string()), None))
            .collect();
    }
    if let Some(def) = a.project.manifest.tool(tool) {
        return match rest_words {
            1 => def.verbs.iter().map(|v| item(v.clone(), CompletionItemKind::METHOD, Some(format!("{tool} verb")), None)).collect(),
            2 => {
                let mut words = def.target_words();
                words.sort();
                words.dedup();
                words.into_iter().map(|w| item(w, CompletionItemKind::ENUM_MEMBER, Some(format!("{tool} target")), None)).collect()
            }
            _ => Vec::new(),
        };
    }
    if rest_words == 1 {
        if let Some((_, items)) = binding_names(a).into_iter().find(|(b, _)| b == tool) {
            return items.into_iter().map(|i| item(i, CompletionItemKind::FUNCTION, Some(format!("{tool} binding item")), None)).collect();
        }
    }
    Vec::new()
}

fn action_items(a: &Analysis, path: &str) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    if let Some(fid) = a.file_id(path) {
        for d in &a.project.files[fid].ast.decls {
            if let Decl::Agent(ag) = d {
                for b in &ag.blocks {
                    if let Trigger::Do(n) = &b.trigger {
                        out.push(item(n.name.clone(), CompletionItemKind::METHOD, Some("## Do block".to_string()), b.doc.as_ref()));
                    }
                }
            }
        }
    }
    for v in visible(a, path) {
        if matches!(v.kind, Kind::Function | Kind::RustBlock) {
            out.push(item(v.name.clone(), CompletionItemKind::FUNCTION, Some(v.kind.noun().to_string()), v.doc.as_ref()));
        }
    }
    out
}

fn rule_items(a: &Analysis, path: &str) -> Vec<CompletionItem> {
    visible(a, path)
        .iter()
        .filter(|v| v.kind == Kind::Rule)
        .map(|v| item(v.name.clone(), CompletionItemKind::FUNCTION, Some("rule".to_string()), v.doc.as_ref()))
        .collect()
}

fn statement_items(a: &Analysis, path: &str) -> Vec<CompletionItem> {
    let mut out = tool_items(a, path);
    for s in STATEMENTS {
        out.push(item(s, CompletionItemKind::KEYWORD, None, None));
    }
    out
}

fn property_items(decl: Option<&str>) -> Vec<CompletionItem> {
    let keys: &[&str] = match decl {
        Some("agent") => &["model:", "tools:", "memory:", "timeout:"],
        Some("rule") => &["for:", "when:"],
        Some("constant") => &["value:"],
        Some("function") => &["in:", "out:", "effect:"],
        Some("service") => &["listen:", "state:"],
        _ => &[],
    };
    keys.iter().map(|k| item(*k, CompletionItemKind::PROPERTY, None, None)).collect()
}

fn expr_context(before: &str, depth: Option<usize>) -> bool {
    if depth.is_some_and(|d| d > 0) && before.ends_with('{') {
        return true;
    }
    if let Some(c) = before.chars().last() {
        if matches!(c, ':' | '=' | '<' | '>' | '(' | ',' | '+' | '*' | '/' | '{') {
            return true;
        }
    }
    let last = before.rsplit(|c: char| c.is_whitespace()).next().unwrap_or("");
    matches!(last, "and" | "or" | "not" | "if" | "in" | "where" | "-" | "=" | "return" | "some" | "match")
}

fn expression_items(a: &Analysis, path: &str, line: usize) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    for v in visible(a, path) {
        match v.kind {
            Kind::Constant => out.push(item(v.name.clone(), CompletionItemKind::CONSTANT, constant_type(a, &v.name), v.doc.as_ref())),
            Kind::Function | Kind::RustBlock => out.push(item(v.name.clone(), CompletionItemKind::FUNCTION, Some(v.kind.noun().to_string()), v.doc.as_ref())),
            _ => {}
        }
    }
    if let Some(ck) = a.any_facts() {
        let fid = ck.files.iter().position(|f| f == path);
        if let Some((var, rec)) = fid.and_then(|f| block_param(ck, f, a.facts_line(path, line))) {
            if let Some(r) = ck.records.get(&rec) {
                for f in &r.fields {
                    out.push(item(f.name.clone(), CompletionItemKind::FIELD, Some(format!("{}: {}", rec, f.ty)), None));
                }
            }
            out.push(item(var, CompletionItemKind::VARIABLE, Some(rec), None));
        }
    }
    for w in ["today", "now", "none"] {
        out.push(item(w, CompletionItemKind::KEYWORD, None, None));
    }
    for m in agentmark::stdlib::MODULES {
        out.push(item(*m, CompletionItemKind::MODULE, Some("built-in library".to_string()), None));
    }
    out.extend(variant_items(a, path));
    out
}

/// The variants of the enums the file can see: values (`Circle { radius: 3 }`) and `match` branches.
/// They come from the last good check, so they are there while the line being typed does not parse.
fn variant_items(a: &Analysis, path: &str) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let Some(ck) = a.any_facts() else { return out };
    for v in visible(a, path).iter().filter(|v| v.kind == Kind::Enum) {
        let base = v.name.rsplit('.').next().unwrap_or(&v.name);
        let Some(e) = ck.enums.get(base) else { continue };
        for var in &e.variants {
            let fields: Vec<&str> = var.fields.iter().map(|x| x.name.as_str()).collect();
            let detail = if fields.is_empty() { v.name.clone() } else { format!("{} ({})", v.name, fields.join(", ")) };
            out.push(item(var.name.clone(), CompletionItemKind::ENUM_MEMBER, Some(detail), None));
        }
    }
    out
}

fn constant_type(a: &Analysis, name: &str) -> Option<String> {
    let bare = name.rsplit('.').next().unwrap_or(name);
    a.any_facts()?.constants.get(bare).map(|c| c.ty.to_string())
}

// ---------------------------------------------------------------------------------- fields

/// Check the buffer of `path` with `new_text` in place of its text. The result is only used when
/// the project checks clean (Q254).
fn check_patched(a: &Analysis, docs: &Docs, path: &str, new_text: &str) -> Option<(Project, Checked)> {
    let mut docs = docs.clone();
    let version = docs.get(path).map_or(0, |d| d.version);
    docs.insert(path.to_string(), Doc { text: new_text.to_string(), version });
    let fs = OverlayFs::new(&docs);
    let target = if a.root.ends_with(".agent.md") { path } else { a.root.as_str() };
    let p = Project::load_with(&fs, target);
    let c = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sema::check(&p))).ok()?.ok()?;
    Some((p, c))
}

#[allow(clippy::too_many_arguments)]
fn field_items(
    a: &Analysis,
    docs: &Docs,
    path: &str,
    text: &str,
    line: usize,
    tok_start: usize,
    dot_col: usize,
    cursor: usize,
    chain: &[&str],
) -> Vec<CompletionItem> {
    let chain_len: usize = chain.iter().map(|s| s.chars().count()).sum::<usize>() + chain.len() - 1;
    // A: ask the checker about the buffer without the half-typed field
    let line_s = line_text(text, line);
    // drop `.partial` and the rest of the word the cursor is in
    let word_end = cursor + line_s.chars().skip(cursor).take_while(|c| is_ident_char(*c)).count();
    let patched_line: String = line_s.chars().take(dot_col).chain(line_s.chars().skip(word_end)).collect();
    let new_text: String = text
        .split('\n')
        .enumerate()
        .map(|(i, l)| if i == line { patched_line.as_str() } else { l })
        .collect::<Vec<_>>()
        .join("\n");
    if let Some((p, ck)) = check_patched(a, docs, path, &new_text) {
        if let Some(fid) = p.file_index(path) {
            let ty = ck.expr_types.iter().find_map(|(k, t)| {
                (k.file == fid && k.span.line as usize == line + 1 && k.span.col as usize == tok_start + 1 && k.span.len as usize == chain_len)
                    .then(|| t.clone())
            });
            if let Some(ty) = ty {
                return fields_of(a, Some(&ck), ty);
            }
        }
    }
    // B: the last good check, then the declarations as written
    let ck = a.any_facts();
    let fid = ck.and_then(|ck| ck.files.iter().position(|f| f == path));
    let base = ck
        .and_then(|ck| var_type(ck, fid, chain[0], a.facts_line(path, line)))
        .or_else(|| state_type(text, chain[0], line));
    match (base, ck) {
        (Some(t), Some(ck)) => {
            if let Some(t) = walk_fields(ck, t, &chain[1..]) {
                return fields_of(a, Some(ck), t);
            }
        }
        (Some(t), None) if chain.len() == 1 => return fields_of(a, None, t),
        _ => {}
    }
    if chain.len() == 1 {
        // a module of the built-in library (SPEC 22.3): `text.` offers its functions
        if agentmark::stdlib::is_module(chain[0]) {
            return agentmark::stdlib::functions_of(chain[0])
                .map(|f| item(f.name, CompletionItemKind::FUNCTION, Some(format!("{} - {}", f.signature(), f.doc)), None))
                .collect();
        }
        if let Some(def) = a.project.manifest.tool(chain[0]) {
            return def
                .sources
                .iter()
                .map(|s| {
                    let rec = agentmark::manifest::singular(s);
                    item(s.clone(), CompletionItemKind::FIELD, Some(format!("source of {rec}, network({})", def.name)), None)
                })
                .collect();
        }
        if prefixes(a, path).iter().any(|p| p == chain[0]) {
            let pre = format!("{}.", chain[0]);
            return visible(a, path)
                .iter()
                .filter_map(|v| v.name.strip_prefix(&pre).map(|n| item(n, kind_of(v.kind), Some(v.kind.noun().to_string()), v.doc.as_ref())))
                .collect();
        }
    }
    Vec::new()
}

/// `state` in a route or command is the record of the service's `- state:` line (Q9). Read from
/// the text, because the file is usually half typed when this is asked.
fn state_type(text: &str, name: &str, line: usize) -> Option<Type> {
    if name != "state" {
        return None;
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let heading = (0..=line.min(lines.len().saturating_sub(1))).rev().find(|&l| lines[l].starts_with("# "))?;
    if !lines[heading].starts_with("# Service") {
        return None;
    }
    lines[heading + 1..]
        .iter()
        .take_while(|l| !l.starts_with("# "))
        .find_map(|l| l.strip_prefix("- state:"))
        .map(|r| Type::Record(r.trim().to_string()))
}

fn type_text(t: &agentmark::syntax::ast::TypeExpr) -> String {
    use agentmark::syntax::ast::TypeKind;
    match &t.kind {
        TypeKind::Named(n) => n.clone(),
        TypeKind::List(i) => format!("list of {}", type_text(i)),
        TypeKind::Maybe(i) => format!("maybe {}", type_text(i)),
        TypeKind::Result(i) => format!("result of {}", type_text(i)),
        TypeKind::Handle { name, mode } => match mode.word() {
            Some(w) => format!("{w} handle of {name}"),
            None => format!("handle of {name}"),
        },
    }
}

/// The fields of a record type: from the check when there is one, else as the AST writes them.
fn fields_of(a: &Analysis, ck: Option<&Checked>, ty: Type) -> Vec<CompletionItem> {
    let ty = crate::index::unwrap_maybe(ty);
    if let (Some(ck), Type::Record(r)) = (ck, &ty) {
        if let Some(rec) = ck.records.get(r) {
            return rec
                .fields
                .iter()
                .map(|f| item(f.name.clone(), CompletionItemKind::FIELD, Some(format!("{}{}", f.ty, if f.inferred { " (inferred)" } else { "" })), None))
                .collect();
        }
    }
    match ty {
        Type::Record(r) => a
            .project
            .files
            .iter()
            .flat_map(|f| f.ast.decls.iter())
            .find_map(|d| match d {
                Decl::Record(rec) if rec.name.name == r => Some(rec),
                _ => None,
            })
            .map(|rec| {
                rec.fields
                    .iter()
                    .map(|f| item(f.name.name.clone(), CompletionItemKind::FIELD, f.ty.as_ref().map(type_text), None))
                    .collect()
            })
            .unwrap_or_default(),
        Type::Object(fields) => fields.iter().map(|(n, t)| item(n.clone(), CompletionItemKind::FIELD, Some(t.to_string()), None)).collect(),
        _ => Vec::new(),
    }
}
