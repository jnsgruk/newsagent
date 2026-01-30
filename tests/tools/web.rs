#[path = "../common/mod.rs"]
mod common;

use common::with_newsagent_env;
use newsagent::tools::web::{
    WebConfig, WebReadabilityArgs, WebReadabilityTool, WebReadabilityToolError,
};
use rig::tool::Tool;
use std::time::Duration;
use tokio::time::Instant;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn fetches_and_truncates_content() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "<html><head><title>Example</title></head><body><article><p>Hello world</p></article></body></html>",
        ))
        .mount(&server)
        .await;

    let tool = WebReadabilityTool::new(WebConfig {
        allowlist: Some("127.0.0.1".to_string()),
        max_chars: Some(5),
        timeout_secs: Some(5),
        min_interval_ms: None,
        user_agent: Some("test-agent".to_string()),
    })
    .expect("Failed to create web tool");

    let url = format!("{}/page", server.uri());
    let output = tool
        .call(WebReadabilityArgs { url: url.clone() })
        .await
        .expect("Web tool call failed");

    assert_eq!(output.title, "Example");
    assert!(output.text.chars().count() <= 5);
    assert!(output.truncated);
    assert_eq!(output.source_url, url);
}

#[tokio::test]
async fn rejects_disallowed_host() {
    let server = MockServer::start().await;

    let tool = WebReadabilityTool::new(WebConfig {
        allowlist: Some("example.com".to_string()),
        max_chars: None,
        timeout_secs: None,
        min_interval_ms: None,
        user_agent: None,
    })
    .expect("Failed to create web tool");

    let url = format!("{}/page", server.uri());
    let err = tool
        .call(WebReadabilityArgs { url })
        .await
        .expect_err("Expected disallowed host error");

    match err {
        WebReadabilityToolError::DisallowedHost(host) => assert_eq!(host, "127.0.0.1"),
        other => panic!("Unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn rejects_invalid_url() {
    let tool = WebReadabilityTool::new(WebConfig::default()).expect("Failed to create web tool");

    let err = tool
        .call(WebReadabilityArgs {
            url: "not a url".to_string(),
        })
        .await
        .expect_err("Expected invalid url error");

    match err {
        WebReadabilityToolError::InvalidUrl(value) => assert_eq!(value, "not a url"),
        other => panic!("Unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn waits_for_rate_limit_between_requests() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "<html><head><title>Example</title></head><body><article><p>Hello world</p></article></body></html>",
        ))
        .mount(&server)
        .await;

    let tool = WebReadabilityTool::new(WebConfig {
        allowlist: Some("127.0.0.1".to_string()),
        max_chars: None,
        timeout_secs: Some(5),
        min_interval_ms: Some(50),
        user_agent: Some("test-agent".to_string()),
    })
    .expect("Failed to create web tool");

    let url = format!("{}/page", server.uri());

    let start = Instant::now();
    tool.call(WebReadabilityArgs { url: url.clone() })
        .await
        .expect("First web tool call failed");
    tool.call(WebReadabilityArgs { url })
        .await
        .expect("Second web tool call failed");

    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(45));
}

#[tokio::test]
async fn uses_config_from_environment() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    let _guard = with_newsagent_env(vec![
        ("NEWSAGENT_WEB_ALLOWLIST", "127.0.0.1"),
        ("NEWSAGENT_WEB_MAX_CHARS", "4"),
        ("NEWSAGENT_WEB_TIMEOUT_SECS", "5"),
        ("NEWSAGENT_WEB_MIN_INTERVAL_MS", "0"),
        ("NEWSAGENT_WEB_USER_AGENT", "env-agent"),
    ]);

    Mock::given(method("GET"))
        .and(path("/page"))
        .and(wiremock::matchers::header("user-agent", "env-agent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "<html><head><title>Example</title></head><body><article><p>Hello world</p></article></body></html>",
        ))
        .mount(&server)
        .await;

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<WebConfig>()
        .expect("Failed to parse WebConfig from env");
    let tool = WebReadabilityTool::new(config).expect("Failed to create web tool");

    let url = format!("{}/page", base_url);
    let output = tool
        .call(WebReadabilityArgs { url })
        .await
        .expect("Web tool call failed");

    assert_eq!(output.title, "Example");
    assert!(output.text.chars().count() <= 4);
    assert!(output.truncated);
}
