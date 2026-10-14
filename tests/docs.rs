//! The documentation cannot silently rot (PROMPT step 12): every code sample in `docs/*.md` is
//! extracted and run against the real front end, and the claims the docs make about themselves
//! (vocabulary of the guide, file paths of the internals page, section order of the reference)
//! are tests too.
//!
//! # What a fenced block in `docs/*.md` may be
//!
//! A block is fenced with three or more backticks (a longer fence is used when the block itself
//! contains a fence, for example a `## Rust` body). The first word of the info string decides
//! what the test does with it:
//!
//! | info string | the test |
//! |---|---|
//! | `agentmark` | a **complete program**: the first line that is not blank or a `>` comment must be a `# ` heading, and `check` (the library, against `examples/01-invoice-chaser/tools.toml`) must report 0 errors. A single-file program may hold several declarations. It also has to be in the canonical style `fmt` writes. It sees the `tools.toml` and the `bindings/` files named by earlier blocks of the same doc (see `name=`). |
//! | `agentmark invalid` | a complete program that is **meant to fail**: `check` must report at least one error (used where the text says "this is an error") |
//! | `agentmark fragment` | a **fragment**: parsed inside the minimal wrapper below, then checked with errors about names it cannot see (other records, actions, files) ignored. |
//! | `agent.md` | same as `agentmark` (the older spelling) |
//! | anything else (`text`, `toml`, `json`, `rust`, `bash`, `console`) | not an AgentMark sample; the modifiers below still apply |
//!
//! Modifiers (any block, after the first word):
//!
//! * `name=<path>`: the block is the file `<path>` of the tutorial project. Later blocks of the same
//!   doc see the latest version of every named file, so a tutorial can grow one file step by step.
//! * `file=<repo path>`: the block must be byte-for-byte (modulo trailing blank lines) the content
//!   of that file in the AgentMark source. This is how the docs quote `examples/` and `fixtures/`.
//! * `cwd=repo` (console blocks): run in the repository root instead of a scratch folder.
//!
//! A `console` block whose first line is `$ agentmark <args>` is **run**; its stdout (or stderr, when
//! the info string also says `stderr`) must equal the rest of the block. Without `cwd=repo` the
//! command runs in a scratch folder that holds every `name=` file written so far, plus
//! `tools.toml` (a copy of `examples/01-invoice-chaser/tools.toml` unless the doc names its own).
//!
//! # The fragment wrapper rule
//!
//! A fragment is never checked, only parsed, after the text is put into the smallest context in
//! which its first line is legal. The first line decides:
//!
//! * starts with `# ` (a declaration or import): parsed as a whole file, no wrapper;
//! * starts with `## GET|POST|PUT|PATCH|DELETE `: inside `# Service: doc-service`;
//! * starts with `## Run`: inside `# Command: doc-command`;
//! * starts with `## Rust`: inside `# Function: doc_host`;
//! * starts with `## Not bound`: inside `# Binding: doc` (a library with one effect line);
//! * starts with any other `## ` (a trigger or `## Do`): inside `# Agent: doc-agent`;
//! * anything else (statements, `- ` lines, `for each`, `if`, `stop`): the body of `## Do doc` in
//!   `# Agent: doc-agent`.
//!
//! A fragment that does not parse in its wrapper fails the test and the message shows the wrapped
//! text, so the author sees what the parser saw. Expressions are shown as the statement that
//! contains them (`- set x = ...`).

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use support::OutputLocked;

use agentmark::diag::{render_all, Severity};
use agentmark::project::Project;
use agentmark::sema;
use agentmark::syntax::parser::parse_file;

const TOOLS: &str = include_str!("../examples/01-invoice-chaser/tools.toml");

const DOCS: [&str; 5] = ["guide", "reference", "libraries", "internals", "editor"];

// ---------------------------------------------------------------- extraction

#[derive(Debug, Clone)]
struct Block {
    doc: String,
    /// 1-based line of the opening fence.
    line: usize,
    info: String,
    body: String,
}

impl Block {
    fn words(&self) -> Vec<&str> {
        self.info.split_whitespace().collect()
    }
    fn lang(&self) -> &str {
        self.words().first().copied().unwrap_or("")
    }
    fn modifier(&self, key: &str) -> Option<String> {
        let prefix = format!("{key}=");
        self.words()
            .iter()
            .skip(1)
            .find_map(|w| w.strip_prefix(prefix.as_str()).map(str::to_string))
    }
    fn has_word(&self, word: &str) -> bool {
        self.words().iter().skip(1).any(|w| *w == word)
    }
    fn is_agentmark(&self) -> bool {
        matches!(self.lang(), "agentmark" | "agent.md")
    }
    fn at(&self) -> String {
        format!("docs/{}.md:{}", self.doc, self.line)
    }
}

fn doc_path(name: &str) -> PathBuf {
    Path::new("docs").join(format!("{name}.md"))
}

