//! `agentmark doc` (SPEC 18.7): the `>` documentation comments of a project as one readable
//! Markdown page, each unit with its effect summary; or the cheat sheet of a bound library.

use std::fmt::Write as _;
use std::path::Path;

use super::files::in_bindings;
use super::{exists, fail, require_source};
use crate::binder::doc_cheatsheet;
use crate::project::{Project, ProjectKind};
use crate::sema::{self, Checked, EffectSet};
use crate::syntax::ast::Decl;

pub(crate) fn run(target: Option<&str>, project_dir: &str) -> u8 {
    match target {
        Some(t) if !exists(t) => crate_sheet(t, project_dir),
        t => project_page(t.unwrap_or(".")),
    }
}

/// `agentmark doc <crate>`: the binder's cheat sheet for `bindings/<crate>.agent.md`.
fn crate_sheet(name: &str, project_dir: &str) -> u8 {
    if name.ends_with(".agent.md") || name.contains('/') {
        return fail(format!("cannot find \"{name}\""));
    }
    for dir in ["bindings", ".agentmark/bindings"] {
        let p = Path::new(project_dir).join(dir).join(format!("{name}.agent.md"));
        if p.is_file() {
            return match doc_cheatsheet(&p) {
                Ok(s) => {
                    print!("{s}");
                    0
                }
                Err(e) => fail(e),
            };
        }
    }
    fail(format!(
        "\"{name}\" is not a file or folder, and there is no binding for a library of that name in \
         \"{project_dir}/bindings\". Run \"agentmark bind {name}\" first."
    ))
}

fn doc_block(out: &mut String, doc: &Option<String>) {
    if let Some(d) = doc {
        let _ = writeln!(out, "{}\n", d.trim());
    }
}

fn project_page(path: &str) -> u8 {
    if let Err(c) = require_source(path) {
        return c;
    }
    let project = Project::load(path);
    let checked = match sema::check(&project) {
        Ok(c) => c,
        Err(d) => {
            eprintln!("{}\n", project.render(&d));
            eprintln!("The project has errors, so its documentation cannot be written yet.");
            return 1;
        }
    };
    print!("{}", page(&project, &checked));
    0
}

