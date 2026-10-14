//! Driver-string parsing (SPEC 19), shared by the runtime, generated programs, `check` and the binder.

use agentmark_rt::{claude_id, request_model_id, Driver, DriverError};

#[test]
fn every_supported_form_parses() {
    assert_eq!(Driver::parse("claude"), Ok(Driver::Claude(None)));
    assert_eq!(Driver::parse("claude:haiku"), Ok(Driver::Claude(Some("haiku".into()))));
    assert_eq!(Driver::parse("claude:claude-opus-4-1"), Ok(Driver::Claude(Some("claude-opus-4-1".into()))));
    assert_eq!(Driver::parse("openai:gpt-4o"), Ok(Driver::OpenAi("gpt-4o".into())));
    assert_eq!(Driver::parse("local:kat-coder"), Ok(Driver::Local("kat-coder".into())));
    assert_eq!(Driver::parse("local:qwen2.5:7b"), Ok(Driver::Local("qwen2.5:7b".into())));
    assert_eq!(Driver::parse("mock"), Ok(Driver::Mock(None)));
    assert_eq!(Driver::parse("mock:/tmp/model.json"), Ok(Driver::Mock(Some("/tmp/model.json".into()))));
}

#[test]
fn enterprise_routes_are_named_as_unsupported() {
    for s in ["azure:my-deployment", "bedrock:anthropic.claude-3", "azure", "bedrock"] {
        let e = Driver::parse(s).unwrap_err();
        assert!(matches!(e, DriverError::NotSupported(_)), "{s}");
        assert!(e.to_string().contains("is not supported in this version"), "{e}");
    }
}

#[test]
fn bad_strings_get_plain_errors() {
    assert_eq!(Driver::parse(""), Err(DriverError::Empty));
    assert_eq!(Driver::parse("gemini:pro"), Err(DriverError::Unknown("gemini".into())));
    assert!(Driver::parse("openai").unwrap_err().to_string().contains("needs a model name"));
    assert!(Driver::parse("local:").unwrap_err().to_string().contains("\"local:llama3\""));
}

#[test]
fn request_ids_keep_the_provider_for_non_claude_drivers() {
    assert_eq!(request_model_id("claude"), "claude-sonnet-4-5");
    assert_eq!(request_model_id("claude:haiku"), "claude-haiku-4-5");
    assert_eq!(request_model_id("claude:opus"), "claude-opus-4-1");
    assert_eq!(request_model_id("claude:my-id"), "my-id");
    assert_eq!(request_model_id("openai:gpt-4o"), "openai:gpt-4o");
    assert_eq!(request_model_id("local:kat-coder"), "local:kat-coder");
    assert_eq!(request_model_id("mock"), "mock");
    assert_eq!(claude_id(None), "claude-sonnet-4-5");
}
