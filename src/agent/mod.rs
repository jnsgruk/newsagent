pub mod prompt;

use anyhow::Error;

use prompt::build_initial_prompt;

use crate::config::AppConfig;
use crate::tools::glean::GleanTool;
use crate::tools::todoist::TodoistTasksTool;
use crate::tools::web::WebReadabilityTool;

use rig::agent::Agent as RigAgent;
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::gemini;
use rig::providers::gemini::completion::CompletionModel;

pub struct Agent {
    agent: RigAgent<CompletionModel>,
    config: AppConfig,
}

impl Agent {
    pub fn new(config: AppConfig) -> Result<Self, Error> {
        let agent = Self::build(&config)?;
        Ok(Self { agent, config })
    }

    fn build(config: &AppConfig) -> Result<RigAgent<CompletionModel>, Error> {
        let todoist_tool = TodoistTasksTool::new(config.todoist.clone())?;
        let web_tool = WebReadabilityTool::new(config.web.clone())?;
        let glean_tool = GleanTool::new(config.glean.clone())?;
        let glean_context = glean_tool.gather_context()?;

        let gemini_client = gemini::Client::new(&config.gemini_api_key)?;

        let mut agent_builder = gemini_client
            .agent(&config.gemini_model)
            .preamble(
                "You are a concise assistant that helps summarize and organize tasks for newsagent.",
            )
            .tool(todoist_tool)
            .tool(web_tool)
            .tool(glean_tool);

        if !glean_context.is_empty() {
            agent_builder = agent_builder.context(&format!(
                "Use the following sample as a style guide for tone and structure:\n\n{}",
                glean_context
            ));
        }

        Ok(agent_builder.build())
    }

    pub async fn prompt(&self) -> Result<String, Error> {
        let prompt = build_initial_prompt(self.config.todoist.project_section.as_deref());
        log::info!("sending prompt to model");
        self.agent
            .prompt(prompt)
            .multi_turn(20)
            .await
            .map_err(Error::from)
    }
}
