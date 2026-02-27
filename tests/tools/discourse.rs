#[path = "../common/mod.rs"]
mod common;

use common::with_newsagent_env;
use newsagent::tools::discourse::{
    DiscourseArgs, DiscourseConfig, DiscourseInstance, DiscourseTool, DiscourseToolError,
};
use rig::tool::Tool;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// -- Config tests --

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

// -- Tool tests --

fn discourse_response(title: &str, posts: &[(&str, &str, &str, u64)]) -> String {
    let posts_json: Vec<String> = posts
        .iter()
        .map(|(username, created_at, cooked, post_number)| {
            format!(
                r#"{{"post_number":{},"username":"{}","created_at":"{}","cooked":"{}"}}"#,
                post_number, username, created_at, cooked,
            )
        })
        .collect();

    format!(
        r#"{{"title":"{}","post_stream":{{"posts":[{}]}}}}"#,
        title,
        posts_json.join(",")
    )
}

fn tool_with_instance(base_url: &str, api_key: &str, max_chars: usize) -> DiscourseTool {
    DiscourseTool::new(
        DiscourseConfig {
            instances: vec![DiscourseInstance {
                base_url: base_url.to_string(),
                api_key: api_key.to_string(),
            }],
        },
        max_chars,
    )
    .expect("Failed to create DiscourseTool")
}

#[test]
fn new_returns_none_when_no_instances() {
    let tool = DiscourseTool::new(DiscourseConfig::default(), 8000);
    assert!(tool.is_none());
}

#[tokio::test]
async fn fetches_topic_from_api() {
    let server = MockServer::start().await;
    let host = server.uri().replace("http://", "");

    let body = discourse_response(
        "Test Topic",
        &[("alice", "2025-06-01T12:00:00Z", "<p>Hello world</p>", 1)],
    );

    Mock::given(method("GET"))
        .and(path("/t/12345.json"))
        .and(header("Api-Key", "test-key"))
        .and(header("Api-Username", "system"))
        .respond_with(ResponseTemplate::new(200).set_body_string(&body))
        .mount(&server)
        .await;

    let tool = tool_with_instance(&host, "test-key", 8000);
    let url = format!("{}/t/some-slug/12345", server.uri());
    let output = tool
        .call(DiscourseArgs { url: url.clone() })
        .await
        .expect("Discourse tool call failed");

    assert_eq!(output.title, "Test Topic");
    assert_eq!(output.author, "alice");
    assert_eq!(output.date, "2025-06-01T12:00:00Z");
    assert_eq!(output.text.trim(), "Hello world");
    assert_eq!(output.source_url, url);
    assert!(!output.truncated);
}

#[tokio::test]
async fn fetches_specific_post_number() {
    let server = MockServer::start().await;
    let host = server.uri().replace("http://", "");

    let body = discourse_response(
        "Multi Post Topic",
        &[
            ("alice", "2025-06-01T12:00:00Z", "<p>First post</p>", 1),
            ("bob", "2025-06-02T12:00:00Z", "<p>Second post</p>", 2),
        ],
    );

    Mock::given(method("GET"))
        .and(path("/t/99.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(&body))
        .mount(&server)
        .await;

    let tool = tool_with_instance(&host, "key", 8000);
    let url = format!("{}/t/slug/99/2", server.uri());
    let output = tool
        .call(DiscourseArgs { url })
        .await
        .expect("Discourse tool call failed");

    assert_eq!(output.author, "bob");
    assert_eq!(output.text.trim(), "Second post");
}

#[tokio::test]
async fn rejects_url_with_no_matching_instance() {
    let tool = tool_with_instance("discourse.example.com", "key", 8000);

    let err = tool
        .call(DiscourseArgs {
            url: "https://other.example.com/t/slug/123".to_string(),
        })
        .await
        .expect_err("Expected NoMatchingInstance error");

    match err {
        DiscourseToolError::NoMatchingInstance(host) => {
            assert_eq!(host, "other.example.com");
        }
        other => panic!("Unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn rejects_non_discourse_url() {
    let tool = tool_with_instance("discourse.example.com", "key", 8000);

    let err = tool
        .call(DiscourseArgs {
            url: "https://discourse.example.com/categories".to_string(),
        })
        .await
        .expect_err("Expected NotATopicUrl error");

    match err {
        DiscourseToolError::NotATopicUrl(url) => {
            assert!(url.contains("/categories"));
        }
        other => panic!("Unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn truncates_long_content() {
    let server = MockServer::start().await;
    let host = server.uri().replace("http://", "");

    let body = discourse_response(
        "Long Topic",
        &[(
            "alice",
            "2025-06-01T12:00:00Z",
            "<p>This is a long post with lots of content</p>",
            1,
        )],
    );

    Mock::given(method("GET"))
        .and(path("/t/1.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(&body))
        .mount(&server)
        .await;

    let tool = tool_with_instance(&host, "key", 10);
    let url = format!("{}/t/slug/1", server.uri());
    let output = tool
        .call(DiscourseArgs { url })
        .await
        .expect("Discourse tool call failed");

    assert!(output.text.chars().count() <= 10);
    assert!(output.truncated);
}
