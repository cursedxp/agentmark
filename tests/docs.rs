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
//! | `agentmark` | a **complete program**: the first line that is not blank or a `>` comment must be a `# ` heading, and `check` (the library, against `examples/tools.toml`) must report 0 errors. A single-file program may hold several declarations. It also has to be in the canonical style `fmt` writes. It sees the `tools.toml` and the `bindings/` files named by earlier blocks of the same doc (see `name=`). |
//! | `agentmark invalid` | a complete program that is **meant to fail**: `check` must report at least one error (used where the text says "this is an error") |
//! | `agentmark fragment` | a **fragment**: only *parsed*, inside the minimal wrapper below |
//! | `agent.md` | same as `agentmark` (the older spelling) |
//! | anything else (`text`, `toml`, `json`, `rust`, `bash`, `console`) | not an AgentMark sample; the modifiers below still apply |
//!
//! Modifiers (any block, after the first word):
//!
//! * `name=<path>`: the block is the file `<path>` of the tutorial project. Later blocks of the same
//!   doc see the latest version of every named file, so a tutorial can grow one file step by step.
//! * `file=<repo path>`: the block must be byte-for-byte (modulo trailing blank lines) the content
//!   of that file of this repository. This is how the docs quote `examples/` and `fixtures/`.
//! * `cwd=repo` (console blocks): run in the repository root instead of a scratch folder.
//!
//! A `console` block whose first line is `$ agentmark <args>` is **run**; its stdout (or stderr, when
//! the info string also says `stderr`) must equal the rest of the block. Without `cwd=repo` the
//! command runs in a scratch folder that holds every `name=` file written so far, plus
//! `tools.toml` (a copy of `examples/tools.toml` unless the doc names its own).
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

use std::path::{Path, PathBuf};
use std::process::Command;

use agentmark::diag::{render_all, Severity};
use agentmark::project::Project;
use agentmark::sema;
use agentmark::syntax::parser::parse_file;

const TOOLS: &str = include_str!("../examples/tools.toml");

const DOCS: [&str; 4] = ["guide", "reference", "libraries", "internals"];

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
        let words = b.words();
        for w in words.iter().skip(1) {
            let known = matches!(*w, "fragment" | "invalid" | "stderr")
                || w.starts_with("name=")
                || w.starts_with("file=")
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
        assert!(
            check_complete(&b.body, &ctx).is_err(),
            "{}: tagged invalid but check reports no error:\n{}",
            b.at(),
            b.body
        );
        n += 1;
    }
    assert!(n >= 3, "the guide and reference should show a few real mistakes, found {n}");
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
                    let p = scratch.path().join(n);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(p, format!("{}\n", trim_end_lines(text))).unwrap();
                }
                scratch.path().to_path_buf()
            };
            let out = Command::new(exe)
                .args(args.split_whitespace())
                .current_dir(&cwd)
                .output()
                .unwrap();
            let got = if b.has_word("stderr") {
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
    let codegen_present = Path::new("src/codegen").exists();
    let mut missing = Vec::new();
    for t in &tokens {
        // The code generator is written in a parallel worktree; its paths are checked as soon as it is merged.
        if !codegen_present && t.starts_with("src/codegen") {
            continue;
        }
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
