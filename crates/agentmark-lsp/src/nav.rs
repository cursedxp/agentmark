//! Go-to-definition and hover.

use agentmark::diag::Span;
use agentmark::project::ResolvedImport;
use agentmark::syntax::ast::Ident;
use agentmark::sema::{BlockInfo, BlockKind, CallKind, Checked, Resolution, Type};
use lsp_types::{Hover, HoverContents, Location, MarkupContent, MarkupKind, Position, Range};

use crate::index::{decl_name, dotted_word_at, var_type, block_param};
use crate::text::{char_pos, line_text, span_contains, span_range};
use crate::uri::uri_from_path;
use crate::workspace::Analysis;

/// A location in file `path` for a span of the facts, in the lines the file has now; `None` when
/// that line was changed since the check.
fn location(a: &Analysis, path: &str, span: Span) -> Option<Location> {
    let cur = a.current_span(path, span)?;
    let text = a.text(path)?;
    Some(Location::new(uri_from_path(path), span_range(text, cur)))
}

fn file_path(ck: &Checked, file: usize) -> Option<&str> {
    ck.files.get(file).map(String::as_str)
}

fn tool_location(a: &Analysis, tool: &str) -> Option<Location> {
    let def = a.project.manifest.tool(tool)?;
    let path = &a.project.manifest.path;
    let text = a.text(path)?;
    Some(Location::new(
        uri_from_path(path),
        span_range(text, Span::new(def.line, 2, tool.chars().count() as u32)),
    ))
}

/// Where the record `rec` declares `field`.
fn field_location(a: &Analysis, ck: &Checked, rec: &str, field: &str) -> Option<Location> {
    let r = ck.records.get(rec)?;
    let f = r.fields.iter().find(|f| f.name == field)?;
    location(a, file_path(ck, r.file)?, f.span)
}

/// The record that declares a field called `name` of type `ty` (the field a path's last segment
/// reads). Several candidates: the record the block works on wins (Q253).
fn field_owner<'c>(ck: &'c Checked, name: &str, ty: &Type, prefer: Option<&str>) -> Option<(&'c str, &'c agentmark::sema::FieldInfo)> {
    let mut found: Vec<(&str, &agentmark::sema::FieldInfo)> = Vec::new();
    for r in ck.records.values() {
        for f in &r.fields {
            if f.name == name && (f.ty == *ty || matches!(&f.ty, Type::Maybe(t) if **t == *ty)) {
                found.push((&r.name, f));
            }
        }
    }
    found
        .iter()
        .find(|(r, _)| Some(*r) == prefer)
        .or(found.first())
        .map(|(r, f)| (*r, *f))
}

fn smallest<'c, T>(it: impl Iterator<Item = (&'c agentmark::sema::NodeKey, T)>, fid: usize, line: usize, col: usize) -> Option<(&'c agentmark::sema::NodeKey, T)> {
    it.filter(|(k, _)| k.file == fid && span_contains(k.span, line, col))
        .min_by_key(|(k, _)| k.span.len)
}

pub fn definition(a: &Analysis, path: &str, text: &str, pos: Position) -> Vec<Location> {
    let (line, col) = char_pos(text, pos);
    let line_s = line_text(text, line);
    if let Some(loc) = import_target(a, path, line, line_s, col) {
        return vec![loc];
    }
    if let Some(loc) = start_target(a, path, line, line_s, col) {
        return vec![loc];
    }
    let Some(word) = dotted_word_at(line_s, col) else { return Vec::new() };
    let (_, _, seg) = &word.segs[word.at];
    let facts = a.facts(path).and_then(|(ck, map)| {
        let fid = ck.files.iter().position(|f| f == path)?;
        Some((ck, fid, map.new_to_old(line)?))
    });
    if let Some((ck, fid, old_line)) = facts {
        if let Some(loc) = resolved_definition(a, ck, fid, old_line, col, &word, seg) {
            return vec![loc];
        }
    }
    by_name(a, path, &word.segs.iter().map(|s| s.2.as_str()).collect::<Vec<_>>(), word.at)
        .into_iter()
        .collect()
}