fn read_doc(name: &str) -> String {
    std::fs::read_to_string(doc_path(name)).unwrap_or_else(|e| panic!("docs/{name}.md: {e}"))
}

/// Length of the backtick run that opens a fence on this line, if any.
fn fence_len(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '`').count();
    (n >= 3).then(|| (n, t[n..].trim()))
}

/// Every fenced block of a doc. A fence closes only on a line of at least as many backticks and
/// nothing else, so a ```` fence can hold a ``` fence (a `## Rust` body).
fn blocks_of(doc: &str) -> Vec<Block> {
    let text = read_doc(doc);
    let mut out = Vec::new();
    // (fence length, opening line, info string, indentation of the fence, body lines)
    let mut open: Option<(usize, usize, String, usize, Vec<String>)> = None;
    for (i, line) in text.lines().enumerate() {
        match &mut open {
            None => {
                if let Some((n, info)) = fence_len(line) {
                    let indent = line.len() - line.trim_start().len();
                    open = Some((n, i + 1, info.to_string(), indent, Vec::new()));
                }
            }
            Some((n, start, info, indent, body)) => {
                let t = line.trim();
                if t.len() >= *n && t.chars().all(|c| c == '`') {
                    out.push(Block {
                        doc: doc.to_string(),
                        line: *start,
                        info: info.clone(),
                        body: body.join("\n"),
                    });
                    open = None;
                } else {
                    // a fence inside a list item is indented; its content is dedented by the same amount
                    let strip = line.len() - line.trim_start().len();
                    body.push(line[strip.min(*indent)..].to_string());
                }
            }
        }
    }
    if let Some((_, start, ..)) = open {
        panic!("docs/{doc}.md:{start}: fenced block is never closed");
    }
    out
}

fn all_blocks() -> Vec<Block> {
    DOCS.iter().flat_map(|d| blocks_of(d)).collect()
}

fn agentmark_blocks() -> Vec<Block> {
    all_blocks().into_iter().filter(Block::is_agentmark).collect()
}

/// Every agentmark block with the named files that earlier blocks of its doc have written.
fn agentmark_blocks_in_context() -> Vec<(Block, Context)> {
    let mut out = Vec::new();
    for doc in DOCS {
        let mut named: Context = Vec::new();
        for b in blocks_of(doc) {
            if b.is_agentmark() {
                out.push((b.clone(), named.clone()));
            }
            if let Some(name) = b.modifier("name") {
                named.retain(|(n, _)| *n != name);
                named.push((name, b.body.clone()));
            }
        }
    }
    out
}

// ---------------------------------------------------------------- helpers

/// The files a program in a doc can see besides itself: `tools.toml` (the examples' unless the doc
/// names its own) and the `bindings/` files named by earlier blocks of the same doc.
type Context = Vec<(String, String)>;

fn check_complete(src: &str, ctx: &Context) -> Result<(), String> {
    let mut files: Vec<(&str, &str)> = vec![("doc.agent.md", src)];
    if !ctx.iter().any(|(n, _)| n == "tools.toml") {
        files.push(("tools.toml", TOOLS));
    }
    for (n, t) in ctx {
        if n == "tools.toml" || n.starts_with("bindings/") {
            files.push((n.as_str(), t.as_str()));
        }
    }
    let project = Project::from_memory("doc.agent.md", files);
    match sema::check(&project) {
        Ok(_) => Ok(()),
        Err(d) => {
            let errors: Vec<_> = d.into_iter().filter(|x| x.severity == Severity::Error).collect();
            if errors.is_empty() {
                Ok(())
            } else {
                Err(project.render(&errors))
            }
        }
    }
}

fn first_content_line(src: &str) -> Option<&str> {
    src.lines().map(str::trim_end).find(|l| !l.trim().is_empty() && !l.starts_with('>'))
}

fn wrapped(fragment: &str) -> String {
    let first = first_content_line(fragment).unwrap_or("");
    let agent = "# Agent: doc-agent\n- model: claude\n- tools: email, crm, http, log\n\n";
    let head = if first.starts_with("# ") {
        ""
    } else if ["## GET ", "## POST ", "## PUT ", "## PATCH ", "## DELETE "].iter().any(|m| first.starts_with(m)) {
        "# Service: doc-service\n\n"
    } else if first.starts_with("## Run") {
        "# Command: doc-command\n\n"
    } else if first.starts_with("## Rust") {
        "# Function: doc_host\n- out: text\n- effect: pure\n  - return \"x\"\n\n"
    } else if first.starts_with("## Not bound") {
        "# Binding: doc\n- crate: doc\n- effect: unverified\n\n"
    } else if first.starts_with("## ") {
        agent
    } else {
        return format!("{agent}## Do doc\n{fragment}\n");
    };
    format!("{head}{fragment}\n")
}

