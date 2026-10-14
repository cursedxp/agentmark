//! The on-demand tier (SPEC 15.2, last row): the author wrote a plain statement against an item
//! listed under `## Not bound`; the model writes the body from that statement and the item's
//! documentation, the target's compiler type-checks it, and only an accepted body is returned.
//! Two attempts, the second one told what was wrong with the first; after that the caller stops
//! the build (never silently wrong code).

use super::model::{CrateItems, Param, Shape, Sig, Ty};
use super::pipeline::{one_line, strip_fences, BoundItem, Tier};
use super::typecheck::{CheckRequest, TypeChecker};
use crate::syntax::ast::{Decl, TypeExpr, TypeKind};
use agentmark_rt::{ModelClient, ModelRequest};

/// How many times the model is asked for one item.
pub const ATTEMPTS: usize = 2;

/// One statement of the author that needs an item written.
pub struct Request<'a> {
    pub crate_name: &'a str,
    /// The item as the binding file lists it under `## Not bound`.
    pub item: &'a str,
    /// The author's statement as written, without the leading `- `.
    pub statement: &'a str,
    /// The reason the binding file gives for the item not being bound.
    pub reason: &'a str,
    /// How many values the statement gives after the item name.
    pub arity: usize,
}

/// Both attempts failed. `complaints` has one plain sentence per attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub item: String,
    pub complaints: Vec<String>,
}

/// The first line of the prompt is `on-demand <crate> <item>`, so mock fixtures can key on it.
pub fn prompt(ci: &CrateItems, req: &Request<'_>, known: Option<&Sig>, previous: Option<&str>) -> String {
    let item = ci.items.iter().find(|i| i.name == req.item);
    let docs = item.map(|i| i.docs.trim()).filter(|d| !d.is_empty()).unwrap_or("(no documentation)");
    let source_sig = item.map(|i| i.source_sig.as_str()).unwrap_or("(not available)");
    let target = match known {
        Some(sig) => format!(
            "AgentMark signature to implement:\n{}\n\nThe body is placed inside:\n{}\n\n\
             Reply with only the {} body (no fences, no explanation).",
            sig.heading(&req.item),
            item.map(|i| i.contract.as_str()).unwrap_or(""),
            ci.language
        ),
        None => format!(
            "The library item has no AgentMark signature yet. Choose one that fits the statement \
             (types: text, int, bool, bytes, date, datetime, duration, list of T, maybe T) with one \
             parameter per value the statement gives ({arity}). Reply with the signature on the first line as \
             `## {item}(name: type, ...) -> type`, then the {lang} body (no fences, no explanation). \
             The body is the body of `pub async fn {item}(..) -> Result<T, Box<dyn Error + Send + Sync>>`.",
            arity = req.arity,
            item = req.item,
            lang = ci.language
        ),
    };
    let retry = previous
        .map(|p| format!("\n\nYour previous answer was rejected: {p}\nWrite it again, fixing exactly that."))
        .unwrap_or_default();
    format!(
        "on-demand {krate} {item}\n\
         You are the AgentMark binder. The author wrote this statement:\n{statement}\n\n\
         It uses `{item}` of the crate `{krate}` {version}, which is not bound yet ({reason}). \
         Write the {lang} that makes the statement work.\n\n\
         {lang} signature of the library item:\n{source_sig}\n\n\
         Documentation of the item:\n{docs}\n\n\
         Style guide:\n{style}\n\n{target}{retry}",
        krate = req.crate_name,
        item = req.item,
        statement = req.statement,
        version = ci.version,
        reason = one_line(req.reason),
        lang = ci.language,
        style = ci.style_guide,
    )
}

fn ty_of(t: &TypeExpr) -> Result<Ty, String> {
    match &t.kind {
        TypeKind::Named(n) => match n.as_str() {
            "text" => Ok(Ty::Text),
            "int" => Ok(Ty::Int),
            "bool" => Ok(Ty::Bool),
            "bytes" => Ok(Ty::Bytes),
            "date" => Ok(Ty::Date),
            "datetime" => Ok(Ty::DateTime),
            "duration" => Ok(Ty::Duration),
            other => Err(format!("the type \"{other}\" cannot be used in a binding signature")),
        },
        TypeKind::List(i) => Ok(Ty::List(Box::new(ty_of(i)?))),
        TypeKind::Maybe(i) => Ok(Ty::Maybe(Box::new(ty_of(i)?))),
        TypeKind::Result(_) => Err("a signature cannot say \"result of\"; failures are already handled".into()),
    }
}

