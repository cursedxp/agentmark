//! The built-in library in the checker (SPEC 22.3): `module.fn(args)` calls, `json.parse(t) as T`,
//! the `file` statement, `read input` and `try <call> -> var`. Every signature comes from the one table in
//! `crate::stdlib`; this file only turns it into types and errors (E340-E359).

use crate::diag::{Diagnostic, Span};
use crate::stdlib::{self as lib, Extra, Func, Ty};
use crate::syntax::ast::{Effect, Expr, ExprKind, FileArg, Ident, Stmt, TypeExpr};

use super::body::Cx;
use super::expr::{E_CALL, E_TYPE};
use super::types::Type;
use super::{Checker, NodeKey, Resolution};

/// `module.fn` is not in the library.
pub const E_UNKNOWN_FN: &str = "E340";
/// A call that can fail is not written with `try` (or `match`).
pub const E_NEEDS_TRY: &str = "E341";
/// `json.parse` / `csv.parse` without (or with a wrong) `as <type>`.
pub const E_DECODE: &str = "E342";
/// A literal pattern that is not valid.
pub const E_PATTERN: &str = "E343";
/// A verb `file` does not have.
pub const E_FILE_VERB: &str = "E344";
/// A `file` statement that is missing a part or has one too many.
pub const E_FILE_SHAPE: &str = "E345";

/// A call of the library as the checker meets it: the whole expression, the dotted name, the
/// arguments and their types.
pub(super) struct StdCall<'a> {
    pub e: &'a Expr,
    pub func: &'a [Ident],
    pub args: &'a [Expr],
    pub types: &'a [Type],
}

/// A `file` statement's parts.
pub(super) struct FileStmt<'a> {
    pub s: &'a Stmt,
    pub verb: &'a Ident,
    pub path: &'a Expr,
    pub arg: &'a Option<FileArg>,
    pub into: &'a Option<Ident>,
}

