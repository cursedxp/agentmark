//! Heading and property grammars: `# Agent: x`, `## Every day at 09:00`, `- model: claude`.

use chumsky::prelude::*;

use super::expr::*;
use crate::diag::{did_you_mean, Span};
use crate::syntax::ast::*;

// ------------------------------------------------------------------ level-1 headings

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    Agent,
    Record,
    Enum,
    TypeAlias,
    Rule,
    Function,
    Service,
    Command,
    Binding,
    Constant,
}

impl DeclKind {
    pub fn word(self) -> &'static str {
        match self {
            DeclKind::Agent => "Agent",
            DeclKind::Record => "Record",
            DeclKind::Enum => "Enum",
            DeclKind::TypeAlias => "Type",
            DeclKind::Rule => "Rule",
            DeclKind::Function => "Function",
            DeclKind::Service => "Service",
            DeclKind::Command => "Command",
            DeclKind::Binding => "Binding",
            DeclKind::Constant => "Constant",
        }
    }
}

/// Every word that may follow `# `.
pub const HEADING_WORDS: &[&str] = &[
    "Agent", "Record", "Enum", "Type", "Rule", "Function", "Service", "Command", "Binding",
    "Constant", "Run", "Import",
];

/// The first word of a level-1 heading (checked first so an unknown word gets "did you mean").
pub fn h1_word<'a>() -> P<'a, Ident> {
    ident().then_ignore(any().repeated()).boxed()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportTarget {
    pub is_crate: bool,
    /// File path / glob / folder, or the crate name.
    pub target: Ident,
    pub alias: Option<Ident>,
}

#[derive(Debug, Clone)]
pub enum H1 {
    Decl(DeclKind, Ident),
    Run,
    Import(ImportTarget),
}

pub fn h1<'a>() -> P<'a, H1> {
    let decl = |word: &'static str, kind: DeclKind| {
        ikw(word)
            .ignore_then(sp())
            .ignore_then(just(':').labelled("\":\""))
            .ignore_then(sp())
            .ignore_then(name())
            .then_ignore(sp())
            .map(move |n| H1::Decl(kind, n))
    };
    let run = ikw("run").then_ignore(sp()).to(H1::Run);

    let path = none_of(" \t")
        .repeated()
        .at_least(1)
        .to_slice()
        .map_with(|s: &str, e| Ident {
            name: s.to_string(),
            span: at(e),
        })
        .labelled("a file path");
    let alias = sp1()
        .ignore_then(kw("as"))
        .ignore_then(sp1())
        .ignore_then(name());
    // an optional trailing `> comment` is allowed after an import
    let trailing_comment = sp().then(just('>')).then(any().repeated()).or_not();
    let crate_import = kw("crate")
        .ignore_then(sp1())
        .ignore_then(name())
        .then(alias.clone().or_not())
        .map(|(target, alias)| ImportTarget {
            is_crate: true,
            target,
            alias,
        });
    let path_import = path
        .then(alias.or_not())
        .map(|(target, alias)| ImportTarget {
            is_crate: false,
            target,
            alias,
        });
    let import = ikw("import")
        .ignore_then(sp())
        .ignore_then(just(':').labelled("\":\""))
        .ignore_then(sp())
        .ignore_then(choice((crate_import, path_import)))
        .then_ignore(trailing_comment)
        .then_ignore(sp())
        .map(H1::Import);

    choice((
        decl("agent", DeclKind::Agent),
        decl("record", DeclKind::Record),
        decl("enum", DeclKind::Enum),
        decl("type", DeclKind::TypeAlias),
        decl("rule", DeclKind::Rule),
        decl("function", DeclKind::Function),
        decl("service", DeclKind::Service),
        decl("command", DeclKind::Command),
        decl("binding", DeclKind::Binding),
        decl("constant", DeclKind::Constant),
        run,
        import,
    ))
    .boxed()
}

// ------------------------------------------------------------------ level-2 headings

