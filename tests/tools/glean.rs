use newsagent::tools::glean::{GleanConfig, GleanTool, GleanToolError};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("Failed to create parent directory");
    }
    fs::write(path, contents).expect("Failed to write test file");
}

#[test]
fn gathers_all_markdown_in_order() {
    let dir = tempdir().expect("Failed to create tempdir");
    write_file(&dir.path().join("a.md"), "Alpha\n");
    write_file(&dir.path().join("b.md"), "Beta");
    write_file(&dir.path().join("sub/c.md"), "Charlie");
    write_file(&dir.path().join("ignored.txt"), "Ignore me");

    let tool = GleanTool::new(GleanConfig {
        dir: dir.path().to_string_lossy().to_string(),
        filter: None,
    })
    .expect("Failed to create tool");

    let context = tool.gather_context().expect("Failed to gather context");

    let expected = "# a.md\n\nAlpha\n\n# b.md\n\nBeta\n\n# sub/c.md\n\nCharlie";
    assert_eq!(context, expected);
}

#[test]
fn gathers_only_matching_files() {
    let dir = tempdir().expect("Failed to create tempdir");
    write_file(&dir.path().join("note.md"), "Note");
    write_file(&dir.path().join("todo.md"), "Todo");
    write_file(&dir.path().join("sub/note-extra.md"), "Extra");

    let tool = GleanTool::new(GleanConfig {
        dir: dir.path().to_string_lossy().to_string(),
        filter: Some("note".to_string()),
    })
    .expect("Failed to create tool");

    let context = tool.gather_context().expect("Failed to gather context");

    let expected = "# note.md\n\nNote\n\n# sub/note-extra.md\n\nExtra";
    assert_eq!(context, expected);
}

#[test]
fn rejects_invalid_filter() {
    let dir = tempdir().expect("Failed to create tempdir");

    let err = GleanTool::new(GleanConfig {
        dir: dir.path().to_string_lossy().to_string(),
        filter: Some("bad/filter".to_string()),
    })
    .expect_err("Expected invalid filter error");

    match err {
        GleanToolError::InvalidFilter(value) => assert_eq!(value, "bad/filter"),
        other => panic!("Unexpected error: {other:?}"),
    }
}
