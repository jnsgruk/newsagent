use anyhow::Context;
use chrono::{Datelike, NaiveDateTime, Utc};
use flate2::read::GzDecoder;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read as IoRead;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum MailingListToolError {
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct MailingListConfig {
    #[serde(
        rename = "mailing_lists",
        default,
        deserialize_with = "deserialize_comma_separated"
    )]
    pub lists: Vec<String>,

    #[serde(rename = "mailing_list_base_url", default)]
    pub base_url: Option<String>,
}

impl MailingListConfig {
    pub fn effective_base_url(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or("https://lists.ubuntu.com/archives")
    }
}

fn deserialize_comma_separated<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    let Some(s) = s.filter(|v| !v.trim().is_empty()) else {
        return Ok(Vec::new());
    };

    Ok(s.split(',')
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
        .collect())
}

#[derive(Debug, Clone)]
pub struct MailingListTool {
    lists: Vec<String>,
    base_url: String,
    client: reqwest::Client,
    max_chars: usize,
}

#[derive(Deserialize, Debug)]
pub struct MailingListArgs {}

#[derive(Serialize, Debug)]
pub struct MailingListOutput {
    pub threads: Vec<ThreadSummary>,
}

#[derive(Serialize, Debug)]
pub struct ThreadSummary {
    pub subject: String,
    pub lists: Vec<String>,
    pub message_count: usize,
    pub authors: Vec<String>,
    pub first_date: String,
    pub last_date: String,
    pub summary: String,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
struct ParsedMessage {
    message_id: Option<String>,
    in_reply_to: Option<String>,
    references: Vec<String>,
    subject: String,
    from: String,
    date: Option<NaiveDateTime>,
    body: String,
    list_name: String,
}

impl Tool for MailingListTool {
    const NAME: &'static str = "mailing_list_threads";

    type Error = MailingListToolError;
    type Args = MailingListArgs;
    type Output = MailingListOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Fetch recent discussion threads from configured Ubuntu mailing lists. Returns deduplicated threads from the last 30 days across all lists. This tool takes no arguments."
                    .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let cutoff = Utc::now().naive_utc() - chrono::Duration::days(30);
        let months = month_strings(Utc::now());

        let mut all_messages = Vec::new();

        for list_name in &self.lists {
            for month in &months {
                match self.fetch_month(list_name, month).await {
                    Ok(data) => {
                        let messages = parse_mbox(&data, list_name, cutoff);
                        log::info!(
                            "{}/{}: {} messages in range",
                            list_name,
                            month,
                            messages.len()
                        );
                        all_messages.extend(messages);
                    }
                    Err(e) => {
                        log::warn!("{}/{}: skipping ({})", list_name, month, e);
                    }
                }
            }
        }

        let threads = build_threads(all_messages);
        let deduplicated = deduplicate_threads(threads);

        let mut summaries: Vec<ThreadSummary> = deduplicated
            .into_iter()
            .map(|t| self.thread_to_summary(t))
            .collect();

        summaries.sort_by(|a, b| b.last_date.cmp(&a.last_date));

        Ok(MailingListOutput { threads: summaries })
    }
}

impl MailingListTool {
    pub fn new(config: MailingListConfig, max_chars: usize) -> Option<Self> {
        if config.lists.is_empty() {
            return None;
        }

        let base_url = config
            .effective_base_url()
            .trim_end_matches('/')
            .to_string();

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("newsagent/0.1")
            .build()
            .ok()?;

        Some(Self {
            lists: config.lists,
            base_url,
            client,
            max_chars,
        })
    }

    pub fn list_names(&self) -> &[String] {
        &self.lists
    }

    async fn fetch_month(&self, list_name: &str, month: &str) -> anyhow::Result<Vec<u8>> {
        let url = format!("{}/{}/{}.txt.gz", self.base_url, list_name, month);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Mailing list archive request failed")?
            .error_for_status()
            .context("Mailing list archive returned error status")?;

        let compressed = response
            .bytes()
            .await
            .context("Failed to read archive response body")?;

        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .context("Failed to decompress gzip archive")?;

        Ok(decompressed)
    }

    fn thread_to_summary(&self, thread: Thread) -> ThreadSummary {
        let mut authors: Vec<String> = thread
            .messages
            .iter()
            .map(|m| m.from.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        authors.sort();

        let mut lists: Vec<String> = thread
            .messages
            .iter()
            .map(|m| m.list_name.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        lists.sort();

        let first_date = thread
            .messages
            .iter()
            .filter_map(|m| m.date)
            .min()
            .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string())
            .unwrap_or_default();

        let last_date = thread
            .messages
            .iter()
            .filter_map(|m| m.date)
            .max()
            .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string())
            .unwrap_or_default();

        let first_body = thread
            .messages
            .iter()
            .min_by_key(|m| m.date)
            .map(|m| &m.body)
            .cloned()
            .unwrap_or_default();

        let truncated = first_body.chars().count() > self.max_chars;
        let summary = if truncated {
            first_body.chars().take(self.max_chars).collect()
        } else {
            first_body
        };

        ThreadSummary {
            subject: thread.subject,
            lists,
            message_count: thread.messages.len(),
            authors,
            first_date,
            last_date,
            summary,
            truncated,
        }
    }
}