fn resolved_definition(
    a: &Analysis,
    ck: &Checked,
    fid: usize,
    line: usize,
    col: usize,
    word: &crate::index::DottedWord,
    seg: &str,
) -> Option<Location> {
    let (key, res) = smallest(ck.resolutions.iter(), fid, line, col)?;
    let here = file_path(ck, fid)?;
    match res {
        Resolution::Var { name, def } => {
            if word.at == 0 && seg == name {
                return match def {
                    Some(d) => location(a, here, *d),
                    None => loop_binding(a, ck, fid, line, name),
                };
            }
            let prefer = block_param(ck, fid, key.span.line).map(|p| p.1);
            let last = word.segs.len() - 1;
            if word.at == last {
                let ty = ck.expr_types.get(key)?;
                let (rec, f) = field_owner(ck, seg, ty, prefer.as_deref())?;
                let r = ck.records.get(rec)?;
                return location(a, file_path(ck, r.file)?, f.span);
            }
            None
        }
        Resolution::Field { record, field, .. } => field_location(a, ck, record, field),
        Resolution::Variant { enum_name, variant } => {
            let e = ck.enums.get(enum_name)?;
            let v = e.variants.iter().find(|v| v.name == *variant)?;
            location(a, file_path(ck, e.file)?, v.span)
        }
        Resolution::Record { record } => {
            let r = ck.records.get(record)?;
            location(a, file_path(ck, r.file)?, r.span)
        }
        Resolution::Const { name } => {
            let c = ck.constants.get(name)?;
            location(a, file_path(ck, c.file)?, c.span)
        }
        Resolution::Source { tool, record, .. } => {
            if seg == tool {
                tool_location(a, tool)
            } else {
                let r = ck.records.get(record.as_ref()?)?;
                location(a, file_path(ck, r.file)?, r.span)
            }
        }
        Resolution::Call { kind, decl } => {
            let last = decl.rsplit('.').next().unwrap_or(decl);
            if seg != last {
                return None;
            }
            match kind {
                CallKind::Function => {
                    let s = ck.functions.get(decl)?;
                    location(a, file_path(ck, s.file)?, s.span)
                }
                CallKind::RustBlock => {
                    let r = ck.rust_blocks.iter().find(|r| r.name == *decl)?;
                    location(a, file_path(ck, r.file)?, r.span)
                }
                CallKind::Binding => {
                    let b = ck.bindings.iter().find(|b| b.name == *decl)?;
                    location(a, file_path(ck, b.file)?, b.span)
                }
                CallKind::Rule => {
                    let d = ck.declarations.iter().find(|d| d.name == *decl && d.kind == agentmark::sema::Kind::Rule)?;
                    location(a, file_path(ck, d.file)?, d.span)
                }
                CallKind::DoBlock => {
                    // only the `## Do` blocks of the agent that calls it
                    let want = format!("do {decl}");
                    let unit = &block_at(ck, fid, line)?.unit;
                    let b = ck.blocks.iter().find(|b| {
                        b.kind == BlockKind::Action && b.file == fid && b.unit == *unit && b.label.eq_ignore_ascii_case(&want)
                    })?;
                    location(a, file_path(ck, b.file)?, b.span)
                }
            }
        }
    }
}

/// The block (function, rule, entry, action...) the line is in.
fn block_at(ck: &Checked, fid: usize, line: usize) -> Option<&BlockInfo> {
    ck.blocks.iter().filter(|b| b.file == fid && b.span.line as usize <= line + 1).max_by_key(|b| b.span.line)
}

/// A `## Do` block receives the record of its caller's `for each`: the variable of the loop that
/// calls the block (read from the current syntax tree; `None` while the file does not parse).
fn loop_binding(a: &Analysis, ck: &Checked, fid: usize, line: usize, name: &str) -> Option<Location> {
    use agentmark::syntax::ast::{Decl, Stmt, StmtKind};
    fn find<'s>(body: &'s [Stmt], target: &str, var: Option<&'s Ident>) -> Option<&'s Ident> {
        for s in body {
            match &s.kind {
                StmtKind::Do { name, .. } if name.name == target && var.is_some() => return var,
                StmtKind::ForEach { var, body, .. } => {
                    if let Some(v) = find(body, target, Some(var)) {
                        return Some(v);
                    }
                }
                StmtKind::If { then, else_, .. } | StmtKind::IfSome { then, else_, .. } => {
                    if let Some(v) = find(then, target, var).or_else(|| find(else_, target, var)) {
                        return Some(v);
                    }
                }
                StmtKind::Try { stmt, otherwise } => {
                    if let Some(v) = find(std::slice::from_ref(stmt), target, var)
                        .or_else(|| otherwise.as_deref().and_then(|o| find(o, target, var)))
                    {
                        return Some(v);
                    }
                }
                StmtKind::Parallel(b) => {
                    if let Some(v) = find(b, target, var) {
                        return Some(v);
                    }
                }
                StmtKind::Match { arms, otherwise, .. } => {
                    let found = arms
                        .iter()
                        .find_map(|a| find(&a.body, target, var))
                        .or_else(|| otherwise.as_ref().and_then(|o| find(&o.body, target, var)));
                    if found.is_some() {
                        return found;
                    }
                }
                _ => {}
            }
        }
        None
    }
    let b = block_at(ck, fid, line)?;
    if b.kind != BlockKind::Action || b.param.as_ref().map(|p| p.0.as_str()) != Some(name) {
        return None;
    }
    let action = b.label.get(3..)?.trim().to_string();
    let f = &a.project.files[a.file_id(ck.files.get(fid)?)?];
    for d in &f.ast.decls {
        if let Decl::Agent(ag) = d {
            if ag.name.name != b.unit {
                continue;
            }
            for blk in &ag.blocks {
                if let Some(v) = find(&blk.body, &action, None) {
                    return location_current(a, &f.path, v.span);
                }
            }
        }
    }
    None
}

