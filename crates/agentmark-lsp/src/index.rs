//! What can be named where: declarations of the project, the word under the cursor, the facts of
//! the last good check.

use agentmark::project::ResolvedImport;
use agentmark::sema::{Checked, Kind, Type};
use agentmark::syntax::ast::{Decl, Ident};

use crate::workspace::Analysis;

/// A declaration as the editor lists it.
#[derive(Debug, Clone)]
pub struct Visible {
    /// The name as written in the importing file (`billing.overdue` through a module alias).
    pub name: String,
    pub kind: Kind,
    pub doc: Option<String>,
}

pub fn decl_name(d: &Decl) -> Option<(Kind, &Ident, Option<&String>)> {
    Some(match d {
        Decl::Record(r) => (Kind::Record, &r.name, r.doc.as_ref()),
        Decl::Rule(r) => (Kind::Rule, &r.name, r.doc.as_ref()),
        Decl::Agent(r) => (Kind::Agent, &r.name, r.doc.as_ref()),
        Decl::Function(r) => (Kind::Function, &r.name, r.doc.as_ref()),
        Decl::Service(r) => (Kind::Service, &r.name, r.doc.as_ref()),
        Decl::Command(r) => (Kind::Command, &r.name, r.doc.as_ref()),
        Decl::Binding(r) => (Kind::Binding, &r.name, r.doc.as_ref()),
        Decl::Constant(r) => (Kind::Constant, &r.name, r.doc.as_ref()),
        Decl::RustBlock(r) => (Kind::RustBlock, &r.name, r.doc.as_ref()),
        Decl::Tool(r) => (Kind::Tool, &r.name, r.doc.as_ref()),
        Decl::Run(_) | Decl::Entry(_) => return None,
    })
}

/// The declarations the file `path` can use: its own and those of the files it imports (with the
/// module prefix of the import). When the file does not parse right now, every declaration of
/// the project, plus those the last good check knew in that file.
pub fn visible(a: &Analysis, path: &str) -> Vec<Visible> {
    let mut out: Vec<Visible> = Vec::new();
    let mut push = |name: String, kind: Kind, doc: Option<&String>| {
        if !out.iter().any(|v| v.name == name && v.kind == kind) {
            out.push(Visible { name, kind, doc: doc.cloned() });
        }
    };
    match a.file_id(path) {
        Some(fid) => {
            let f = &a.project.files[fid];
            for d in &f.ast.decls {
                if let Some((k, n, doc)) = decl_name(d) {
                    push(n.name.clone(), k, doc);
                }
            }
            for imp in &f.imports {
                if let ResolvedImport::Files(list) = imp {
                    for i in list {
                        for d in &a.project.files[i.file].ast.decls {
                            if let Some((k, n, doc)) = decl_name(d) {
                                let name = match &i.prefix {
                                    Some(p) => format!("{p}.{}", n.name),
                                    None => n.name.clone(),
                                };
                                push(name, k, doc);
                            }
                        }
                    }
                }
            }
        }
        None => {
            for f in &a.project.files {
                for d in &f.ast.decls {
                    if let Some((k, n, doc)) = decl_name(d) {
                        push(n.name.clone(), k, doc);
                    }
                }
            }
            if let Some(ck) = a.any_facts() {
                for d in ck.declarations.iter().filter(|d| ck.files.get(d.file).map(String::as_str) == Some(path)) {
                    push(d.name.clone(), d.kind, None);
                }
            }
        }
    }
    out
}

/// Module prefixes (`billing`) the file imports under.
pub fn prefixes(a: &Analysis, path: &str) -> Vec<String> {
    let Some(fid) = a.file_id(path) else { return Vec::new() };
    let mut v = Vec::new();
    for imp in &a.project.files[fid].imports {
        if let ResolvedImport::Files(list) = imp {
            for i in list {
                if let Some(p) = &i.prefix {
                    if !v.contains(p) {
                        v.push(p.clone());
                    }
                }
            }
        }
    }
    v
}

pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// A dotted name around column `col` (characters) of `line`: `invoice.amount`, `billing.overdue`.
#[derive(Debug, Clone)]
pub struct DottedWord {
    /// Start column of the whole token.
    pub start: usize,
    /// (start, end, text) of each segment.
    pub segs: Vec<(usize, usize, String)>,
    /// The segment the cursor is on.
    pub at: usize,
}

pub fn dotted_word_at(line: &str, col: usize) -> Option<DottedWord> {
    let chars: Vec<char> = line.chars().collect();
    let ok = |c: char| is_ident_char(c) || c == '.';
    let mut s = col.min(chars.len());
    // the cursor may sit right behind the word
    if s == chars.len() || !ok(chars[s]) {
        if s > 0 && ok(chars[s - 1]) {
            s -= 1;
        } else {
            return None;
        }
    }
    let mut start = s;
    while start > 0 && ok(chars[start - 1]) {
        start -= 1;
    }
    let mut end = s + 1;
    while end < chars.len() && ok(chars[end]) {
        end += 1;
    }
    let mut segs = Vec::new();
    let mut i = start;
    while i < end {
        let st = i;
        while i < end && chars[i] != '.' {
            i += 1;
        }
        let text: String = chars[st..i].iter().collect();
        let trimmed = text.trim_matches('-');
        if !trimmed.is_empty() && trimmed.chars().next().is_some_and(|c| c.is_alphabetic()) {
            segs.push((st, i, text));
        }
        i += 1;
    }
    let at = segs.iter().position(|(a, b, _)| *a <= col && col <= *b)?;
    Some(DottedWord { start, segs, at })
}

/// Type of a path prefix through record fields: `Invoice` + `["customer"]`.
pub fn walk_fields(ck: &Checked, mut ty: Type, rest: &[&str]) -> Option<Type> {
    for seg in rest {
        ty = unwrap_maybe(ty);
        ty = match &ty {
            Type::Record(r) => ck.records.get(r)?.fields.iter().find(|f| f.name == *seg)?.ty.clone(),
            Type::Object(fields) => fields.iter().find(|(n, _)| n == seg)?.1.clone(),
            _ => return None,
        };
    }
    Some(unwrap_maybe(ty))
}

pub fn unwrap_maybe(t: Type) -> Type {
    match t {
        Type::Maybe(inner) => unwrap_maybe(*inner),
        other => other,
    }
}

/// The record a variable stands for, from the last good check: a variable the block receives
/// (rule record, event, `## Do` record) or a bare use of it somewhere (Q252).
pub fn var_type(ck: &Checked, fid: Option<usize>, name: &str, line: u32) -> Option<Type> {
    use agentmark::sema::Resolution;
    let mut best: Option<(u32, Type)> = None;
    for (key, res) in &ck.resolutions {
        let Resolution::Var { name: n, .. } = res else { continue };
        if n != name || Some(key.file) != fid {
            continue;
        }
        // a path of one segment: its type is the variable's type
        if key.span.len as usize != name.chars().count() {
            continue;
        }
        let Some(t) = ck.expr_types.get(key) else { continue };
        let t = unwrap_maybe(t.clone());
        if !matches!(t, Type::Record(_) | Type::Object(_)) {
            continue;
        }
        let dist = key.span.line.abs_diff(line);
        if best.as_ref().is_none_or(|(d, _)| dist < *d) {
            best = Some((dist, t));
        }
    }
    if let Some((_, t)) = best {
        return Some(t);
    }
    block_param(ck, fid?, line).filter(|(v, _)| v == name).map(|(_, r)| Type::Record(r))
}

/// `(variable, record)` the block around `line` works on.
pub fn block_param(ck: &Checked, fid: usize, line: u32) -> Option<(String, String)> {
    ck.blocks
        .iter()
        .filter(|b| b.file == fid && b.span.line <= line)
        .max_by_key(|b| b.span.line)
        .and_then(|b| b.param.clone())
}
