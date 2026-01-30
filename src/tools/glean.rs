use anyhow::Context;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, thiserror::Error)]
pub enum GleanToolError {
    #[error("NEWSAGENT_GLEAN_DIR environment variable must be set")]
    MissingGleanDir,
    #[error("Invalid NEWSAGENT_GLEAN_FILTER: {0}")]
    InvalidFilter(String),
    #[error("Glean directory not found: {0}")]
    MissingDirectory(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Deserialize, Debug, Clone)]
pub struct GleanConfig {
    #[serde(rename = "glean_dir")]
    pub dir: String,
    #[serde(rename = "glean_filter")]
    pub filter: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GleanTool {
    root: PathBuf,
    filter: Option<String>,
}

impl GleanTool {
    pub fn new(config: GleanConfig) -> Result<Self, GleanToolError> {
        let root = PathBuf::from(config.dir);
        if !root.exists() {
            return Err(GleanToolError::MissingDirectory(root.display().to_string()));
        }
        let filter = config
            .filter
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(filter) = filter.as_ref() {
            if filter.contains('/') || filter.contains('\\') {
                return Err(GleanToolError::InvalidFilter(filter.clone()));
            }
        }
        Ok(Self { root, filter })
    }

    pub fn gather_context(&self) -> Result<String, GleanToolError> {
        match self.filter.as_ref() {
            Some(f) => log::info!(
                "gathering context from '{}' with filter '{}'",
                self.root.display(),
                f
            ),
            None => log::info!(
                "gathering context from '{}' with no filter",
                self.root.display()
            ),
        };

        let mut files = Vec::new();
        for entry in WalkDir::new(&self.root).follow_links(false) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            if let Some(filter) = self.filter.as_ref() {
                let file_name = match path.file_name().and_then(|name| name.to_str()) {
                    Some(name) => name,
                    None => continue,
                };
                if !file_name.contains(filter) {
                    continue;
                }
            }
            files.push(path.to_path_buf());
        }

        files.sort_by(|a, b| a.as_os_str().cmp(b.as_os_str()));

        log::info!("found {} documents to use as context", files.len());

        let mut output = String::new();
        for path in files {
            log::debug!("using {}", path.display());
            let content =
                fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
            let relative = path.strip_prefix(&self.root).unwrap_or(&path);
            output.push_str(&format!("# {}\n\n", display_path(relative)));
            output.push_str(content.trim());
            output.push_str("\n\n");
        }

        Ok(output.trim().to_string())
    }
}

#[derive(Deserialize, Debug)]
pub struct GleanArgs {}

#[derive(Serialize, Debug)]
pub struct GleanOutput {
    pub context: String,
}

impl Tool for GleanTool {
    const NAME: &'static str = "local_markdown_context";

    type Error = GleanToolError;
    type Args = GleanArgs;
    type Output = GleanOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Collect markdown files from the configured local directory and return concatenated context."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let context = self.gather_context()?;
        Ok(GleanOutput { context })
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\u{2028}', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
}
