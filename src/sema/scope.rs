//! Names (SPEC 18.2): every declaration of the project in one registry, and for each file the
//! scope it sees: its own declarations plus whatever its imports bring in. There is no global
//! scope, and imports are not transitive.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::diag::{Diagnostic, Span};
use crate::project::{Project, ResolvedImport};
use crate::syntax::ast::Decl;

pub const E_DUPLICATE: &str = "E200";
/// A record, enum or type alias with a name another one already has (SPEC 22.1).
pub const E_TYPE_CLASH: &str = "E300";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum Kind {
    Record,
    Enum,
    TypeAlias,
    Rule,
    Function,
    Constant,
    RustBlock,
    Binding,
    /// `# Tool` written in AgentMark (SPEC 22.5).
    Tool,
    Agent,
    Service,
    Command,
}

impl Kind {
    /// "record", "rule", ... for messages.
    pub fn noun(self) -> &'static str {
        match self {
            Kind::Record => "record",
            Kind::Enum => "enum",
            Kind::TypeAlias => "type",
            Kind::Rule => "rule",
            Kind::Function => "function",
            Kind::Constant => "constant",
            Kind::RustBlock => "Rust block",
            Kind::Binding => "binding",
            Kind::Tool => "tool",
            Kind::Agent => "agent",
            Kind::Service => "service",
            Kind::Command => "command",
        }
    }
}

/// Name of a declaration: (kind, declared name). Declared names are unique per kind across the
/// whole project (OPEN_QUESTIONS Q31).
pub type Key = (Kind, String);

#[derive(Debug, Clone)]
pub struct Entity {
    pub kind: Kind,
    pub name: String,
    pub file: usize,
    /// Index into `files[file].ast.decls`.
    pub decl: usize,
    /// Span of the name.
    pub span: Span,
}

impl Entity {
    pub fn key(&self) -> Key {
        (self.kind, self.name.clone())
    }
}

/// `(kind, name, name span)` of a declaration, if it has a name.
pub fn decl_header(d: &Decl) -> Option<(Kind, &str, Span)> {
    Some(match d {
        Decl::Record(x) => (Kind::Record, &x.name.name, x.name.span),
        Decl::Enum(x) => (Kind::Enum, &x.name.name, x.name.span),
        Decl::TypeAlias(x) => (Kind::TypeAlias, &x.name.name, x.name.span),
        Decl::Rule(x) => (Kind::Rule, &x.name.name, x.name.span),
        Decl::Agent(x) => (Kind::Agent, &x.name.name, x.name.span),
        Decl::Function(x) => (Kind::Function, &x.name.name, x.name.span),
        Decl::Service(x) => (Kind::Service, &x.name.name, x.name.span),
        Decl::Command(x) => (Kind::Command, &x.name.name, x.name.span),
        Decl::Binding(x) => (Kind::Binding, &x.name.name, x.name.span),
        Decl::Tool(x) => (Kind::Tool, &x.name.name, x.name.span),
        Decl::Constant(x) => (Kind::Constant, &x.name.name, x.name.span),
        Decl::RustBlock(x) => (Kind::RustBlock, &x.name.name, x.name.span),
        Decl::Run(_) | Decl::Entry(_) => return None,
    })
}

#[derive(Debug, Default)]
pub struct Registry {
    pub ents: BTreeMap<Key, Entity>,
}

impl Registry {
    pub fn get(&self, kind: Kind, name: &str) -> Option<&Entity> {
        self.ents.get(&(kind, name.to_string()))
    }
    pub fn of_kind(&self, kind: Kind) -> impl Iterator<Item = &Entity> {
        self.ents.values().filter(move |e| e.kind == kind)
    }
}

/// What one file sees. A visible name (`overdue`, `billing.overdue`) can stand for several
/// declarations of different kinds (a record and a rule may share a word).
#[derive(Debug, Default)]
pub struct Scope {
    /// visible name -> (declaration, index of the import that brought it, if any)
    pub names: HashMap<String, Vec<(Key, Option<usize>)>>,
}

impl Scope {
    pub fn find(&self, visible: &str, kinds: &[Kind]) -> Option<(Key, Option<usize>)> {
        self.names
            .get(visible)?
            .iter()
            .find(|(k, _)| kinds.contains(&k.0))
            .cloned()
    }

    /// Visible names of the given kinds, for did-you-mean.
    pub fn names_of(&self, kinds: &[Kind]) -> Vec<String> {
        let mut v: Vec<String> = self
            .names
            .iter()
            .filter(|(_, ents)| ents.iter().any(|(k, _)| kinds.contains(&k.0)))
            .map(|(n, _)| n.clone())
            .collect();
        v.sort();
        v
    }
}

/// Did-you-mean over visible names. A name written without its module prefix (`overdue` for
/// `billing.overdue`) is also recognised.
pub fn suggest(word: &str, candidates: &[String]) -> Option<String> {
    if let Some(hit) = crate::diag::did_you_mean(word, candidates.iter().map(String::as_str)) {
        return Some(hit);
    }
    let suffixes: Vec<(&str, &String)> = candidates
        .iter()
        .filter_map(|c| c.rsplit_once('.').map(|(_, last)| (last, c)))
        .collect();
    if let Some((_, full)) = suffixes.iter().find(|(last, _)| *last == word) {
        return Some((*full).clone());
    }
    let hit = crate::diag::did_you_mean(word, suffixes.iter().map(|(l, _)| *l))?;
    suffixes
        .iter()
        .find(|(l, _)| *l == hit)
        .map(|(_, f)| (*f).clone())
}

