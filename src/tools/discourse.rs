use serde::{Deserialize, Deserializer};

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
