//! Collection words and statements -> IR (SPEC 22.2). Types come from `Checked`; the
//! per-element expressions are built with the element bound to [`ELEM`].

use super::Builder;
use crate::ir::collections::{CollOp, CollStmt, ELEM};
use crate::ir::{self, Type};
use crate::syntax::ast;
use crate::syntax::collections as ac;

fn elem_of(t: &Type) -> Type {
    match t {
        Type::List(e) => (**e).clone(),
        _ => Type::Unknown,
    }
}

impl Builder<'_> {
    /// Build a per-element expression with the element in scope.
    fn per_element(&mut self, elem: &Type, x: &ast::Expr) -> ir::Expr {
        self.push();
        self.define(ELEM, elem.clone());
        let out = self.expr(x);
        self.pop();
        out
    }

    fn boxed(&mut self, x: &ast::Expr) -> Box<ir::Expr> {
        Box::new(self.expr(x))
    }

    pub(super) fn coll(&mut self, e: &ast::Expr, c: &ac::CollExpr, known: Option<Type>) -> ir::Expr {
        let mut ty = known.unwrap_or(Type::Unknown);
        let op = match c {
            ac::CollExpr::ListLit(items) => {
                let items: Vec<ir::Expr> = items.iter().map(|x| self.expr(x)).collect();
                if ty == Type::Unknown {
                    ty = Type::List(Box::new(items.first().map_or(Type::Unknown, |x| x.ty.clone())));
                }
                CollOp::List(items)
            }
            ac::CollExpr::EmptyList(te) => {
                ty = Type::List(Box::new(self.ty(te)));
                CollOp::List(Vec::new())
            }
            ac::CollExpr::MapLit(entries) => {
                let entries: Vec<(ir::Expr, ir::Expr)> = entries
                    .iter()
                    .map(|en| (self.expr(&en.key), self.expr(&en.value)))
                    .collect();
                if ty == Type::Unknown {
                    if let Some((k, v)) = entries.first() {
                        ty = Type::Map(Box::new(k.ty.clone()), Box::new(v.ty.clone()));
                    }
                }
                CollOp::Map(entries)
            }
            ac::CollExpr::EmptyMap { key, value } => {
                ty = Type::Map(Box::new(self.ty(key)), Box::new(self.ty(value)));
                CollOp::Map(Vec::new())
            }
            ac::CollExpr::Where { source, cond } => {
                let source = self.boxed(source);
                let cond = Box::new(self.per_element(&elem_of(&source.ty), cond));
                CollOp::Where { source, cond }
            }
            ac::CollExpr::CountWhere { source, cond } => {
                let source = self.boxed(source);
                let cond = Box::new(self.per_element(&elem_of(&source.ty), cond));
                CollOp::CountWhere { source, cond }
            }
            ac::CollExpr::MapTo { source, expr } => {
                let source = self.boxed(source);
                let expr = Box::new(self.per_element(&elem_of(&source.ty), expr));
                CollOp::MapTo { source, expr }
            }
            ac::CollExpr::Sort {
                source,
                key,
                descending,
            } => {
                let source = self.boxed(source);
                let key = Box::new(self.per_element(&elem_of(&source.ty), key));
                CollOp::Sort {
                    source,
                    key,
                    descending: *descending,
                }
            }
            ac::CollExpr::GroupBy { source, key } => {
                let source = self.boxed(source);
                let key = Box::new(self.per_element(&elem_of(&source.ty), key));
                CollOp::Group { source, key }
            }
            ac::CollExpr::Sum { expr, source } => {
                let source = self.boxed(source);
                let expr = expr
                    .as_ref()
                    .map(|x| Box::new(self.per_element(&elem_of(&source.ty), x)));
                CollOp::Sum { source, expr }
            }
            ac::CollExpr::Extreme { max, expr, source } => {
                let source = self.boxed(source);
                let expr = expr
                    .as_ref()
                    .map(|x| Box::new(self.per_element(&elem_of(&source.ty), x)));
                CollOp::Extreme {
                    max: *max,
                    source,
                    expr,
                }
            }
            ac::CollExpr::Edge { last, source } => CollOp::Edge {
                last: *last,
                source: self.boxed(source),
            },
            ac::CollExpr::Slice {
                skip,
                count,
                source,
            } => CollOp::Slice {
                skip: *skip,
                count: self.boxed(count),
                source: self.boxed(source),
            },
            ac::CollExpr::Unique { expr, source } => {
                let source = self.boxed(source);
                let expr = expr
                    .as_ref()
                    .map(|x| Box::new(self.per_element(&elem_of(&source.ty), x)));
                CollOp::Unique { source, expr }
            }
            ac::CollExpr::Join { source, sep } => CollOp::Join {
                source: self.boxed(source),
                sep: self.boxed(sep),
            },
            ac::CollExpr::At { map, key } => CollOp::At {
                map: self.boxed(map),
                key: self.boxed(key),
            },
            ac::CollExpr::MapPart { values, map } => CollOp::MapPart {
                values: *values,
                map: self.boxed(map),
            },
        };
        self.mke(e, ir::ExprKind::Coll(Box::new(op)), ty)
    }

    pub(super) fn coll_stmt(&mut self, c: &ac::CollStmt) -> ir::StmtKind {
        let kind = match c {
            ac::CollStmt::Put { value, key, map } => CollStmt::Put {
                value: self.expr(value),
                key: self.expr(key),
                map: self.expr(map),
            },
            ac::CollStmt::Remove { key, map } => CollStmt::Remove {
                key: self.expr(key),
                map: self.expr(map),
            },
            ac::CollStmt::ForEachIndex {
                var,
                index,
                source,
                body,
            } => {
                let source = self.expr(source);
                let elem = elem_of(&source.ty);
                self.push();
                self.define(&var.name, elem.clone());
                self.define(&index.name, Type::Int);
                let body = self.stmts(body);
                self.pop();
                CollStmt::ForEachIndex {
                    var: ir::LocalVar {
                        name: var.name.clone(),
                        ty: elem,
                    },
                    index: ir::LocalVar {
                        name: index.name.clone(),
                        ty: Type::Int,
                    },
                    source,
                    body,
                }
            }
            ac::CollStmt::ForEachEntry {
                key,
                value,
                source,
                body,
            } => {
                let source = self.expr(source);
                let (k, v) = match &source.ty {
                    Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                    _ => (Type::Unknown, Type::Unknown),
                };
                self.push();
                self.define(&key.name, k.clone());
                self.define(&value.name, v.clone());
                let body = self.stmts(body);
                self.pop();
                CollStmt::ForEachEntry {
                    key: ir::LocalVar {
                        name: key.name.clone(),
                        ty: k,
                    },
                    value: ir::LocalVar {
                        name: value.name.clone(),
                        ty: v,
                    },
                    source,
                    body,
                }
            }
        };
        ir::StmtKind::Coll(kind)
    }
}