/// Look the word up among the declarations (and tools) of the project. `segs` are the parts of
/// the dotted word, `at` the one under the cursor.
fn by_name(a: &Analysis, _path: &str, segs: &[&str], at: usize) -> Option<Location> {
    let word = segs[at];
    let name = word;
    if at == 0 {
        if let Some(l) = tool_location(a, word) {
            if !a.project.files.iter().any(|f| f.ast.decls.iter().any(|d| decl_name(d).is_some_and(|(_, n, _)| n.name == word))) {
                return Some(l);
            }
        }
    }
    for f in &a.project.files {
        for d in &f.ast.decls {
            if let Some((_, n, _)) = decl_name(d) {
                if n.name == name {
                    return location_current(a, &f.path, n.span);
                }
            }
        }
    }
    if let Some(ck) = a.any_facts() {
        if let Some(d) = ck.declarations.iter().find(|d| d.name == name) {
            return location(a, file_path(ck, d.file)?, d.span);
        }
    }
    None
}

/// A location computed from the current AST (always up to date).
fn location_current(a: &Analysis, path: &str, span: Span) -> Option<Location> {
    let text = a.text(path)?;
    Some(Location::new(uri_from_path(path), span_range(text, span)))
}

/// A `# Uses` link (`- [Invoice](../records/invoice.agent.md)`) or a `# Import: crate x` line
/// -> the top of the file it names.
fn import_target(a: &Analysis, path: &str, line: usize, line_s: &str, col: usize) -> Option<Location> {
    let t = line_s.trim_start();
    let on_uses_link = t.starts_with("- [") || t.starts_with("* [");
    if !on_uses_link && !(t.starts_with("# Import:") && col > line_s.find(':')?) {
        return None;
    }
    let fid = a.file_id(path)?;
    let f = &a.project.files[fid];
    let idx = f.ast.imports.iter().position(|i| i.span.line as usize == line + 1)?;
    match f.imports.get(idx)? {
        ResolvedImport::Files(list) => {
            let target = &a.project.files[list.first()?.file];
            Some(Location::new(uri_from_path(&target.path), Range::default()))
        }
        ResolvedImport::Crate { binding: Some(b), .. } => {
            Some(Location::new(uri_from_path(&a.project.files[*b].path), Range::default()))
        }
        _ => None,
    }
}

/// `- start: [sales-report](#command-sales-report)` -> the heading of that command. The target is
/// the anchor of the heading, the file that declares the command, or the plain name.
fn start_target(a: &Analysis, path: &str, line: usize, line_s: &str, col: usize) -> Option<Location> {
    use agentmark::syntax::ast::{Command, Decl, ImportKind};
    if !line_s.trim_start().starts_with("- start:") || col < line_s.find(':')? + 1 {
        return None;
    }
    let fid = a.file_id(path)?;
    let f = &a.project.files[fid];
    let start = f.ast.decls.iter().find_map(|d| match d {
        Decl::Program(p) => p.start.as_ref().filter(|s| s.span.line as usize == line + 1),
        _ => None,
    })?;
    let target = start.target.as_str();
    let commands = |only: Option<usize>| -> Vec<(usize, &Command)> {
        a.project
            .files
            .iter()
            .enumerate()
            .filter(|(i, _)| only.is_none_or(|o| o == *i))
            .flat_map(|(i, f)| {
                f.ast.decls.iter().filter_map(move |d| match d {
                    Decl::Command(c) => Some((i, c)),
                    _ => None,
                })
            })
            .collect()
    };
    let anchor_of = |name: &str| {
        let kept: String = name.to_lowercase().chars().filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_')).collect();
        format!("command-{kept}")
    };
    let heading = |(i, c): (usize, &Command)| location_current(a, &a.project.files[i].path, c.name.span);
    if let Some(anchor) = target.strip_prefix('#') {
        let anchor = anchor.to_lowercase();
        return commands(None).into_iter().find(|(_, c)| anchor_of(&c.name.name) == anchor).and_then(heading);
    }
    let (file_part, anchor) = match target.split_once('#') {
        Some((p, an)) => (p, Some(an.to_lowercase())),
        None => (target, None),
    };
    if file_part.ends_with(".agent.md") {
        let wanted = agentmark::project::normalize(file_part);
        let idx = f
            .ast
            .imports
            .iter()
            .position(|i| matches!(&i.kind, ImportKind::File { path: w, .. } if agentmark::project::normalize(w) == wanted))?;
        let ResolvedImport::Files(list) = f.imports.get(idx)? else { return None };
        let target_file = list.first()?.file;
        let cmds = commands(Some(target_file));
        let pick = match &anchor {
            Some(an) => cmds.into_iter().find(|(_, c)| anchor_of(&c.name.name) == *an),
            None => cmds.into_iter().next(),
        };
        return match pick {
            Some(c) => heading(c),
            None => Some(Location::new(uri_from_path(&a.project.files[target_file].path), Range::default())),
        };
    }
    commands(None).into_iter().find(|(_, c)| c.name.name == target).and_then(heading)
}

