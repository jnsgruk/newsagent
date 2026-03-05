#[path = "../common/mod.rs"]
mod common;

use chrono::Datelike;
use common::with_newsagent_env;
use flate2::write::GzEncoder;
use flate2::Compression;
use newsagent::tools::mailing_list::{MailingListArgs, MailingListConfig, MailingListTool};
use rig::tool::Tool;
use std::io::Write;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// -- Config tests --

#[test]
fn config_parses_comma_separated_lists() {
    let _guard = with_newsagent_env(vec![(
        "NEWSAGENT_MAILING_LISTS",
        "ubuntu-release,ubuntu-devel,technical-board",
    )]);

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<MailingListConfig>()
        .expect("Failed to parse MailingListConfig from env");

    assert_eq!(config.lists.len(), 3);
    assert_eq!(config.lists[0], "ubuntu-release");
    assert_eq!(config.lists[1], "ubuntu-devel");
    assert_eq!(config.lists[2], "technical-board");
}

#[test]
fn config_single_list() {
    let _guard = with_newsagent_env(vec![("NEWSAGENT_MAILING_LISTS", "ubuntu-release")]);

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<MailingListConfig>()
        .expect("Failed to parse MailingListConfig from env");

    assert_eq!(config.lists.len(), 1);
    assert_eq!(config.lists[0], "ubuntu-release");
}

#[test]
fn config_empty_when_var_missing() {
    let _guard = with_newsagent_env(vec![]);

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<MailingListConfig>()
        .expect("Failed to parse MailingListConfig from env");

    assert!(config.lists.is_empty());
}

#[test]
fn config_empty_when_var_blank() {
    let _guard = with_newsagent_env(vec![("NEWSAGENT_MAILING_LISTS", "  ")]);

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<MailingListConfig>()
        .expect("Failed to parse MailingListConfig from env");

    assert!(config.lists.is_empty());
}

#[test]
fn config_custom_base_url() {
    let _guard = with_newsagent_env(vec![
        ("NEWSAGENT_MAILING_LISTS", "ubuntu-release"),
        (
            "NEWSAGENT_MAILING_LIST_BASE_URL",
            "https://example.com/archives",
        ),
    ]);

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<MailingListConfig>()
        .expect("Failed to parse MailingListConfig from env");

    assert_eq!(config.effective_base_url(), "https://example.com/archives");
}

#[test]
fn config_default_base_url() {
    let _guard = with_newsagent_env(vec![("NEWSAGENT_MAILING_LISTS", "ubuntu-release")]);

    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<MailingListConfig>()
        .expect("Failed to parse MailingListConfig from env");

    assert_eq!(
        config.effective_base_url(),
        "https://lists.ubuntu.com/archives"
    );
}

#[test]
fn new_returns_none_when_no_lists() {
    let tool = MailingListTool::new(MailingListConfig::default(), 8000);
    assert!(tool.is_none());
}

// -- Tool tests --

fn gzip_bytes(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("gzip write failed");
    encoder.finish().expect("gzip finish failed")
}

fn make_mbox_message(
    message_id: &str,
    from_name: &str,
    from_email: &str,
    subject: &str,
    date: &str,
    body: &str,
    in_reply_to: Option<&str>,
) -> String {
    let irt_header = in_reply_to
        .map(|id| format!("In-Reply-To: <{}>\n", id))
        .unwrap_or_default();
    format!(
        "From {} {}\nFrom: {} <{}>\nSubject: {}\nDate: {}\nMessage-ID: <{}>\n{}Content-Type: text/plain; charset=\"UTF-8\"\n\n{}\n\n",
        from_email, date, from_name, from_email, subject, date, message_id, irt_header, body,
    )
}

fn current_month_string() -> String {
    let now = chrono::Utc::now();
    let month_names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    format!("{}-{}", now.year(), month_names[now.month0() as usize])
}

fn prev_month_string() -> String {
    let now = chrono::Utc::now();
    let month_names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let (year, month0) = if now.month0() == 0 {
        (now.year() - 1, 11usize)
    } else {
        (now.year(), (now.month0() - 1) as usize)
    };
    format!("{}-{}", year, month_names[month0])
}

fn recent_date_string() -> String {
    let date = chrono::Utc::now() - chrono::Duration::days(5);
    date.format("%a, %d %b %Y %H:%M:%S +0000").to_string()
}

