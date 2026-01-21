use anyhow::Context;
use readability::extractor;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Deserializer, Serialize};
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum WebReadabilityToolError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Disallowed host: {0}")]
    DisallowedHost(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

fn deserialize_option_usize<'de, D>(deserializer: D) -> Result<Option<usize>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    if let Some(s) = s {
        s.parse::<usize>()
            .map(Some)
            .map_err(serde::de::Error::custom)
    } else {
        Ok(None)
    }
}

fn deserialize_option_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    if let Some(s) = s {
        s.parse::<u64>().map(Some).map_err(serde::de::Error::custom)
    } else {
        Ok(None)
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct WebConfig {
    #[serde(rename = "web_allowlist")]
    pub allowlist: Option<String>,
    #[serde(
        rename = "web_max_chars",
        default,
        deserialize_with = "deserialize_option_usize"
    )]
    pub max_chars: Option<usize>,
    #[serde(
        rename = "web_timeout_secs",
        default,
        deserialize_with = "deserialize_option_u64"
    )]
    pub timeout_secs: Option<u64>,
    #[serde(
        rename = "web_min_interval_ms",
        default,
        deserialize_with = "deserialize_option_u64"
    )]
    pub min_interval_ms: Option<u64>,
    #[serde(rename = "web_user_agent")]
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WebReadabilityTool {
    allowlist: Vec<String>,
    max_chars: usize,
    min_interval: Duration,
    last_request: Arc<Mutex<Option<Instant>>>,
    client: reqwest::Client,
}

#[derive(Deserialize, Debug)]
pub struct WebReadabilityArgs {
    /// URL to fetch and extract text from.
    pub url: String,
}

#[derive(Serialize, Debug)]
pub struct WebReadabilityOutput {
    pub title: String,
    pub text: String,
    pub source_url: String,
    pub truncated: bool,
}

impl Tool for WebReadabilityTool {
    const NAME: &'static str = "browse_web";

    type Error = WebReadabilityToolError;
    type Args = WebReadabilityArgs;
    type Output = WebReadabilityOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetch a web page, extract the main content using Readability, and return plain text."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch and extract content from."
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        log::info!("fetching {}...", args.url);
        let url =
            Url::parse(&args.url).map_err(|_| WebReadabilityToolError::InvalidUrl(args.url))?;
        let host = url
            .host_str()
            .ok_or_else(|| WebReadabilityToolError::InvalidUrl(url.to_string()))?;
        if !self.is_host_allowed(host) {
            return Err(WebReadabilityToolError::DisallowedHost(host.to_string()));
        }

        self.wait_for_rate_limit().await;

        let url_string = url.to_string();
        let source_url = url_string.clone();
        let response = self
            .client
            .get(&url_string)
            .send()
            .await
            .context("Web request failed")?
            .error_for_status()
            .context("Web request returned error status")?;
        let body = response.text().await.context("Web response body")?;
        let mut cursor = Cursor::new(body);
        let product = extractor::extract(&mut cursor, &url).context("Readability extract")?;

        let mut text = product.text;
        let truncated = if text.chars().count() > self.max_chars {
            text = text.chars().take(self.max_chars).collect::<String>();
            true
        } else {
            false
        };

        Ok(WebReadabilityOutput {
            title: product.title,
            text,
            source_url,
            truncated,
        })
    }
}

impl WebReadabilityTool {
    pub fn new(config: WebConfig) -> Result<Self, WebReadabilityToolError> {
        let allowlist = config
            .allowlist
            .unwrap_or_default()
            .split(',')
            .map(|entry| entry.trim().to_string())
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>();

        let max_chars = config.max_chars.unwrap_or(8000);
        let timeout_secs = config.timeout_secs.unwrap_or(15);
        let min_interval_ms = config.min_interval_ms.unwrap_or(0);
        let user_agent = config
            .user_agent
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "newsagent/0.1".to_string());

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent(user_agent.clone())
            .build()
            .context("Failed to build web HTTP client")?;

        Ok(Self {
            allowlist,
            max_chars,
            min_interval: Duration::from_millis(min_interval_ms),
            last_request: Arc::new(Mutex::new(None)),
            client,
        })
    }

    fn is_host_allowed(&self, host: &str) -> bool {
        if self.allowlist.is_empty() {
            return true;
        }
        self.allowlist.iter().any(|entry| {
            if entry == host {
                return true;
            }
            if let Some(stripped) = entry.strip_prefix('.') {
                return host.ends_with(stripped);
            }
            host.ends_with(entry)
        })
    }

    async fn wait_for_rate_limit(&self) {
        if self.min_interval == Duration::from_millis(0) {
            return;
        }
        let sleep_for = {
            let mut guard = self.last_request.lock().unwrap();
            let now = Instant::now();
            let sleep_for = match *guard {
                Some(last) => self.min_interval.saturating_sub(now.duration_since(last)),
                None => Duration::from_millis(0),
            };
            *guard = Some(now + sleep_for);
            sleep_for
        };
        if sleep_for > Duration::from_millis(0) {
            tokio::time::sleep(sleep_for).await;
        }
    }
}
