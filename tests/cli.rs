use std::process::{Command, Output};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn newsagent(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_newsagent"))
        .args(args)
        .env_clear()
        .env("NEWSAGENT_DOTENV_PATH", "/nonexistent")
        .output()
        .expect("Failed to run newsagent")
}

#[test]
fn info_flags_exit_without_configuration() {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "help",
            "--help",
            &[
                "Draft a source-linked Tech Updates outline",
                "--output",
                "check-mentions",
                "check-links",
            ],
        ),
        (
            "version",
            "--version",
            &[concat!("newsagent ", env!("CARGO_PKG_VERSION"))],
        ),
    ];

    for (name, flag, fragments) in cases {
        let output = newsagent(&[flag]);
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "{name}: {:?}", output.status);
        for fragment in *fragments {
            assert!(
                stdout.contains(fragment),
                "{name}: {fragment:?} missing from {stdout}"
            );
        }
    }
}

#[test]
fn existing_output_is_rejected_before_generation() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("outline.md");
    std::fs::write(&path, "draft").unwrap();

    let output = newsagent(&["--output", path.to_str().unwrap()]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "draft");
}

#[test]
fn check_mentions_requires_discourse_instances() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("draft.md");
    std::fs::write(&path, "Thanks @alice").unwrap();

    let output = newsagent(&["check-mentions", path.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("NEWSAGENT_DISCOURSE_INSTANCES"));
}

#[test]
fn output_cannot_be_combined_with_a_subcommand() {
    for subcommand in ["check-mentions", "check-links"] {
        let output = newsagent(&["--output", "/tmp/x.md", subcommand, "draft.md"]);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert_eq!(output.status.code(), Some(2), "{subcommand}");
        assert!(
            stderr.contains(&format!("'{subcommand}' cannot be used with '--output")),
            "{subcommand}: {stderr}"
        );
    }
}

#[tokio::test]
async fn check_links_reports_unreachable_links_and_exits_accordingly() {
    let server = MockServer::start().await;
    for (route, status) in [("/ok", 200), ("/missing", 404)] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
    }
    let uri = server.uri();
    let cases = [
        (
            "no links",
            "Only [a note](other.md).".to_string(),
            0,
            "SUMMARY ok=0 broken=0 temporary=0\n".to_string(),
        ),
        (
            "all reachable",
            format!("[ok]({uri}/ok)"),
            0,
            "SUMMARY ok=1 broken=0 temporary=0\n".to_string(),
        ),
        (
            "one broken",
            format!("[ok]({uri}/ok)\n\n[missing]({uri}/missing)"),
            1,
            format!("broken 404 {uri}/missing (line 3)\nSUMMARY ok=1 broken=1 temporary=0\n"),
        ),
    ];

    for (name, markdown, code, stdout) in cases {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("draft.md");
        std::fs::write(&file, markdown).unwrap();

        let output = tokio::task::spawn_blocking(move || {
            newsagent(&["check-links", file.to_str().unwrap()])
        })
        .await
        .unwrap();

        assert_eq!(output.status.code(), Some(code), "{name}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), stdout, "{name}");
    }
}

#[test]
fn check_links_fails_on_an_unreadable_file() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("missing.md");

    let output = newsagent(&["check-links", file.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(&format!("Reading {}", file.display()))
    );
}