fn time<'a>() -> P<'a, (u8, u8)> {
    let two = any()
        .filter(|c: &char| c.is_ascii_digit())
        .repeated()
        .exactly(2)
        .to_slice();
    two.then_ignore(just(':'))
        .then(two)
        .map_with(|(h, m): (&str, &str), e| {
            let (h, m): (u8, u8) = (h.parse().unwrap_or(99), m.parse().unwrap_or(99));
            if h >= 24 || m >= 60 {
                flag(
                    e,
                    "A time looks like 09:00 (hours 00-23, minutes 00-59).",
                    None,
                );
            }
            (h, m)
        })
        .labelled("a time such as 09:00")
        .boxed()
}

fn weekday<'a>() -> P<'a, Weekday> {
    text::ascii::ident()
        .try_map(|s: &str, span| {
            Ok(match s.to_ascii_lowercase().as_str() {
                "monday" => Weekday::Monday,
                "tuesday" => Weekday::Tuesday,
                "wednesday" => Weekday::Wednesday,
                "thursday" => Weekday::Thursday,
                "friday" => Weekday::Friday,
                "saturday" => Weekday::Saturday,
                "sunday" => Weekday::Sunday,
                _ => return Err(Rich::custom(span, format!("\"{s}\" is not a weekday."))),
            })
        })
        .labelled("a weekday")
        .boxed()
}

/// `Every 30 minutes`, `Every day at 09:00`, `Every Monday at 08:00`, `When crm receives lead`,
/// `On start`, `Do remind`.
pub fn trigger<'a>() -> P<'a, Trigger> {
    let daily = ikw("day")
        .ignore_then(sp1())
        .ignore_then(ikw("at"))
        .ignore_then(sp1())
        .ignore_then(time())
        .map(|(hour, minute)| Trigger::DailyAt { hour, minute });
    let weekly = weekday()
        .then_ignore(sp1())
        .then_ignore(ikw("at"))
        .then_ignore(sp1())
        .then(time())
        .map(|(day, (hour, minute))| Trigger::Weekly { day, hour, minute });
    let every = ikw("every").ignore_then(sp1()).ignore_then(choice((
        daily,
        weekly,
        duration().map(Trigger::Every),
    )));
    let when = ikw("when")
        .ignore_then(sp1())
        .ignore_then(name())
        .then_ignore(sp1())
        .then_ignore(ikw("receives"))
        .then_ignore(sp1())
        .then(name())
        .map(|(tool, event)| Trigger::OnEvent { tool, event });
    let on_start = ikw("on")
        .ignore_then(sp1())
        .ignore_then(ikw("start"))
        .to(Trigger::OnStart);
    let do_ = ikw("do")
        .ignore_then(sp1())
        .ignore_then(name())
        .map(Trigger::Do);
    choice((every, when, on_start, do_))
        .then_ignore(sp())
        .labelled("a trigger")
        .boxed()
}

/// `GET /invoices/{id}` -> (method, path, path params).
pub fn route<'a>() -> P<'a, (String, String, Vec<String>)> {
    let method = text::ascii::ident()
        .map_with(|s: &str, e| {
            let up = s.to_ascii_uppercase();
            if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&up.as_str()) {
                let dym = did_you_mean(&up, ["GET", "POST", "PUT", "PATCH", "DELETE"]);
                flag(e, format!("\"{s}\" is not an HTTP method."), dym);
            }
            up
        })
        .labelled("an HTTP method such as GET or POST");
    let segment = choice((
        none_of("{ \t").repeated().at_least(1).to(None),
        ident()
            .delimited_by(just('{'), just('}'))
            .map(|i| Some(i.name)),
    ));
    let path = just('/')
        .ignore_then(segment.repeated().collect::<Vec<_>>())
        .map_with(|segs, e| {
            let params = segs.into_iter().flatten().collect::<Vec<String>>();
            (e.slice().to_string(), params)
        });
    method
        .then_ignore(sp1())
        .then(path)
        .then_ignore(sp())
        .map(|(m, (p, ps))| (m, p, ps))
        .boxed()
}

