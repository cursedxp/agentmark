//! agent_lower (L3 -> L2) and the IR (step 4).

use agentmark::ir::{self, EffectCall, ExprKind as IX, StmtKind as IS};
use agentmark::lower::{agent_lower, Lowered};
use agentmark::project::Project;
use agentmark::sema::{self, Checked};
use agentmark::syntax::ast::*;

fn load(path: &str) -> (Project, Checked) {
    let project = Project::load(path);
    match sema::check(&project) {
        Ok(c) => (project, c),
        Err(d) => panic!("{path}:\n{}", project.render(&d)),
    }
}

fn mem(src: &str) -> (Project, Checked) {
    let tools = std::fs::read_to_string("examples/tools.toml").unwrap();
    let project = Project::from_str("t.agent.md", src, &tools);
    match sema::check(&project) {
        Ok(c) => (project, c),
        Err(d) => panic!("{}", project.render(&d)),
    }
}

// ------------------------------------------------------------ walking the L2 AST

fn walk_stmts(body: &[Stmt], f: &mut dyn FnMut(&Stmt)) {
    for s in body {
        f(s);
        match &s.kind {
            StmtKind::ForEach { body, .. } | StmtKind::Parallel(body) => walk_stmts(body, f),
            StmtKind::If { then, else_, .. } | StmtKind::IfSome { then, else_, .. } => {
                walk_stmts(then, f);
                walk_stmts(else_, f);
            }
            StmtKind::Try { stmt, otherwise } => {
                walk_stmts(std::slice::from_ref(&**stmt), f);
                if let Some(o) = otherwise {
                    walk_stmts(o, f);
                }
            }
            StmtKind::Guard { else_: Some(e), .. } => walk_stmts(std::slice::from_ref(&**e), f),
            _ => {}
        }
    }
}

fn all_bodies(l: &Lowered) -> Vec<&Vec<Stmt>> {
    let mut v = Vec::new();
    for f in &l.files {
        for d in &f.decls {
            match d {
                Decl::Function(x) => v.push(&x.body),
                Decl::Entry(x) => v.push(&x.body),
                Decl::Command(x) => v.push(&x.body),
                Decl::Service(s) => s.routes.iter().for_each(|r| v.push(&r.body)),
                _ => {}
            }
        }
    }
    v
}

fn assert_no_agent_nodes(l: &Lowered) {
    for f in &l.files {
        for d in &f.decls {
            assert!(!matches!(d, Decl::Agent(_) | Decl::Rule(_)), "agent decl left: {d:?}");
        }
    }
    for b in all_bodies(l) {
        walk_stmts(b, &mut |s| match &s.kind {
            StmtKind::AskModel { .. } | StmtKind::Guard { .. } => panic!("L3 stmt left: {s:?}"),
            StmtKind::ForEach { filter_rule, .. } => assert!(filter_rule.is_none()),
            _ => {}
        });
    }
}

/// The serialized IR names no agent concept.
fn assert_ir_clean(p: &ir::Program) {
    let json = serde_json::to_string(p).unwrap();
    for word in ["\"Agent\"", "\"Rule\"", "\"AskModel\"", "\"Guard\":{\"cond\"", "\"Trigger\":{\"Do\""] {
        assert!(!json.contains(word), "{word} in IR");
    }
}

fn lower_example(path: &str) -> (Project, Checked, Lowered, ir::Program) {
    let (p, c) = load(path);
    let l = agent_lower(&p, &c);
    let prog = ir::build::build(&p, &c, &l);
    (p, c, l, prog)
}

fn summary(p: &ir::Program) -> Vec<(String, String, String)> {
    ir::effect_summary(p)
        .into_iter()
        .map(|(u, l, e)| (u, l, e.to_string()))
        .collect()
}

fn entry_summary(p: &ir::Program) -> Vec<String> {
    p.entries.iter().map(|e| format!("{} / {}: {}", e.unit, e.label, e.effect)).collect()
}

// ------------------------------------------------------------ examples

