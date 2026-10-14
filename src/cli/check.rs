//! `agentmark check`: diagnostics plus the reviewer-facing report (SPEC 8, 9, 15.3, 17, 18.2).

use serde_json::{json, Value};

use super::{plural, require_source};
use crate::diag::{Diagnostic, Severity};
use crate::project::Project;
use crate::sema::effects::effect_name;
use crate::sema::{self, BlockInfo, BlockKind, Checked, EffectSet, Kind};
use crate::syntax::ast::Decl;

/// One line of the effect summary, SPEC 8 style: `invoice-chaser / Every day at 09:00: model, ...`.
/// Functions and rules have no trigger heading, so they read `function net_amount: pure`.
pub(crate) fn summary_line(b: &BlockInfo) -> String {
    match b.kind {
        BlockKind::Function => format!("function {}: {}", b.unit, b.effect),
        BlockKind::Rule => format!("rule {}: {}", b.unit, b.effect),
        _ => format!("{} / {}: {}", b.unit, b.label, b.effect),
    }
}

fn kind_word(k: Option<Kind>, b: BlockKind) -> &'static str {
    match k {
        Some(Kind::Agent) => "agent",
        Some(Kind::Service) => "service",
        Some(Kind::Command) => "command",
        Some(Kind::Function) => "function",
        Some(Kind::Rule) => "rule",
        _ => match b {
            BlockKind::Function => "function",
            BlockKind::Rule => "rule",
            BlockKind::Route => "service",
            BlockKind::Command => "command",
            _ => "agent",
        },
    }
}

fn effects_json(e: &EffectSet) -> Value {
    json!({
        "class": effect_name(e.class()),
        "summary": e.to_string(),
        "tools": { "model": e.model, "network": e.network, "local": e.local, "unverified": e.unverified },
    })
}

fn diag_json(d: &Diagnostic) -> Value {
    json!({
        "code": d.code,
        "severity": if d.severity == Severity::Error { "error" } else { "warning" },
        "file": d.file,
        "line": d.span.line,
        "column": d.span.col,
        "length": d.span.len,
        "message": d.message,
        "did_you_mean": d.did_you_mean,
        "help": d.help,
    })
}

/// Where the `- effect:` line of a binding is, for "change this line".
pub(crate) fn effect_line(project: &Project, file: usize, name: &str) -> u32 {
    project.files[file]
        .ast
        .decls
        .iter()
        .find_map(|d| match d {
            Decl::Binding(b) if b.name.name == name => Some(b.effect.span.line),
            _ => None,
        })
        .unwrap_or(1)
}

/// The facts a `Checked` result reports, in the order the text prints them.
struct Report {
    units: Vec<Value>,
    lines: Vec<String>,
    inferred: Vec<Value>,
    inferred_lines: Vec<String>,
    unverified: Vec<Value>,
    unverified_lines: Vec<String>,
    rust: Vec<Value>,
    rust_lines: Vec<String>,
}