impl Checker<'_> {
    fn lib_type(&self, ty: Ty, first: Option<&Type>) -> Type {
        match ty {
            Ty::Text => Type::Text,
            Ty::Int => Type::Int,
            Ty::Bool => Type::Bool,
            Ty::Money => Type::Money,
            Ty::Date => Type::Date,
            Ty::Num => first.cloned().unwrap_or(Type::Unknown),
            Ty::DateAny => Type::Date,
            Ty::Any => Type::Unknown,
            Ty::AnyList => Type::Unknown.list(),
            Ty::ListText => Type::Text.list(),
            Ty::MaybeText => Type::Text.maybe(),
            Ty::MaybeListText => Type::Text.list().maybe(),
            Ty::ResultInt => Type::Result(Box::new(Type::Int)),
            Ty::ResultMoney => Type::Result(Box::new(Type::Money)),
            Ty::ResultDate => Type::Result(Box::new(Type::Date)),
            Ty::Decoded => Type::Unknown,
        }
    }

    /// Does an argument of type `t` fit a parameter that takes `want`?
    fn lib_accepts(&self, want: Ty, t: &Type) -> bool {
        if t.is_unknown() {
            return true;
        }
        match want {
            Ty::Num => matches!(t, Type::Int | Type::Money),
            Ty::DateAny => matches!(t, Type::Date | Type::DateTime),
            Ty::Any => true,
            Ty::AnyList => matches!(t, Type::List(_)),
            other => self.is_assignable(t, &self.lib_type(other, None)),
        }
    }

    fn lib_want_words(want: Ty) -> &'static str {
        match want {
            Ty::Num => "a number or money",
            Ty::DateAny => "a date or datetime",
            Ty::Any => "a value",
            Ty::AnyList => "a list",
            Ty::Text => "text",
            Ty::Int => "a number",
            Ty::Bool => "yes or no (bool)",
            Ty::Money => "money",
            Ty::Date => "a date",
            _ => "a value",
        }
    }

    /// The text of a literal argument, looking through a constant: `"[0-9]+"` or `camera_pattern`.
    fn literal_text(&self, cx: &Cx, a: &Expr) -> Option<String> {
        match &a.kind {
            ExprKind::Str(s) => Some(s.clone()),
            ExprKind::Path(_) => match self.resolutions.get(&NodeKey::new(cx.file, a.span)) {
                Some(Resolution::Const { name }) => match &self.const_infos.get(name)?.value.kind {
                    ExprKind::Str(s) => Some(s.clone()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    /// `module.name(args)` where `module` is one of the library's. `ok_result` says a `result`
    /// is fine here (directly after `try`).
    pub(super) fn std_call(&mut self, cx: &mut Cx, c: &StdCall, decode: Option<&TypeExpr>, ok_result: bool) -> Type {
        let StdCall { e, func, args, types: arg_types } = *c;
        let (module, name) = (&func[0], &func[1]);
        let Some(f) = lib::lookup(&module.name, &name.name) else {
            let cands: Vec<&str> = lib::functions_of(&module.name).map(|f| f.name).collect();
            let span = name.span;
            let list = cands.join(", ");
            let d = Diagnostic::error(
                E_UNKNOWN_FN,
                self.path(cx.file),
                span,
                format!("The module \"{}\" has no function \"{}\".", module.name, name.name),
            )
            .suggesting(&name.name, cands.iter().copied())
            .with_help(format!("\"{}\" has: {list}.", module.name));
            self.diags.push(d);
            return Type::Unknown;
        };
        self.std_args(cx, e, f, args, arg_types);
        self.std_patterns(cx, f, args);
        let result = match (f.ret, decode) {
            (Ty::Decoded, Some(te)) => {
                let want = self.resolve_type(cx.file, te);
                self.std_decode_type(cx, f, te, &want)
            }
            (Ty::Decoded, None) => {
                let d = Diagnostic::error(
                    E_DECODE,
                    self.path(cx.file),
                    e.span,
                    format!("\"{}.{}\" needs to know what to read: write the type after the call.", f.module, f.name),
                )
                .with_help(match f.module {
                    "csv" => "For example: csv.parse(content) as list of Sale",
                    _ => "For example: json.parse(body) as Note",
                });
                self.diags.push(d);
                Type::Unknown
            }
            (ret, _) => self.lib_type(ret, arg_types.first()),
        };
        if f.is_result() && !ok_result {
            let d = Diagnostic::error(
                E_NEEDS_TRY,
                self.path(cx.file),
                e.span,
                format!("\"{}.{}\" can fail (it returns \"{}\"), so it has to be called with \"try\".", f.module, f.name, result),
            )
            .with_help(format!(
                "Write \"- try {}.{}(...) -> value\", optionally with an \"otherwise:\" branch.",
                f.module, f.name
            ));
            self.diags.push(d);
            return match result {
                Type::Result(t) => *t,
                t => t,
            };
        }
        result
    }

    /// Argument count and kinds against the table row.
    fn std_args(&mut self, cx: &mut Cx, e: &Expr, f: &Func, args: &[Expr], types: &[Type]) {
        let name = format!("{}.{}", f.module, f.name);
        if args.len() != f.params.len() {
            let names: Vec<&str> = f.params.iter().map(|p| p.0).collect();
            let d = Diagnostic::error(
                E_CALL,
                self.path(cx.file),
                e.span,
                format!(
                    "\"{name}\" takes {} value{} ({}), but {} {} given.",
                    f.params.len(),
                    if f.params.len() == 1 { "" } else { "s" },
                    names.join(", "),
                    args.len(),
                    if args.len() == 1 { "was" } else { "were" },
                ),
            );
            self.diags.push(d);
            return;
        }
        for (i, ((pname, want), a)) in f.params.iter().zip(args).enumerate() {
            let t = &types[i];
            if !self.lib_accepts(*want, t) {
                let d = Diagnostic::error(
                    E_TYPE,
                    self.path(cx.file),
                    a.span,
                    format!(
                        "\"{pname}\" of \"{name}\" must be {}, but this is {}.{}",
                        Self::lib_want_words(*want),
                        t.with_article(),
                        if t.is_maybe() { " Check it with \"is some\" or \"is not empty\" first." } else { "" }
                    ),
                );
                self.diags.push(d);
            }
        }
        // `number.min(3, 5 EUR)`: both sides must be the same kind
        if f.params.len() == 2 && f.params.iter().all(|p| p.1 == Ty::Num) {
            let (a, b) = (&types[0], &types[1]);
            if !a.is_unknown() && !b.is_unknown() && a != b && matches!(a, Type::Int | Type::Money) && matches!(b, Type::Int | Type::Money) {
                let d = Diagnostic::error(
                    E_TYPE,
                    self.path(cx.file),
                    args[1].span,
                    format!("\"{name}\" compares two values of the same kind, but the first is {} and this is {}.", a.with_article(), b.with_article()),
                );
                self.diags.push(d);
            }
        }
    }

    /// A literal regex or date pattern that cannot work is an error here, not at run time.
    fn std_patterns(&mut self, cx: &mut Cx, f: &Func, args: &[Expr]) {
        let Some(i) = f.params.iter().position(|p| p.0 == "pattern") else { return };
        let Some(a) = args.get(i) else { return };
        let Some(text) = self.literal_text(cx, a) else { return };
        let checked = match f.module {
            "regex" => agentmark_rt::stdlib::regex::check(&text),
            "date" => agentmark_rt::stdlib::date::check(&text),
            _ => return,
        };
        if let Err(msg) = checked {
            let d = Diagnostic::error(E_PATTERN, self.path(cx.file), a.span, msg);
            self.diags.push(d);
        }
    }

    /// What `json.parse(t) as T` / `csv.parse(t) as list of T` gives.
    fn std_decode_type(&mut self, cx: &mut Cx, f: &Func, te: &TypeExpr, want: &Type) -> Type {
        if want.is_unknown() {
            return Type::Unknown;
        }
        if f.module == "csv" {
            let ok = matches!(want, Type::List(inner) if matches!(**inner, Type::Record(_)));
            if !ok {
                let d = Diagnostic::error(
                    E_DECODE,
                    self.path(cx.file),
                    te.span,
                    format!("\"csv.parse\" reads a list of records (one per row), but \"as {want}\" is not one."),
                )
                .with_help("Declare a record for the columns and write: csv.parse(content) as list of Sale");
                self.diags.push(d);
                return Type::Unknown;
            }
        }
        Type::Result(Box::new(want.clone()))
    }

    /// `json.parse(t) as T`.
    pub(super) fn std_decode(&mut self, cx: &mut Cx, e: &Expr, call: &Expr, ty: &TypeExpr) -> Type {
        let ExprKind::Call { func, args } = &call.kind else { return Type::Unknown };
        let arg_types: Vec<Type> = args.iter().map(|a| self.expr(cx, a)).collect();
        let ok = cx.std_ok == Some(e.span);
        let t = self.std_call(cx, &StdCall { e, func, args, types: &arg_types }, Some(ty), ok);
        self.expr_types.insert(NodeKey::new(cx.file, call.span), t.clone());
        t
    }

    // ------------------------------------------------------------------------ statements

    /// `try csv.parse(content) as list of Sale -> sales`, `do text.trim(x) -> y`.
    pub(super) fn call_into_stmt(&mut self, cx: &mut Cx, s: &Stmt, call: &Expr, into: &Ident, in_try: bool) {
        let saved = cx.std_ok.take();
        if in_try {
            cx.std_ok = Some(call.span);
        }
        let t = self.expr(cx, call);
        cx.std_ok = saved;
        let t = match t {
            Type::Result(inner) => *inner,
            t => t,
        };
        cx.define(&into.name, t, into.span);
        let _ = s;
    }

    /// `read input -> text`.
    pub(super) fn read_input_stmt(&mut self, cx: &mut Cx, into: &Ident) {
        cx.define(&into.name, Type::Text, into.span);
        cx.acc.add(Effect::Local, "input");
    }

    /// `file <verb> <path> [to <new> | : <text>] [-> name]`.
    pub(super) fn file_stmt(&mut self, cx: &mut Cx, f: &FileStmt, in_try: bool) {
        let FileStmt { s, verb, path, arg, into } = *f;
        let pt = self.expr(cx, path);
        let want_text = |this: &mut Self, cx: &Cx, e: &Expr, t: &Type, what: &str| {
            if !matches!(t, Type::Text | Type::Unknown) {
                let d = Diagnostic::error(
                    E_TYPE,
                    this.path(cx.file),
                    e.span,
                    format!("{what} must be text, but this is {}.", t.with_article()),
                );
                this.diags.push(d);
            }
        };
        want_text(self, cx, path, &pt, "A file path");
        cx.acc.add(Effect::Local, "file");
        let Some(v) = lib::file_verb(&verb.name) else {
            let names: Vec<&str> = lib::FILE_VERBS.iter().map(|v| v.verb).collect();
            let d = Diagnostic::error(
                E_FILE_VERB,
                self.path(cx.file),
                verb.span,
                format!("The built-in tool \"file\" has no verb \"{}\".", verb.name),
            )
            .suggesting(&verb.name, names.iter().copied())
            .with_help(format!("\"file\" can: {}.", names.join(", ")));
            self.diags.push(d);
            if let Some(i) = into {
                cx.define(&i.name, Type::Unknown, i.span);
            }
            if let Some(FileArg::To(e) | FileArg::Content(e)) = arg {
                self.expr(cx, e);
            }
            return;
        };
        let shape_err = |this: &mut Self, cx: &Cx, span: Span, why: String| {
            let d = Diagnostic::error(E_FILE_SHAPE, this.path(cx.file), span, why)
                .with_help(format!("Write it like \"{}\".", v.shape));
            this.diags.push(d);
        };
        match (v.extra, arg) {
            (Extra::None, None) => {}
            (Extra::To, Some(FileArg::To(e))) | (Extra::Content, Some(FileArg::Content(e))) => {
                let t = self.expr(cx, e);
                want_text(self, cx, e, &t, if v.extra == Extra::To { "The new path" } else { "The content" });
            }
            (Extra::None, Some(FileArg::To(e) | FileArg::Content(e))) => {
                self.expr(cx, e);
                shape_err(self, cx, e.span, format!("\"file {}\" takes only a path.", v.verb));
            }
            (Extra::To, Some(FileArg::Content(e))) => {
                self.expr(cx, e);
                shape_err(self, cx, e.span, format!("\"file {}\" needs the new path after \"to\", not after a colon.", v.verb));
            }
            (Extra::Content, Some(FileArg::To(e))) => {
                self.expr(cx, e);
                shape_err(self, cx, e.span, format!("\"file {}\" needs the text after a colon, not after \"to\".", v.verb));
            }
            (Extra::To, None) => shape_err(self, cx, s.span, format!("\"file {}\" needs the new path: \"to <new>\".", v.verb)),
            (Extra::Content, None) => shape_err(self, cx, s.span, format!("\"file {}\" needs the text after a colon.", v.verb)),
        }
        match (v.result, into) {
            (Some(ty), Some(i)) => cx.define(&i.name, self.lib_type(ty, None), i.span),
            (Some(_), None) => shape_err(self, cx, s.span, format!("\"file {}\" gives something back: name it with \"-> name\".", v.verb)),
            (None, Some(i)) => {
                shape_err(self, cx, i.span, format!("\"file {}\" gives nothing back, so there is nothing to name.", v.verb));
                cx.define(&i.name, Type::Unknown, i.span);
            }
            (None, None) => {}
        }
        if v.can_fail && !in_try {
            let d = Diagnostic::error(
                E_NEEDS_TRY,
                self.path(cx.file),
                s.span,
                format!("\"file {}\" can fail (the file may not exist), so it has to be written with \"try\".", v.verb),
            )
            .with_help("Write \"- try file read path -> content\", optionally with an \"otherwise:\" branch.");
            self.diags.push(d);
        }
    }
}
