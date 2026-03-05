pub mod prompt;

use anyhow::Error;

use prompt::build_initial_prompt;

use crate::config::AppConfig;
use crate::tools::discourse::DiscourseTool;
use crate::tools::glean::GleanTool;
use crate::tools::mailing_list::MailingListTool;
use crate::tools::todoist::TodoistTasksTool;
use crate::tools::web::WebReadabilityTool;

use rig::agent::Agent as RigAgent;
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::gemini;
use rig::providers::gemini::completion::CompletionModel;

struct BuildResult {
    agent: RigAgent<CompletionModel>,
    discourse_hosts: Vec<String>,
    mailing_list_names: Vec<String>,
}

pub struct Agent {
    agent: RigAgent<CompletionModel>,
    config: AppConfig,
    discourse_hosts: Vec<String>,
    mailing_list_names: Vec<String>,
}

impl Agent {
    pub fn new(config: AppConfig) -> Result<Self, Error> {
        let result = Self::build(&config)?;
        Ok(Self {
            agent: result.agent,
            config,
            discourse_hosts: result.discourse_hosts,
            mailing_list_names: result.mailing_list_names,
        })
    }

    fn build(config: &AppConfig) -> Result<BuildResult, Error> {
        let todoist_tool = TodoistTasksTool::new(config.todoist.clone())?;
        let web_tool = WebReadabilityTool::new(config.web.clone())?;
        let glean_tool = GleanTool::new(config.glean.clone())?;
        let glean_context = glean_tool.gather_context()?;

        let discourse_tool = DiscourseTool::new(
            config.discourse.clone(),
            config.web.max_chars.unwrap_or(8000),
        );
        let discourse_hosts = discourse_tool
            .as_ref()
            .map(|t| t.base_urls())
            .unwrap_or_default();

        let gemini_client = gemini::Client::new(&config.gemini_api_key)?;

        let mut agent_builder = gemini_client
            .agent(&config.gemini_model)
            .preamble(
                "You are a concise assistant that helps summarize and organize tasks for newsagent.",
            )
            .tool(todoist_tool)
            .tool(web_tool)
            .tool(glean_tool);

        if let Some(tool) = discourse_tool {
            agent_builder = agent_builder.tool(tool);
        }

        let mailing_list_tool = MailingListTool::new(
            config.mailing_list.clone(),
            config.web.max_chars.unwrap_or(8000),
        );
        let mailing_list_names = mailing_list_tool
            .as_ref()
            .map(|t| t.list_names().to_vec())
            .unwrap_or_default();

        if let Some(tool) = mailing_list_tool {
            agent_builder = agent_builder.tool(tool);
        }

        if !glean_context.is_empty() {
            agent_builder = agent_builder.context(&format!(
                "Use the following sample as a style guide for tone and structure:\n\n{}",
                glean_context
            ));
        }

        Ok(BuildResult {
            agent: agent_builder.build(),
            discourse_hosts,
            mailing_list_names,
        })
    }

    pub async fn prompt(&self) -> Result<String, Error> {
        let prompt = build_initial_prompt(
            self.config.todoist.project_section.as_deref(),
            &self.discourse_hosts,
            &self.mailing_list_names,
        );
        log::info!("sending prompt to model");
        self.agent
            .prompt(prompt)
            .multi_turn(20)
            .await
            .map_err(Error::from)
    }
}
