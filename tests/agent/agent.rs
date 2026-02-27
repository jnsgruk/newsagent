use newsagent::agent::Agent;
use newsagent::config::AppConfig;
use newsagent::tools::discourse::DiscourseConfig;
use newsagent::tools::glean::GleanConfig;
use newsagent::tools::todoist::TodoistConfig;
use newsagent::tools::web::WebConfig;

#[test]
fn agent_new_fails_when_glean_dir_missing() {
    let config = AppConfig {
        gemini_api_key: "test-key".to_string(),
        gemini_model: "test-model".to_string(),
        todoist: TodoistConfig {
            api_token: "todo-token".to_string(),
            project_id: "project-id".to_string(),
            project_section: None,
            base_url: "https://api.todoist.com".to_string(),
        },
        glean: GleanConfig {
            dir: "/does/not/exist".to_string(),
            filter: None,
        },
        web: WebConfig::default(),
        discourse: DiscourseConfig::default(),
    };

    match Agent::new(config) {
        Ok(_) => panic!("Expected Agent::new to fail"),
        Err(err) => {
            let message = err.to_string();
            assert!(message.contains("Glean directory not found"));
        }
    }
}
