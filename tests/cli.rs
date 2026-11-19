use std::process::{Command, Output};

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
            &["Draft a source-linked Tech Updates outline"],
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
