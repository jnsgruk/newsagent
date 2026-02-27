#[path = "../common/mod.rs"]
mod common;

use common::with_newsagent_env;
use newsagent::tools::discourse::DiscourseConfig;

#[test]
fn config_parses_comma_separated_instances() {
    let _guard = with_newsagent_env(vec![(
        "NEWSAGENT_DISCOURSE_INSTANCES",
        "discourse.canonical.com=abc123,discourse.charmhub.io=def456",
    )]);

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<DiscourseConfig>()
        .expect("Failed to parse DiscourseConfig from env");

    assert_eq!(config.instances.len(), 2);
    assert_eq!(config.instances[0].base_url, "discourse.canonical.com");
    assert_eq!(config.instances[0].api_key, "abc123");
    assert_eq!(config.instances[1].base_url, "discourse.charmhub.io");
    assert_eq!(config.instances[1].api_key, "def456");
}

#[test]
fn config_single_instance() {
    let _guard = with_newsagent_env(vec![(
        "NEWSAGENT_DISCOURSE_INSTANCES",
        "discourse.canonical.com=abc123",
    )]);

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<DiscourseConfig>()
        .expect("Failed to parse DiscourseConfig from env");

    assert_eq!(config.instances.len(), 1);
    assert_eq!(config.instances[0].base_url, "discourse.canonical.com");
    assert_eq!(config.instances[0].api_key, "abc123");
}

#[test]
fn config_empty_when_var_missing() {
    let _guard = with_newsagent_env(vec![]);

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<DiscourseConfig>()
        .expect("Failed to parse DiscourseConfig from env");

    assert!(config.instances.is_empty());
}

#[test]
fn config_empty_when_var_blank() {
    let _guard = with_newsagent_env(vec![("NEWSAGENT_DISCOURSE_INSTANCES", "  ")]);

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<DiscourseConfig>()
        .expect("Failed to parse DiscourseConfig from env");

    assert!(config.instances.is_empty());
}