fn report(project: &Project, c: &Checked) -> Report {
    let kind_of = |name: &str| c.declarations.iter().find(|d| d.name == name).map(|d| d.kind);
    let mut units: Vec<Value> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    for b in &c.blocks {
        if !order.contains(&b.unit) {
            order.push(b.unit.clone());
        }
    }
    for name in &order {
        let blocks: Vec<&BlockInfo> = c.blocks.iter().filter(|b| &b.unit == name).collect();
        let kind = kind_word(kind_of(name), blocks[0].kind);
        units.push(json!({
            "name": name,
            "kind": kind,
            "file": c.files[blocks[0].file],
            "effects": effects_json(&c.unit_effect(name)),
            "blocks": blocks.iter().map(|b| json!({
                "label": b.label,
                "file": c.files[b.file],
                "line": b.span.line,
                "effects": effects_json(&b.effect),
            })).collect::<Vec<_>>(),
        }));
    }
    let lines = c.blocks.iter().map(summary_line).collect();

    let mut inferred = Vec::new();
    let mut inferred_lines = Vec::new();
    for f in &c.inferred_fields {
        let file = &c.files[f.file];
        inferred.push(json!({
            "record": f.record, "field": f.field, "type": f.ty.to_string(),
            "fallback": f.fallback, "file": file, "line": f.span.line,
        }));
        let why = if f.fallback { "no naming rule matched, so it is text; write the type to be sure" } else { "from the field name" };
        inferred_lines.push(format!("{}.{}: {} ({why}; {file}:{})", f.record, f.field, f.ty, f.span.line));
    }

    let mut unverified = Vec::new();
    let mut unverified_lines = Vec::new();
    for b in c.unverified_bindings() {
        let file = &c.files[b.file];
        let line = effect_line(project, b.file, &b.name);
        let proposed = b.proposed_effect.map(effect_name);
        unverified.push(json!({
            "name": b.name, "crate": b.crate_name, "file": file, "line": line,
            "proposed_effect": proposed, "items": b.items.len(),
        }));
        let to = proposed.unwrap_or("network");
        unverified_lines.push(format!(
            "{} (library {}): {file}:{line}: change \"- effect: unverified\" to \"- effect: {to}\" (or the right class) to approve",
            b.name, b.crate_name
        ));
    }

    let mut rust = Vec::new();
    let mut rust_lines = Vec::new();
    for r in &c.rust_blocks {
        let file = &c.files[r.file];
        let eff = effect_name(r.sig.effect);
        rust.push(json!({
            "name": r.name, "file": file, "line": r.span.line,
            "parent": r.parent, "effect": eff,
        }));
        let under = r.parent.as_ref().map(|p| format!(" in {p}")).unwrap_or_default();
        rust_lines.push(format!("{}{under}: declared {eff} ({file}:{})", r.name, r.span.line));
    }
    Report { units, lines, inferred, inferred_lines, unverified, unverified_lines, rust, rust_lines }
}

pub(crate) fn run(path: &str, json_out: bool, allow_unverified: bool) -> u8 {
    if let Err(c) = require_source(path) {
        return c;
    }
    let project = Project::load(path);
    let result = sema::check(&project);
    let (diags, checked) = match result {
        Ok(c) => (c.warnings.clone(), Some(c)),
        Err(d) => (d, None),
    };
    let errors = diags.iter().filter(|d| d.severity == Severity::Error).count();
    let rep = checked.as_ref().map(|c| report(&project, c));
    let unverified_count = rep.as_ref().map_or(0, |r| r.unverified.len());
    let blocked = unverified_count > 0 && !allow_unverified;
    let ok = errors == 0 && !blocked;

    if json_out {
        let empty = Vec::new();
        let doc = json!({
            "schema_version": 1,
            "ok": ok,
            "path": path,
            "diagnostics": diags.iter().map(diag_json).collect::<Vec<_>>(),
            "units": rep.as_ref().map_or(&empty, |r| &r.units),
            "inferred_fields": rep.as_ref().map_or(&empty, |r| &r.inferred),
            "unverified_bindings": rep.as_ref().map_or(&empty, |r| &r.unverified),
            "rust_blocks": rep.as_ref().map_or(&empty, |r| &r.rust),
        });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
        return u8::from(!ok);
    }

    // an unapproved library is reported once, in its own section (the JSON keeps the W005 diagnostic)
    let shown: Vec<_> = diags.iter().filter(|d| rep.is_none() || d.code != "W005").cloned().collect();
    let warnings = shown.iter().filter(|d| d.severity == Severity::Warning).count();
    if !shown.is_empty() {
        eprintln!("{}\n", project.render(&shown));
    }
    if let Some(r) = &rep {
        section("Effects (what each block can touch)", &r.lines, "  ");
        section("Field types guessed from names", &r.inferred_lines, "  ");
        section("Libraries nobody approved yet (one line each to approve)", &r.unverified_lines, "  ");
        section("Rust blocks (trusted, not checked by AgentMark)", &r.rust_lines, "  ");
    }
    if errors > 0 {
        println!("{}, {}: not ok.", plural(errors, "error"), plural(warnings, "warning"));
    } else if blocked {
        println!(
            "{}, but {unverified_count} unapproved librar{}: approve the lines above, or use --allow-unverified while developing.",
            plural(warnings, "warning"),
            if unverified_count == 1 { "y" } else { "ies" }
        );
    } else {
        println!("ok: 0 errors, {}.", plural(warnings, "warning"));
    }
    u8::from(!ok)
}

fn section(title: &str, lines: &[String], indent: &str) {
    if lines.is_empty() {
        return;
    }
    println!("{title}");
    for l in lines {
        println!("{indent}{l}");
    }
    println!();
}
