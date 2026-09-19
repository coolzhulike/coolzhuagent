#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolCallIdentity {
    pub call_id: String,
    pub provider_tool_call_id: String,
    pub session_id: String,
    pub turn_id: String,
}

impl ToolCallIdentity {
    pub(crate) fn from_provider(
        provider_tool_call_id: &str,
        session_id: &str,
        turn_id: &str,
    ) -> Self {
        let provider_tool_call_id = provider_tool_call_id.trim().to_string();
        let session_id = session_id.trim().to_string();
        let turn_id = turn_id.trim().to_string();
        let call_id = format!(
            "cu-{}-{}-{}",
            stable_id_component(&session_id),
            stable_id_component(&turn_id),
            stable_id_component(&provider_tool_call_id),
        );
        Self {
            call_id,
            provider_tool_call_id,
            session_id,
            turn_id,
        }
    }
}

fn stable_id_component(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if normalized.is_empty() {
        "missing".to_string()
    } else {
        normalized
    }
}

pub(crate) fn terminal_status_is_error(status: &str) -> bool {
    matches!(
        status,
        "failed" | "rejected" | "timeout" | "timed_out" | "blocked" | "cancelled"
    )
}

#[cfg(test)]
mod tests {
    use api::ToolDefinition;

    use super::*;

    #[test]
    fn provider_tool_id_is_preserved_for_computer_use() {
        let ids = ToolCallIdentity::from_provider("toolu_abc", "session-1", "turn-1");

        assert_eq!(ids.provider_tool_call_id, "toolu_abc");
        assert_eq!(ids.session_id, "session-1");
        assert_eq!(ids.turn_id, "turn-1");
        assert!(ids.call_id.ends_with("toolu_abc"));
        assert_eq!(
            ids,
            ToolCallIdentity::from_provider("toolu_abc", "session-1", "turn-1")
        );
    }

    #[test]
    fn computer_use_tool_schema_is_task_level_and_closed() {
        let definition = crate::computer_use_tool_definition();
        let schema = definition.input_schema;

        assert_eq!(definition.name, crate::COMPUTER_USE_TOOL_NAME);
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["target"]["additionalProperties"],
            false
        );
        assert_eq!(
            schema["required"],
            serde_json::json!(["objective", "success_criteria"])
        );
        assert_eq!(
            schema["properties"]["surface"]["enum"],
            serde_json::json!(["auto", "desktop", "browser"])
        );
        let properties = schema["properties"].as_object().unwrap();
        for forbidden in ["x", "y", "execute", "approved", "retry", "max_retries"] {
            assert!(
                !properties.contains_key(forbidden),
                "forbidden field {forbidden}"
            );
        }
    }

    #[test]
    fn compact_context_keeps_file_tools_alongside_computer_use() {
        let tools = Some(vec![
            crate::semantic_dispatch_tool_definition(),
            crate::computer_use_tool_definition(),
            ToolDefinition {
                name: "read_file".into(),
                description: None,
                input_schema: serde_json::json!({"type":"object"}),
            },
            ToolDefinition {
                name: "write_file".into(),
                description: None,
                input_schema: serde_json::json!({"type":"object"}),
            },
        ]);

        let selected = crate::select_tools_for_request(tools, true, 8_192, true)
            .expect("小上下文仍保留文件和 UI 的合法工具入口");

        let names = selected.iter().map(|tool| tool.name.as_str()).collect::<Vec<_>>();
        assert_eq!(names.len(), 4);
        assert!(names.contains(&crate::COMPUTER_USE_TOOL_NAME));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"tools_semantic_dispatch"));
    }

    #[test]
    fn no_tool_intent_still_hides_compact_tool() {
        let selected = crate::select_tools_for_request(
            Some(vec![crate::computer_use_tool_definition()]),
            true,
            8_192,
            false,
        );
        assert!(selected.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn provider_identity_reaches_computer_use_tool_result() {
        let _guard = crate::tests::config_test_guard();
        let mut config = crate::WorkspaceConfig::default();
        config.model.enable_llm_tools = true;
        config.model.llm_tool_exposure = Some("all".to_string());
        config.computer_use.enabled = true;
        let previous = std::mem::replace(&mut *crate::workspace_config().lock().unwrap(), config);
        let response = crate::run_model_tool_dispatch_for_session_with_identity(
            "computer_use.perform",
            &serde_json::json!({
                "objective": "click submit",
                "surface": "browser",
                "success_criteria": ["success is visible"]
            }),
            Some("session-1"),
            Some("toolu_abc"),
            Some("turn-1"),
            None,
        )
        .await;
        *crate::workspace_config().lock().unwrap() = previous;
        let response = response.expect("已显式开放的 Computer Use 保留 provider 身份");
        let result: computer_use::ComputerUseResult = serde_json::from_str(
            response
                .tool_result_text
                .as_deref()
                .expect("structured terminal result"),
        )
        .unwrap();

        assert_eq!(result.provider_tool_call_id.as_deref(), Some("toolu_abc"));
        assert!(result.call_id.ends_with("toolu_abc"));
        assert_eq!(response.route, "computer-use-task-controller");
        assert!(!result.goal_achieved);
        assert!(result.error.is_some());
    }

    #[test]
    fn every_non_success_terminal_status_is_an_error_for_the_model() {
        assert!(!super::terminal_status_is_error("ok"));
        for status in [
            "failed",
            "rejected",
            "timeout",
            "timed_out",
            "blocked",
            "cancelled",
        ] {
            assert!(
                super::terminal_status_is_error(status),
                "status {status} must set ToolResult.is_error"
            );
        }
    }
}