#[test]
fn example_01_lowers() {
    let (_, c, l, p) = lower_example("examples/01-invoice-chaser");
    assert_no_agent_nodes(&l);
    assert_ir_clean(&p);
    assert_eq!(
        entry_summary(&p),
        ["invoice-chaser / Every day at 09:00: model, network(crm, email), local(log)"]
    );
    let remind = p.functions.iter().find(|f| f.name == "invoice_chaser__remind").unwrap();
    assert_eq!(remind.effect.to_string(), "model, network(email, crm), local(log)");
    // the IR agrees with the check, block by block
    for b in c.blocks.iter().filter(|b| b.label == "Every day at 09:00") {
        assert_eq!(b.effect.to_string(), p.entries[0].effect.to_string());
    }
    assert_eq!(p.run, ["invoice-chaser"]);
    let overdue = p.functions.iter().find(|f| f.name == "overdue").unwrap();
    assert_eq!(overdue.ret, Some(sema::Type::Bool));
    assert!(overdue.effect.is_pure());
    // grace_period is inlined as `14 days`
    let json = serde_json::to_string(&overdue.body).unwrap();
    assert!(json.contains("\"Duration\":{\"amount\":14,\"unit\":\"Day\"}"), "{json}");
}

#[test]
fn example_02_lowers() {
    let (_, _, l, p) = lower_example("examples/02-lead-qualifier.agent.md");
    assert_no_agent_nodes(&l);
    assert_ir_clean(&p);
    assert_eq!(
        entry_summary(&p),
        ["lead-qualifier / When crm receives lead: model, network(crm, email), local(log)"]
    );
    let e = &p.entries[0];
    assert_eq!(e.trigger, ir::Trigger::Event { tool: "crm".into(), event: "lead".into() });
    let param = e.param.as_ref().unwrap();
    assert_eq!((param.name.as_str(), param.ty.to_string().as_str()), ("lead", "Lead"));
    let IS::Effect { call: EffectCall::Model(m), into } = &e.body[0].kind else { panic!() };
    assert_eq!(m.schema.as_deref(), Some("Qualification"));
    assert_eq!(m.model.as_deref(), Some("claude:haiku"));
    assert!(m.context.is_some());
    assert_eq!(into.as_ref().unwrap().ty, sema::Type::Record("Qualification".into()));
    // guard: q matches Qualification
    let names = guard_names(&e.body);
    assert_eq!(names, ["q matches Qualification"]);
}

#[test]
fn example_03_lowers() {
    let (_, _, l, p) = lower_example("examples/03-support-triage.agent.md");
    assert_no_agent_nodes(&l);
    assert_ir_clean(&p);
    assert_eq!(
        entry_summary(&p),
        ["support-triage / Every 15 minutes: model, network(crm, http), local(log)"]
    );
    let names = guard_names(&p.entries[0].body);
    assert_eq!(
        names,
        ["t.priority = \"low\" or t.priority = \"medium\" or t.priority = \"high\""]
    );
}

#[test]
fn example_04_lowers() {
    let (_, _, l, p) = lower_example("examples/04-daily-report.agent.md");
    assert_no_agent_nodes(&l);
    assert_ir_clean(&p);
    assert_eq!(
        entry_summary(&p),
        ["daily-report / Every Monday at 08:00: model, network(crm, email), local(log)"]
    );
    let e = &p.entries[0];
    assert!(matches!(e.trigger, ir::Trigger::Weekly { day: Weekday::Monday, hour: 8, minute: 0 }));
    let IS::Let { value, .. } = &e.body[0].kind else { panic!() };
    let IX::Count { filter, .. } = &value.kind else { panic!("{value:?}") };
    assert_eq!(filter.as_deref(), Some("open"));
    let IS::Effect { call: EffectCall::Model(m), .. } = &e.body[2].kind else { panic!() };
    assert!(m.context.is_none());
    // email send address "…"
    let json = serde_json::to_string(&e.body).unwrap();
    assert!(json.contains("\"Literal\":{\"word\":\"address\""), "{json}");
}

