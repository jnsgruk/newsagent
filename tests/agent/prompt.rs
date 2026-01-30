use newsagent::agent::prompt::{build_initial_prompt, PROMPT};

#[test]
fn build_initial_prompt_includes_base_prompt_and_web_hint() {
    let output = build_initial_prompt(None);

    assert!(output.starts_with(PROMPT));
    assert!(output
        .contains("When a response needs link verification or summaries, use the browse_web tool"));
}

#[test]
fn build_initial_prompt_includes_section_hint() {
    let output = build_initial_prompt(Some("  Weekly Updates  "));

    assert!(output.contains("Use the todoist_tasks tool with section: \"Weekly Updates\"."));
    assert!(output.contains("browse_web tool"));
}

#[test]
fn build_initial_prompt_ignores_blank_section() {
    let output = build_initial_prompt(Some("  "));

    assert!(!output.contains("todoist_tasks tool"));
    assert!(output.contains("browse_web tool"));
}
