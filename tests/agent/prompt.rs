use newsagent::agent::prompt::{build_initial_prompt, PROMPT};

#[test]
fn prompt_contains_key_sections() {
    assert!(PROMPT.contains("# Role & Audience"));
    assert!(PROMPT.contains("# Tools Available"));
    assert!(PROMPT.contains("# Output Format"));
    assert!(PROMPT.contains("# Constraints"));
    assert!(PROMPT.contains("# Editor Review Notes"));
    assert!(PROMPT.contains("# Example Entry"));
}

#[test]
fn prompt_contains_browse_web_instruction() {
    assert!(PROMPT.contains("browse_web"));
}

#[test]
fn build_initial_prompt_includes_base_prompt() {
    let output = build_initial_prompt(None, &[]);

    assert!(output.starts_with(PROMPT));
}

#[test]
fn build_initial_prompt_includes_section_hint() {
    let output = build_initial_prompt(Some("  Weekly Updates  "), &[]);

    assert!(output.contains("Use the todoist_tasks tool with section: \"Weekly Updates\"."));
}

#[test]
fn build_initial_prompt_ignores_blank_section() {
    let output = build_initial_prompt(Some("  "), &[]);

    assert!(!output.contains("todoist_tasks tool"));
}

#[test]
fn build_initial_prompt_includes_discourse_hint_when_hosts_present() {
    let hosts = vec![
        "discourse.canonical.com".to_string(),
        "discourse.charmhub.io".to_string(),
    ];
    let output = build_initial_prompt(None, &hosts);

    assert!(output.contains("discourse_fetch"));
    assert!(output.contains("discourse.canonical.com, discourse.charmhub.io"));
    assert!(output.contains("private/restricted posts"));
}

#[test]
fn build_initial_prompt_omits_discourse_hint_when_no_hosts() {
    let output = build_initial_prompt(None, &[]);

    // The static prompt mentions discourse_fetch in the Tools section,
    // but the dynamic discourse host hint should not be appended.
    assert!(!output.contains("discourse.canonical.com"));
}