#[test]
fn example_05_lowers_unchanged() {
    let (project, c, l, p) = lower_example("examples/05-invoice-service.agent.md");
    for (lf, sf) in l.files.iter().zip(&project.files) {
        assert_eq!(lf.decls, sf.ast.decls, "agent_lower must not touch 05");
    }
    assert_ir_clean(&p);
    assert_eq!(
        summary(&p),
        [
            ("api", "GET /invoices/{id}", "pure"),
            ("api", "GET /invoices/{id}/net", "pure"),
            ("api", "POST /invoices", "local(state)"),
            ("import-csv", "Run", "local(file, print, state)"),
            ("net_amount", "net_amount", "pure"),
        ]
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
    );
    assert_eq!(p.rust_fns.len(), 1);
    assert_eq!(p.rust_fns[0].name, "fingerprint");
    assert!(p.rust_fns[0].code.contains("Sha256"));
    assert_eq!(p.crates[0].name, "sha2");
    // guard … else respond 404 is `if not … { respond 404 }`
    let route = &p.services[0].routes[0];
    let IS::If { then, .. } = &route.body[1].kind else { panic!("{:?}", route.body[1]) };
    assert!(matches!(then[0].kind, IS::Respond { status: 404, .. }));
    // `found.id` after the guard unwraps `found`
    let json = serde_json::to_string(&route.body[2]).unwrap();
    assert!(json.contains("\"Unwrap\""), "{json}");
    assert_eq!(p.commands[0].state.as_deref(), Some("AppState"));
    assert!(matches!(p.commands[0].body[0].kind, IS::Try { .. }));
    let _ = c;
}

fn guard_names(body: &[ir::Stmt]) -> Vec<String> {
    let mut out = Vec::new();
    fn go(body: &[ir::Stmt], out: &mut Vec<String>) {
        for s in body {
            match &s.kind {
                IS::Effect { call: EffectCall::Trace(ir::TraceEvent::Guard { name, .. }), .. } => {
                    out.push(name.clone())
                }
                IS::If { then, else_, .. } => {
                    go(then, out);
                    go(else_, out);
                }
                IS::ForEach { body, .. } => go(body, out),
                _ => {}
            }
        }
    }
    go(body, &mut out);
    out
}

// ------------------------------------------------------------ targeted

const HEAD: &str = "# Record: Invoice\n- id: text\n- customer: text\n- customer_email: text\n- amount: money\n- due: date\n- status: text\n\n# Rule: open\n- for: Invoice\n- when: status != \"paid\"\n\n# Constant: limit\n- value: 10\n\n";

#[test]
fn guard_lowers_to_let_trace_if_with_constants_inlined() {
    let src = format!(
        "{HEAD}# Agent: a\n- model: claude\n- tools: log\n\n## On start\n- ask model: \"hi\" -> r\n- guard: r length < limit\n- log: \"{{r}}\"\n"
    );
    let (p, c) = mem(&src);
    let l = agent_lower(&p, &c);
    let Some(Decl::Entry(e)) = l.files[0].decls.iter().find(|d| matches!(d, Decl::Entry(_))) else { panic!() };
    // ask model, set tmp, trace, if, log
    let kinds: Vec<&str> = e.body.iter().map(|s| match &s.kind {
        StmtKind::ModelCall { .. } => "model",
        StmtKind::Set { .. } => "set",
        StmtKind::Trace(_) => "trace",
        StmtKind::If { .. } => "if",
        StmtKind::ToolCall { .. } => "tool",
        _ => "?",
    }).collect();
    assert_eq!(kinds, ["model", "set", "trace", "if", "tool"]);
    let prog = ir::build::build(&p, &c, &l);
    let body = &prog.entries[0].body;
    assert_eq!(guard_names(body), ["r length < 10"]);
    let IS::If { cond, then, .. } = &body[3].kind else { panic!() };
    assert!(matches!(cond.kind, IX::Not(_)));
    assert!(matches!(then[0].kind, IS::Stop));
    // trace and check are pure; the summary is not changed by guards
    assert_eq!(prog.entries[0].effect.to_string(), "model, local(log)");
}