/// Generate the Mailman month directory names for the current and previous month.
/// E.g. for March 2026: ["2026-March", "2026-February"]
fn month_strings(now: chrono::DateTime<Utc>) -> Vec<String> {
    let month_names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];

    let current = format!("{}-{}", now.year(), month_names[now.month0() as usize]);

    let (prev_year, prev_month0) = if now.month0() == 0 {
        (now.year() - 1, 11usize)
    } else {
        (now.year(), (now.month0() - 1) as usize)
    };
    let previous = format!("{}-{}", prev_year, month_names[prev_month0]);

    vec![current, previous]
}

/// Split raw mbox data into individual messages and parse them.
/// Filters to messages within the cutoff date.
fn parse_mbox(data: &[u8], list_name: &str, cutoff: NaiveDateTime) -> Vec<ParsedMessage> {
    let text = String::from_utf8_lossy(data);
    let mut messages = Vec::new();

    // Split on mbox "From " separator lines
    let mut current_message = String::new();
    for line in text.lines() {
        if line.starts_with("From ") && !current_message.is_empty() {
            if let Some(msg) = parse_single_message(&current_message, list_name, cutoff) {
                messages.push(msg);
            }
            current_message.clear();
        }
        current_message.push_str(line);
        current_message.push('\n');
    }
    // Don't forget the last message
    if !current_message.is_empty() {
        if let Some(msg) = parse_single_message(&current_message, list_name, cutoff) {
            messages.push(msg);
        }
    }

    messages
}

fn parse_single_message(
    raw: &str,
    list_name: &str,
    cutoff: NaiveDateTime,
) -> Option<ParsedMessage> {
    let parsed = mail_parser::MessageParser::default().parse(raw.as_bytes())?;

    let date = parsed.date().and_then(|d| {
        NaiveDateTime::new(
            chrono::NaiveDate::from_ymd_opt(d.year as i32, d.month as u32, d.day as u32)?,
            chrono::NaiveTime::from_hms_opt(d.hour as u32, d.minute as u32, d.second as u32)?,
        )
        .into()
    });

    // Filter by date if we have one
    if let Some(d) = date {
        if d < cutoff {
            return None;
        }
    }

    let subject = parsed.subject().unwrap_or("").to_string();

    let from = parsed
        .from()
        .and_then(|addr| {
            addr.first().map(|a| {
                a.name()
                    .unwrap_or_else(|| a.address().unwrap_or("unknown"))
                    .to_string()
            })
        })
        .unwrap_or_else(|| "unknown".to_string());

    let message_id = parsed.message_id().map(|s| s.to_string());

    let in_reply_to = parsed.in_reply_to().as_text().map(|s| s.to_string());

    let references: Vec<String> = parsed
        .references()
        .as_text_list()
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    let body = parsed
        .body_text(0)
        .map(|b| b.to_string())
        .unwrap_or_default();

    Some(ParsedMessage {
        message_id,
        in_reply_to,
        references,
        subject,
        from,
        date,
        body,
        list_name: list_name.to_string(),
    })
}

#[derive(Debug)]
struct Thread {
    subject: String,
    messages: Vec<ParsedMessage>,
}

/// Group messages into threads using In-Reply-To/References headers,
/// falling back to normalized subject matching.
fn build_threads(messages: Vec<ParsedMessage>) -> Vec<Thread> {
    // Map message IDs to thread indices
    let mut id_to_thread: HashMap<String, usize> = HashMap::new();
    let mut subject_to_thread: HashMap<String, usize> = HashMap::new();
    let mut threads: Vec<Thread> = Vec::new();

    for msg in messages {
        let mut thread_idx: Option<usize> = None;

        // Try to find thread via In-Reply-To
        if let Some(ref irt) = msg.in_reply_to {
            thread_idx = id_to_thread.get(irt).copied();
        }

        // Try References if In-Reply-To didn't match
        if thread_idx.is_none() {
            for ref_id in &msg.references {
                if let Some(idx) = id_to_thread.get(ref_id).copied() {
                    thread_idx = Some(idx);
                    break;
                }
            }
        }

        // Fall back to normalized subject
        let norm_subj = normalize_subject(&msg.subject);
        if thread_idx.is_none() && !norm_subj.is_empty() {
            thread_idx = subject_to_thread.get(&norm_subj).copied();
        }

        match thread_idx {
            Some(idx) => {
                if let Some(ref mid) = msg.message_id {
                    id_to_thread.insert(mid.clone(), idx);
                }
                threads[idx].messages.push(msg);
            }
            None => {
                let idx = threads.len();
                if let Some(ref mid) = msg.message_id {
                    id_to_thread.insert(mid.clone(), idx);
                }
                if !norm_subj.is_empty() {
                    subject_to_thread.insert(norm_subj, idx);
                }
                threads.push(Thread {
                    subject: msg.subject.clone(),
                    messages: vec![msg],
                });
            }
        }
    }

    threads
}