/// `Rust: fingerprint -> text`
pub fn rust_heading<'a>() -> P<'a, (Ident, Option<TypeExpr>)> {
    ikw("rust")
        .ignore_then(sp())
        .ignore_then(just(':').labelled("\":\""))
        .ignore_then(sp())
        .ignore_then(name())
        .then(
            sp().ignore_then(just("->"))
                .ignore_then(sp())
                .ignore_then(type_expr())
                .or_not(),
        )
        .then_ignore(sp())
        .boxed()
}

/// The first word of a level-2 heading (to recognise `Rust:`, `Run`, `Not bound`).
pub fn h2_is<'a>(word: &'static str) -> P<'a, ()> {
    ikw(word).ignore_then(any().repeated()).boxed()
}

pub fn run_heading<'a>() -> P<'a, ()> {
    ikw("run").then_ignore(sp()).boxed()
}

pub fn not_bound_heading<'a>() -> P<'a, ()> {
    ikw("not")
        .ignore_then(sp1())
        .ignore_then(ikw("bound"))
        .then_ignore(sp())
        .boxed()
}

/// `render(doc: Document) -> bytes`
pub fn binding_item_heading<'a>() -> P<'a, (Ident, Vec<Param>, Option<TypeExpr>)> {
    name()
        .then(
            params_inner()
                .padded_by(sp())
                .delimited_by(just('('), just(')')),
        )
        .then(
            sp().ignore_then(just("->"))
                .ignore_then(sp())
                .ignore_then(type_expr())
                .or_not(),
        )
        .then_ignore(sp())
        .map(|((n, p), o)| (n, p, o))
        .boxed()
}

// ------------------------------------------------------------------ properties

/// `key: value` -> (key, value text, byte range of the value inside the line text).
pub fn property_head<'a>() -> P<'a, (Ident, &'a str, SimpleSpan)> {
    name()
        .then_ignore(sp())
        .then_ignore(just(':').labelled("\":\""))
        .then_ignore(sp())
        .then(
            any()
                .repeated()
                .to_slice()
                .map_with(|s: &str, e| (s, e.span())),
        )
        .map(|(k, (v, sp))| (k, v, sp))
        .boxed()
}

/// A record field: `name`, `name:` or `name: type`.
pub fn field<'a>() -> P<'a, (Ident, Option<TypeExpr>)> {
    name()
        .then(
            sp().ignore_then(just(':'))
                .ignore_then(sp())
                .ignore_then(type_expr().or_not())
                .or_not(),
        )
        .then_ignore(sp())
        .map(|(n, t)| (n, t.flatten()))
        .boxed()
}

/// A variant of an enum: `Empty`, `Circle: radius: int`, `Rect: width: int, height: int`.
pub fn variant<'a>() -> P<'a, (Ident, Vec<(Ident, TypeExpr)>)> {
    let fld = ident()
        .then_ignore(sp())
        .then_ignore(just(':').labelled("\":\""))
        .then_ignore(sp())
        .then(type_expr());
    let fields = sp()
        .ignore_then(just(':'))
        .ignore_then(sp())
        .ignore_then(
            fld.separated_by(sp().then(just(',')).then(sp()))
                .at_least(1)
                .collect::<Vec<_>>(),
        );
    name()
        .then(fields.or_not())
        .then_ignore(sp())
        .map(|(n, f)| (n, f.unwrap_or_default()))
        .boxed()
}

fn params_inner<'a>() -> P<'a, Vec<Param>> {
    let default = sp()
        .ignore_then(just('='))
        .ignore_then(sp())
        .ignore_then(expr());
    ident()
        .then_ignore(sp())
        .then_ignore(just(':').labelled("\":\""))
        .then_ignore(sp())
        .then(type_expr())
        .then(default.or_not())
        .map(|((name, ty), default)| Param { name, ty, default })
        .separated_by(sp().then(just(',')).then(sp()))
        .allow_trailing()
        .collect::<Vec<_>>()
        .boxed()
}

/// `invoice: Invoice, vat: int` / `path: text, dry_run: bool = false`
pub fn params<'a>() -> P<'a, Vec<Param>> {
    params_inner().then_ignore(sp()).boxed()
}