/// Build the registry (reporting duplicate declarations) and one scope per file.
pub fn build(p: &Project, diags: &mut Vec<Diagnostic>) -> (Registry, Vec<Scope>) {
    let mut reg = Registry::default();
    for (fi, f) in p.files.iter().enumerate() {
        for (di, d) in f.ast.decls.iter().enumerate() {
            let Some((kind, name, span)) = decl_header(d) else {
                continue;
            };
            let ent = Entity {
                kind,
                name: name.to_string(),
                file: fi,
                decl: di,
                span,
            };
            if let Some(prev) = reg.ents.get(&ent.key()) {
                diags.push(
                    Diagnostic::error(
                        E_DUPLICATE,
                        &f.path,
                        span,
                        format!(
                            "The {} \"{}\" is already declared in {}:{}.",
                            kind.noun(),
                            name,
                            p.files[prev.file].path,
                            prev.span.line
                        ),
                    )
                    .with_help("Names are unique across the project; rename one of them."),
                );
                continue;
            }
            reg.ents.insert(ent.key(), ent);
        }
    }

    // A record, an enum and a type alias are all written where a type is expected, so they share
    // one namespace (SPEC 22.1).
    let mut types: Vec<&Entity> = reg
        .ents
        .values()
        .filter(|e| matches!(e.kind, Kind::Record | Kind::Enum | Kind::TypeAlias))
        .collect();
    types.sort_by_key(|e| (e.file, e.decl));
    let mut first_of: HashMap<&str, &Entity> = HashMap::new();
    for e in types {
        match first_of.get(e.name.as_str()) {
            Some(prev) => diags.push(
                Diagnostic::error(
                    E_TYPE_CLASH,
                    &p.files[e.file].path,
                    e.span,
                    format!(
                        "The {} \"{}\" has the same name as the {} in {}:{}.",
                        e.kind.noun(),
                        e.name,
                        prev.kind.noun(),
                        p.files[prev.file].path,
                        prev.span.line
                    ),
                )
                .with_help("Records, enums and types share one namespace; rename one of them."),
            ),
            None => {
                first_of.insert(e.name.as_str(), e);
            }
        }
    }

    let mut scopes = Vec::new();
    for (fi, f) in p.files.iter().enumerate() {
        let mut scope = Scope::default();
        let put = |scope: &mut Scope, visible: String, key: Key, via: Option<usize>| {
            let slot = scope.names.entry(visible).or_default();
            if !slot.iter().any(|(k, _)| *k == key) {
                slot.push((key, via));
            }
        };
        for d in &f.ast.decls {
            if let Some((kind, name, _)) = decl_header(d) {
                put(&mut scope, name.to_string(), (kind, name.to_string()), None);
            }
        }
        for (ii, imp) in f.imports.iter().enumerate() {
            let sources: Vec<(usize, Option<String>)> = match imp {
                ResolvedImport::Files(v) => v.iter().map(|i| (i.file, i.prefix.clone())).collect(),
                ResolvedImport::Crate {
                    binding: Some(b), ..
                } => vec![(*b, None)],
                _ => continue,
            };
            for (file, prefix) in sources {
                for d in &p.files[file].ast.decls {
                    if let Some((kind, name, _)) = decl_header(d) {
                        let visible = match &prefix {
                            Some(pre) => format!("{pre}.{name}"),
                            None => name.to_string(),
                        };
                        put(&mut scope, visible, (kind, name.to_string()), Some(ii));
                    }
                }
            }
        }
        let _ = fi;
        scopes.push(scope);
    }
    (reg, scopes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(name: &str) -> String {
        format!("# Record: {name}\n- id: text\n")
    }

    #[test]
    fn a_file_sees_its_own_and_imported_names_only() {
        let main = format!("# Import: ./a.agent.md\n\n{}", rec("M"));
        let p = Project::from_memory(
            "app/main.agent.md",
            vec![
                ("app/main.agent.md", &main),
                (
                    "app/a.agent.md",
                    &format!("# Import: ./b.agent.md\n\n{}", rec("A")),
                ),
                ("app/b.agent.md", &rec("B")),
            ],
        );
        let mut d = Vec::new();
        let (reg, scopes) = build(&p, &mut d);
        assert!(d.is_empty());
        assert_eq!(reg.ents.len(), 3);
        let main_scope = &scopes[0];
        assert!(main_scope.find("M", &[Kind::Record]).is_some());
        assert!(main_scope.find("A", &[Kind::Record]).is_some());
        // imports are not transitive
        assert!(main_scope.find("B", &[Kind::Record]).is_none());
    }

    #[test]
    fn aliased_imports_get_a_prefix() {
        let p = Project::from_memory(
            "app/main.agent.md",
            vec![
                (
                    "app/main.agent.md",
                    &format!("# Import: ./a.agent.md as billing\n\n{}", rec("M")),
                ),
                ("app/a.agent.md", &rec("A")),
            ],
        );
        let mut d = Vec::new();
        let (_, scopes) = build(&p, &mut d);
        assert!(scopes[0].find("billing.A", &[Kind::Record]).is_some());
        assert!(scopes[0].find("A", &[Kind::Record]).is_none());
    }

    #[test]
    fn duplicate_declarations_are_reported_at_the_second() {
        let p = Project::from_str(
            "x.agent.md",
            "# Record: A\n- id: text\n\n# Record: A\n- id: text\n",
            "",
        );
        let mut d = Vec::new();
        build(&p, &mut d);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].span.line, 4);
        assert!(
            d[0].message.contains("already declared in x.agent.md:1"),
            "{}",
            d[0].message
        );
    }
}
