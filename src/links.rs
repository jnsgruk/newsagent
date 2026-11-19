use pulldown_cmark::{Event, Options, Parser, Tag};
use reqwest::redirect::Policy;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const CONCURRENCY: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub url: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkStatus {
    Ok,
    Broken(String),
    Temporary(String),
}

#[derive(Clone)]
pub struct LinkChecker {
    client: reqwest::Client,
    retry_delays: Vec<Duration>,
}

impl LinkChecker {
    pub fn new(retry_delays: Vec<Duration>) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none())
            .user_agent("newsagent/0.1")
            .build()?;
        Ok(Self {
            client,
            retry_delays,
        })
    }

    pub fn with_default_retries() -> anyhow::Result<Self> {
        Self::new([1, 5, 10].into_iter().map(Duration::from_secs).collect())
    }

    pub async fn check(&self, url: &str) -> LinkStatus {
        let mut status = self.attempt(url).await;
        for delay in &self.retry_delays {
            if !matches!(status, LinkStatus::Temporary(_)) {
                break;
            }
            tokio::time::sleep(*delay).await;
            status = self.attempt(url).await;
        }
        status
    }

    pub async fn check_all(&self, links: Vec<Link>) -> Vec<(Link, LinkStatus)> {
        let permits = Arc::new(Semaphore::new(CONCURRENCY));
        let mut tasks = JoinSet::new();
        for (index, link) in links.into_iter().enumerate() {
            let checker = self.clone();
            let permits = permits.clone();
            tasks.spawn(async move {
                let _permit = permits.acquire_owned().await.expect("semaphore closed");
                let status = checker.check(&link.url).await;
                (index, link, status)
            });
        }
        let mut results: Vec<_> = tasks.join_all().await;
        results.sort_by_key(|(index, _, _)| *index);
        results
            .into_iter()
            .map(|(_, link, status)| (link, status))
            .collect()
    }

    async fn attempt(&self, url: &str) -> LinkStatus {
        match self.client.get(url).send().await {
            Ok(response) => classify(response.status().as_u16()),
            Err(err) if err.is_timeout() => LinkStatus::Temporary("timeout".into()),
            Err(err) if err.is_connect() => LinkStatus::Temporary("connection failed".into()),
            Err(err) if err.is_builder() => LinkStatus::Broken("invalid url".into()),
            Err(_) => LinkStatus::Temporary("request failed".into()),
        }
    }
}

fn classify(code: u16) -> LinkStatus {
    match code {
        200..=399 => LinkStatus::Ok,
        403 | 408 | 429 | 500..=599 => LinkStatus::Temporary(code.to_string()),
        _ => LinkStatus::Broken(code.to_string()),
    }
}

pub fn extract_links(markdown: &str) -> Vec<Link> {
    let mut seen = HashSet::new();
    let mut links = Vec::new();
    for (event, range) in Parser::new_ext(markdown, Options::all()).into_offset_iter() {
        let url = match event {
            Event::Start(Tag::Link { dest_url, .. })
            | Event::Start(Tag::Image { dest_url, .. }) => dest_url,
            _ => continue,
        };
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        if seen.insert(url.to_string()) {
            let line = markdown[..range.start].matches('\n').count() + 1;
            links.push(Link {
                url: url.to_string(),
                line,
            });
        }
    }
    links
}
