use newsagent::links::{extract_links, Link, LinkChecker, LinkStatus};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// -- Extraction tests --

#[test]
fn extracts_links() {
    let cases: &[(&str, &str, &[(&str, usize)])] = &[
        (
            "markdown link kinds",
            "\
- [Release](https://example.com/release) notes
- <https://example.com/auto>
- ![Diagram](https://example.com/diagram.png)
- [Rust](https://en.wikipedia.org/wiki/Rust_(programming_language))
- [Spec][spec]

[spec]: https://example.com/spec
",
            &[
                ("https://example.com/release", 1),
                ("https://example.com/auto", 2),
                ("https://example.com/diagram.png", 3),
                (
                    "https://en.wikipedia.org/wiki/Rust_(programming_language)",
                    4,
                ),
                ("https://example.com/spec", 5),
            ],
        ),
        (
            "code, non-http links and bare text",
            "\
`[x](https://example.com/inline-code)`

```
[y](https://example.com/fenced)
```

[Mail](mailto:someone@example.com), [Note](other-note.md), [Anchor](#section)
Plain text https://example.com/bare is not a Markdown link.
",
            &[],
        ),
        (
            "duplicates keep their first line",
            "Intro\n\n[a](https://example.com/a)\n\n[again](https://example.com/a)\n",
            &[("https://example.com/a", 3)],
        ),
    ];

    for (name, markdown, expected) in cases {
        let expected: Vec<Link> = expected
            .iter()
            .map(|(url, line)| Link {
                url: url.to_string(),
                line: *line,
            })
            .collect();
        assert_eq!(extract_links(markdown), expected, "{name}");
    }
}

// -- Reachability tests --

fn checker() -> LinkChecker {
    LinkChecker::new(vec![Duration::ZERO; 3]).unwrap()
}

#[tokio::test]
async fn classifies_responses_and_retries_temporary_failures() {
    let temporary = |code: &str| LinkStatus::Temporary(code.into());
    let cases = [
        ("success", 200, LinkStatus::Ok, 1),
        ("not found", 404, LinkStatus::Broken("404".into()), 1),
        ("forbidden", 403, temporary("403"), 4),
        ("request timeout", 408, temporary("408"), 4),
        ("rate limited", 429, temporary("429"), 4),
        ("server error", 503, temporary("503"), 4),
    ];

    for (name, status, expected, attempts) in cases {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(status))
            .expect(attempts)
            .named(name)
            .mount(&server)
            .await;

        assert_eq!(checker().check(&server.uri()).await, expected, "{name}");
    }
}

#[tokio::test]
async fn temporary_failure_that_recovers_is_ok() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(checker().check(&server.uri()).await, LinkStatus::Ok);
}

#[tokio::test]
async fn redirect_is_ok_without_following_it() {
    let server = MockServer::start().await;
    let target = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .expect(0)
        .mount(&target)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(301).insert_header("Location", target.uri()))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(checker().check(&server.uri()).await, LinkStatus::Ok);
}

#[tokio::test]
async fn connection_failure_is_temporary() {
    assert_eq!(
        checker().check("http://127.0.0.1:1/").await,
        LinkStatus::Temporary("connection failed".into())
    );
}

#[tokio::test]
async fn check_all_keeps_document_order() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(ResponseTemplate::new(404).set_delay(Duration::from_millis(300)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/fast"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let links: Vec<Link> = ["/slow", "/fast"]
        .iter()
        .enumerate()
        .map(|(i, p)| Link {
            url: format!("{}{}", server.uri(), p),
            line: i + 1,
        })
        .collect();

    let results = checker().check_all(links.clone()).await;

    assert_eq!(
        results,
        vec![
            (links[0].clone(), LinkStatus::Broken("404".into())),
            (links[1].clone(), LinkStatus::Ok),
        ]
    );
}
