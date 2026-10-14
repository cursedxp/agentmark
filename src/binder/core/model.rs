//! What a target tells the binder about a library: one [`Item`] per public function or method,
//! already classified by the target's own rules ([`Shape`]).

use std::fmt;

pub use crate::syntax::ast::HandleMode;

/// An AgentMark type a binding can mention (SPEC 6 plus `bytes`, OPEN_QUESTIONS Q8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Text,
    Int,
    Bool,
    Bytes,
    Date,
    DateTime,
    Duration,
    List(Box<Ty>),
    Maybe(Box<Ty>),
    /// `handle of sha2.Sha256` (SPEC 22.4): a library type with no AgentMark equivalent. `name`
    /// is the dotted path from the crate; `mode` is how a parameter holds it (a return value is
    /// always owned, mode `Shared` there means "not borrowed").
    Handle { name: String, mode: HandleMode },
}

impl Ty {
    pub fn handle(name: impl Into<String>, mode: HandleMode) -> Ty {
        Ty::Handle { name: name.into(), mode }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Text => f.write_str("text"),
            Ty::Int => f.write_str("int"),
            Ty::Bool => f.write_str("bool"),
            Ty::Bytes => f.write_str("bytes"),
            Ty::Date => f.write_str("date"),
            Ty::DateTime => f.write_str("datetime"),
            Ty::Duration => f.write_str("duration"),
            Ty::List(t) => write!(f, "list of {t}"),
            Ty::Maybe(t) => write!(f, "maybe {t}"),
            Ty::Handle { name, mode } => match mode.word() {
                Some(w) => write!(f, "{w} handle of {name}"),
                None => write!(f, "handle of {name}"),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
}

/// The AgentMark-level signature of a binding item. `ret: None` means "returns nothing".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sig {
    pub params: Vec<Param>,
    pub ret: Option<Ty>,
}

/// SPEC 18.3: no line over 120 characters.
pub const MAX_LINE: usize = 120;

/// The lines of a binding item above its code (Q416): the one-line heading `## name(a: T) -> R` when it fits in
/// 120 characters; otherwise `## name` followed by `- in: a: T, b: U` (one `- in:` line per parameter when the
/// parameters together do not fit) and `- out: R`, the words of a `# Function`. `None` when even that does not fit.
pub fn item_lines(name: &str, params: &[String], out: Option<&str>) -> Option<Vec<String>> {
    let one = match out {
        Some(r) => format!("## {name}({}) -> {r}", params.join(", ")),
        None => format!("## {name}({})", params.join(", ")),
    };
    if one.chars().count() <= MAX_LINE {
        return Some(vec![one]);
    }
    let mut lines = vec![format!("## {name}")];
    let joined = format!("- in: {}", params.join(", "));
    if params.is_empty() {
    } else if joined.chars().count() <= MAX_LINE {
        lines.push(joined);
    } else {
        lines.extend(params.iter().map(|p| format!("- in: {p}")));
    }
    if let Some(r) = out {
        lines.push(format!("- out: {r}"));
    }
    lines.iter().all(|l| l.chars().count() <= MAX_LINE).then_some(lines)
}

impl Sig {
    /// The lines of this item above its code (see [`item_lines`]).
    pub fn heading_lines(&self, name: &str) -> Option<Vec<String>> {
        let params: Vec<String> = self.params.iter().map(|p| format!("{}: {}", p.name, p.ty)).collect();
        let ret = self.ret.as_ref().map(|r| r.to_string());
        item_lines(name, &params, ret.as_deref())
    }

    /// `name(a: text, b: int) -> bytes`, the text after `## ` in a binding file.
    pub fn heading(&self, name: &str) -> String {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.ty))
            .collect();
        match &self.ret {
            Some(r) => format!("{name}({}) -> {r}", params.join(", ")),
            None => format!("{name}({})", params.join(", ")),
        }
    }
}

/// How the target classified an item, before any model is involved (SPEC 15.2 table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// Every type maps 1:1: the target already wrote the body by rules. `verify`: the rules
    /// cannot see everything (a handle, another crate's trait), so the type checker has the
    /// last word.
    Direct { sig: Sig, body: String, verify: bool },
    /// Needs the model to write the body for the declared `sig`. `why` says what kept it from
    /// being direct (shown to the model).
    Assisted { sig: Sig, why: String },
    /// Cannot be bound; `reason` is listed under `## Not bound`.
    Skipped { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Unique AgentMark item name (the heading name, and the key of `## Not bound`).
    pub name: String,
    /// The name the `##` heading carries when it is not `name`: a method of a handle is written
    /// with its method name (`update`), since the handle type tells methods apart (SPEC 22.4).
    pub heading: Option<String>,
    /// Where the item lives in the library, for messages and prompts (`chrono::NaiveDate::year`).
    pub path: String,
    /// First sentence of the item's documentation, or empty when there is none.
    pub summary: String,
    /// The item's documentation (possibly trimmed), for the model prompt.
    pub docs: String,
    /// The item's signature in the library's own language, for the model prompt and effect
    /// heuristics.
    pub source_sig: String,
    /// What the body is placed into (for Rust the `async fn` header), shown to the model.
    pub contract: String,
    pub shape: Shape,
}

/// A public thing that is not a callable item but must not be dropped silently (traits,
/// re-exports of foreign items, type aliases to foreign types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotBoundEntry {
    pub name: String,
    pub reason: String,
}

/// Everything a target extracted from one library at one version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateItems {
    pub name: String,
    pub version: String,
    /// Name of the target language, used in prompts (`Rust`).
    pub language: String,
    /// Short style guide handed to the model for assisted items.
    pub style_guide: String,
    /// Fenced-code language tag in the binding file (`rust`).
    pub code_lang: String,
    pub hints: crate::binder::core::effects::EffectHints,
    pub items: Vec<Item>,
    pub extra_not_bound: Vec<NotBoundEntry>,
    /// Handle name -> its Rust type, for the handles that are not simply `<crate>::<Type>`
    /// (types with lifetimes or type parameters, as the signatures instantiate them).
    pub handle_types: std::collections::BTreeMap<String, String>,
}