/// A quoted project file that imports its neighbours cannot be checked on its own (the folder
/// projects are checked as projects by `tests/sema_examples.rs`).
fn needs_other_files(b: &Block) -> bool {
    b.modifier("file").is_some() && b.body.lines().any(|l| l.starts_with("# Import: ."))
}

fn trim_end_lines(s: &str) -> String {
    s.trim_end_matches(['\n', ' ']).to_string()
}

// ---------------------------------------------------------------- the samples

#[test]
fn every_doc_exists_and_has_samples() {
    for d in DOCS {
        assert!(doc_path(d).exists(), "docs/{d}.md is missing");
    }
    for d in ["guide", "reference", "libraries"] {
        assert!(
            blocks_of(d).iter().any(Block::is_agentmark),
            "docs/{d}.md has no agentmark sample"
        );
    }
}

#[test]
fn info_strings_are_known() {
    for b in all_blocks() {
        assert!(!b.info.is_empty(), "{}: a fenced block with no info string (is a fence closing early? give it text/console/...)", b.at());
        let words = b.words();
        for w in words.iter().skip(1) {
            let known = matches!(*w, "fragment" | "invalid" | "stderr" | "both" | "fresh")
                || w.starts_with("name=")
                || w.starts_with("file=")
                || w.starts_with("says=")
                || w.starts_with("env=")
                || *w == "cwd=repo";
            assert!(known, "{}: unknown word \"{w}\" in the info string \"{}\"", b.at(), b.info);
        }
        if b.has_word("fragment") || b.has_word("invalid") {
            assert!(b.is_agentmark(), "{}: fragment/invalid belong on agentmark blocks", b.at());
        }
        if b.lang().starts_with("agent") && !b.is_agentmark() {
            panic!("{}: \"{}\" looks like a typo for agentmark", b.at(), b.lang());
        }
    }
}

