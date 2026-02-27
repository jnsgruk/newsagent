use crate::tools::glean::GleanConfig;
use crate::tools::todoist::TodoistConfig;
use crate::tools::web::WebConfig;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub gemini_api_key: String,
    #[serde(default = "default_gemini_model")]
    pub gemini_model: String,

    #[serde(flatten)]
    pub todoist: TodoistConfig,
    #[serde(flatten)]
    pub glean: GleanConfig,
    #[serde(flatten)]
    pub web: WebConfig,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(envy::prefixed("NEWSAGENT_").from_env::<AppConfig>()?)
    }
}

fn default_gemini_model() -> String {
    "gemini-3.1-pro-preview".to_string()
}
