//! The `[lsp]` table of `agentmark.toml` (Q250).

use agentmark::project::Project;

fn load(toml: &str) -> Project {
    Project::from_memory(
        "p",
        vec![("p/agentmark.toml", toml), ("p/a.agent.md", "# Constant: a\n- value: 1\n")],
    )
}

#[test]
fn model_actions_are_off_unless_the_project_turns_them_on() {
    assert!(!load("name = \"p\"\n").config.lsp_model_actions);
    assert!(!load("[lsp]\nmodel_actions = false\n").config.lsp_model_actions);
    let on = load("[lsp]\nmodel_actions = true\n");
    assert!(on.load_diagnostics.is_empty(), "{:?}", on.load_diagnostics);
    assert!(on.config.lsp_model_actions);
}

#[test]
fn a_wrong_lsp_table_is_a_config_error() {
    for (toml, want) in [
        ("[lsp]\nmodel_actions = \"yes\"\n", "must be true or false"),
        ("[lsp]\nmodel_action = true\n", "Unknown key \"model_action\" under [lsp]"),
        ("lsp = 3\n", "must be a table"),
    ] {
        let p = load(toml);
        let d = p.load_diagnostics.first().unwrap_or_else(|| panic!("{toml}"));
        assert_eq!(d.code, "E114");
        assert!(d.message.contains(want), "{}", d.message);
    }
}
