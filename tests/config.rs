mod common;

use common::with_newsagent_env;
use newsagent::config::AppConfig;

fn required_env_vars() -> Vec<(&'static str, &'static str)> {
    vec![
        ("NEWSAGENT_GEMINI_API_KEY", "test_key"),
        ("NEWSAGENT_TODOIST_API_TOKEN", "todo_token"),
        ("NEWSAGENT_TODOIST_PROJECT_ID", "12345"),
        ("NEWSAGENT_GLEAN_DIR", "/tmp/glean"),
    ]
}

#[test]
fn test_config_loads_valid_config() {
    let _guard = with_newsagent_env(required_env_vars());

    let config = AppConfig::from_env().expect("Failed to parse config");

    assert_eq!(config.gemini_api_key, "test_key");
    assert_eq!(config.todoist.api_token, "todo_token");
    assert_eq!(config.todoist.project_id, "12345");
    assert_eq!(config.glean.dir, "/tmp/glean");
    // Check default
    assert_eq!(config.gemini_model, "gemini-3.1-pro-preview");
}

#[test]
fn test_config_with_optional_fields() {
    let mut vars = required_env_vars();
    vars.extend([
        ("NEWSAGENT_GEMINI_MODEL", "custom-model"),
        ("NEWSAGENT_GLEAN_FILTER", "some-filter"),
        ("NEWSAGENT_WEB_TIMEOUT_SECS", "30"),
    ]);
    let _guard = with_newsagent_env(vars);

    let config = AppConfig::from_env().expect("Failed to parse config");

    assert_eq!(config.gemini_model, "custom-model");
    assert_eq!(config.glean.filter, Some("some-filter".to_string()));
    assert_eq!(config.web.timeout_secs, Some(30));
}

#[test]
fn test_config_missing_required_fields() {
    let _guard = with_newsagent_env(vec![
        ("NEWSAGENT_GEMINI_API_KEY", "test_key"),
        // Missing TODOIST fields
        ("NEWSAGENT_GLEAN_DIR", "/tmp/glean"),
    ]);

    let config = AppConfig::from_env();
    assert!(config.is_err());
}