fn page(project: &Project, c: &Checked) -> String {
    let mut out = String::new();
    let title = match (&project.config.name, project.kind) {
        (Some(n), _) => n.clone(),
        (None, ProjectKind::SingleFile) => project
            .files
            .first()
            .map(|f| {
                let name = f.path.rsplit('/').next().unwrap_or(&f.path);
                name.strip_suffix(".agent.md").unwrap_or(name).to_string()
            })
            .unwrap_or_else(|| project.root.clone()),
        (None, ProjectKind::Folder) => project.root.clone(),
    };
    let _ = writeln!(out, "# {title}\n");
    let files: Vec<_> = project.files.iter().filter(|f| !in_bindings(&f.path)).collect();
    for f in &files {
        if let Some(d) = &f.ast.doc {
            let _ = writeln!(out, "{}\n", d.trim());
        }
    }
    section(&mut out, "Agents", &files, |out, d| {
        let Decl::Agent(a) = d else { return false };
        let _ = writeln!(out, "### {}\n", a.name.name);
        doc_block(out, &a.doc);
        let tools: Vec<_> = a.tools.iter().map(|t| t.name.as_str()).collect();
        let _ = writeln!(out, "- model: {}\n- tools: {}\n- can touch: {}\n", a.model.value, tools.join(", "), touch(&c.unit_effect(&a.name.name)));
        for b in &a.blocks {
            let eff = block_effect(c, &a.name.name, &b.label);
            let _ = writeln!(out, "**{}** (can touch: {eff})\n", b.label);
            doc_block(out, &b.doc);
        }
        true
    });
    section(&mut out, "Services", &files, |out, d| {
        let Decl::Service(s) = d else { return false };
        let _ = writeln!(out, "### {}\n", s.name.name);
        doc_block(out, &s.doc);
        let _ = writeln!(out, "- can touch: {}\n", touch(&c.unit_effect(&s.name.name)));
        for r in &s.routes {
            let label = format!("{} {}", r.method, r.path);
            let eff = block_effect(c, &s.name.name, &label);
            let _ = writeln!(out, "**{label}** (can touch: {eff})\n");
            doc_block(out, &r.doc);
        }
        true
    });
    section(&mut out, "Commands", &files, |out, d| {
        let Decl::Command(x) = d else { return false };
        let _ = writeln!(out, "### {}\n", x.name.name);
        doc_block(out, &x.doc);
        let _ = writeln!(out, "- can touch: {}\n", touch(&c.unit_effect(&x.name.name)));
        true
    });
    section(&mut out, "Tools", &files, |out, d| {
        let Decl::Tool(t) = d else { return false };
        let _ = writeln!(out, "### {}\n", t.name.name);
        doc_block(out, &t.doc);
        let _ = writeln!(out, "- declared effect: {}", crate::sema::effects::effect_name(t.effect.value));
        for v in &t.verbs {
            let params: Vec<String> = v.params.iter().map(|p| p.name.name.clone()).collect();
            let _ = writeln!(out, "- **{}**({}) (can touch: {})", v.name.name, params.join(", "), block_effect(c, &t.name.name, &v.name.name));
        }
        out.push('\n');
        true
    });
    section(&mut out, "Records", &files, |out, d| {
        let Decl::Record(r) = d else { return false };
        let _ = writeln!(out, "### {}\n", r.name.name);
        doc_block(out, &r.doc);
        if let Some(info) = c.records.get(&r.name.name) {
            for f in &info.fields {
                let _ = writeln!(out, "- {}: {}", f.name, f.ty);
            }
            out.push('\n');
        }
        true
    });
    section(&mut out, "Enums", &files, |out, d| {
        let Decl::Enum(e) = d else { return false };
        let _ = writeln!(out, "### {}\n", e.name.name);
        doc_block(out, &e.doc);
        if let Some(info) = c.enums.get(&e.name.name) {
            for v in &info.variants {
                if v.fields.is_empty() {
                    let _ = writeln!(out, "- {}", v.name);
                } else {
                    let fields: Vec<String> = v.fields.iter().map(|f| format!("{}: {}", f.name, f.ty)).collect();
                    let _ = writeln!(out, "- {}: {}", v.name, fields.join(", "));
                }
            }
            out.push('\n');
        }
        true
    });
    section(&mut out, "Types", &files, |out, d| {
        let Decl::TypeAlias(t) = d else { return false };
        let _ = writeln!(out, "### {}\n", t.name.name);
        doc_block(out, &t.doc);
        if let Some(info) = c.aliases.get(&t.name.name) {
            let _ = writeln!(out, "Another name for `{}`.\n", info.ty);
        }
        true
    });
    section(&mut out, "Rules", &files, |out, d| {
        let Decl::Rule(r) = d else { return false };
        let _ = writeln!(out, "### {}\n", r.name.name);
        doc_block(out, &r.doc);
        let _ = writeln!(out, "Applies to {}. Always pure.\n", r.for_record.name);
        true
    });
    section(&mut out, "Functions", &files, |out, d| {
        let Decl::Function(f) = d else { return false };
        let _ = writeln!(out, "### {}\n", f.name.name);
        doc_block(out, &f.doc);
        let eff = c.unit_effect(&f.name.name);
        let _ = writeln!(out, "- can touch: {}\n", touch(&eff));
        true
    });
    out
}

/// `pure` reads as a fact about the code, not as an effect.
fn touch(e: &EffectSet) -> String {
    if e.is_pure() {
        "nothing outside itself (pure)".to_string()
    } else {
        e.to_string()
    }
}

fn block_effect(c: &Checked, unit: &str, label: &str) -> String {
    c.blocks
        .iter()
        .find(|b| b.unit == unit && b.label == label)
        .map(|b| touch(&b.effect))
        .unwrap_or_else(|| touch(&EffectSet::pure()))
}

/// Print `## title` once if at least one declaration of that kind exists.
fn section(
    out: &mut String,
    title: &str,
    files: &[&crate::project::SourceFile],
    mut each: impl FnMut(&mut String, &Decl) -> bool,
) {
    let mut body = String::new();
    for f in files {
        for d in &f.ast.decls {
            each(&mut body, d);
        }
    }
    if !body.is_empty() {
        let _ = writeln!(out, "## {title}\n\n{body}");
    }
}
