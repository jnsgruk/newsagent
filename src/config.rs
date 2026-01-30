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
    "gemini-3-pro-preview".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    // Ensure tests run sequentially to avoid env var clashes.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            clear_env();
        }
    }

    fn clear_env() {
        for (key, _) in env::vars() {
            if key.starts_with("NEWSAGENT_") {
                env::remove_var(key);
            }
        }
    }

    fn with_env<'a>(vars: impl IntoIterator<Item = (&'a str, &'a str)>) -> EnvGuard {
        let guard = ENV_LOCK.lock().unwrap();
        clear_env();
        for (k, v) in vars {
            env::set_var(k, v);
        }
        EnvGuard { _lock: guard }
    }

    fn required_env_vars() -> Vec<(&'static str, &'static str)> {
        vec![
            ("NEWSAGENT_GEMINI_API_KEY", "test_key"),
            ("NEWSAGENT_TODOIST_API_TOKEN", "todo_token"),
            ("NEWSAGENT_TODOIST_PROJECT_ID", "12345"),
            ("NEWSAGENT_GLEAN_DIR", "/tmp/glean"),
        ]
    }

    #[test]
    fn test_valid_config_loading() {
        let _guard = with_env(required_env_vars());

        let config = AppConfig::from_env().expect("Failed to parse config");

        assert_eq!(config.gemini_api_key, "test_key");
        assert_eq!(config.todoist.api_token, "todo_token");
        assert_eq!(config.todoist.project_id, "12345");
        assert_eq!(config.glean.dir, "/tmp/glean");
        // Check default
        assert_eq!(config.gemini_model, "gemini-3-pro-preview");
    }

    #[test]
    fn test_config_with_optional_fields() {
        let mut vars = required_env_vars();
        vars.extend([
            ("NEWSAGENT_GEMINI_MODEL", "custom-model"),
            ("NEWSAGENT_GLEAN_FILTER", "some-filter"),
            ("NEWSAGENT_WEB_TIMEOUT_SECS", "30"),
        ]);
        let _guard = with_env(vars);

        let config = AppConfig::from_env().expect("Failed to parse config");

        assert_eq!(config.gemini_model, "custom-model");
        assert_eq!(config.glean.filter, Some("some-filter".to_string()));
        assert_eq!(config.web.timeout_secs, Some(30));
    }

    #[test]
    fn test_missing_required_fields() {
        let _guard = with_env(vec![
            ("NEWSAGENT_GEMINI_API_KEY", "test_key"),
            // Missing TODOIST fields
            ("NEWSAGENT_GLEAN_DIR", "/tmp/glean"),
        ]);

        let config = AppConfig::from_env();
        assert!(config.is_err());
    }
}