fn old_date_string() -> String {
    let date = chrono::Utc::now() - chrono::Duration::days(60);
    date.format("%a, %d %b %Y %H:%M:%S +0000").to_string()
}

fn tool_with_server(server_uri: &str, lists: Vec<&str>) -> MailingListTool {
    MailingListTool::new(
        MailingListConfig {
            lists: lists.into_iter().map(|s| s.to_string()).collect(),
            base_url: Some(server_uri.to_string()),
        },
        8000,
    )
    .expect("Failed to create MailingListTool")
}

async fn mount_404_for_prev_month(server: &MockServer, list_name: &str) {
    let prev = prev_month_string();
    Mock::given(method("GET"))
        .and(path(format!("/{}/{}.txt.gz", list_name, prev)))
        .respond_with(ResponseTemplate::new(404))
        .mount(server)
        .await;
}

#[tokio::test]
async fn fetches_and_parses_mbox() {
    let server = MockServer::start().await;
    let date = recent_date_string();

    let mbox = make_mbox_message(
        "msg1@example.com",
        "Alice",
        "alice@example.com",
        "Test Subject",
        &date,
        "Hello from the mailing list",
        None,
    );

    let current = current_month_string();

    Mock::given(method("GET"))
        .and(path(format!("/test-list/{}.txt.gz", current)))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(gzip_bytes(mbox.as_bytes())))
        .mount(&server)
        .await;
    mount_404_for_prev_month(&server, "test-list").await;

    let tool = tool_with_server(&server.uri(), vec!["test-list"]);
    let output = tool
        .call(MailingListArgs {})
        .await
        .expect("Tool call failed");

    assert_eq!(output.threads.len(), 1);
    assert_eq!(output.threads[0].subject, "Test Subject");
    assert_eq!(output.threads[0].authors, vec!["Alice"]);
    assert!(output.threads[0]
        .summary
        .contains("Hello from the mailing list"));
}

#[tokio::test]
async fn filters_messages_to_last_30_days() {
    let server = MockServer::start().await;

    let recent = recent_date_string();
    let old = old_date_string();

    let mbox = format!(
        "{}{}",
        make_mbox_message(
            "recent@example.com",
            "Alice",
            "alice@example.com",
            "Recent Topic",
            &recent,
            "Recent message",
            None,
        ),
        make_mbox_message(
            "old@example.com",
            "Bob",
            "bob@example.com",
            "Old Topic",
            &old,
            "Old message",
            None,
        ),
    );

    let current = current_month_string();

    Mock::given(method("GET"))
        .and(path(format!("/test-list/{}.txt.gz", current)))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(gzip_bytes(mbox.as_bytes())))
        .mount(&server)
        .await;
    mount_404_for_prev_month(&server, "test-list").await;

    let tool = tool_with_server(&server.uri(), vec!["test-list"]);
    let output = tool
        .call(MailingListArgs {})
        .await
        .expect("Tool call failed");

    assert_eq!(output.threads.len(), 1);
    assert_eq!(output.threads[0].subject, "Recent Topic");
}

#[tokio::test]
async fn threads_messages_by_reply() {
    let server = MockServer::start().await;
    let date = recent_date_string();

    let mbox = format!(
        "{}{}",
        make_mbox_message(
            "thread1@example.com",
            "Alice",
            "alice@example.com",
            "Discussion Topic",
            &date,
            "Starting a discussion",
            None,
        ),
        make_mbox_message(
            "reply1@example.com",
            "Bob",
            "bob@example.com",
            "Re: Discussion Topic",
            &date,
            "I agree",
            Some("thread1@example.com"),
        ),
    );

    let current = current_month_string();

    Mock::given(method("GET"))
        .and(path(format!("/test-list/{}.txt.gz", current)))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(gzip_bytes(mbox.as_bytes())))
        .mount(&server)
        .await;
    mount_404_for_prev_month(&server, "test-list").await;

    let tool = tool_with_server(&server.uri(), vec!["test-list"]);
    let output = tool
        .call(MailingListArgs {})
        .await
        .expect("Tool call failed");

    assert_eq!(output.threads.len(), 1);
    assert_eq!(output.threads[0].message_count, 2);
    assert!(output.threads[0].authors.contains(&"Alice".to_string()));
    assert!(output.threads[0].authors.contains(&"Bob".to_string()));
}