#[test]
fn do_block_becomes_a_function_called_with_the_loop_record() {
    let src = format!(
        "{HEAD}# Agent: my-agent\n- model: claude\n- tools: log\n\n## Every day at 08:00\nfor each invoice in crm.invoices:\n  - do show\n\n## Do show\n- log: \"{{customer}}\"\n"
    );
    let (p, c) = mem(&src.replace("tools: log", "tools: log, crm"));
    let l = agent_lower(&p, &c);
    let names: Vec<String> = l.files[0].decls.iter().filter_map(|d| match d {
        Decl::Function(f) => Some(f.name.name.clone()),
        _ => None,
    }).collect();
    assert!(names.contains(&"my_agent__show".to_string()), "{names:?}");
    let prog = ir::build::build(&p, &c, &l);
    let f = prog.functions.iter().find(|f| f.name == "my_agent__show").unwrap();
    assert_eq!(f.params[0].name, "invoice");
    let e = &prog.entries[0];
    assert_eq!((e.unit.as_str(), e.label.as_str()), ("my-agent", "Every day at 08:00"));
    let IS::ForEach { body, .. } = &e.body[0].kind else { panic!() };
    let IS::Effect { call: EffectCall::Fn { name, args }, .. } = &body[0].kind else { panic!("{:?}", body[0]) };
    assert_eq!(name, "my_agent__show");
    assert!(matches!(&args[0].kind, IX::Var(v) if v == "invoice"));
    assert_eq!(e.effect.to_string(), "network(crm), local(log)");
}

#[test]
fn filtered_loop_wraps_the_body_in_a_rule_call() {
    let src = format!(
        "{HEAD}# Agent: a\n- model: claude\n- tools: log, crm\n\n## On start\nfor each open invoice in crm.invoices:\n  - log: \"{{customer}}\"\n"
    );
    let (p, c) = mem(&src);
    let l = agent_lower(&p, &c);
    let Some(Decl::Entry(e)) = l.files[0].decls.iter().find(|d| matches!(d, Decl::Entry(_))) else { panic!() };
    let StmtKind::ForEach { filter_rule, body, .. } = &e.body[0].kind else { panic!() };
    assert!(filter_rule.is_none());
    assert!(matches!(&body[0].kind, StmtKind::If { cond, .. } if matches!(&cond.kind, ExprKind::Call { .. })));
    let prog = ir::build::build(&p, &c, &l);
    let IS::ForEach { body, .. } = &prog.entries[0].body[0].kind else { panic!() };
    let IS::If { cond, then, .. } = &body[0].kind else { panic!() };
    assert!(matches!(&cond.kind, IX::Call { name, .. } if name == "open"));
    assert_eq!(then.len(), 1);
    // the rule is a pure Fn(Invoice) -> bool
    let rule = prog.functions.iter().find(|f| f.name == "open").unwrap();
    assert!(rule.effect.is_pure());
    assert_eq!(rule.ret, Some(sema::Type::Bool));
}

#[test]
fn count_of_rule_keeps_the_filter_name() {
    let src = format!(
        "{HEAD}# Agent: a\n- model: claude\n- tools: log, crm\n\n## On start\n- set n = count of open invoices in crm.invoices\n- log: \"{{n}}\"\n"
    );
    let (p, c) = mem(&src);
    let prog = ir::lower(&p, &c);
    let IS::Let { value, ty, .. } = &prog.entries[0].body[0].kind else { panic!() };
    assert_eq!(*ty, sema::Type::Int);
    assert!(matches!(&value.kind, IX::Count { filter: Some(f), .. } if f == "open"));
    assert_eq!(value.effect.to_string(), "network(crm)");
}

#[test]
fn rule_named_like_a_function_does_not_collide() {
    let src = "# Record: Invoice\n- id: text\n- status: text\n\n# Rule: check\n- for: Invoice\n- when: status != \"paid\"\n\n# Function: check\n- in: x: int\n- out: int\n- effect: pure\n  - return x\n";
    let (p, c) = mem(src);
    let prog = ir::lower(&p, &c);
    let mut names: Vec<_> = prog.functions.iter().map(|f| f.name.clone()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn unwrap_is_explicit_and_tool_targets_are_resolved() {
    let (_, _, _, p) = lower_example("examples/03-support-triage.agent.md");
    let json = serde_json::to_string(&p.entries[0].body).unwrap();
    // crm update ticket: priority = t.priority -> source "tickets", id from ticket.id
    assert!(json.contains("\"source\":\"tickets\""), "{json}");
    assert!(json.contains("\"id_from\":[\"ticket\",\"id\"]"), "{json}");
    // the http post carries its URL as a literal target and a record body
    assert!(json.contains("\"tool\":\"http\""));
}
