//! All five examples check clean (zero errors), as project folders.

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
    let (p, c) = check("examples/02-lead-qualifier");
    assert!(c.warnings.is_empty(), "{}", warnings(&p, &c));
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
    let (p, c) = check("examples/03-support-triage");
    assert!(c.warnings.is_empty(), "{}", warnings(&p, &c));
}

#[test]
fn daily_report_checks() {
    let (p, c) = check("examples/04-daily-report");
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
fn invoice_service_checks_clean_with_its_approved_binding() {
    let (p, c) = check("examples/05-invoice-service");
    assert!(c.warnings.is_empty(), "{}", warnings(&p, &c));
    assert_eq!(c.rust_blocks.len(), 1);
    assert_eq!(c.rust_blocks[0].name, "fingerprint");
    let net = c.blocks.iter().find(|b| b.label == "net_amount").unwrap();
    assert!(net.effect.is_pure());
}
