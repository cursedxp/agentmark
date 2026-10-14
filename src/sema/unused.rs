//! Unused imports and declarations, and crate imports that have no binding yet (SPEC 18.2, 15.1).

use crate::project::ResolvedImport;
use crate::syntax::ast::{Decl, ImportKind, Trigger};

use super::scope::Kind;
use super::Checker;

pub const W_UNUSED_IMPORT: &str = "W002";
pub const W_UNUSED_DECL: &str = "W003";
pub const W_UNBOUND: &str = "W004";
pub const E_UNBOUND_USED: &str = "E219";

/// Does any Rust code in this file mention the crate (`sha2::…`)?
fn crate_used_in_code(file: &crate::project::SourceFile, name: &str) -> bool {
    let needle = format!("{}::", name.replace('-', "_"));
    file.ast.decls.iter().any(|d| match d {
        Decl::RustBlock(r) => r.rust.code.contains(&needle),
        Decl::Binding(b) => b.items.iter().any(|i| i.rust.code.contains(&needle)),
        _ => false,
    })
}

pub fn check(ck: &mut Checker<'_>) {
    let p = ck.p;
    for (fi, f) in p.files.iter().enumerate() {
        for (ii, imp) in f.ast.imports.iter().enumerate() {
            let resolved = &f.imports[ii];
            match (resolved, &imp.kind) {
                (ResolvedImport::Files(v), kind) => {
                    if !v.is_empty() && !ck.used_imports.contains(&(fi, ii)) {
                        let what = match kind {
                            ImportKind::File { path, .. } | ImportKind::Folder { path, .. } => {
                                path.clone()
                            }
                            ImportKind::Glob { pattern } => pattern.clone(),
                            ImportKind::Crate { name, .. } => name.name.clone(),
                        };
                        ck.warn(
                            W_UNUSED_IMPORT,
                            fi,
                            imp.span,
                            format!("Unused import: nothing from \"{what}\" is used in this file."),
                        );
                        ck.diags.last_mut().unwrap().help = Some("Remove the import line.".into());
                    }
                }
                (ResolvedImport::Crate { name, binding }, _) => {
                    let used_code = crate_used_in_code(f, name);
                    match binding {
                        None => {
                            // OPEN_QUESTIONS Q36, Q201: nothing binds it yet. Used as a tool, that
                            // is an error on this line (and the only one); otherwise Rust code
                            // decides whether the import is unused.
                            let help = format!(
                                "Run \"agentmark bind {name}\" to create bindings/{name}.agent.md (\"agentmark build\" and \"agentmark check\" do it for you when a model is reachable)."
                            );
                            if ck.unbound_used.contains(&(fi, ii)) {
                                let d = crate::diag::Diagnostic::error(
                                    E_UNBOUND_USED,
                                    ck.path(fi),
                                    imp.span,
                                    format!("The crate \"{name}\" has no binding yet, but this file uses it."),
                                )
                                .with_help(help);
                                ck.diags.push(d);
                            } else {
                                ck.warn(
                                    W_UNBOUND,
                                    fi,
                                    imp.span,
                                    format!("The crate \"{name}\" has no binding yet."),
                                );
                                ck.diags.last_mut().unwrap().help = Some(format!(
                                    "Run \"agentmark bind {name}\" to create bindings/{name}.agent.md; \"## Rust\" blocks can already use it."
                                ));
                                if !used_code {
                                    ck.warn(
                                        W_UNUSED_IMPORT,
                                        fi,
                                        imp.span,
                                        format!("Unused import: the crate \"{name}\" is not used in this file."),
                                    );
                                }
                            }
                        }
                        Some(_) => {
                            if !used_code && !ck.used_imports.contains(&(fi, ii)) {
                                ck.warn(
                                    W_UNUSED_IMPORT,
                                    fi,
                                    imp.span,
                                    format!("Unused import: the crate \"{name}\" is not used in this file."),
                                );
                            }
                        }
                    }
                }
                (ResolvedImport::Unresolved, _) => {}
            }
        }
    }

    let unused: Vec<_> = ck
        .reg
        .ents
        .values()
        .filter(|e| {
            matches!(
                e.kind,
                Kind::Record
                    | Kind::Rule
                    | Kind::Function
                    | Kind::Constant
                    | Kind::RustBlock
                    | Kind::Binding
                    | Kind::Tool
            ) && !ck.used.contains(&e.key())
        })
        .cloned()
        .collect();
    for e in unused {
        ck.warn(
            W_UNUSED_DECL,
            e.file,
            e.span,
            format!(
                "Unused {}: nothing refers to \"{}\".",
                e.kind.noun(),
                e.name
            ),
        );
    }
    for (fi, f) in p.files.iter().enumerate() {
        for (di, d) in f.ast.decls.iter().enumerate() {
            let Decl::Agent(a) = d else { continue };
            for (bi, b) in a.blocks.iter().enumerate() {
                if let Trigger::Do(n) = &b.trigger {
                    if !ck.called_blocks.contains(&(fi, di, bi)) {
                        ck.warn(
                            W_UNUSED_DECL,
                            fi,
                            n.span,
                            format!("Unused action: no block calls \"do {}\".", n.name),
                        );
                    }
                }
            }
        }
    }
}