#[test]
fn complete_programs_check_without_errors() {
    let mut n = 0;
    let mut failures: Vec<String> = Vec::new();
    for (b, ctx) in agentmark_blocks_in_context() {
        if b.has_word("fragment") || b.has_word("invalid") || needs_other_files(&b) {
            continue;
        }
        let first = first_content_line(&b.body).unwrap_or("");
        assert!(
            first.starts_with("# "),
            "{}: a complete program starts with a declaration heading; tag the block \"agentmark fragment\" \
             if it is a piece (first line: {first:?})",
            b.at()
        );
        if let Err(rendered) = check_complete(&b.body, &ctx) {
            failures.push(format!("{}: should check with 0 errors:\n{rendered}", b.at()));
        }
        n += 1;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(n >= 15, "expected many complete programs in the docs, found {n}");
}

#[test]
fn complete_programs_are_in_the_canonical_style() {
    let mut failures: Vec<String> = Vec::new();
    for b in agentmark_blocks() {
        if b.has_word("fragment") || b.has_word("invalid") || needs_other_files(&b) {
            continue;
        }
        let src = format!("{}\n", trim_end_lines(&b.body));
        match agentmark::fmt::format_source(&src, "doc.agent.md") {
            Ok(formatted) if formatted == src => {}
            Ok(formatted) => failures.push(format!(
                "{}: not in the canonical style; `agentmark fmt` writes:\n{formatted}",
                b.at()
            )),
            Err(d) => failures.push(format!("{}: does not parse:\n{}", b.at(), render_all(&d, &src))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn invalid_programs_really_fail() {
    let mut n = 0;
    for (b, ctx) in agentmark_blocks_in_context().into_iter().filter(|(b, _)| b.has_word("invalid")) {
        let err = check_complete(&b.body, &ctx)
            .expect_err(&format!("{}: tagged invalid but check reports no error:\n{}", b.at(), b.body));
        if let Some(says) = b.modifier("says") {
            let says = says.replace('_', " ");
            assert!(err.contains(&says), "{}: expected the error to say \"{says}\", got:\n{err}", b.at());
        }
        n += 1;
    }
    assert!(n >= 3, "the guide and reference should show a few real mistakes, found {n}");
}

/// A fragment refers to records, rules and variables that are not in it, so errors about names that
/// cannot be found are expected; any other error (a wrong type, a bad verb, a purity violation inside
/// the fragment itself) is a mistake in the doc.
fn fragment_semantic_errors(src: &str) -> Option<String> {
    let project = Project::from_str("fragment.agent.md", src, TOOLS);
    let Err(diags) = sema::check(&project) else { return None };
    // Only names a fragment cannot see are ignored: missing imports, unknown names/types, and an unknown `do`
    // action or function (E209 also covers wrong arguments, so it is matched by its message). The wrapper
    // lists every manifest tool, so E207 (unknown tool/verb/target/event, missing verb) and E208 stay errors.
    const UNRESOLVED: [&str; 5] = ["E111", "E112", "E113", "E201", "E202"];
    let unresolved = |d: &agentmark::diag::Diagnostic| {
        UNRESOLVED.contains(&d.code) || (d.code == "E209" && d.message.starts_with("Unknown action or function"))
    };
    let rest: Vec<_> = diags.into_iter().filter(|d| d.severity == Severity::Error && !unresolved(d)).collect();
    (!rest.is_empty()).then(|| project.render(&rest))
}

/// The semantic check of fragments must keep catching mistakes inside the fragment itself.
#[test]
fn fragment_check_still_catches_tool_mistakes() {
    for bad in ["- crm delete invoice: x", "- email send custmer: \"x\"", "- crm: \"x\"", "- email teleport customer: \"x\""] {
        let src = wrapped(bad);
        assert!(fragment_semantic_errors(&src).is_some(), "{bad:?} should be an error in a fragment");
    }
    assert!(fragment_semantic_errors(&wrapped("- log: \"fine\"")).is_none());
}

#[test]
fn fragments_parse_inside_the_wrapper() {
    let mut n = 0;
    let mut failures: Vec<String> = Vec::new();
    for b in agentmark_blocks().into_iter().filter(|b| b.has_word("fragment")) {
        let src = wrapped(&b.body);
        if let Err(d) = parse_file("fragment.agent.md", &src) {
            failures.push(format!("{}: fragment does not parse:\n{}", b.at(), render_all(&d, &src)));
        }
        else if let Some(rendered) = fragment_semantic_errors(&src) {
            failures.push(format!("{}: fragment has errors beyond unresolved names:\n{rendered}", b.at()));
        }
        n += 1;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(n >= 8, "the reference shows several fragments, found {n}");
}

#[test]
fn quoted_files_match_the_repository() {
    let mut n = 0;
    for b in all_blocks() {
        let Some(path) = b.modifier("file") else { continue };
        let real = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: file={path}: {e}", b.at()));
        // A quoted `tools.toml` may leave out the file's leading comment lines.
        let real = if b.lang() == "toml" && !b.body.starts_with('#') {
            real.lines().skip_while(|l| l.starts_with('#') || l.trim().is_empty()).collect::<Vec<_>>().join("\n")
        } else {
            real
        };
        assert_eq!(
            trim_end_lines(&b.body),
            trim_end_lines(&real),
            "{}: the block no longer matches {path}",
            b.at()
        );
        n += 1;
    }
    assert!(n >= 5, "the docs should quote examples/ files, found {n}");
}

/// Runs `$ agentmark ...` console blocks and compares the output with what the doc prints.
#[test]
fn console_output_is_real() {
    let exe = env!("CARGO_BIN_EXE_agentmark");
    let repo = std::env::current_dir().unwrap();
    let mut ran = 0;
    for doc in DOCS {
        let mut named: Vec<(String, String)> = Vec::new();
        for b in blocks_of(doc) {
            if let Some(name) = b.modifier("name") {
                named.retain(|(n, _)| *n != name);
                named.push((name, b.body.clone()));
            }
            if b.lang() != "console" {
                continue;
            }
            let mut lines = b.body.lines();
            let cmd = lines.next().unwrap_or("");
            let Some(args) = cmd.strip_prefix("$ agentmark ") else {
                panic!("{}: a console block starts with \"$ agentmark ...\"", b.at())
            };
            let expected = lines.collect::<Vec<_>>().join("\n");
            let scratch = tempfile::tempdir().unwrap();
            let cwd = if b.modifier("cwd").as_deref() == Some("repo") {
                repo.clone()
            } else {
                if !named.iter().any(|(n, _)| n == "tools.toml") {
                    std::fs::write(scratch.path().join("tools.toml"), TOOLS).unwrap();
                }
                for (n, text) in &named {
                    if b.has_word("fresh") && n.starts_with("bindings/") {
                        continue;
                    }
                    let p = scratch.path().join(n);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(p, format!("{}\n", trim_end_lines(text))).unwrap();
                }
                scratch.path().to_path_buf()
            };
            let mut cmd = Command::new(exe);
            cmd.args(args.split_whitespace()).current_dir(&cwd);
            // keep what the compiler caches and builds out of the system temp folder
            cmd.env("TMPDIR", support::compiler_tmp()).env("CARGO_TARGET_DIR", support::gen_target());
            for w in b.words().iter().skip(1) {
                if let Some(kv) = w.strip_prefix("env=") {
                    let (k, v) = kv.split_once('=').expect("env=KEY=VALUE");
                    cmd.env(k, v.replace("$REPO", repo.to_str().unwrap()));
                }
            }
            let out = cmd.output_locked();
            let got = if b.has_word("both") {
                format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout))
            } else if b.has_word("stderr") {
                String::from_utf8_lossy(&out.stderr).to_string()
            } else {
                String::from_utf8_lossy(&out.stdout).to_string()
            };
            assert_eq!(
                trim_end_lines(&got),
                trim_end_lines(&expected),
                "{}: `agentmark {args}` prints something else now",
                b.at()
            );
            ran += 1;
        }
    }
    assert!(ran >= 6, "the docs should show real command output, found {ran} verified blocks");
}

// ---------------------------------------------------------------- claims about the pages

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

#[test]
fn guide_never_uses_the_programmer_words() {
    let text = read_doc("guide");
    let ws = words(&text);
    for banned in ["compiler", "ir", "binding"] {
        assert!(
            !ws.iter().any(|w| w == banned),
            "docs/guide.md uses the word \"{banned}\"; say \"library\", \"what it can access\" or \"safety check\""
        );
    }
    let lower = text.to_lowercase();
    assert!(!lower.contains("effect class"), "docs/guide.md says \"effect class\"");
    // the bigger family of the same idea: the guide is for someone who never programmed
    for w in ["compile", "compiles", "compiled", "bindings"] {
        assert!(!ws.iter().any(|x| x == w), "docs/guide.md uses \"{w}\"");
    }
}

#[test]
fn guide_vocabulary_check_catches_what_it_should() {
    // the checker itself: whole words, any case
    assert!(words("The Compiler, the IR.").contains(&"compiler".to_string()));
    assert!(words("The Compiler, the IR.").contains(&"ir".to_string()));
    assert!(!words("a directory of irises").contains(&"ir".to_string()));
}

/// Backticked tokens that look like repository paths.
fn path_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if fence_len(line).is_some() {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let parts: Vec<&str> = line.split('`').collect();
        for (i, p) in parts.iter().enumerate() {
            if i % 2 == 0 {
                continue;
            }
            let ok_chars = !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "_./-".contains(c));
            let has_ext = [".rs", ".md", ".toml", ".json", ".sh", ".py", ".jsonl"].iter().any(|e| p.ends_with(e));
            let is_dir = p.ends_with('/');
            if ok_chars && (has_ext || is_dir) && !p.starts_with("http") && !p.starts_with('.') {
                out.push(p.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn internals_mentions_only_files_that_exist() {
    let text = read_doc("internals");
    let tokens = path_tokens(&text);
    assert!(tokens.len() >= 40, "internals.md should name many files, found {}", tokens.len());
    let mut missing = Vec::new();
    for t in &tokens {
        if !Path::new(t).exists() {
            missing.push(t.clone());
        }
    }
    assert!(missing.is_empty(), "docs/internals.md names files that do not exist: {missing:?}");
}

#[test]
fn path_extraction_is_sane() {
    let t = path_tokens("see `src/sema/mod.rs` and `x.rs`, not `a b.rs`, not `fn main()`, dir `src/ir/`.\n```\n`src/no.rs`\n```\n");
    assert_eq!(t, vec!["src/ir/", "src/sema/mod.rs", "x.rs"]);
}

fn numbered_h2(text: &str) -> Vec<u32> {
    text.lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("## ")?;
            let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            (!num.is_empty() && rest[num.len()..].starts_with('.')).then(|| num.parse().unwrap())
        })
        .collect()
}

#[test]
fn reference_has_one_section_per_spec_section_in_order() {
    let spec = std::fs::read_to_string("SPEC.md").unwrap();
    let reference = read_doc("reference");
    assert_eq!(
        numbered_h2(&reference),
        numbered_h2(&spec),
        "docs/reference.md must have one \"## N. Title\" section per SPEC.md section, same order"
    );
}

/// Every statement form of SPEC 4, 13, 14, 16, 17 appears in a reference sample.
#[test]
fn reference_shows_every_statement_form() {
    let samples: String = blocks_of("reference")
        .into_iter()
        .filter(Block::is_agentmark)
        .map(|b| b.body)
        .collect::<Vec<_>>()
        .join("\n");
    let forms = [
        "for each ",
        "for each overdue ",
        "- if ",
        "- else:",
        "is some ",
        "stop",
        "- do ",
        "ask model:",
        "ask model with ",
        " as Qualification ->",
        "- guard: ",
        " else respond ",
        "no amounts changed in",
        " is not empty",
        " matches ",
        " length < ",
        "- set ",
        "- return ",
        "- respond ",
        "read body as",
        "read file ",
        "- append ",
        "- print: ",
        "- try ",
        "otherwise:",
        "- parallel:",
        "- log: ",
        "email send address",
        "crm update ",
        "http post ",
        "count of ",
        "first ",
        "## Rust: ",
        "## Not bound",
        "# Import: crate ",
        "# Constant: ",
        "# Run",
        "## Every ",
        "## When ",
        "## On start",
        "## Do ",
        "## GET ",
        "- pdf render ",
    ];
    for f in forms {
        assert!(samples.contains(f), "docs/reference.md never shows the form {f:?} in a sample");
    }
}

/// SPEC 15.4 and PROMPT step 12: every bound item of a binding file has a one-line description above its
/// heading, taken from the library's own documentation. The binding files the docs show and the one that
/// ships with example 05 must keep that shape.
#[test]
fn bound_items_carry_a_description_line() {
    fn bound_headings(text: &str) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        let mut in_fence = false;
        for (i, l) in text.lines().enumerate() {
            if l.trim_start().starts_with("```") {
                in_fence = !in_fence;
            }
            if !in_fence && l.starts_with("## ") && l.contains('(') && !l.starts_with("## Not bound") {
                out.push((i, l.to_string()));
            }
        }
        out
    }
    let mut checked = 0;
    let mut files: Vec<(String, String)> = Vec::new();
    files.push((
        "examples/05-invoice-service/bindings/sha2.agent.md".into(),
        std::fs::read_to_string("examples/05-invoice-service/bindings/sha2.agent.md").unwrap(),
    ));
    for b in agentmark_blocks() {
        if b.body.lines().any(|l| l.starts_with("# Binding: ")) {
            files.push((b.at(), b.body.clone()));
        }
    }
    for (name, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        for (i, heading) in bound_headings(&text) {
            let above = if i > 0 { lines[i - 1] } else { "" };
            assert!(
                above.starts_with("> ") && above.len() > 3,
                "{name}: \"{heading}\" has no one-line description above it"
            );
            checked += 1;
        }
    }
    assert!(checked >= 4, "expected several bound items, found {checked}");
}

/// The sentence above `sha256_digest` is rustdoc's first sentence for `Digest::digest`.
#[test]
fn sha2_description_matches_the_library_documentation() {
    let digest = std::fs::read_to_string("tests/binder/digest.json").unwrap();
    assert!(digest.contains("Compute hash of `data`."), "the recorded rustdoc of digest changed");
    let binding = std::fs::read_to_string("examples/05-invoice-service/bindings/sha2.agent.md").unwrap();
    assert!(binding.contains("> Compute hash of `data`.\n## sha256_digest(data: bytes) -> bytes"));
}

/// Every `$ agentmark` line of a doc is the first line of a `console` block, and so is executed.
#[test]
fn every_transcript_is_executed() {
    for doc in DOCS {
        let text = read_doc(doc);
        let lines = text.lines().filter(|l| l.trim_start().starts_with("$ agentmark")).count();
        let executed = blocks_of(doc)
            .iter()
            .filter(|b| b.lang() == "console" && b.body.starts_with("$ agentmark "))
            .count();
        assert_eq!(lines, executed, "docs/{doc}.md has `$ agentmark` lines outside an executed console block");
    }
}

/// docs/check-json.md: the sample is JSON and has the keys the real output has.
#[test]
fn check_json_doc_matches_the_real_output() {
    let text = std::fs::read_to_string("docs/check-json.md").unwrap();
    let start = text.find("```json\n").expect("a json sample") + 8;
    let end = start + text[start..].find("\n```").unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text[start..end]).expect("the sample parses as JSON");
    // a program with one warning (an unused record), one unit and one guessed field type
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tools.toml"), TOOLS).unwrap();
    std::fs::write(
        dir.path().join("a.agent.md"),
        "# Record: Order\n- total\n\n> Logs.\n# Agent: logger\n- model: claude\n- tools: log\n\n## On start\n- log: \"hi\"\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_agentmark"))
        .args(["check", "--json", "a.agent.md"])
        .current_dir(dir.path())
        .env("TMPDIR", support::compiler_tmp())
        .env("CARGO_TARGET_DIR", support::gen_target())
        .output_locked();
    let real: serde_json::Value = serde_json::from_slice(&out.stdout).expect("check --json prints JSON");
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(keys(&doc), keys(&real), "top-level keys of docs/check-json.md and `check --json`");
    for section in ["diagnostics", "units", "inferred_fields"] {
        let d = doc[section].as_array().and_then(|a| a.first()).unwrap_or_else(|| panic!("doc sample: {section} is empty"));
        let r = real[section].as_array().and_then(|a| a.first()).unwrap_or_else(|| panic!("real output: {section} is empty"));
        assert_eq!(keys(d), keys(r), "keys of {section}[0]");
    }
    let (d, r) = (&doc["units"][0]["blocks"][0], &real["units"][0]["blocks"][0]);
    assert_eq!(keys(d), keys(r), "keys of units[0].blocks[0]");
    assert_eq!(keys(&doc["units"][0]["effects"]), keys(&real["units"][0]["effects"]), "keys of effects");
}

// ---------------------------------------------------------------- docs/editor.md and templates/

/// Files a doc may name that live in the user's project or editor setup, not in the AgentMark source.
const USER_SIDE_PATHS: [&str; 1] = ["languages/agentmark/"];

/// Paths in the fenced shell commands of `text` (`sh`/`bash` blocks): every word with a `/` that is
/// not an option, a variable, a home path, an absolute path or a placeholder.
fn shell_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_sh = false;
    let mut in_fence = false;
    for line in text.lines() {
        if let Some((_, info)) = fence_len(line) {
            in_fence = !in_fence;
            in_sh = in_fence && matches!(info.split_whitespace().next(), Some("sh" | "bash"));
            continue;
        }
        if !in_sh {
            continue;
        }
        for w in line.split_whitespace() {
            let w = w.trim_matches(|c| c == '"' || c == '\'');
            if w.contains('/') && !w.starts_with(['-', '~', '/', '$', '<', '@']) && !w.contains("://") {
                // `dir/*.scm` names the folder
                let w = w.split('*').next().unwrap();
                out.push(w.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Every path-looking token of `text` with a `/` in it must exist in the repository, in the
/// tree-sitter folder (the queries are named relative to it), or be a known user-side path.
fn missing_paths(text: &str) -> Vec<String> {
    path_tokens(text)
        .into_iter()
        .filter(|t| t.contains('/') && !USER_SIDE_PATHS.contains(&t.as_str()))
        .filter(|t| !Path::new(t).exists() && !Path::new("editors/tree-sitter-agentmark").join(t).exists())
        .collect()
}

#[test]
fn editor_doc_mentions_only_files_that_exist() {
    let text = read_doc("editor");
    assert!(path_tokens(&text).len() >= 8, "docs/editor.md should name the files it talks about");
    let mut missing = missing_paths(&text);
    let shell = shell_paths(&text);
    assert!(shell.len() >= 3, "docs/editor.md should show shell commands with paths, found {shell:?}");
    missing.extend(shell.into_iter().filter(|t| !Path::new(t).exists()));
    assert!(missing.is_empty(), "docs/editor.md names files that do not exist: {missing:?}");
    // the markdown links resolve too
    for rest in text.split("](").skip(1) {
        let target = rest.split(')').next().unwrap();
        if !target.starts_with("http") && !target.starts_with('#') {
            assert!(Path::new("docs").join(target.split('#').next().unwrap()).exists(), "docs/editor.md links to {target}");
        }
    }
}

/// The settings and commands the doc names are the ones the extension declares.
#[test]
fn editor_doc_names_the_real_settings_and_commands() {
    let text = read_doc("editor");
    let pkg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("editors/vscode/package.json").unwrap()).unwrap();
    let props = pkg["contributes"]["configuration"]["properties"].as_object().unwrap();
    for key in props.keys() {
        assert!(text.contains(&format!("`{key}`")), "docs/editor.md does not explain the setting {key}");
    }
    for word in text.split('`').skip(1).step_by(2).filter(|w| w.starts_with("agentmark.") && w.contains('.')) {
        let known = props.contains_key(word) || word.ends_with(".toml");
        assert!(known, "docs/editor.md names `{word}`, which the extension does not declare");
    }
    for c in pkg["contributes"]["commands"].as_array().unwrap() {
        let title = c["title"].as_str().unwrap().trim_start_matches("AgentMark: ");
        assert!(text.contains(title), "docs/editor.md does not mention the command \"{title}\"");
    }
}

/// The complete `agentmark.toml` of the doc is accepted by the compiler and uses exactly the
/// top-level keys of the JSON schema the extension ships.
#[test]
fn editor_doc_toml_is_a_valid_config() {
    let block = blocks_of("editor")
        .into_iter()
        .find(|b| b.modifier("name").as_deref() == Some("agentmark.toml"))
        .expect("docs/editor.md shows a complete agentmark.toml (name=agentmark.toml)");
    let parsed: toml::Table = toml::from_str(&block.body).expect("the sample is TOML");
    let schema: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("editors/vscode/schemas/agentmark.schema.json").unwrap()).unwrap();
    let mut want: Vec<&str> = schema["properties"].as_object().unwrap().keys().map(String::as_str).collect();
    let mut got: Vec<&str> = parsed.keys().map(String::as_str).collect();
    want.sort();
    got.sort();
    assert_eq!(got, want, "the sample shows every key the schema knows");

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("agentmark.toml"), format!("{}\n", block.body)).unwrap();
    std::fs::write(dir.path().join("tools.toml"), TOOLS).unwrap();
    std::fs::write(dir.path().join("main.agent.md"), "> Logs once.\n# Agent: hello\n- model: claude\n- tools: log\n\n## On start\n- log: \"hi\"\n").unwrap();
    let project = Project::load(dir.path().to_str().unwrap());
    let errors: Vec<_> = match sema::check(&project) {
        Ok(_) => Vec::new(),
        Err(d) => d.into_iter().filter(|x| x.severity == Severity::Error).collect(),
    };
    assert!(errors.is_empty(), "docs/editor.md agentmark.toml is rejected:\n{}", project.render(&errors));
}

/// The text rendering of the panel names every unit, kind and tool that `check --json` reports.
#[test]
fn editor_doc_panel_matches_check_json() {
    let block = blocks_of("editor")
        .into_iter()
        .find(|b| b.body.starts_with("What can this program touch"))
        .expect("docs/editor.md shows the panel as text");
    let out = Command::new(env!("CARGO_BIN_EXE_agentmark"))
        .args(["check", "--json", "examples/01-invoice-chaser"])
        .env("TMPDIR", support::compiler_tmp())
        .env("CARGO_TARGET_DIR", support::gen_target())
        .output_locked();
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("check --json prints JSON");
    assert!(block.body.contains("Check passed for examples/01-invoice-chaser"));
    for u in report["units"].as_array().unwrap() {
        let head = format!("{}  {}  [{}]", u["name"].as_str().unwrap(), u["kind"].as_str().unwrap(), u["effects"]["class"].as_str().unwrap());
        assert!(block.body.contains(&head), "the panel sample lacks the heading \"{head}\"");
        let tools = &u["effects"]["tools"];
        for n in tools["network"].as_array().unwrap().iter().chain(tools["local"].as_array().unwrap()) {
            assert!(block.body.contains(n.as_str().unwrap()), "the panel sample lacks the tool {n}");
        }
        for b in u["blocks"].as_array().unwrap() {
            assert!(block.body.contains(b["label"].as_str().unwrap()), "the panel sample lacks the block {}", b["label"]);
        }
    }
}

const TEMPLATES: [&str; 2] = ["templates/CLAUDE.md", "templates/.cursorrules"];

/// Both templates send the assistant to docs/reference.md, and every `agentmark <command>` they
/// show is a command `agentmark --help` lists.
#[test]
fn templates_point_at_the_reference_and_show_real_commands() {
    let help = Command::new(env!("CARGO_BIN_EXE_agentmark"))
        .arg("--help")
        .output_locked();
    let help = String::from_utf8_lossy(&help.stdout).to_string();
    let commands: Vec<String> = help
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    assert!(commands.len() >= 8, "could not read the command list from --help: {commands:?}");
    for path in TEMPLATES {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(text.contains("docs/reference.md"), "{path} must point at docs/reference.md");
        let mut shown = 0;
        for rest in text.split("agentmark ").skip(1) {
            let word: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
            // prose such as "agentmark.toml" or "agentmark build compiles" is not a command line
            let is_cmd_line = rest.starts_with(&word) && !rest[word.len()..].starts_with('.');
            if word.is_empty() || !is_cmd_line || !text.contains(&format!("`agentmark {word}")) {
                continue;
            }
            assert!(commands.contains(&word), "{path} shows `agentmark {word}`, which `agentmark --help` does not list");
            shown += 1;
        }
        assert!(shown >= 5, "{path} should show the workflow commands, found {shown}");
        for c in ["check", "fmt", "lint", "trace"] {
            assert!(text.contains(&format!("agentmark {c}")), "{path} must teach `agentmark {c}`");
        }
        // the other paths are the user's project layout, not the AgentMark source's
    }
    assert!(Path::new("docs/reference.md").exists());
}

/// Every `--flag` a template teaches exists in the `--help` of some command.
#[test]
fn template_flags_exist() {
    let exe = env!("CARGO_BIN_EXE_agentmark");
    let mut help = String::from_utf8_lossy(&Command::new(exe).arg("--help").output_locked().stdout).to_string();
    for cmd in ["check", "fmt", "lint", "doc", "bind", "build", "run", "trace"] {
        help.push_str(&String::from_utf8_lossy(&Command::new(exe).args([cmd, "--help"]).output_locked().stdout));
    }
    for path in TEMPLATES {
        let text = std::fs::read_to_string(path).unwrap();
        let flags: Vec<String> = text
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .filter(|w| w.starts_with("--") && w.len() > 2)
            .map(str::to_string)
            .collect();
        assert!(flags.len() >= 3, "{path} should teach some flags, found {flags:?}");
        for f in flags {
            assert!(help.contains(&f), "{path} teaches {f}, which no `agentmark <command> --help` lists");
        }
    }
}

/// SPEC 17: the effect line of a `## Rust` block is mandatory, so the templates must not forbid it.
#[test]
fn templates_do_not_forbid_the_mandatory_rust_effect_line() {
    for path in TEMPLATES {
        let text = std::fs::read_to_string(path).unwrap().to_lowercase();
        for bad in ["never add an `- effect:`", "never write an `- effect:`", "add an `- effect:` line to a rust block"] {
            assert!(!text.contains(bad), "{path} says \"{bad}\", which contradicts SPEC 17");
        }
        assert!(text.contains("a `## rust` block needs an `- effect:` line"), "{path} must say the Rust block needs an effect line");
        assert!(text.contains("trusted, not verified"), "{path} must say the effect line is trusted, not verified");
    }
}
