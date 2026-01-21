use anyhow::Context;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum TodoistToolError {
    #[error("Todoist API error (status {status}): {body}")]
    ApiStatus { status: u16, body: String },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Deserialize, Debug, Clone)]
pub struct TodoistConfig {
    #[serde(rename = "todoist_api_token")]
    pub api_token: String,
    #[serde(rename = "todoist_project_id")]
    pub project_id: String,
    #[serde(rename = "todoist_project_section")]
    pub project_section: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TodoistTasksTool {
    project_id: String,
    client: Client,
}

#[derive(Deserialize, Debug)]
pub struct TodoistTasksArgs {
    /// Filter tasks by section name (case-insensitive)
    pub section: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct TodoistTasksOutput {
    pub markdown: String,
}

#[derive(Deserialize, Debug, Clone)]
struct Task {
    id: String,
    content: String,
    description: String,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    section_id: Option<String>,
    #[serde(rename = "child_order")]
    order: i32,
    #[serde(rename = "checked")]
    is_completed: bool,
}

#[derive(Deserialize, Debug, Clone)]
struct Section {
    id: String,
    #[serde(rename = "section_order")]
    order: i32,
    name: String,
}

#[derive(Deserialize)]
struct ApiListResponse<T> {
    results: Vec<T>,
    next_cursor: Option<String>,
}

impl Tool for TodoistTasksTool {
    const NAME: &'static str = "todoist_tasks";

    type Error = TodoistToolError;
    type Args = TodoistTasksArgs;
    type Output = TodoistTasksOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Fetch Todoist tasks for the configured project and return them as Markdown."
                    .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "section": {
                        "type": "string",
                        "description": "Optional section name to filter by (case-insensitive)."
                    }
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let section_filter = args.section.as_deref().filter(|s| !s.trim().is_empty());

        if let Some(name) = section_filter {
            let all_sections = self.fetch_sections().await?;
            let section = all_sections
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(name));

            match section {
                Some(s) => {
                    log::info!(
                        "fetching tasks for section '{}' (project_id: {})...",
                        s.name,
                        s.id
                    );
                    let tasks = self.fetch_tasks(Some(&s.id)).await?;
                    log::info!("fetched {} tasks", tasks.len());
                    let markdown = self.render_tasks(&tasks, &[s.clone()], true);
                    Ok(TodoistTasksOutput { markdown })
                }
                None => {
                    log::warn!("section '{}' not found", name);
                    Ok(TodoistTasksOutput {
                        markdown: String::new(),
                    })
                }
            }
        } else {
            log::info!("fetching all tasks (project_id: {})...", self.project_id);
            let tasks = self.fetch_tasks(None).await?;
            let sections = self.fetch_sections().await?;
            log::info!("fetched {} tasks", tasks.len());
            let markdown = self.render_tasks(&tasks, &sections, false);
            Ok(TodoistTasksOutput { markdown })
        }
    }
}

impl TodoistTasksTool {
    pub fn new(config: TodoistConfig) -> Result<Self, TodoistToolError> {
        let token = config.api_token;
        let project_id = config.project_id;
        let mut headers = HeaderMap::new();
        let auth_value = format!("Bearer {}", token);
        let auth_value = HeaderValue::from_str(&auth_value)
            .context("Invalid TODOIST_API_TOKEN for Authorization header")?;
        headers.insert(AUTHORIZATION, auth_value);
        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .build()
            .context("Failed to build Todoist HTTP client")?;
        Ok(Self { project_id, client })
    }

    async fn fetch_tasks(&self, section_id: Option<&str>) -> Result<Vec<Task>, TodoistToolError> {
        let mut tasks = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut url = format!(
                "https://api.todoist.com/api/v1/tasks?project_id={}",
                self.project_id
            );
            if let Some(sid) = section_id {
                url.push_str(&format!("&section_id={}", sid));
            }
            if let Some(c) = &cursor {
                url.push_str("&cursor=");
                url.push_str(c);
            }

            let response = self
                .client
                .get(&url)
                .send()
                .await
                .context("Todoist tasks request failed")?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                return Err(TodoistToolError::ApiStatus { status, body });
            }