/// `email, crm, log`
pub fn name_list<'a>() -> P<'a, Vec<Ident>> {
    name()
        .separated_by(sp().then(just(',')).then(sp()))
        .at_least(1)
        .collect::<Vec<_>>()
        .then_ignore(sp())
        .boxed()
}

/// A model driver string: `claude`, `claude:haiku`, `local:kat-coder`.
pub fn model_string<'a>() -> P<'a, Spanned<String>> {
    any()
        .filter(|c: &char| c.is_ascii_alphanumeric() || ":_.-/".contains(*c))
        .repeated()
        .at_least(1)
        .to_slice()
        .map_with(|s: &str, e| Spanned {
            value: s.to_string(),
            span: at(e),
        })
        .then_ignore(sp())
        .labelled("a model such as claude or claude:haiku")
        .boxed()
}

pub fn memory<'a>() -> P<'a, Memory> {
    choice((kw("none").to(Memory::None), kw("file").to(Memory::File)))
        .then_ignore(sp())
        .labelled("\"none\" or \"file\"")
        .boxed()
}

pub fn effect<'a>() -> P<'a, Effect> {
    choice((
        kw("pure").to(Effect::Pure),
        kw("local").to(Effect::Local),
        kw("network").to(Effect::Network),
        kw("model").to(Effect::Model),
        kw("unverified").to(Effect::Unverified),
    ))
    .then_ignore(sp())
    .labelled("pure, local, network or model")
    .boxed()
}

/// A single name (record name after `for:` / `state:`).
pub fn single_name<'a>() -> P<'a, Ident> {
    name().then_ignore(sp()).boxed()
}

/// The whole rest of the line as text (`listen:` address, a `Not bound` reason).
pub fn rest_of_line<'a>() -> P<'a, Spanned<String>> {
    any()
        .repeated()
        .to_slice()
        .map_with(|s: &str, e| Spanned {
            value: s.trim().to_string(),
            span: at(e),
        })
        .boxed()
}

/// `# Run` item: `invoice-chaser` or `billing.api`.
pub fn run_item<'a>() -> P<'a, Ident> {
    name()
        .separated_by(just('.'))
        .at_least(1)
        .to_slice()
        .map_with(|s: &str, e| Ident {
            name: s.to_string(),
            span: at(e),
        })
        .then_ignore(sp())
        .boxed()
}

/// An item under `## Not bound`: `name` or `name: reason`.
pub fn not_bound_item<'a>() -> P<'a, (String, Option<String>)> {
    any()
        .filter(|c: &char| *c != ':')
        .repeated()
        .at_least(1)
        .to_slice()
        .then(just(':').ignore_then(any().repeated().to_slice()).or_not())
        .map(|(item, reason): (&str, Option<&str>)| {
            (
                item.trim().to_string(),
                reason.map(|r| r.trim().to_string()),
            )
        })
        .boxed()
}

