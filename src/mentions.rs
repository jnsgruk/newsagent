use anyhow::{bail, Context};
use reqwest::redirect::Policy;
use reqwest::StatusCode;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::time::Duration;

const MAX_CANDIDATES: usize = 5;

#[derive(Debug, PartialEq)]
pub struct Candidate {
    pub username: String,
    pub host: String,
}

#[derive(Debug, PartialEq)]
pub enum MentionStatus {
    Valid(String),
    Suggested(Candidate),
    Unknown(Vec<Candidate>),
}

pub struct MentionChecker {
    host: String,
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct SearchResponse {
    users: Vec<SearchUser>,
}

#[derive(Deserialize)]
struct SearchUser {
    username: String,
}

impl MentionChecker {
    pub fn new(host: &str, base_url: &str, api_key: Option<String>) -> anyhow::Result<Self> {
        // Redirects are refused so the API key is never sent to another origin.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none())
            .user_agent("newsagent/0.1")
            .build()?;
        Ok(Self {
            host: host.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            client,
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub async fn user_exists(&self, handle: &str) -> anyhow::Result<bool> {
        let response = self
            .get(&format!("{}/u/{}.json", self.base_url, handle))
            .send()
            .await
            .with_context(|| format!("{}: user lookup failed", self.host))?;
        match response.status() {
            StatusCode::OK => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => bail!("{}: user lookup returned {}", self.host, status),
        }
    }

    pub async fn search_users(&self, term: &str) -> anyhow::Result<Vec<String>> {
        let search: SearchResponse = self
            .get(&format!("{}/u/search/users.json", self.base_url))
            .query(&[("term", term)])
            .send()
            .await
            .with_context(|| format!("{}: user search failed", self.host))?
            .error_for_status()
            .with_context(|| format!("{}: user search returned error status", self.host))?
            .json()
            .await
            .with_context(|| format!("{}: failed to parse user search", self.host))?;
        Ok(search.users.into_iter().map(|u| u.username).collect())
    }

    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        let request = self.client.get(url);
        match &self.api_key {
            Some(key) => request
                .header("Api-Key", key)
                .header("Api-Username", "system"),
            None => request,
        }
    }
}

pub async fn check_mention(
    checkers: &[MentionChecker],
    handle: &str,
) -> anyhow::Result<MentionStatus> {
    for checker in checkers {
        if checker.user_exists(handle).await? {
            return Ok(MentionStatus::Valid(checker.host().to_string()));
        }
    }

    let mut candidates = Vec::new();
    for checker in checkers {
        for username in checker.search_users(handle).await? {
            candidates.push(Candidate {
                username,
                host: checker.host().to_string(),
            });
        }
    }
    if candidates.len() == 1 {
        return Ok(MentionStatus::Suggested(candidates.remove(0)));
    }
    candidates.truncate(MAX_CANDIDATES);
    Ok(MentionStatus::Unknown(candidates))
}

pub fn extract_mentions(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut mentions = BTreeSet::new();
    for (i, &ch) in chars.iter().enumerate() {
        if ch != '@' || i > 0 && is_mention_prefix(chars[i - 1]) {
            continue;
        }
        if !chars.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        let end = (i + 1..chars.len())
            .find(|&j| !is_handle_char(chars[j]))
            .unwrap_or(chars.len());
        let handle: String = chars[i + 1..end].iter().collect();
        let handle = handle.trim_end_matches('.');

        // Link text such as [@user](https://github.com/user) is not a Discourse mention.
        let after = handle.len() + i + 1;
        let is_link_text = i > 0
            && chars[i - 1] == '['
            && chars.get(after) == Some(&']')
            && chars.get(after + 1) == Some(&'(');
        if !is_link_text {
            mentions.insert(handle.to_string());
        }
    }
    mentions.into_iter().collect()
}

fn is_mention_prefix(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | '/')
}

fn is_handle_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}
