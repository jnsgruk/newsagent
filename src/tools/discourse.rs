use anyhow::Context;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Deserializer, Serialize};
use std::time::Duration;
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum DiscourseToolError {
    #[error("No configured Discourse instance for host: {0}")]
    NoMatchingInstance(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Not a Discourse topic URL: {0}")]
    NotATopicUrl(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone)]
pub struct DiscourseInstance {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct DiscourseConfig {
    #[serde(
        rename = "discourse_instances",
        default,
        deserialize_with = "deserialize_discourse_instances"
    )]
    pub instances: Vec<DiscourseInstance>,
}

fn deserialize_discourse_instances<'de, D>(
    deserializer: D,
) -> Result<Vec<DiscourseInstance>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    let Some(s) = s.filter(|v| !v.trim().is_empty()) else {
        return Ok(Vec::new());
    };

    s.split(',')
        .map(|entry| {
            let entry = entry.trim();
            let (base_url, api_key) = entry.split_once('=').ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "invalid discourse instance '{}': expected 'host=api_key'",
                    entry
                ))
            })?;
            Ok(DiscourseInstance {
                base_url: base_url.trim().to_string(),
                api_key: api_key.trim().to_string(),
            })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct DiscourseTool {
    instances: Vec<DiscourseInstance>,
    max_chars: usize,
    client: reqwest::Client,
}

#[derive(Deserialize, Debug)]
pub struct DiscourseArgs {
    /// The Discourse topic URL to fetch.
    pub url: String,
}

#[derive(Serialize, Debug)]
pub struct DiscourseOutput {
    pub title: String,
    pub author: String,
    pub date: String,
    pub text: String,
    pub source_url: String,
    pub truncated: bool,
}

#[derive(Deserialize, Debug)]
struct TopicResponse {
    title: String,
    post_stream: PostStream,
}

#[derive(Deserialize, Debug)]
struct PostStream {
    posts: Vec<Post>,
}

#[derive(Deserialize, Debug)]
struct Post {
    post_number: u64,
    username: String,
    created_at: String,
    cooked: String,
}

impl Tool for DiscourseTool {
    const NAME: &'static str = "discourse_fetch";

    type Error = DiscourseToolError;
    type Args = DiscourseArgs;
    type Output = DiscourseOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Fetch a Discourse topic or post using the API. Use this for URLs matching configured Discourse instances."
                    .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The Discourse topic URL to fetch."
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        log::info!("fetching discourse topic {}...", args.url);
        let url =
            Url::parse(&args.url).map_err(|_| DiscourseToolError::InvalidUrl(args.url.clone()))?;

        let instance = self.find_instance(&url).ok_or_else(|| {
            DiscourseToolError::NoMatchingInstance(url.host_str().unwrap_or("unknown").to_string())
        })?;

        let (topic_id, post_number) = Self::parse_topic_url(&url)
            .ok_or_else(|| DiscourseToolError::NotATopicUrl(args.url.clone()))?;

        let scheme = url.scheme();
        let api_url = format!("{}://{}/t/{}.json", scheme, instance.base_url, topic_id);
        let response = self
            .client
            .get(&api_url)
            .header("Api-Key", &instance.api_key)
            .header("Api-Username", "system")
            .send()
            .await
            .context("Discourse API request failed")?
            .error_for_status()
            .context("Discourse API returned error status")?;

        let topic: TopicResponse = response
            .json()
            .await
            .context("Failed to parse Discourse API response")?;

        let post = if let Some(num) = post_number {
            topic
                .post_stream
                .posts
                .iter()
                .find(|p| p.post_number == num)
                .or_else(|| topic.post_stream.posts.first())
        } else {
            topic.post_stream.posts.first()
        }
        .ok_or_else(|| anyhow::anyhow!("No posts found in topic"))?;

        let mut text = strip_html(&post.cooked);
        let truncated = if text.chars().count() > self.max_chars {
            text = text.chars().take(self.max_chars).collect::<String>();
            true
        } else {
            false
        };

        Ok(DiscourseOutput {
            title: topic.title,
            author: post.username.clone(),
            date: post.created_at.clone(),
            text,
            source_url: args.url,
            truncated,
        })
    }
}

impl DiscourseTool {
    pub fn new(config: DiscourseConfig, max_chars: usize) -> Option<Self> {
        if config.instances.is_empty() {
            return None;
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("newsagent/0.1")
            .build()
            .ok()?;

        Some(Self {
            instances: config.instances,
            max_chars,
            client,
        })
    }

    pub fn base_urls(&self) -> Vec<String> {
        self.instances.iter().map(|i| i.base_url.clone()).collect()
    }

    fn find_instance(&self, url: &Url) -> Option<&DiscourseInstance> {
        let host = url.host_str()?;
        self.instances.iter().find(|i| {
            if let Some((cfg_host, cfg_port)) = i.base_url.rsplit_once(':') {
                // base_url has an explicit port — match host and port
                host == cfg_host && url.port().map(|p| p.to_string()).as_deref() == Some(cfg_port)
            } else {
                host == i.base_url
            }
        })
    }

    /// Parse a Discourse topic URL path like `/t/some-slug/12345` or `/t/some-slug/12345/2`.
    /// Returns `(topic_id, Option<post_number>)`.
    fn parse_topic_url(url: &Url) -> Option<(u64, Option<u64>)> {
        let segments: Vec<&str> = url.path_segments()?.collect();
        // Expect: ["t", slug, topic_id] or ["t", slug, topic_id, post_number]
        if segments.len() < 3 || segments[0] != "t" {
            return None;
        }
        let topic_id = segments[2].parse::<u64>().ok()?;
        let post_number = segments.get(3).and_then(|s| s.parse::<u64>().ok());
        Some((topic_id, post_number))
    }
}

/// Simple HTML tag stripper. Replaces tags with nothing and decodes basic entities.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}