/// Helper for callers: a single-line span.
pub fn line_span(line: u32, col: u32, text: &str) -> Span {
    Span::new(line, col, text.chars().count() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok<'a, T>(p: &P<'a, T>, src: &'a str) -> T {
        match run(p, src, 1, 1) {
            Ok(v) => v,
            Err((_, e)) => panic!("failed on {src:?}: {e:?}"),
        }
    }

    #[test]
    fn triggers() {
        let p = trigger();
        assert_eq!(
            ok(&p, "Every 30 minutes"),
            Trigger::Every(DurationLit {
                amount: 30,
                unit: DurationUnit::Minute
            })
        );
        assert_eq!(
            ok(&p, "Every day at 09:00"),
            Trigger::DailyAt { hour: 9, minute: 0 }
        );
        assert_eq!(
            ok(&p, "Every Monday at 08:00"),
            Trigger::Weekly {
                day: Weekday::Monday,
                hour: 8,
                minute: 0
            }
        );
        assert!(matches!(
            ok(&p, "When crm receives lead"),
            Trigger::OnEvent { .. }
        ));
        assert_eq!(ok(&p, "On start"), Trigger::OnStart);
        assert!(matches!(ok(&p, "Do remind"), Trigger::Do(n) if n.name == "remind"));
    }

    #[test]
    fn triggers_are_case_insensitive() {
        let p = trigger();
        assert_eq!(
            ok(&p, "every DAY at 23:59"),
            Trigger::DailyAt {
                hour: 23,
                minute: 59
            }
        );
        assert_eq!(ok(&p, "ON START"), Trigger::OnStart);
    }

    #[test]
    fn bad_times_are_rejected() {
        let p = trigger();
        assert!(run(&p, "Every day at 25:00", 1, 1).is_err());
        assert!(run(&p, "Every day at 9:00", 1, 1).is_err());
    }

    #[test]
    fn routes() {
        let p = route();
        let (m, path, params) = ok(&p, "GET /invoices/{id}");
        assert_eq!((m.as_str(), path.as_str()), ("GET", "/invoices/{id}"));
        assert_eq!(params, vec!["id".to_string()]);
        let (_, path, params) = ok(&p, "get /invoices/{id}/net");
        assert_eq!(path, "/invoices/{id}/net");
        assert_eq!(params, vec!["id".to_string()]);
        let (m, path, params) = ok(&p, "POST /invoices");
        assert_eq!((m.as_str(), path.as_str()), ("POST", "/invoices"));
        assert!(params.is_empty());
        assert!(run(&p, "FETCH /x", 1, 1).is_err());
    }

    #[test]
    fn headings_level_one() {
        let p = h1();
        let H1::Decl(DeclKind::Agent, n) = ok(&p, "Agent: invoice-chaser") else {
            panic!()
        };
        assert_eq!(n.name, "invoice-chaser");
        assert!(matches!(ok(&p, "run"), H1::Run));
        assert!(matches!(ok(&p, "Run"), H1::Run));
        let H1::Import(t) = ok(&p, "Import: ./records/invoice.agent.md") else {
            panic!()
        };
        assert!(!t.is_crate && t.alias.is_none());
        let H1::Import(t) = ok(&p, "Import: ./billing as billing") else {
            panic!()
        };
        assert_eq!(t.alias.unwrap().name, "billing");
        let H1::Import(t) = ok(&p, "Import: crate reqwest as http_client") else {
            panic!()
        };
        assert!(t.is_crate);
        let H1::Import(t) = ok(
            &p,
            "Import: ./rules/*.agent.md                > glob, imports every rule",
        ) else {
            panic!()
        };
        assert_eq!(t.target.name, "./rules/*.agent.md");
    }

    #[test]
    fn rust_and_binding_headings() {
        let (n, out) = ok(&rust_heading(), "Rust: hash_payload -> text");
        assert_eq!(n.name, "hash_payload");
        assert!(out.is_some());
        let (n, params, out) = ok(&binding_item_heading(), "render(doc: Document) -> bytes");
        assert_eq!(n.name, "render");
        assert_eq!(params.len(), 1);
        assert!(out.is_some());
        ok(&not_bound_heading(), "Not bound");
    }

    #[test]
    fn param_lists() {
        let p = params();
        let v = ok(&p, "invoice: Invoice, vat: int");
        assert_eq!(v.len(), 2);
        let v = ok(&p, "path: text, dry_run: bool = false");
        assert!(v[1].default.is_some());
    }

    #[test]
    fn properties() {
        let (k, v, _) = ok(&property_head(), "proposed-effect: network");
        assert_eq!((k.name.as_str(), v), ("proposed-effect", "network"));
        let (k, v, _) = ok(&property_head(), "listen: 0.0.0.0:8080");
        assert_eq!((k.name.as_str(), v), ("listen", "0.0.0.0:8080"));
        let (n, ty) = ok(&field(), "score: maybe int");
        assert_eq!(n.name, "score");
        assert!(ty.is_some());
        let (_, ty) = ok(&field(), "due_date");
        assert!(ty.is_none());
        assert_eq!(ok(&effect(), "pure"), Effect::Pure);
        assert_eq!(ok(&model_string(), "claude:haiku").value, "claude:haiku");
        assert_eq!(ok(&name_list(), "email, crm, log").len(), 3);
    }
}