/// `## name(a: text) -> bytes` read back with the real parser.
fn parse_heading(heading: &str, item: &str) -> Result<Sig, String> {
    let text = format!("# Binding: x\n- crate: x\n- effect: unverified\n\n{heading}\n  ```rust\n  Ok(())\n  ```\n");
    let file = crate::syntax::parser::parse_file("signature", &text)
        .map_err(|_| format!("the signature line \"{heading}\" is not valid"))?;
    let Some(it) = file.decls.iter().find_map(|d| match d {
        Decl::Binding(b) => b.items.first(),
        _ => None,
    }) else {
        return Err(format!("the signature line \"{heading}\" is not valid"));
    };
    if it.name.name != item {
        return Err(format!("the signature must be for \"{item}\", not \"{}\"", it.name.name));
    }
    let mut params = Vec::new();
    for p in &it.params {
        params.push(Param { name: p.name.name.clone(), ty: ty_of(&p.ty)? });
    }
    let ret = it.out.as_ref().map(ty_of).transpose()?;
    Ok(Sig { params, ret })
}

/// Ask for the body of `req.item`, up to [`ATTEMPTS`] times. `log` gets one plain line per attempt.
pub async fn write_item(
    ci: &CrateItems,
    req: &Request<'_>,
    model: &dyn ModelClient,
    checker: &dyn TypeChecker,
    log: &mut (dyn FnMut(&str) + Send),
) -> Result<BoundItem, Failure> {
    let item = ci.items.iter().find(|i| i.name == req.item);
    let known: Option<Sig> = item.and_then(|i| match &i.shape {
        Shape::Direct { sig, .. } | Shape::Assisted { sig, .. } => Some(sig.clone()),
        Shape::Skipped { .. } => None,
    });
    let mut complaints: Vec<String> = Vec::new();
    for attempt in 1..=ATTEMPTS {
        let tries = if attempt > 1 { format!(" (try {attempt} of {ATTEMPTS})") } else { String::new() };
        let say = |log: &mut (dyn FnMut(&str) + Send), outcome: &str| {
            log(&format!("binding {}: writing {} on demand{tries}\u{2026} {outcome}", req.crate_name, req.item));
        };
        match attempt_once(ci, req, known.as_ref(), complaints.last().map(String::as_str), model, checker).await {
            Ok((sig, body)) => {
                say(log, "done");
                let base = match item {
                    Some(i) if !i.summary.trim().is_empty() => i.summary.trim().to_string(),
                    Some(i) => format!("Calls `{}`.", i.path),
                    None => format!("Calls `{}` of the library.", req.item),
                };
                return Ok(BoundItem {
                    name: req.item.to_string(),
                    path: item.map(|i| i.path.clone()).unwrap_or_default(),
                    summary: format!("{base} Written on demand; not reviewed yet."),
                    sig,
                    body,
                    tier: Tier::Assisted,
                });
            }
            Err(c) => {
                say(log, "failed");
                complaints.push(c);
            }
        }
    }
    Err(Failure { item: req.item.to_string(), complaints })
}

async fn attempt_once(
    ci: &CrateItems,
    req: &Request<'_>,
    known: Option<&Sig>,
    previous: Option<&str>,
    model: &dyn ModelClient,
    checker: &dyn TypeChecker,
) -> Result<(Sig, String), String> {
    let resp = model
        .complete(&ModelRequest::new(prompt(ci, req, known, previous)))
        .await
        .map_err(|e| format!("the model gave no code ({})", one_line(&e.to_string())))?;
    let mut text = strip_fences(&resp.text);
    let sig = match known {
        Some(s) => s.clone(),
        None => {
            let (first, rest) = text.split_once('\n').unwrap_or((text.as_str(), ""));
            if !first.trim_start().starts_with("## ") {
                return Err("the first line must be the signature, \"## name(params) -> type\"".into());
            }
            let sig = parse_heading(first.trim(), req.item)?;
            if sig.params.len() != req.arity {
                return Err(format!(
                    "the statement gives {} value{} but the signature has {} parameter{}",
                    req.arity,
                    if req.arity == 1 { "" } else { "s" },
                    sig.params.len(),
                    if sig.params.len() == 1 { "" } else { "s" }
                ));
            }
            text = strip_fences(rest);
            sig
        }
    };
    if text.is_empty() {
        return Err("the code was empty".into());
    }
    if text.contains("```") {
        return Err("the code contains a code fence".into());
    }
    checker
        .check(&CheckRequest { crate_name: &ci.name, version: &ci.version, name: req.item, sig: &sig, body: &text })
        .map_err(|e| format!("the code does not compile: {}", one_line(&e)))?;
    Ok((sig, text))
}