// ---------------------------------------------------------------------------------- hover

/// The description under the heading at `line1` (1-based) of `path`: of a part, or of a `##` block.
fn heading_doc(a: &Analysis, path: &str, line1: u32) -> Option<String> {
    let f = &a.project.files[a.file_id(path)?];
    for d in &f.ast.decls {
        if let Some((_, n, doc)) = decl_name(d) {
            if n.span.line == line1 && doc.is_some() {
                return doc.cloned();
            }
        }
        if let agentmark::syntax::ast::Decl::Agent(ag) = d {
            if let Some(b) = ag.blocks.iter().find(|b| b.span.line == line1) {
                return b.doc.clone();
            }
        }
    }
    None
}

/// `**title**`, the description of the part, then what it touches.
fn heading_text(a: &Analysis, path: &str, title: &str, line1: u32, touches: &str) -> String {
    let mut s = format!("**{title}**");
    if let Some(doc) = heading_doc(a, path, line1) {
        s.push_str(&format!("\n\n{doc}"));
    }
    s.push_str(&format!("\n\nTouches: `{touches}`"));
    s
}

fn md(text: String, range: Option<Range>) -> Hover {
    Hover { contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: text }), range }
}

/// `invoice-chaser / Every day at 09:00`, `function net_amount`.
fn block_title(b: &BlockInfo) -> String {
    match b.kind {
        BlockKind::Function => format!("function {}", b.unit),
        BlockKind::Rule => format!("rule {}", b.unit),
        _ => format!("{} / {}", b.unit, b.label),
    }
}

