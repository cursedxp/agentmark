//! User-defined types (SPEC 22.1): `# Enum`, `# Type` aliases, enum values and `match`.
//!
//! Error codes E300-E319 (OPEN_QUESTIONS Q300-Q319):
//!
//! | code | meaning |
//! |---|---|
//! | E300 | a record, enum or type with the name another one has (`scope.rs`) |
//! | E301 | an enum without variants |
//! | E302 | a variant written twice in one enum |
//! | E303 | a variant field written twice, or named `kind` |
//! | E304 | a type alias defined in terms of itself |
//! | E305 | a variant (written in a value or a branch) that does not exist |
//! | E306 | a variant value with wrong fields: missing, unknown, wrong type, or fields where it has none |
//! | E307 | a variant name that several visible enums have |
//! | E308 | `match` on something that is not an enum, a `maybe` or a `result` |
//! | E309 | a branch that does not belong to what is matched |
//! | E310 | a `match` that leaves a variant unhandled |
//! | E311 | the same branch twice |
//! | E312 | a branch that names a value it does not have (or leaves one unnamed) |
//! | E313 | a `result` that is matched somewhere other than at the call that makes it |
//! | E314 | an enum value written into text as a whole |
//! | E315 | an enum that would contain itself without end (no `list of` in between) |
//! | E317 | `none` whose type nothing decides |
//! | E316 | the name of a `match` branch used as a value instead of through one of its fields |
//! | W300 | an `otherwise:` that nothing can reach |

use std::collections::HashSet;

use crate::diag::{did_you_mean, Diagnostic, Span};
use crate::syntax::ast::{Decl, Expr, ExprKind, Ident, MatchArm, Otherwise, RecordField, Stmt};

use super::body::Cx;
use super::scope::Kind;
use super::types::Type;
use super::{AliasInfo, Checker, EnumInfo, FieldInfo, NodeKey, Pattern, Resolution, VariantInfo};

pub const E_NO_VARIANTS: &str = "E301";
pub const E_DUP_VARIANT: &str = "E302";
pub const E_VARIANT_FIELD: &str = "E303";
pub const E_ALIAS_CYCLE: &str = "E304";
pub const E_UNKNOWN_VARIANT: &str = "E305";
pub const E_VARIANT_VALUE: &str = "E306";
pub const E_AMBIGUOUS: &str = "E307";
pub const E_MATCH_TYPE: &str = "E308";
pub const E_BRANCH: &str = "E309";
pub const E_NOT_EXHAUSTIVE: &str = "E310";
pub const E_DUP_BRANCH: &str = "E311";
pub const E_BINDER: &str = "E312";
pub const E_RESULT_MATCH: &str = "E313";
pub const E_WHOLE_ENUM: &str = "E314";
pub const E_RECURSIVE: &str = "E315";
pub const E_PAYLOAD_VALUE: &str = "E316";
pub const E_UNTYPED_NONE: &str = "E317";
pub const W_UNREACHABLE: &str = "W300";

/// The JSON tag of an enum value (`{"kind":"Circle","radius":3}`); a variant field cannot use it.
pub const TAG: &str = "kind";

/// A variant found by name: which enum and which of its variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub enum_name: String,
    pub variant: usize,
}

/// "a" / "a and b" / "a, b and c" with every word in quotes.
fn quoted_list(names: &[String]) -> String {
    let q: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
    match q.len() {
        0 => String::new(),
        1 => q[0].clone(),
        n => format!("{} and {}", q[..n - 1].join(", "), q[n - 1]),
    }
}

