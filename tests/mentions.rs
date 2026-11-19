use newsagent::mentions::{
    check_mention, extract_mentions, Candidate, MentionChecker, MentionStatus,
};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// -- Extraction tests --

#[test]
fn extracts_mentions() {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "sorted and unique",
            "Thanks to @bob and @alice.\n- **Alice** (@alice) shipped it!",
            &["alice", "bob"],
        ),
        (
            "punctuation inside handles",
            "Hello @jane.doe-smith_2, and @x.",
            &["jane.doe-smith_2", "x"],
        ),
        (
            "emails, urls and non-handles",
            "Mail bob@example.com, see https://example.com/@carol or @123 or @ alone.",
            &[],
        ),
        (
            "versioned product names",
            "Try ubuntu@26.04, ubuntu@latest, `snap@edge` and postgresql@16.",
            &[],
        ),
        (
            "markdown link text",
            "Kudos to [@octocat](https://github.com/octocat) and @dave.",
            &["dave"],
        ),
    ];

    for (name, text, expected) in cases {
        assert_eq!(extract_mentions(text), *expected, "{name}");
    }
}

// -- Lookup tests --

fn user_lookup(handle: &str, status: u16) -> Mock {
    Mock::given(method("GET"))
        .and(path(format!("/u/{handle}.json")))
        .respond_with(ResponseTemplate::new(status))
}

fn user_search(term: &str, usernames: &[&str]) -> Mock {
    let users: Vec<_> = usernames
        .iter()
        .map(|u| json!({ "username": u, "name": "Someone" }))
        .collect();
    Mock::given(method("GET"))
        .and(path("/u/search/users.json"))
        .and(query_param("term", term))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "users": users })))
}

fn checker(host: &str, server: &MockServer) -> MentionChecker {
    MentionChecker::new(host, &server.uri(), None).unwrap()
}

fn candidate(username: &str, host: &str) -> Candidate {
    Candidate {
        username: username.into(),
        host: host.into(),
    }
}

#[tokio::test]
async fn existing_user_is_valid_and_sends_the_api_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/u/alice.json"))
        .and(header("Api-Key", "secret"))
        .and(header("Api-Username", "system"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let checkers = [MentionChecker::new("one", &server.uri(), Some("secret".into())).unwrap()];

    assert_eq!(
        check_mention(&checkers, "alice").await.unwrap(),
        MentionStatus::Valid("one".into())
    );
}

#[tokio::test]
async fn user_on_a_later_instance_is_valid() {
    let first = MockServer::start().await;
    let second = MockServer::start().await;
    user_lookup("bob", 404).mount(&first).await;
    user_lookup("bob", 200).mount(&second).await;
    let checkers = [checker("one", &first), checker("two", &second)];

    assert_eq!(
        check_mention(&checkers, "bob").await.unwrap(),
        MentionStatus::Valid("two".into())
    );
}

#[tokio::test]
async fn unknown_handles_are_searched_on_every_instance() {
    let cases: Vec<(&str, [&[&str]; 2], MentionStatus)> = vec![
        ("no results", [&[], &[]], MentionStatus::Unknown(vec![])),
        (
            "one result on a later instance",
            [&[], &["alice"]],
            MentionStatus::Suggested(candidate("alice", "two")),
        ),
        (
            "one result on each instance",
            [&["alice"], &["alicia"]],
            MentionStatus::Unknown(vec![candidate("alice", "one"), candidate("alicia", "two")]),
        ),
        (
            "candidates capped at five",
            [&["sam1", "sam2", "sam3", "sam4"], &["sam", "sam5"]],
            MentionStatus::Unknown(vec![
                candidate("sam1", "one"),
                candidate("sam2", "one"),
                candidate("sam3", "one"),
                candidate("sam4", "one"),
                candidate("sam", "two"),
            ]),
        ),
    ];

    for (name, results, expected) in cases {
        let servers = [MockServer::start().await, MockServer::start().await];
        for (server, usernames) in servers.iter().zip(results) {
            user_lookup("someone", 404).mount(server).await;
            user_search("someone", usernames).mount(server).await;
        }
        let checkers = [checker("one", &servers[0]), checker("two", &servers[1])];

        assert_eq!(
            check_mention(&checkers, "someone").await.unwrap(),
            expected,
            "{name}"
        );
    }
}

#[tokio::test]
async fn unexpected_status_is_an_error_naming_the_host() {
    let server = MockServer::start().await;
    user_lookup("alice", 500).mount(&server).await;

    let err = check_mention(&[checker("one", &server)], "alice")
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("one: user lookup returned 500"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let server = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&elsewhere)
        .await;
    Mock::given(method("GET"))
        .and(path("/u/alice.json"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/steal", elsewhere.uri())),
        )
        .mount(&server)
        .await;
    let checkers = [MentionChecker::new("one", &server.uri(), Some("secret".into())).unwrap()];

    let err = check_mention(&checkers, "alice").await.unwrap_err();

    assert!(
        err.to_string().contains("one: user lookup returned 302"),
        "unexpected error: {err}"
    );
}