/// Deduplicate threads across lists by merging threads that share Message-IDs
/// or have matching normalized subjects.
fn deduplicate_threads(threads: Vec<Thread>) -> Vec<Thread> {
    let mut result: Vec<Thread> = Vec::new();
    let mut subject_to_idx: HashMap<String, usize> = HashMap::new();
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();

    for thread in threads {
        let norm_subj = normalize_subject(&thread.subject);

        // Check if any message ID already exists in a previous thread
        let mut merge_idx: Option<usize> = None;
        for msg in &thread.messages {
            if let Some(ref mid) = msg.message_id {
                if let Some(idx) = id_to_idx.get(mid).copied() {
                    merge_idx = Some(idx);
                    break;
                }
            }
        }

        // Also check normalized subject
        if merge_idx.is_none() && !norm_subj.is_empty() {
            merge_idx = subject_to_idx.get(&norm_subj).copied();
        }

        match merge_idx {
            Some(idx) => {
                // Merge into existing thread
                for msg in &thread.messages {
                    if let Some(ref mid) = msg.message_id {
                        id_to_idx.insert(mid.clone(), idx);
                    }
                }
                result[idx].messages.extend(thread.messages);
            }
            None => {
                let idx = result.len();
                for msg in &thread.messages {
                    if let Some(ref mid) = msg.message_id {
                        id_to_idx.insert(mid.clone(), idx);
                    }
                }
                if !norm_subj.is_empty() {
                    subject_to_idx.insert(norm_subj, idx);
                }
                result.push(thread);
            }
        }
    }

    result
}

/// Strip Re:, Fwd:, and [list-name] prefixes from a subject line.
fn normalize_subject(subject: &str) -> String {
    let mut s = subject.trim().to_string();
    loop {
        let before = s.clone();
        // Strip Re: / Fwd: (case-insensitive)
        for prefix in &["re:", "fwd:", "fw:"] {
            if s.to_lowercase().starts_with(prefix) {
                s = s[prefix.len()..].trim_start().to_string();
            }
        }
        // Strip [anything] prefix
        if s.starts_with('[') {
            if let Some(end) = s.find(']') {
                s = s[end + 1..].trim_start().to_string();
            }
        }
        if s == before {
            break;
        }
    }
    s.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_subject() {
        assert_eq!(
            normalize_subject("Re: [ubuntu-devel] Some topic"),
            "some topic"
        );
        assert_eq!(
            normalize_subject("Fwd: Re: [ubuntu-release] Another topic"),
            "another topic"
        );
        assert_eq!(normalize_subject("Simple subject"), "simple subject");
        assert_eq!(normalize_subject("Re: Re: Double re"), "double re");
    }

    #[test]
    fn test_month_strings() {
        use chrono::TimeZone;

        let march = Utc.with_ymd_and_hms(2026, 3, 15, 0, 0, 0).unwrap();
        let months = month_strings(march);
        assert_eq!(months, vec!["2026-March", "2026-February"]);

        let january = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap();
        let months = month_strings(january);
        assert_eq!(months, vec!["2026-January", "2025-December"]);
    }

    #[test]
    fn test_build_threads_by_subject() {
        let msg1 = ParsedMessage {
            message_id: Some("msg1@example.com".to_string()),
            in_reply_to: None,
            references: vec![],
            subject: "Test topic".to_string(),
            from: "Alice".to_string(),
            date: None,
            body: "First message".to_string(),
            list_name: "test-list".to_string(),
        };
        let msg2 = ParsedMessage {
            message_id: Some("msg2@example.com".to_string()),
            in_reply_to: None,
            references: vec![],
            subject: "Re: Test topic".to_string(),
            from: "Bob".to_string(),
            date: None,
            body: "Reply".to_string(),
            list_name: "test-list".to_string(),
        };

        let threads = build_threads(vec![msg1, msg2]);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].messages.len(), 2);
    }

    #[test]
    fn test_build_threads_by_references() {
        let msg1 = ParsedMessage {
            message_id: Some("msg1@example.com".to_string()),
            in_reply_to: None,
            references: vec![],
            subject: "Topic A".to_string(),
            from: "Alice".to_string(),
            date: None,
            body: "First".to_string(),
            list_name: "test-list".to_string(),
        };
        let msg2 = ParsedMessage {
            message_id: Some("msg2@example.com".to_string()),
            in_reply_to: Some("msg1@example.com".to_string()),
            references: vec!["msg1@example.com".to_string()],
            subject: "Different subject entirely".to_string(),
            from: "Bob".to_string(),
            date: None,
            body: "Reply".to_string(),
            list_name: "test-list".to_string(),
        };

        let threads = build_threads(vec![msg1, msg2]);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].messages.len(), 2);
    }
}