impl Checker<'_> {
    // ------------------------------------------------------------------------ declarations

    /// The type a `# Type` alias stands for, resolved in the alias's own file.
    pub(super) fn alias_type(&mut self, name: &str) -> Type {
        match self.alias_state.get(name) {
            Some(Some(t)) => return t.clone(),
            Some(None) => {
                if let Some(e) = self.reg.get(Kind::TypeAlias, name).cloned() {
                    self.error(
                        E_ALIAS_CYCLE,
                        e.file,
                        e.span,
                        format!("The type \"{name}\" is defined in terms of itself."),
                    );
                }
                return Type::Unknown;
            }
            None => {}
        }
        let Some(ent) = self.reg.get(Kind::TypeAlias, name).cloned() else {
            return Type::Unknown;
        };
        self.alias_state.insert(name.to_string(), None);
        let p = self.p;
        let Decl::TypeAlias(a) = &p.files[ent.file].ast.decls[ent.decl] else {
            return Type::Unknown;
        };
        let ty = self.resolve_type(ent.file, &a.target);
        self.alias_state.insert(name.to_string(), Some(ty.clone()));
        self.alias_infos.insert(
            name.to_string(),
            AliasInfo {
                name: name.to_string(),
                file: ent.file,
                span: a.name.span,
                ty: ty.clone(),
            },
        );
        ty
    }

    /// Every alias is resolved, used or not, so a bad one is reported.
    pub(super) fn declare_aliases(&mut self) {
        let names: Vec<String> = self
            .reg
            .of_kind(Kind::TypeAlias)
            .map(|e| e.name.clone())
            .collect();
        for n in names {
            self.alias_type(&n);
        }
    }

    pub(super) fn declare_enums(&mut self) {
        let p = self.p;
        for (fi, f) in p.files.iter().enumerate() {
            for d in &f.ast.decls {
                let Decl::Enum(e) = d else { continue };
                if self.enums.contains_key(&e.name.name) {
                    continue; // duplicate, already reported
                }
                if e.variants.is_empty() {
                    let d = Diagnostic::error(
                        E_NO_VARIANTS,
                        self.path(fi),
                        e.name.span,
                        format!("The enum \"{}\" has no variants.", e.name.name),
                    )
                    .with_help("List the variants under the heading, like \"- Circle: radius: int\" or \"- Empty\".");
                    self.diags.push(d);
                }
                let mut variants: Vec<VariantInfo> = Vec::new();
                for v in &e.variants {
                    if variants.iter().any(|x| x.name == v.name.name) {
                        self.error(
                            E_DUP_VARIANT,
                            fi,
                            v.name.span,
                            format!(
                                "The enum \"{}\" has the variant \"{}\" twice.",
                                e.name.name, v.name.name
                            ),
                        );
                        continue;
                    }
                    let mut fields: Vec<FieldInfo> = Vec::new();
                    for fld in &v.fields {
                        if fld.name.name == TAG {
                            let d = Diagnostic::error(
                                E_VARIANT_FIELD,
                                self.path(fi),
                                fld.name.span,
                                format!(
                                    "A variant cannot have a field called \"{TAG}\": the JSON of an enum value uses it to say which variant it is."
                                ),
                            )
                            .with_help("Pick another name, for example \"type\" or \"label\".");
                            self.diags.push(d);
                            continue;
                        }
                        if fields.iter().any(|x| x.name == fld.name.name) {
                            self.error(
                                E_VARIANT_FIELD,
                                fi,
                                fld.name.span,
                                format!(
                                    "The variant \"{}\" has the field \"{}\" twice.",
                                    v.name.name, fld.name.name
                                ),
                            );
                            continue;
                        }
                        let ty = match &fld.ty {
                            Some(te) => self.resolve_type(fi, te),
                            None => Type::Unknown,
                        };
                        fields.push(FieldInfo {
                            name: fld.name.name.clone(),
                            ty,
                            inferred: false,
                            span: fld.name.span,
                        });
                    }
                    variants.push(VariantInfo {
                        name: v.name.name.clone(),
                        span: v.name.span,
                        fields,
                    });
                }
                self.enums.insert(
                    e.name.name.clone(),
                    EnumInfo {
                        name: e.name.name.clone(),
                        file: fi,
                        span: e.name.span,
                        variants,
                    },
                );
            }
        }
    }

    // ------------------------------------------------------------------------ lookup

    /// Mark an enum (and the import that brought it) as used.
    fn use_enum(&mut self, file: usize, visible: &str) {
        let _ = self.lookup(file, visible, &[Kind::Enum]);
    }

    /// Every variant name that `file` can write bare, with the enum it belongs to.
    fn visible_variants(&self, file: usize) -> Vec<(String, String, String)> {
        // (visible enum name, enum name, variant name)
        let mut out = Vec::new();
        for visible in self.scopes[file].names_of(&[Kind::Enum]) {
            let Some(((_, name), _)) = self.scopes[file].find(&visible, &[Kind::Enum]) else {
                continue;
            };
            if let Some(e) = self.enums.get(&name) {
                for v in &e.variants {
                    out.push((visible.clone(), name.clone(), v.name.clone()));
                }
            }
        }
        out
    }

    /// Names to suggest for a misspelled variant written in `file`.
    pub(super) fn variant_candidates(&self, file: usize) -> Vec<String> {
        let mut v: Vec<String> = self
            .visible_variants(file)
            .into_iter()
            .map(|(_, _, variant)| variant)
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Find the variant written as `qualifier.name` (or bare `name`) in `file`. `Ok(None)`: no
    /// such variant (the caller decides whether that is an error); `Err(())`: it exists in several
    /// enums, already reported at `span`.
    pub(super) fn find_variant(
        &mut self,
        file: usize,
        qualifier: Option<&str>,
        name: &str,
        span: Span,
    ) -> Result<Option<Found>, ()> {
        let visible = self.visible_variants(file);
        let mut hits: Vec<(String, String)> = Vec::new(); // (visible enum name, enum)
        match qualifier {
            Some(q) => {
                // `Shape.Empty`: the qualifier names an enum
                if let Some((_, enum_name)) = self.lookup(file, q, &[Kind::Enum]) {
                    if self.enum_variant(&enum_name, name).is_some() {
                        hits.push((q.to_string(), enum_name));
                    }
                } else {
                    // `billing.Empty`: the qualifier is the module an enum was imported under
                    let prefix = format!("{q}.");
                    for (vis, en, vn) in &visible {
                        if vn == name && vis.starts_with(&prefix) && !vis[prefix.len()..].contains('.') {
                            hits.push((vis.clone(), en.clone()));
                        }
                    }
                }
            }
            None => {
                for (vis, en, vn) in &visible {
                    if vn == name {
                        hits.push((vis.clone(), en.clone()));
                    }
                }
            }
        }
        hits.dedup_by(|a, b| a.1 == b.1);
        match hits.as_slice() {
            [] => Ok(None),
            [(vis, en)] => {
                self.use_enum(file, vis);
                let idx = self.enum_variant(en, name).expect("checked above");
                Ok(Some(Found {
                    enum_name: en.clone(),
                    variant: idx,
                }))
            }
            many => {
                let spelled: Vec<String> = many.iter().map(|(vis, _)| format!("{vis}.{name}")).collect();
                let d = Diagnostic::error(
                    E_AMBIGUOUS,
                    self.path(file),
                    span,
                    format!(
                        "\"{name}\" is a variant of several enums here: {}.",
                        spelled.join(", ")
                    ),
                )
                .with_help(format!("Say which one you mean, for example \"{}\".", spelled[0]));
                self.diags.push(d);
                Err(())
            }
        }
    }

    fn enum_variant(&self, enum_name: &str, variant: &str) -> Option<usize> {
        self.enums
            .get(enum_name)?
            .variants
            .iter()
            .position(|v| v.name == variant)
    }

    /// Fields of the variant `variant` of `enum_name`, if both exist.
    pub fn variant_fields(&self, enum_name: &str, variant: &str) -> Option<Vec<(String, Type)>> {
        let e = self.enums.get(enum_name)?;
        let v = e.variants.iter().find(|v| v.name == variant)?;
        Some(v.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect())
    }

    // ------------------------------------------------------------------------ values

    /// A bare or qualified path that names a field-less variant. `None`: the path is not a
    /// variant at all (the caller reports the unknown name).
    pub(super) fn variant_path(&mut self, cx: &Cx, segs: &[Ident], key: NodeKey) -> Option<Type> {
        let (last, rest) = segs.split_last()?;
        let qualifier = (!rest.is_empty()).then(|| join(rest));
        let span = segs[0].span.to(last.span);
        let found = match self.find_variant(cx.file, qualifier.as_deref(), &last.name, span) {
            Ok(Some(f)) => f,
            Ok(None) => {
                // `Shape.Emtpy`: the enum is right and the variant is not
                if let Some(q) = &qualifier {
                    if self.scopes[cx.file].find(q, &[Kind::Enum]).is_some() {
                        self.unknown_variant(cx.file, Some(q), last);
                        return Some(Type::Unknown);
                    }
                }
                return None;
            }
            Err(()) => return Some(Type::Unknown),
        };
        let e = &self.enums[&found.enum_name];
        let v = &e.variants[found.variant];
        if !v.fields.is_empty() {
            let names: Vec<String> = v.fields.iter().map(|f| format!("{}: …", f.name)).collect();
            let d = Diagnostic::error(
                E_VARIANT_VALUE,
                self.path(cx.file),
                span,
                format!(
                    "The variant \"{}\" of {} has fields, so it is written with them.",
                    v.name, found.enum_name
                ),
            )
            .with_help(format!("Write \"{} {{ {} }}\".", v.name, names.join(", ")));
            self.diags.push(d);
        }
        let (enum_name, variant) = (found.enum_name.clone(), v.name.clone());
        self.resolutions.insert(
            key,
            Resolution::Variant {
                enum_name: enum_name.clone(),
                variant,
            },
        );
        Some(Type::Enum(enum_name))
    }

    /// `Circle { radius: 3 }`
    pub(super) fn variant_value(
        &mut self,
        cx: &mut Cx,
        e: &Expr,
        qualifier: &Option<Ident>,
        name: &Ident,
        fields: &[RecordField],
    ) -> Type {
        // the values are checked whatever happens to the name
        let given: Vec<(&RecordField, Type)> = fields
            .iter()
            .map(|f| {
                let t = self.expr(cx, &f.value);
                (f, t)
            })
            .collect();
        let q = qualifier.as_ref().map(|q| q.name.clone());
        let span = match qualifier {
            Some(q) => q.span.to(name.span),
            None => name.span,
        };
        let found = match self.find_variant(cx.file, q.as_deref(), &name.name, span) {
            Ok(Some(f)) => f,
            Ok(None) => {
                if q.is_none() {
                    if let Some(t) = self.record_value(cx, e, name, &given) {
                        return t;
                    }
                }
                self.unknown_variant(cx.file, q.as_deref(), name);
                return Type::Unknown;
            }
            Err(()) => return Type::Unknown,
        };
        let enum_name = found.enum_name.clone();
        let v = self.enums[&enum_name].variants[found.variant].clone();
        let what = format!("variant \"{}\"", v.name);
        self.check_fields(cx, e, &what, &v.name, &v.fields, &given);
        self.resolutions.insert(
            NodeKey::new(cx.file, e.span),
            Resolution::Variant {
                enum_name: enum_name.clone(),
                variant: v.name.clone(),
            },
        );
        Type::Enum(enum_name)
    }

    /// The fields given in `Name { … }` against the declared ones (a variant or a record).
    pub(super) fn check_fields(
        &mut self,
        cx: &Cx,
        e: &Expr,
        what: &str,
        owner: &str,
        decl: &[FieldInfo],
        given: &[(&RecordField, Type)],
    ) {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut misspelled = false;
        for (f, t) in given {
            if !seen.insert(f.name.name.as_str()) {
                self.error(
                    E_VARIANT_VALUE,
                    cx.file,
                    f.name.span,
                    format!("The field \"{}\" is given twice.", f.name.name),
                );
                continue;
            }
            match decl.iter().find(|x| x.name == f.name.name) {
                None => {
                    misspelled = true;
                    let d = Diagnostic::error(
                        E_VARIANT_VALUE,
                        self.path(cx.file),
                        f.name.span,
                        if decl.is_empty() {
                            format!("The {what} has no fields.")
                        } else {
                            format!("The {what} has no field \"{}\".", f.name.name)
                        },
                    )
                    .suggesting(&f.name.name, decl.iter().map(|x| x.name.as_str()));
                    self.diags.push(d);
                }
                Some(want) => {
                    if !self.is_assignable(t, &want.ty) {
                        let d = Diagnostic::error(
                            super::expr::E_TYPE,
                            self.path(cx.file),
                            f.value.span,
                            format!(
                                "The field \"{}\" of {owner} is {}, but this is {}.{}",
                                f.name.name,
                                want.ty,
                                t.with_article(),
                                self.note(t, &want.ty)
                            ),
                        );
                        self.diags.push(d);
                    }
                }
            }
        }
        let missing: Vec<String> = decl
            .iter()
            .filter(|x| !x.ty.is_maybe() && !given.iter().any(|(f, _)| f.name.name == x.name))
            .map(|x| x.name.clone())
            .collect();
        // a misspelled field is the likelier mistake; say only that
        if !missing.is_empty() && !misspelled {
            let d = Diagnostic::error(
                E_VARIANT_VALUE,
                self.path(cx.file),
                e.span,
                format!(
                    "The {what} needs {}: {}.",
                    if missing.len() == 1 { "a value for" } else { "values for" },
                    quoted_list(&missing)
                ),
            );
            self.diags.push(d);
        }
    }

    /// `Sale { region: "EU", … }`: a record built field by field (same syntax as a variant).
    fn record_value(&mut self, cx: &Cx, e: &Expr, name: &Ident, given: &[(&RecordField, Type)]) -> Option<Type> {
        let (_, record) = self.lookup(cx.file, &name.name, &[Kind::Record])?;
        let fields = self.records.get(&record)?.fields.clone();
        let what = format!("record \"{}\"", name.name);
        self.check_fields(cx, e, &what, &name.name, &fields, given);
        self.resolutions.insert(NodeKey::new(cx.file, e.span), Resolution::Record { record: record.clone() });
        Some(Type::Record(record))
    }

    fn unknown_variant(&mut self, file: usize, qualifier: Option<&str>, name: &Ident) {
        match qualifier {
            Some(q) => {
                if let Some((_, en)) = self.lookup(file, q, &[Kind::Enum]) {
                    let names: Vec<String> = self.enums[&en].variants.iter().map(|v| v.name.clone()).collect();
                    let d = Diagnostic::error(
                        E_UNKNOWN_VARIANT,
                        self.path(file),
                        name.span,
                        format!("{en} has no variant \"{}\".", name.name),
                    )
                    .suggesting(&name.name, names.iter().map(String::as_str));
                    self.diags.push(d);
                } else {
                    let cands = self.scopes[file].names_of(&[Kind::Enum]);
                    let d = Diagnostic::error(
                        E_UNKNOWN_VARIANT,
                        self.path(file),
                        name.span,
                        format!("Unknown enum \"{q}\"."),
                    )
                    .with_did_you_mean(super::scope::suggest(q, &cands))
                    .with_help("A variant is written bare, or after the enum it belongs to: Shape.Empty.");
                    self.diags.push(d);
                }
            }
            None => {
                let mut cands = self.variant_candidates(file);
                cands.extend(self.scopes[file].names_of(&[Kind::Record]));
                let d = Diagnostic::error(
                    E_UNKNOWN_VARIANT,
                    self.path(file),
                    name.span,
                    format!("Unknown variant \"{}\".", name.name),
                )
                .with_did_you_mean(super::scope::suggest(&name.name, &cands))
                .with_help("Variants come from the \"# Enum\" declarations this file declares or imports.");
                self.diags.push(d);
            }
        }
    }

    /// A stored `result` read anywhere but as the subject of a `match` (E313).
    pub(super) fn result_outside_match(&mut self, cx: &Cx, span: Span, segs: &[Ident], t: &Type) -> Type {
        if !matches!(t, Type::Result(_)) || cx.match_call == Some(span) {
            return t.clone();
        }
        let d = Diagnostic::error(
            E_RESULT_MATCH,
            self.path(cx.file),
            segs[0].span,
            format!("\"{}\" holds a result, which can only be matched.", segs[0].name),
        )
        .with_help(format!(
            "Write \"- match {}:\" with the branches \"ok value\" and \"error problem\".",
            segs[0].name
        ));
        self.diags.push(d);
        Type::Unknown
    }

    /// `none` whose type nothing decides (`set d = none`) would not compile.
    pub(super) fn untyped_none(&mut self, cx: &Cx, span: Span, t: &Type) {
        if matches!(t, Type::Maybe(i) if i.is_unknown()) {
            let d = Diagnostic::error(
                E_UNTYPED_NONE,
                self.path(cx.file),
                span,
                "This \"none\" does not say what it is a \"none\" of.",
            )
            .with_help("Use \"none\" where a \"maybe\" value is expected, for example \"return none\" in a function that returns \"maybe int\".");
            self.diags.push(d);
        }
    }

    /// An enum value written into text as a whole.
    pub(super) fn whole_enum_in_text(&mut self, cx: &Cx, span: Span, t: &Type) {
        let Type::Enum(n) = t else { return };
        let d = Diagnostic::error(
            E_WHOLE_ENUM,
            self.path(cx.file),
            span,
            format!("A {n} cannot be written into text as a whole."),
        )
        .with_help("Use \"- match <value>:\" and write what each variant carries.");
        self.diags.push(d);
    }

    /// The name of a branch (`c` of `Circle c:`) used as a value: it only gives access to the
    /// fields of its variant.
    pub(super) fn payload_as_value(&mut self, cx: &Cx, segs: &[Ident], t: &Type) -> Type {
        let Type::Variant(e, v) = t else { return t.clone() };
        if segs.len() != 1 {
            return t.clone();
        }
        let first = self.enums.get(e).and_then(|en| en.variants.iter().find(|x| x.name == *v)).and_then(|x| x.fields.first()).map(|f| f.name.clone());
        let d = Diagnostic::error(
            E_PAYLOAD_VALUE,
            self.path(cx.file),
            segs[0].span,
            format!("\"{}\" stands for the fields of {v}, not for a value of {e}.", segs[0].name),
        )
        .with_help(match first {
            Some(f) => format!("Read a field, like {}.{f}.", segs[0].name),
            None => "Read one of its fields.".to_string(),
        });
        self.diags.push(d);
        Type::Unknown
    }

    /// An enum may not hold itself (directly, in a `maybe`, or through a record or another enum)
    /// without a `list of` in between: the generated program could not store it.
    pub(super) fn check_recursive_types(&mut self) {
        // what a type holds inline (a `list of` keeps its elements on the heap)
        fn inline(t: &Type, out: &mut Vec<(bool, String)>) {
            match t {
                Type::Enum(n) => out.push((true, n.clone())),
                Type::Record(n) => out.push((false, n.clone())),
                Type::Maybe(i) | Type::Result(i) => inline(i, out),
                _ => {}
            }
        }
        let holds = |this: &Self, node: &(bool, String)| -> Vec<(bool, String)> {
            let mut out = Vec::new();
            if node.0 {
                if let Some(e) = this.enums.get(&node.1) {
                    e.variants.iter().flat_map(|v| &v.fields).for_each(|f| inline(&f.ty, &mut out));
                }
            } else if let Some(r) = this.records.get(&node.1) {
                r.fields.iter().for_each(|f| inline(&f.ty, &mut out));
            }
            out
        };
        let names: Vec<String> = self.enums.keys().cloned().collect();
        for name in names {
            let me = (true, name.clone());
            let e = self.enums[&name].clone();
            'variants: for v in &e.variants {
                for f in &v.fields {
                    let mut start = Vec::new();
                    inline(&f.ty, &mut start);
                    // can `me` be reached from what this field holds?
                    let mut seen: Vec<(bool, String)> = Vec::new();
                    let mut todo = start;
                    while let Some(n) = todo.pop() {
                        if n == me {
                            self.error(
                                E_RECURSIVE,
                                e.file,
                                f.span,
                                format!(
                                    "The variant \"{}\" of {name} holds a {name} inside itself, which would never end.",
                                    v.name
                                ),
                            );
                            self.diags.last_mut().unwrap().help = Some(format!(
                                "Keep the children in a list instead, like \"{}: list of {name}\".",
                                f.name
                            ));
                            continue 'variants;
                        }
                        if !seen.contains(&n) {
                            todo.extend(holds(self, &n));
                            seen.push(n);
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------------ match

    /// `match value:` with its branches (SPEC 22.1).
    pub(super) fn match_stmt(
        &mut self,
        cx: &mut Cx,
        s: &Stmt,
        scrutinee: &Expr,
        arms: &[MatchArm],
        otherwise: &Option<Otherwise>,
    ) {
        // a `result of T` function is called right here, without `try`: the match looks at both
        // outcomes (OPEN_QUESTIONS Q300)
        let saved = cx.match_call.take();
        if is_call_or_name(scrutinee) {
            cx.match_call = Some(scrutinee.span);
        }
        let t = self.expr(cx, scrutinee);
        cx.match_call = saved;
        self.check_limit(cx, s);

        let kind = match &t {
            Type::Enum(e) => Subject::Enum(e.clone()),
            Type::Maybe(i) => Subject::Maybe((**i).clone()),
            Type::Result(i) => {
                if is_call_or_name(scrutinee) {
                    Subject::Result((**i).clone())
                } else {
                    let d = Diagnostic::error(
                        E_RESULT_MATCH,
                        self.path(cx.file),
                        scrutinee.span,
                        "A result can only be matched: it cannot be read like other values.",
                    )
                    .with_help("Write the call or the variable that holds it after \"match\", with the branches \"ok value\" and \"error problem\".");
                    self.diags.push(d);
                    Subject::Unknown
                }
            }
            Type::Unknown => Subject::Unknown,
            other => {
                let d = Diagnostic::error(
                    E_MATCH_TYPE,
                    self.path(cx.file),
                    scrutinee.span,
                    format!(
                        "\"match\" works on an enum, a \"maybe\" or a \"result\", but this is {}.",
                        other.with_article()
                    ),
                )
                .with_help("For a yes/no or a comparison, use \"if\".");
                self.diags.push(d);
                Subject::Unknown
            }
        };

        let mut handled: Vec<String> = Vec::new(); // variants / patterns seen, by name
        let mut had_unknown = false;
        // what a branch used up (handles, a matched result) is used up afterwards when that
        // branch goes on; a branch that leaves contributes nothing (Q362)
        let before = cx.consumed.clone();
        let mut merged = before.clone();
        for arm in arms {
            cx.consumed = before.clone();
            let (pattern, bind_ty) = self.arm_pattern(cx, arm, &kind, &mut handled, &mut had_unknown);
            if let Some(p) = pattern {
                self.patterns.insert(NodeKey::new(cx.file, arm.span), p);
            }
            cx.push();
            if let (Some(b), Some(ty)) = (&arm.bind, bind_ty) {
                cx.top().vars.insert(b.name.clone(), ty);
                cx.top().defs.insert(b.name.clone(), b.span);
            }
            self.stmts(cx, &arm.body);
            cx.pop();
            if !super::stmt::terminates(&arm.body) {
                for (k, v) in cx.consumed.clone() {
                    merged.entry(k).or_insert(v);
                }
            }
        }
        if let Some(o) = otherwise {
            cx.consumed = before.clone();
            cx.push();
            self.stmts(cx, &o.body);
            cx.pop();
            if !super::stmt::terminates(&o.body) {
                for (k, v) in cx.consumed.clone() {
                    merged.entry(k).or_insert(v);
                }
            }
        }
        cx.consumed = merged;
        // a stored result is moved into the match: it cannot be matched or read again
        if let (Type::Result(_), ExprKind::Path(segs)) = (&t, &scrutinee.kind) {
            if segs.len() == 1 {
                cx.consumed.entry(segs[0].name.clone()).or_insert(scrutinee.span);
            }
        }

        // every case must be handled, or `otherwise:` must say what happens to the rest
        let all: Vec<String> = match &kind {
            Subject::Enum(e) => self.enums.get(e).map(|e| e.variants.iter().map(|v| v.name.clone()).collect()).unwrap_or_default(),
            Subject::Maybe(_) => vec!["some".into(), "none".into()],
            Subject::Result(_) => vec!["ok".into(), "error".into()],
            Subject::Unknown => Vec::new(),
        };
        let missing: Vec<String> = all.iter().filter(|n| !handled.contains(n)).cloned().collect();
        if had_unknown || all.is_empty() {
            return;
        }
        match (missing.is_empty(), otherwise) {
            (false, None) => self.not_exhaustive(cx, s, &kind, &missing),
            (true, Some(o)) => {
                self.warn(
                    W_UNREACHABLE,
                    cx.file,
                    o.span,
                    "Every case is handled above, so this \"otherwise:\" can never run.",
                );
                self.diags.last_mut().unwrap().help = Some("Remove it.".into());
            }
            _ => {}
        }
    }

    fn not_exhaustive(&mut self, cx: &Cx, s: &Stmt, kind: &Subject, missing: &[String]) {
        let what = match kind {
            Subject::Enum(e) => format!("of {e}"),
            Subject::Maybe(_) => "of a \"maybe\"".to_string(),
            Subject::Result(_) => "of a \"result\"".to_string(),
            Subject::Unknown => String::new(),
        };
        let (noun, fix) = match kind {
            Subject::Enum(e) => {
                let first = self.enums[e].variants.iter().find(|v| v.name == missing[0]).cloned();
                let line = match first {
                    Some(v) if v.fields.is_empty() => format!("- {}:", v.name),
                    Some(v) => format!("- {} {}:", v.name, crate::sema::body::snake(&v.name)),
                    None => String::new(),
                };
                (if missing.len() == 1 { "variant" } else { "variants" }, line)
            }
            Subject::Maybe(_) => ("case", format!("- {}:", if missing[0] == "some" { "some value" } else { "none" })),
            _ => ("case", format!("- {}:", if missing[0] == "ok" { "ok value" } else { "error problem" })),
        };
        let d = Diagnostic::error(
            E_NOT_EXHAUSTIVE,
            self.path(cx.file),
            s.span,
            format!(
                "This \"match\" does not handle the {noun} {} {what}.",
                quoted_list(missing)
            ),
        )
        .with_help(format!(
            "Add a branch for {} (\"{fix}\"), or an \"- otherwise:\" branch for everything else.",
            if missing.len() == 1 { "it" } else { "each" }
        ));
        self.diags.push(d);
    }

    /// Resolve one branch against what is matched. Returns the pattern and the type of the name
    /// the branch binds.
    fn arm_pattern(
        &mut self,
        cx: &Cx,
        arm: &MatchArm,
        kind: &Subject,
        handled: &mut Vec<String>,
        had_unknown: &mut bool,
    ) -> (Option<Pattern>, Option<Type>) {
        let word = arm.pattern.name.as_str();
        let q = arm.qualifier.as_ref().map(|q| q.name.as_str());
        let span = match &arm.qualifier {
            Some(qq) => qq.span.to(arm.pattern.span),
            None => arm.pattern.span,
        };
        let (pattern, bind_ty, key, takes_value): (Pattern, Option<Type>, String, bool) = match kind {
            Subject::Enum(e) => {
                // the variant is looked up inside the enum being matched, so `Empty` needs no
                // qualifier even when another enum has an `Empty` too
                let in_enum = self.enums.get(e).and_then(|en| en.variants.iter().position(|v| v.name == word));
                let qualifier_ok = match q {
                    None => true,
                    Some(qs) => {
                        let hit = self.lookup(cx.file, qs, &[Kind::Enum]).is_some_and(|(_, n)| n == *e);
                        if !hit {
                            let d = Diagnostic::error(
                                E_BRANCH,
                                self.path(cx.file),
                                arm.qualifier.as_ref().map_or(span, |x| x.span),
                                format!("This \"match\" is on {e}, but the branch names \"{qs}\"."),
                            );
                            self.diags.push(d);
                            *had_unknown = true;
                        }
                        hit
                    }
                };
                match (in_enum, qualifier_ok) {
                    (Some(i), true) => {
                        let v = self.enums[e].variants[i].clone();
                        (
                            Pattern::Variant {
                                enum_name: e.clone(),
                                variant: v.name.clone(),
                            },
                            Some(Type::Variant(e.clone(), v.name.clone())),
                            v.name.clone(),
                            !v.fields.is_empty(),
                        )
                    }
                    (None, true) => {
                        *had_unknown = true;
                        let names: Vec<String> = self.enums[e].variants.iter().map(|v| v.name.clone()).collect();
                        let other = self.find_variant(cx.file, None, word, span).ok().flatten();
                        let mut d = Diagnostic::error(
                            E_UNKNOWN_VARIANT,
                            self.path(cx.file),
                            arm.pattern.span,
                            format!("{e} has no variant \"{word}\"."),
                        )
                        .suggesting(word, names.iter().map(String::as_str));
                        if let Some(o) = other {
                            d = d.with_help(format!("\"{word}\" belongs to {}, but this \"match\" is on {e}.", o.enum_name));
                        } else {
                            d = d.with_help(format!("The variants of {e} are: {}.", names.join(", ")));
                        }
                        self.diags.push(d);
                        return (None, None);
                    }
                    _ => return (None, None),
                }
            }
            Subject::Maybe(inner) => match word {
                "some" => (Pattern::Some, Some(inner.clone()), "some".into(), true),
                "none" => (Pattern::None, None, "none".into(), false),
                _ => return self.wrong_branch(cx, arm, "a \"maybe\"", &["some", "none"], had_unknown),
            },
            Subject::Result(inner) => match word {
                "ok" => (Pattern::Ok, Some(inner.clone()), "ok".into(), true),
                "error" => (Pattern::Error, Some(Type::Text), "error".into(), true),
                _ => return self.wrong_branch(cx, arm, "a \"result\"", &["ok", "error"], had_unknown),
            },
            Subject::Unknown => {
                // nothing to check against; the branch still binds its name
                return (None, arm.bind.as_ref().map(|_| Type::Unknown));
            }
        };
        if handled.contains(&key) {
            self.error(
                E_DUP_BRANCH,
                cx.file,
                arm.pattern.span,
                format!("This \"match\" already has a branch for \"{key}\"."),
            );
        } else {
            handled.push(key.clone());
        }
        // naming the value
        match (&arm.bind, takes_value) {
            (Some(b), false) => {
                let d = Diagnostic::error(
                    E_BINDER,
                    self.path(cx.file),
                    b.span,
                    match &pattern {
                        Pattern::Variant { variant, .. } => format!("The variant \"{variant}\" has no fields, so there is nothing to name \"{}\".", b.name),
                        _ => format!("\"{key}\" carries no value, so there is nothing to name \"{}\".", b.name),
                    },
                )
                .with_help(format!("Write just \"- {key}:\"."));
                self.diags.push(d);
            }
            _ => {}
        }
        (Some(pattern), bind_ty)
    }

    fn wrong_branch(
        &mut self,
        cx: &Cx,
        arm: &MatchArm,
        what: &str,
        words: &[&str],
        had_unknown: &mut bool,
    ) -> (Option<Pattern>, Option<Type>) {
        *had_unknown = true;
        let d = Diagnostic::error(
            E_BRANCH,
            self.path(cx.file),
            arm.pattern.span,
            format!("\"{}\" is not a branch of {what}.", arm.pattern.name),
        )
        .with_did_you_mean(did_you_mean(&arm.pattern.name, words.iter().copied()))
        .with_help(format!("The branches of {what} are: {}.", words.join(", ")));
        self.diags.push(d);
        (None, None)
    }
}

/// What a `match` looks at.
enum Subject {
    Enum(String),
    Maybe(Type),
    Result(Type),
    /// Something already reported (or unknown): branches are not checked.
    Unknown,
}

fn join(segs: &[Ident]) -> String {
    segs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(".")
}

/// A call, or a plain name (a stored result).
fn is_call_or_name(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Call { .. } => true,
        ExprKind::Path(p) => p.len() == 1,
        _ => false,
    }
}