#[tokio::test]
async fn deduplicates_across_lists() {
    let server = MockServer::start().await;
    let date = recent_date_string();

    let mbox = make_mbox_message(
        "crosspost@example.com",
        "Alice",
        "alice@example.com",
        "Cross-posted Topic",
        &date,
        "Important announcement",
        None,
    );

    let current = current_month_string();

    Mock::given(method("GET"))
        .and(path(format!("/list-a/{}.txt.gz", current)))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(gzip_bytes(mbox.as_bytes())))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/list-b/{}.txt.gz", current)))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(gzip_bytes(mbox.as_bytes())))
        .mount(&server)
        .await;

    mount_404_for_prev_month(&server, "list-a").await;
    mount_404_for_prev_month(&server, "list-b").await;

    let tool = tool_with_server(&server.uri(), vec!["list-a", "list-b"]);
    let output = tool
        .call(MailingListArgs {})
        .await
        .expect("Tool call failed");

    assert_eq!(output.threads.len(), 1);
    assert!(output.threads[0].lists.contains(&"list-a".to_string()));
    assert!(output.threads[0].lists.contains(&"list-b".to_string()));
}

#[tokio::test]
async fn truncates_message_body() {
    let server = MockServer::start().await;
    let date = recent_date_string();

    let long_body = "x".repeat(500);
    let mbox = make_mbox_message(
        "long@example.com",
        "Alice",
        "alice@example.com",
        "Long Message",
        &date,
        &long_body,
        None,
    );

    let current = current_month_string();

    Mock::given(method("GET"))
        .and(path(format!("/test-list/{}.txt.gz", current)))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(gzip_bytes(mbox.as_bytes())))
        .mount(&server)
        .await;
    mount_404_for_prev_month(&server, "test-list").await;

    let tool = MailingListTool::new(
        MailingListConfig {
            lists: vec!["test-list".to_string()],
            base_url: Some(server.uri()),
        },
        50,
    )
    .expect("Failed to create tool");

    let output = tool
        .call(MailingListArgs {})
        .await
        .expect("Tool call failed");

    assert_eq!(output.threads.len(), 1);
    assert!(output.threads[0].summary.chars().count() <= 50);
    assert!(output.threads[0].truncated);
}

#[tokio::test]
async fn handles_missing_month_gracefully() {
    let server = MockServer::start().await;

    let current = current_month_string();
    let prev = prev_month_string();

    Mock::given(method("GET"))
        .and(path(format!("/test-list/{}.txt.gz", current)))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/test-list/{}.txt.gz", prev)))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let tool = tool_with_server(&server.uri(), vec!["test-list"]);
    let output = tool
        .call(MailingListArgs {})
        .await
        .expect("Tool call should succeed even with missing archives");

    assert!(output.threads.is_empty());
}

#[tokio::test]
async fn sorts_threads_by_recent_activity() {
    let server = MockServer::start().await;

    let older = (chrono::Utc::now() - chrono::Duration::days(10))
        .format("%a, %d %b %Y %H:%M:%S +0000")
        .to_string();
    let newer = (chrono::Utc::now() - chrono::Duration::days(2))
        .format("%a, %d %b %Y %H:%M:%S +0000")
        .to_string();

    let mbox = format!(
        "{}{}",
        make_mbox_message(
            "older@example.com",
            "Alice",
            "alice@example.com",
            "Older Topic",
            &older,
            "This is older",
            None,
        ),
        make_mbox_message(
            "newer@example.com",
            "Bob",
            "bob@example.com",
            "Newer Topic",
            &newer,
            "This is newer",
            None,
        ),
    );

    let current = current_month_string();

    Mock::given(method("GET"))
        .and(path(format!("/test-list/{}.txt.gz", current)))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(gzip_bytes(mbox.as_bytes())))
        .mount(&server)
        .await;
    mount_404_for_prev_month(&server, "test-list").await;

    let tool = tool_with_server(&server.uri(), vec!["test-list"]);
    let output = tool
        .call(MailingListArgs {})
        .await
        .expect("Tool call failed");

    assert_eq!(output.threads.len(), 2);
    assert_eq!(output.threads[0].subject, "Newer Topic");
    assert_eq!(output.threads[1].subject, "Older Topic");
}
