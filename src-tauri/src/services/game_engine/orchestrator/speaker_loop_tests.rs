use super::*;
    use crate::services::llm::client::{ChatRequest, ChatResponse};
    use rusqlite::Connection;

    fn test_world() -> WorldDefinition {
        WorldDefinition {
            id: "world-1".to_string(),
            name: "通知测试世界".to_string(),
            genre: String::new(),
            background_prompt: String::new(),
            opening_scene: "开场".to_string(),
            summary: String::new(),
            time_system: String::new(),
            map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
            triggers: Vec::new(),
            time_config: serde_json::json!({}),
            director_config: serde_json::json!({}),
            ui_theme_config: serde_json::json!({}),
            director_system_prompt_base: String::new(),
            director_runtime_system_prompt: String::new(),
            opening_messages: Vec::new(),
            opening_character_ids: Vec::new(),
            player_character_id: None,
        }
    }

    fn notification_call(id: &str) -> ChatToolCall {
        ChatToolCall {
            id: id.to_string(),
            tool_name: "schedule_notification".to_string(),
            arguments: serde_json::json!({
                "action": "create",
                "title": "提醒",
                "body": "该出发了",
                "delay_minutes": 30
            }),
        }
    }

    /// 未在世界包授权的工具调用必须拿到明确的拒绝回执，而不是被静默丢弃——
    /// 静默丢弃会让模型以为工具不存在，转而凭记忆编数据。
    #[tokio::test]
    async fn unauthorized_custom_tool_call_gets_rejection_receipt() {
        let world = test_world();
        let call = ChatToolCall {
            id: "call-x".to_string(),
            tool_name: "stock_quote".to_string(),
            arguments: serde_json::json!({ "code": "sh600519" }),
        };

        let results = execute_speaker_custom_tool_calls(&world, &[], &[], Some(&[call])).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result["ok"], serde_json::json!(false));
        assert_eq!(results[0].result["tool_name"], serde_json::json!("stock_quote"));
        assert_eq!(results[0].result["id"], serde_json::json!("call-x"));
        assert!(
            results[0].result["error"]
                .as_str()
                .unwrap_or("")
                .contains("未在本世界授权"),
            "错误文案应说明未授权: {:?}",
            results[0].result["error"]
        );
    }

    /// schedule_notification 由通知落库路径处理，不该被自定义工具执行器重复执行。
    #[tokio::test]
    async fn custom_tool_executor_skips_notification_calls() {
        let world = test_world();
        let results =
            execute_speaker_custom_tool_calls(&world, &[], &[], Some(&[notification_call("call-1")]))
                .await;

        assert!(results.is_empty());
    }

    fn valid_pending_notification_json(tool_call_id: &str) -> serde_json::Value {
        serde_json::json!({
            "tool_call_id": tool_call_id,
            "source": "character",
            "title": "提醒",
            "body": "该出发了",
            "requested_time": "30分钟后",
            "scheduled_at": "2026-08-02T20:30:00Z",
            "arguments": { "action": "create" }
        })
    }

    // ---- parse_recovered_pending_notifications ----

    #[test]
    fn parse_recovered_pending_notifications_reads_valid_entries() {
        let payload = serde_json::json!({
            "llm_output": {
                "pending_notifications": [
                    valid_pending_notification_json("call-1"),
                    valid_pending_notification_json("call-2"),
                ]
            }
        });

        let parsed = parse_recovered_pending_notifications(&payload);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].tool_call_id, "call-1");
        assert_eq!(parsed[0].source, "character");
        assert_eq!(parsed[0].title, "提醒");
        assert_eq!(parsed[0].scheduled_at, "2026-08-02T20:30:00Z");
        assert_eq!(parsed[1].tool_call_id, "call-2");
    }

    #[test]
    fn parse_recovered_pending_notifications_returns_empty_when_section_missing() {
        // 完全没有 llm_output
        assert!(parse_recovered_pending_notifications(&serde_json::json!({})).is_empty());
        // 有 llm_output 但没有 pending_notifications
        assert!(parse_recovered_pending_notifications(&serde_json::json!({
            "llm_output": { "speaker": "Alice" }
        }))
        .is_empty());
        // pending_notifications 为 null
        assert!(parse_recovered_pending_notifications(&serde_json::json!({
            "llm_output": { "pending_notifications": null }
        }))
        .is_empty());
    }

    #[test]
    fn parse_recovered_pending_notifications_returns_empty_for_non_array_section() {
        let object_payload = serde_json::json!({
            "llm_output": { "pending_notifications": { "tool_call_id": "call-1" } }
        });
        assert!(parse_recovered_pending_notifications(&object_payload).is_empty());

        let string_payload = serde_json::json!({
            "llm_output": { "pending_notifications": "not-an-array" }
        });
        assert!(parse_recovered_pending_notifications(&string_payload).is_empty());
    }

    #[test]
    fn parse_recovered_pending_notifications_skips_malformed_entries() {
        let payload = serde_json::json!({
            "llm_output": {
                "pending_notifications": [
                    valid_pending_notification_json("call-ok"),
                    { "tool_call_id": "call-missing-fields" },
                    "garbage",
                    42,
                ]
            }
        });

        let parsed = parse_recovered_pending_notifications(&payload);

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].tool_call_id, "call-ok");
    }

    // ---- execute_speaker_notification_tool_calls（runtime 需要 AppHandle，仅测纯路径）----

    #[test]
    fn execute_speaker_notification_tool_calls_ignores_unrelated_tools_and_empty_input() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        let world = test_world();

        // None 输入
        assert!(execute_speaker_notification_tool_calls(
            &conn, None, "sess-1", &world, 1, "Alice", "", None
        )
        .is_empty());
        // 空切片
        assert!(execute_speaker_notification_tool_calls(
            &conn,
            None,
            "sess-1",
            &world,
            1,
            "Alice",
            "",
            Some(&[])
        )
        .is_empty());
        // 非 schedule_notification 工具被过滤
        let unrelated = ChatToolCall {
            id: "call-x".to_string(),
            tool_name: "roll_dice".to_string(),
            arguments: serde_json::json!({}),
        };
        assert!(execute_speaker_notification_tool_calls(
            &conn,
            None,
            "sess-1",
            &world,
            1,
            "Alice",
            "",
            Some(&[unrelated])
        )
        .is_empty());
    }

    #[test]
    fn execute_speaker_notification_tool_calls_trims_tool_name_before_matching() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        let world = test_world();
        let padded = ChatToolCall {
            id: "call-padded".to_string(),
            tool_name: "  schedule_notification  ".to_string(),
            arguments: serde_json::json!({}),
        };

        let results = execute_speaker_notification_tool_calls(
            &conn,
            None,
            "sess-1",
            &world,
            1,
            "Alice",
            "",
            Some(std::slice::from_ref(&padded)),
        );

        // trim 后仍识别为通知工具，因此走到 runtime 缺失分支而不是被过滤
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].call.id, padded.id);
    }

    #[test]
    fn execute_speaker_notification_tool_calls_reports_missing_runtime() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        let world = test_world();
        let calls = vec![notification_call("call-1"), notification_call("call-2")];

        let results = execute_speaker_notification_tool_calls(
            &conn,
            None,
            "sess-1",
            &world,
            3,
            "Alice",
            "avatar.png",
            Some(&calls),
        );

        assert_eq!(results.len(), 2);
        for (call, item) in calls.iter().zip(results.iter()) {
            assert_eq!(item.call.id, call.id);
            assert_eq!(
                item.result,
                serde_json::json!({
                    "tool_name": "schedule_notification",
                    "tool_call_id": call.id,
                    "ok": false,
                    "error": "notification runtime is not available",
                })
            );
        }
    }

    #[test]
    fn execute_speaker_notification_tool_calls_prefers_runtime_error_over_argument_shape() {
        // match 分支顺序：runtime 缺失时不再校验参数形状，统一报 runtime 不可用。
        let conn = Connection::open_in_memory().expect("open sqlite");
        let world = test_world();
        let mut call = notification_call("call-1");
        call.arguments = serde_json::json!("not-an-object");

        let results = execute_speaker_notification_tool_calls(
            &conn,
            None,
            "sess-1",
            &world,
            1,
            "Alice",
            "",
            Some(std::slice::from_ref(&call)),
        );

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].result.get("error").and_then(|v| v.as_str()),
            Some("notification runtime is not available")
        );
    }

    #[test]
    fn speaker_notification_tool_error_builds_expected_payload() {
        // 覆盖 execute 中 (_, None) 分支的 payload 形状（该分支需 AppHandle，无法直接走到）。
        let call = notification_call("call-9");

        let value =
            speaker_notification_tool_error(&call, "tool arguments must be a JSON object");

        assert_eq!(
            value,
            serde_json::json!({
                "tool_name": "schedule_notification",
                "tool_call_id": "call-9",
                "ok": false,
                "error": "tool arguments must be a JSON object",
            })
        );
    }

    // ---- build_speaker_tool_followup_request ----

    fn base_chat_request() -> ChatRequest {
        ChatRequest {
            model: "gpt-test".to_string(),
            messages: vec![crate::services::llm::client::ChatMessage {
                role: "user".to_string(),
                content: serde_json::Value::String("帮我安排一个提醒".to_string()),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            }],
            generation: Default::default(),
            stream: Some(true),
            json_mode: None,
            response_schema: None,
            tools: Some(vec![crate::services::llm::client::ChatToolDefinition {
                name: "schedule_notification".to_string(),
                description: None,
                input_schema: serde_json::json!({ "type": "object" }),
            }]),
            tool_choice: Some(crate::services::llm::client::ChatToolChoice::Auto),
            native_tool_calling: Some(true),
        }
    }

    fn sample_tool_results() -> Vec<SpeakerNotificationToolResult> {
        vec![
            SpeakerNotificationToolResult {
                call: notification_call("call-1"),
                result: serde_json::json!({ "ok": true, "notification_id": "n-1" }),
            },
            SpeakerNotificationToolResult {
                call: notification_call("call-2"),
                result: serde_json::json!({ "ok": false, "error": "boom" }),
            },
        ]
    }

    #[test]
    fn build_speaker_tool_followup_request_appends_assistant_and_tool_messages() {
        let request = base_chat_request();
        let response = ChatResponse {
            content: "{\"speaker\":\"Alice\",\"content\":\"好\"}".to_string(),
            reasoning: Some("think".to_string()),
            tool_calls: None,
            usage: None,
            finish_reason: None,
        };
        let tool_results = sample_tool_results();

        let followup = build_speaker_tool_followup_request(&request, &response, &tool_results, false);

        // 原消息 + assistant + 每个 tool result 一条
        assert_eq!(followup.messages.len(), 1 + 1 + tool_results.len());
        assert_eq!(followup.messages[0].role, "user");

        let assistant = &followup.messages[1];
        assert_eq!(assistant.role, "assistant");
        assert_eq!(
            assistant.content,
            serde_json::Value::String(response.content.clone())
        );
        assert_eq!(assistant.reasoning_content.as_deref(), Some("think"));
        assert!(assistant.tool_call_id.is_none());
        let assistant_tool_calls = assistant.tool_calls.as_ref().expect("assistant tool calls");
        assert_eq!(assistant_tool_calls.len(), 2);
        assert_eq!(assistant_tool_calls[0].id, "call-1");
        assert_eq!(assistant_tool_calls[1].id, "call-2");

        for (index, item) in tool_results.iter().enumerate() {
            let tool_message = &followup.messages[2 + index];
            assert_eq!(tool_message.role, "tool");
            assert_eq!(
                tool_message.tool_call_id.as_deref(),
                Some(item.call.id.as_str())
            );
            assert!(tool_message.tool_calls.is_none());
            assert!(tool_message.reasoning_content.is_none());
            let expected_content =
                serde_json::to_string(&item.result).expect("serialize tool result");
            assert_eq!(
                tool_message.content,
                serde_json::Value::String(expected_content)
            );
        }
    }

    #[test]
    fn build_speaker_tool_followup_request_keeps_tool_config_and_applies_stream_flag() {
        let request = base_chat_request();
        let response = ChatResponse {
            content: String::new(),
            reasoning: None,
            tool_calls: None,
            usage: None,
            finish_reason: None,
        };
        let tool_results = sample_tool_results();

        let followup = build_speaker_tool_followup_request(&request, &response, &tool_results, false);

        assert_eq!(followup.stream, Some(false));
        // 工具配置保留：模型才能在后续轮次补调失败或遗漏的工具
        assert_eq!(followup.tools.as_ref().map(|tools| tools.len()), Some(1));
        assert!(matches!(
            followup.tool_choice,
            Some(crate::services::llm::client::ChatToolChoice::Auto)
        ));
        assert_eq!(followup.native_tool_calling, Some(true));
        // 其余请求配置保持原样
        assert_eq!(followup.model, request.model);
        assert_eq!(followup.generation, request.generation);
        assert!(followup.json_mode.is_none());
        assert!(followup.response_schema.is_none());

        // 流式标志按调用方透传
        let streamed = build_speaker_tool_followup_request(&request, &response, &tool_results, true);
        assert_eq!(streamed.stream, Some(true));
        assert_eq!(streamed.native_tool_calling, Some(true));
    }

    // ---- fallback_notification_tool_response ----

    #[test]
    fn fallback_notification_tool_response_returns_parseable_character_payload() {
        let raw = fallback_notification_tool_response("Alice");

        let value: serde_json::Value = serde_json::from_str(&raw).expect("fallback json");
        assert_eq!(value.get("speaker").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(
            value.get("intent").and_then(|v| v.as_str()),
            Some("notification_tool_result")
        );
        assert_eq!(
            value.get("emotion").and_then(|v| v.as_str()),
            Some("neutral")
        );
        assert_eq!(value.get("narration").and_then(|v| v.as_str()), Some(""));
        assert_eq!(
            value.get("content").and_then(|v| v.as_str()),
            Some("\u{5df2}\u{5904}\u{7406}\u{901a}\u{77e5}\u{8bf7}\u{6c42}\u{3002}")
        );
    }

    // ---- build_tool_activity_value / summarize_tool_call_arguments ----

    fn stock_quote_call() -> ChatToolCall {
        ChatToolCall {
            id: "call-quote".to_string(),
            tool_name: "stock_quote".to_string(),
            arguments: serde_json::json!({ "secid": "1.600519", "lmt": 60 }),
        }
    }

    fn stock_tool_definitions() -> Vec<crate::models::mcp_tool::McpToolDefinition> {
        vec![crate::models::mcp_tool::McpToolDefinition {
            id: "mcp-tool-stock-quote".to_string(),
            name: "A股实时行情".to_string(),
            description: String::new(),
            server_name: String::new(),
            tool_name: "stock_quote".to_string(),
            enabled: true,
            exposure_policy: serde_json::json!({}),
            risk_level: String::new(),
            trigger_keywords: Vec::new(),
            input_schema: serde_json::json!({}),
            server_id: String::new(),
            impl_kind: "builtin_http".to_string(),
            impl_config: serde_json::json!({}),
        }]
    }

    #[test]
    fn tool_activity_calling_uses_definition_display_name_and_args_preview() {
        let value =
            build_tool_activity_value(&[stock_quote_call()], &stock_tool_definitions(), "calling");

        assert_eq!(value.get("status").and_then(|v| v.as_str()), Some("calling"));
        let tools = value.get("tools").and_then(|v| v.as_array()).expect("tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(
            tools[0].get("name").and_then(|v| v.as_str()),
            Some("A股实时行情")
        );
        assert_eq!(
            tools[0].get("args_preview").and_then(|v| v.as_str()),
            Some("1.600519, 60")
        );
    }

    #[test]
    fn tool_activity_falls_back_to_call_name_when_definition_missing() {
        let value = build_tool_activity_value(&[stock_quote_call()], &[], "calling");

        let tools = value.get("tools").and_then(|v| v.as_array()).expect("tools");
        assert_eq!(
            tools[0].get("name").and_then(|v| v.as_str()),
            Some("stock_quote")
        );
    }

    #[test]
    fn tool_activity_done_dedupes_and_omits_args_preview() {
        let calls = vec![stock_quote_call(), stock_quote_call()];
        let value = build_tool_activity_value(&calls, &stock_tool_definitions(), "done");

        let tools = value.get("tools").and_then(|v| v.as_array()).expect("tools");
        assert_eq!(tools.len(), 1);
        assert!(tools[0].get("args_preview").is_none());
    }

    #[test]
    fn summarize_tool_call_arguments_skips_non_scalars_and_truncates() {
        let value = summarize_tool_call_arguments(&serde_json::json!({
            "secid": "1.600519",
            "nested": { "a": 1 },
            "flag": true,
            "extra": "x"
        }))
        .expect("preview");
        assert_eq!(value, "true, x");

        let long = summarize_tool_call_arguments(&serde_json::json!({
            "text": "一".repeat(60)
        }))
        .expect("long preview");
        assert_eq!(long.chars().count(), 41);
        assert!(long.ends_with('…'));

        assert!(
            summarize_tool_call_arguments(&serde_json::json!({ "nested": { "a": 1 } })).is_none()
        );
        assert!(summarize_tool_call_arguments(&serde_json::json!([])).is_none());
    }