            let body = response.text().await.context("Todoist tasks body")?;
            let response_data: ApiListResponse<Task> =
                serde_json::from_str(&body).context("Todoist tasks JSON")?;
            tasks.extend(response_data.results);
            cursor = response_data.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(tasks)
    }

    async fn fetch_sections(&self) -> Result<Vec<Section>, TodoistToolError> {
        let mut sections = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut url = format!(
                "https://api.todoist.com/api/v1/sections?project_id={}",
                self.project_id
            );
            if let Some(c) = &cursor {
                url.push_str("&cursor=");
                url.push_str(c);
            }

            let response = self
                .client
                .get(&url)
                .send()
                .await
                .context("Todoist sections request failed")?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                return Err(TodoistToolError::ApiStatus { status, body });
            }

            let body = response.text().await.context("Todoist sections body")?;
            let response_data: ApiListResponse<Section> =
                serde_json::from_str(&body).context("Todoist sections JSON")?;
            sections.extend(response_data.results);
            cursor = response_data.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(sections)
    }

    fn render_tasks(
        &self,
        tasks: &[Task],
        sections: &[Section],
        hide_section_headers: bool,
    ) -> String {
        let mut sections_sorted = sections.to_vec();
        sections_sorted.sort_by_key(|s| s.order);

        let mut tasks_by_parent: HashMap<Option<String>, Vec<Task>> = HashMap::new();
        for task in tasks {
            tasks_by_parent
                .entry(task.parent_id.clone())
                .or_default()
                .push(task.clone());
        }

        for tasks in tasks_by_parent.values_mut() {
            tasks.sort_by_key(|t| t.order);
        }

        let root_tasks = tasks_by_parent.get(&None).cloned().unwrap_or_default();
        let mut root_tasks_by_section: HashMap<Option<String>, Vec<Task>> = HashMap::new();
        for task in root_tasks {
            root_tasks_by_section
                .entry(task.section_id.clone())
                .or_default()
                .push(task);
        }

        let mut output = String::new();

        for section in &sections_sorted {
            if let Some(tasks) = root_tasks_by_section.get(&Some(section.id.clone())) {
                if !hide_section_headers {
                    output.push_str(&format!("## {}\n\n", section.name));
                }
                for task in tasks {
                    format_task_recursive(task, &tasks_by_parent, 0, &mut output);
                }
                if !hide_section_headers {
                    output.push('\n');
                }
            }
        }

        if !hide_section_headers {
            if let Some(tasks) = root_tasks_by_section.get(&None) {
                if !sections_sorted.is_empty() {
                    output.push_str("## (No Section)\n\n");
                }
                for task in tasks {
                    format_task_recursive(task, &tasks_by_parent, 0, &mut output);
                }
            }
        }

        output.trim().to_string()
    }
}

fn format_task_recursive(
    task: &Task,
    tasks_by_parent: &HashMap<Option<String>, Vec<Task>>,
    indent_level: usize,
    output: &mut String,
) {
    let indent = "  ".repeat(indent_level);
    let checkbox = if task.is_completed { "[x]" } else { "[ ]" };
    output.push_str(&format!("{}- {} {}\n", indent, checkbox, task.content));

    if !task.description.is_empty() {
        let desc_indent = "  ".repeat(indent_level + 1);
        let mut lines = task.description.lines();
        if let Some(first) = lines.next() {
            output.push_str(&format!("{}- **Description**: {}\n", desc_indent, first));
            for line in lines {
                output.push_str(&format!("{}  {}\n", desc_indent, line));
            }
        }
    }

    if let Some(children) = tasks_by_parent.get(&Some(task.id.clone())) {
        for child in children {
            format_task_recursive(child, tasks_by_parent, indent_level + 1, output);
        }
    }
}
