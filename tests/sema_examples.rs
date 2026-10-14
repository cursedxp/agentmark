//! All five examples check clean (zero errors), as single files and as a project folder.

use agentmark::diag::Severity;
use agentmark::project::Project;
use agentmark::sema::{self, Checked};

fn check(path: &str) -> (Project, Checked) {
    let project = Project::load(path);
    match sema::check(&project) {
        Ok(c) => (project, c),
        Err(d) => panic!("{path} should check:\n{}", project.render(&d)),
    }
}

fn warnings(project: &Project, c: &Checked) -> String {
    project.render(&c.warnings)
}

#[test]
fn invoice_chaser_folder_checks_with_no_diagnostics() {
    let (p, c) = check("examples/01-invoice-chaser");
    assert!(c.warnings.is_empty(), "{}", warnings(&p, &c));
    assert_eq!(c.records["Invoice"].fields.len(), 6);
    // the effect summary of the `remind` action, SPEC 8 / DESIGN 4.2
    let remind = c
        .blocks
        .iter()
        .find(|b| b.label == "Do remind")
        .expect("remind block");
    assert_eq!(
        remind.effect.to_string(),
        "model, network(email, crm), local(log)"
    );
    let every = c
        .blocks
        .iter()
        .find(|b| b.label == "Every day at 09:00")
        .unwrap();
    assert_eq!(
        every.effect.to_string(),
        "model, network(crm, email), local(log)"
    );
    assert_eq!(
        c.unit_effect("invoice-chaser").to_string(),
        "model, network(crm, email), local(log)"
    );
    assert_eq!(c.constants["grace_period"].ty.to_string(), "duration");
    assert_eq!(c.constants["max_reminder_length"].ty.to_string(), "int");
}

#[test]
fn lead_qualifier_checks() {
    let (p, c) = check("examples/02-lead-qualifier.agent.md");
    // the model prompt makes line 17 longer than 120 characters (Q35): a warning, not an error
    assert!(c.warnings.iter().all(|w| w.code == "W007"), "{}", warnings(&p, &c));
    assert_eq!(c.warnings.len(), 1);
    let b = c
        .blocks
        .iter()
        .find(|b| b.label == "When crm receives lead")
        .unwrap();
    assert_eq!(
        b.effect.to_string(),
        "model, network(crm, email), local(log)"
    );
}

#[test]
fn support_triage_checks() {
    let (p, c) = check("examples/03-support-triage.agent.md");
    assert!(c.warnings.iter().all(|w| w.code == "W007"), "{}", warnings(&p, &c));
    assert_eq!(c.warnings.len(), 1);
}

#[test]
fn daily_report_checks() {
    let (p, c) = check("examples/04-daily-report.agent.md");
    assert!(c.warnings.is_empty(), "{}", warnings(&p, &c));
    let b = c
        .blocks
        .iter()
        .find(|b| b.label == "Every Monday at 08:00")
        .unwrap();
    assert_eq!(
        b.effect.to_string(),
        "model, network(crm, email), local(log)"
    );
}

#[test]
fn invoice_service_checks_with_the_unbound_crate_and_single_file_warnings() {
    let (p, c) = check("examples/05-invoice-service.agent.md");
    let w = warnings(&p, &c);
    let codes: Vec<_> = c.warnings.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["W004", "W006"], "{w}");
    assert!(w.contains("The crate \"sha2\" has no binding yet."), "{w}");
    assert!(w.contains("single-file draft"), "{w}");
    assert!(c.warnings.iter().all(|d| d.severity == Severity::Warning));
    assert_eq!(c.rust_blocks.len(), 1);
    assert_eq!(c.rust_blocks[0].name, "fingerprint");
    let net = c.blocks.iter().find(|b| b.label == "net_amount").unwrap();
    assert!(net.effect.is_pure());
}
