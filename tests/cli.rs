use std::process::{Command, Output};
use tempfile::TempDir;

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
    for subcommand in ["check-mentions"] {
        let output = newsagent(&["--output", "/tmp/x.md", subcommand, "draft.md"]);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert_eq!(output.status.code(), Some(2), "{subcommand}");
        assert!(
            stderr.contains(&format!("'{subcommand}' cannot be used with '--output")),
            "{subcommand}: {stderr}"
        );
    }
}