pub fn hover(a: &Analysis, path: &str, text: &str, pos: Position) -> Option<Hover> {
    let (line, col) = char_pos(text, pos);
    let (ck, map) = a.facts(path)?;
    let fid = ck.files.iter().position(|f| f == path)?;
    // the facts talk about the lines of the last good check; the text is the current one
    let line_s = line_text(text, line);
    let line = map.new_to_old(line)?;
    let line1 = line as u32 + 1;
    let mut parts: Vec<String> = Vec::new();

    // 1. a heading: what the part is for (its description) and what it touches (SPEC 23.7)
    for b in ck.blocks.iter().filter(|b| b.file == fid && b.span.line == line1) {
        parts.push(heading_text(a, path, &block_title(b), b.span.line, &b.effect.touches()));
    }
    if parts.is_empty() {
        // a unit heading (`# Agent: x`): the union of its blocks
        if let Some(d) = ck.declarations.iter().find(|d| d.file == fid && d.span.line == line1 && matches!(d.kind, agentmark::sema::Kind::Agent | agentmark::sema::Kind::Service | agentmark::sema::Kind::Command)) {
            let mut s = heading_text(a, path, &format!("{} {}", d.kind.noun(), d.name), d.span.line, &ck.unit_effect(&d.name).touches());
            for b in ck.blocks.iter().filter(|b| b.unit == d.name) {
                s.push_str(&format!("\n- {}: touches {}", b.label, b.effect.touches()));
            }
            parts.push(s);
        }
    }
    if parts.is_empty() {
        // any other heading with a description (a record, an enum, a constant ...)
        if let Some(d) = ck.declarations.iter().find(|d| d.file == fid && d.span.line == line1) {
            if let Some(doc) = heading_doc(a, path, d.span.line) {
                parts.push(format!("**{} {}**\n\n{doc}", d.kind.noun(), d.name));
            }
        }
    }
    // 2. a field declaration
    for r in ck.records.values().filter(|r| r.file == fid) {
        for f in r.fields.iter().filter(|f| f.span.line == line1 && span_contains(f.span, line, col)) {
            let how = if f.inferred { " (type inferred from the name)" } else { "" };
            parts.push(format!("`{}.{}`: `{}`{how}", r.name, f.name, f.ty));
        }
    }
    // 3. a name in an expression
    if parts.is_empty() {
        if let Some(word) = dotted_word_at(line_s, col) {
            if let Some(s) = expression_hover(a, ck, fid, line, col, &word) {
                parts.push(s);
            }
        }
    }
    // 4. what the statement on this line touches
    if let Some((_, eff)) = smallest(ck.stmt_effects.iter(), fid, line, col) {
        if !eff.is_pure() && !parts.iter().any(|p| p.contains("Touches:")) {
            parts.push(format!("Touches: `{}`", eff.touches()));
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(md(parts.join("\n\n"), None))
}

fn expression_hover(a: &Analysis, ck: &Checked, fid: usize, line: usize, col: usize, word: &crate::index::DottedWord) -> Option<String> {
    let (key, res) = smallest(ck.resolutions.iter(), fid, line, col)?;
    let (_, _, seg) = &word.segs[word.at];
    let full: String = word.segs.iter().map(|s| s.2.as_str()).collect::<Vec<_>>().join(".");
    let ty = ck.expr_types.get(key);
    Some(match res {
        Resolution::Var { name, .. } => {
            let last = word.at + 1 == word.segs.len();
            if word.at == 0 && seg == name {
                match var_type(ck, Some(fid), name, key.span.line) {
                    Some(t) => format!("`{name}`: `{t}`"),
                    None if last => format!("`{name}`: `{}`", ty?),
                    None => format!("variable `{name}`"),
                }
            } else if last {
                format!("`{full}`: `{}`", ty?)
            } else {
                let prefer = block_param(ck, fid, key.span.line).map(|p| p.1);
                let (rec, f) = field_owner(ck, seg, ty?, prefer.as_deref())?;
                format!("`{rec}.{}`: `{}`", f.name, f.ty)
            }
        }
        Resolution::Field { record, field, .. } => {
            let f = ck.records.get(record)?.fields.iter().find(|f| f.name == *field)?;
            format!("`{record}.{field}`: `{}`", f.ty)
        }
        Resolution::Record { record } => format!("`{record}`: record"),
        Resolution::Variant { enum_name, variant } => {
            let v = ck.enums.get(enum_name)?.variants.iter().find(|v| v.name == *variant)?;
            if v.fields.is_empty() {
                format!("`{enum_name}.{variant}`: `{enum_name}`")
            } else {
                let fields: Vec<String> = v.fields.iter().map(|f| format!("{}: {}", f.name, f.ty)).collect();
                format!("`{enum_name}.{variant}` {{ {} }}: `{enum_name}`", fields.join(", "))
            }
        }
        Resolution::Const { name } => {
            let c = ck.constants.get(name)?;
            format!("constant `{name}`: `{}`", c.ty)
        }
        Resolution::Source { tool, source, record } => match record {
            Some(r) => format!("`{tool}.{source}`: `list of {r}`\n\nTouches: `{tool}`"),
            None => format!("`{tool}.{source}`"),
        },
        Resolution::Call { kind, decl } => {
            let touches: Option<String> = match kind {
                CallKind::Function => ck
                    .blocks
                    .iter()
                    .find(|b| b.kind == BlockKind::Function && b.unit == *decl)
                    .map(|b| b.effect.touches()),
                CallKind::RustBlock => a.project.files.iter().flat_map(|f| f.ast.decls.iter()).find_map(|d| match d {
                    agentmark::syntax::ast::Decl::RustBlock(r) if r.name.name == *decl => Some(r.touches.words.iter().map(|w| w.name.as_str()).collect::<Vec<_>>().join(", ")),
                    _ => None,
                }),
                _ => None,
            };
            match (touches, ty) {
                (Some(e), Some(t)) => format!("`{decl}(...)`: `{t}`\n\nTouches: `{e}`"),
                (Some(e), None) => format!("`{decl}`\n\nTouches: `{e}`"),
                (None, Some(t)) => format!("`{decl}`: `{t}`"),
                _ => format!("`{decl}`"),
            }
        }
    })
}
