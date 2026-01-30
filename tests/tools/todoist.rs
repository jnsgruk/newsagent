use newsagent::tools::todoist::{TodoistConfig, TodoistTasksArgs, TodoistTasksTool};
use rig::tool::Tool;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn todoist_tool_uses_mocked_api() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    let token = "test-token";
    let project_id = "proj-1";
    let section_id = "sec-1";

    Mock::given(method("GET"))
        .and(path("/api/v1/sections"))
        .and(query_param("project_id", project_id))
        .and(header("authorization", format!("Bearer {token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "id": section_id,
                    "section_order": 1,
                    "name": "Inbox"
                }
            ],
            "next_cursor": null
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/tasks"))
        .and(query_param("project_id", project_id))
        .and(query_param("section_id", section_id))
        .and(header("authorization", format!("Bearer {token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "id": "task-1",
                    "content": "Test task",
                    "description": "",
                    "parent_id": null,
                    "section_id": section_id,
                    "child_order": 1,
                    "checked": false
                }
            ],
            "next_cursor": null
        })))
        .mount(&server)
        .await;

    let tool = TodoistTasksTool::new(TodoistConfig {
        api_token: token.to_string(),
        project_id: project_id.to_string(),
        project_section: None,
        base_url,
    })
    .expect("Failed to create Todoist tool");

    let output = tool
        .call(TodoistTasksArgs {
            section: Some("Inbox".to_string()),
        })
        .await
        .expect("Todoist tool call failed");

    assert_eq!(output.markdown, "- [ ] Test task");
}

#[tokio::test]
async fn renders_tasks_grouped_by_section_with_descriptions() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    let token = "test-token";
    let project_id = "proj-1";

    Mock::given(method("GET"))
        .and(path("/api/v1/sections"))
        .and(query_param("project_id", project_id))
        .and(header("authorization", format!("Bearer {token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {"id": "s2", "section_order": 2, "name": "Later"},
                {"id": "s1", "section_order": 1, "name": "Now"}
            ],
            "next_cursor": null
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/tasks"))
        .and(query_param("project_id", project_id))
        .and(header("authorization", format!("Bearer {token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "id": "1",
                    "content": "Task 1",
                    "description": "",
                    "parent_id": null,
                    "section_id": "s1",
                    "child_order": 2,
                    "checked": false
                },
                {
                    "id": "2",
                    "content": "Task 2",
                    "description": "line1\nline2",
                    "parent_id": null,
                    "section_id": "s1",
                    "child_order": 1,
                    "checked": false
                },
                {
                    "id": "3",
                    "content": "Subtask",
                    "description": "",
                    "parent_id": "2",
                    "section_id": "s1",
                    "child_order": 1,
                    "checked": true
                },
                {
                    "id": "4",
                    "content": "Later task",
                    "description": "",
                    "parent_id": null,
                    "section_id": "s2",
                    "child_order": 1,
                    "checked": false
                },
                {
                    "id": "5",
                    "content": "No section task",
                    "description": "",
                    "parent_id": null,
                    "section_id": null,
                    "child_order": 1,
                    "checked": false
                }
            ],
            "next_cursor": null
        })))
        .mount(&server)
        .await;

    let tool = TodoistTasksTool::new(TodoistConfig {
        api_token: token.to_string(),
        project_id: project_id.to_string(),
        project_section: None,
        base_url: base_url.clone(),
    })
    .expect("Failed to create Todoist tool");

    let output = tool
        .call(TodoistTasksArgs { section: None })
        .await
        .expect("Todoist tool call failed");

    let expected = "## Now\n\n- [ ] Task 2\n  - **Description**: line1\n    line2\n  - [x] Subtask\n- [ ] Task 1\n\n## Later\n\n- [ ] Later task\n\n## (No Section)\n\n- [ ] No section task";
    assert_eq!(output.markdown, expected);
}

#[tokio::test]
async fn renders_filtered_section_without_headers() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    let token = "test-token";
    let project_id = "proj-1";
    let section_id = "s1";

    Mock::given(method("GET"))
        .and(path("/api/v1/sections"))
        .and(query_param("project_id", project_id))
        .and(header("authorization", format!("Bearer {token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {"id": section_id, "section_order": 1, "name": "Now"}
            ],
            "next_cursor": null
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/tasks"))
        .and(query_param("project_id", project_id))
        .and(query_param("section_id", section_id))
        .and(header("authorization", format!("Bearer {token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "id": "1",
                    "content": "Task 1",
                    "description": "",
                    "parent_id": null,
                    "section_id": section_id,
                    "child_order": 1,
                    "checked": false
                }
            ],
            "next_cursor": null
        })))
        .mount(&server)
        .await;

    let tool = TodoistTasksTool::new(TodoistConfig {
        api_token: token.to_string(),
        project_id: project_id.to_string(),
        project_section: None,
        base_url,
    })
    .expect("Failed to create Todoist tool");

    let output = tool
        .call(TodoistTasksArgs {
            section: Some("Now".to_string()),
        })
        .await
        .expect("Todoist tool call failed");

    assert_eq!(output.markdown, "- [ ] Task 1");
}
