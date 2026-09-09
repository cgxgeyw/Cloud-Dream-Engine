    use super::*;
    use crate::models::session::{AssetSelection, SceneRuntime, SessionState};

    fn sample_world(director_config: serde_json::Value) -> WorldDefinition {
        WorldDefinition {
            id: "world-1".to_string(),
            name: "World".to_string(),
            genre: "".to_string(),
            background_prompt: "".to_string(),
            opening_scene: "Dock".to_string(),
            summary: "".to_string(),
            time_system: "".to_string(),
            map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
            triggers: vec![],
            time_config: serde_json::json!({}),
            director_config,
            ui_theme_config: serde_json::json!({}),
            director_system_prompt_base: "".to_string(),
            director_runtime_system_prompt: "".to_string(),
            opening_messages: vec![],
            opening_character_ids: vec![],
            player_character_id: Some("char-player".to_string()),
        }
    }

    fn sample_session() -> SessionSnapshot {
        SessionSnapshot {
            id: "sess-1".to_string(),
            world_name: "World".to_string(),
            location: "Dock".to_string(),
            time_label: "Night".to_string(),
            current_speaker: "Alice".to_string(),
            current_line: "".to_string(),
            player_character_id: "char-player".to_string(),
            player_character_name: "Player".to_string(),
            visible_characters: vec!["Alice".to_string(), "Bob".to_string()],
            messages: vec![],
            player_stats: vec![],
            map_graph_nodes: vec![],
            map_graph_edges: vec![],
            inventory_items: vec![],
            system_log: vec![],
            scene: SceneRuntime {
                scene_id: "dock-scene".to_string(),
                name: "Dock".to_string(),
                background_hint: "rain".to_string(),
                temporary_tags: vec![],
                present_characters: vec![
                    "Player".to_string(),
                    "Alice".to_string(),
                    "Bob".to_string(),
                ],
            },
            assets: AssetSelection::default(),
            state: SessionState::default(),
            generation_params: Default::default(),
        }
    }

    #[test]
    fn resolve_tool_loop_limit_respects_bounds() {
        let service = WorldDirectorService::new();
        let low_world = sample_world(serde_json::json!({ "director_tool_loop_limit": 0 }));
        let high_world = sample_world(serde_json::json!({ "director_tool_loop_limit": 99 }));
        let mid_world = sample_world(serde_json::json!({ "director_tool_loop_limit": 6 }));

        assert_eq!(service.resolve_tool_loop_limit(&low_world), 1);
        assert_eq!(service.resolve_tool_loop_limit(&high_world), 12);
        assert_eq!(service.resolve_tool_loop_limit(&mid_world), 6);
    }

    #[test]
    fn resolve_tool_call_limit_respects_bounds() {
        let service = WorldDirectorService::new();
        let low_world = sample_world(serde_json::json!({ "director_tool_call_limit": 0 }));
        let high_world = sample_world(serde_json::json!({ "director_tool_call_limit": 99 }));
        let mid_world = sample_world(serde_json::json!({ "director_tool_call_limit": 3 }));

        assert_eq!(service.resolve_tool_call_limit(&low_world), 1);
        assert_eq!(service.resolve_tool_call_limit(&high_world), 8);
        assert_eq!(service.resolve_tool_call_limit(&mid_world), 3);
    }

    fn runtime_attribute_group(
        owner_id: &str,
        key: &str,
    ) -> crate::models::session::RuntimeAttributeGroup {
        crate::models::session::RuntimeAttributeGroup {
            owner_type: "session_character".to_string(),
            owner_id: owner_id.to_string(),
            owner_label: owner_id.to_string(),
            items: vec![crate::models::session::RuntimeAttributeItem {
                schema_id: format!("schema-{key}"),
                key: key.to_string(),
                label: key.to_string(),
                value_type: "number".to_string(),
                value: serde_json::json!(1),
                source: "test".to_string(),
                display_policy: serde_json::json!({}),
                influence_policy: serde_json::json!({}),
            }],
        }
    }

    #[test]
    fn append_runtime_attributes_matches_player_group_by_exact_owner_id() {
        // "li" 与 "han-li" 互为后缀:玩家组必须按完整 owner_id 精确匹配。
        let mut session = sample_session();
        session.player_character_id = "li".to_string();
        let mut payload = serde_json::json!({ "current_state": {} });
        let runtime = crate::models::session::SessionRuntimeAttributesResponse {
            session_attributes: vec![],
            character_attributes: vec![
                runtime_attribute_group("sess-1:han-li", "han-li-attr"),
                runtime_attribute_group("sess-1:li", "player-attr"),
            ],
        };

        append_runtime_attributes(&mut payload, &session, &runtime);

        let player = &payload["current_state"]["runtime_attributes"]["player"];
        let attrs = player["attributes"].as_array().unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0]["key"], serde_json::json!("player-attr"));
    }

    #[test]
    fn append_runtime_attributes_ignores_foreign_owner_with_same_character_suffix() {
        // 其它来源的 owner_id 即使以 ":li" 结尾(旧后缀匹配会误中),也不能当作玩家组。
        let mut session = sample_session();
        session.player_character_id = "li".to_string();
        let mut payload = serde_json::json!({ "current_state": {} });
        let runtime = crate::models::session::SessionRuntimeAttributesResponse {
            session_attributes: vec![],
            character_attributes: vec![runtime_attribute_group("other-sess:li", "foreign-attr")],
        };

        append_runtime_attributes(&mut payload, &session, &runtime);

        let player = &payload["current_state"]["runtime_attributes"]["player"];
        let attrs = player["attributes"].as_array().unwrap();
        assert!(attrs.is_empty());
    }

    #[test]
    fn resolve_director_prompt_is_empty_when_world_prompt_empty() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({}));

        let prompt = service.resolve_director_system_prompt(&world);

        assert!(prompt.trim().is_empty());
    }

    #[test]
    fn parse_runtime_payload_drops_director_authored_speech_line() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({}));
        let session = sample_session();
        let parsed = serde_json::json!({
            "current_line": "Li Bai said: poem",
            "planned_speakers": ["Alice"]
        });

        let payload = service.parse_runtime_payload(&parsed, &session, &world, "continue");

        assert!(payload.current_line.is_none());
    }

    #[test]
    fn build_tool_followup_request_rejects_response_body_tool_calls() {
        let service = WorldDirectorService::new();
        let request = ChatRequest {
            model: "model".to_string(),
            messages: vec![crate::services::llm::client::ChatMessage {
                role: "system".to_string(),
                content: serde_json::json!("prompt"),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            }],
            generation: GenerationParams {
                temperature: Some(0.7),
                max_tokens: Some(500),
                ..Default::default()
            },
            stream: Some(false),
            json_mode: Some(true),
            response_schema: None,
            tools: None,
            tool_choice: None,
            native_tool_calling: None,
        };
        let parsed = serde_json::json!({
            "tool_calls": [
                { "id": "call-1", "tool_name": "list_scenes", "arguments": {} }
            ]
        });
        let tool_enriched = serde_json::json!({
            "tool_results": [
                { "id": "call-1", "tool_name": "list_scenes", "ok": true }
            ]
        });

        let error = service
            .build_tool_followup_request(&request, &parsed, &tool_enriched, false, None)
            .expect_err("response body tool calls should be rejected");

        assert!(error.contains("native tool_calls"));
    }

    #[test]
    fn build_tool_followup_request_uses_native_tool_messages_when_requested() {
        let service = WorldDirectorService::new();
        let request = ChatRequest {
            model: "model".to_string(),
            messages: vec![crate::services::llm::client::ChatMessage {
                role: "system".to_string(),
                content: serde_json::json!("prompt"),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            }],
            generation: GenerationParams {
                temperature: Some(0.7),
                max_tokens: Some(500),
                ..Default::default()
            },
            stream: Some(false),
            json_mode: Some(true),
            response_schema: None,
            tools: Some(vec![ChatToolDefinition {
                name: "list_scenes".to_string(),
                description: Some("List scenes".to_string()),
                input_schema: serde_json::json!({ "type": "object" }),
            }]),
            tool_choice: Some(ChatToolChoice::Auto),
            native_tool_calling: Some(true),
        };
        let parsed = serde_json::json!({
            "tool_calls": [
                { "id": "call-1", "tool_name": "list_scenes", "arguments": {} }
            ]
        });
        let tool_enriched = serde_json::json!({
            "tool_results": [
                { "id": "call-1", "tool_name": "list_scenes", "ok": true, "result": { "scenes": [] } }
            ]
        });

        let followup = service
            .build_tool_followup_request(&request, &parsed, &tool_enriched, true, None)
            .expect("followup request");

        assert_eq!(followup.messages.len(), 3);
        assert_eq!(followup.messages[1].role, "assistant");
        assert_eq!(followup.messages[2].role, "tool");
        assert_eq!(followup.messages[2].tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(
            followup.messages[1]
                .tool_calls
                .as_ref()
                .map(|items| items.len()),
            Some(1)
        );
    }

    #[test]
    fn native_tool_calls_override_response_body_tool_calls() {
        let service = WorldDirectorService::new();
        let parsed_body = serde_json::json!({
            "world_phase": "runtime",
            "tool_calls": [
                { "id": "body-call", "tool_name": "change_scene", "arguments": { "scene_name": "Body" } }
            ]
        });
        let native_calls = vec![ChatToolCall {
            id: "native-call".to_string(),
            tool_name: "list_scenes".to_string(),
            arguments: serde_json::json!({}),
        }];

        let stripped = service.remove_response_body_tool_calls(&parsed_body);
        let merged = service.merge_native_tool_calls(&stripped, Some(&native_calls));
        let tool_calls = merged
            .get("tool_calls")
            .and_then(|value| value.as_array())
            .expect("native tool calls should be merged");

        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "native-call");
        assert_eq!(tool_calls[0]["tool_name"], "list_scenes");
    }

    #[test]
    fn resolve_runtime_stage_label_uses_world_config() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({
            "director_stage_labels": {
                "default_turn": "棣栬疆",
                "tool_loop_turn": "tool turn"
            }
        }));

        let default_stage = service.resolve_runtime_stage_label(&world, &[]);
        assert_eq!(default_stage, "棣栬疆");

        let tool_loop_stage = service.resolve_runtime_stage_label(
            &world,
            &[crate::services::llm::client::ChatMessage {
                role: "user".to_string(),
                content: serde_json::json!("{}"),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: Some(serde_json::json!({ "tool_phase": true })),
            }],
        );
        assert_eq!(tool_loop_stage, "tool turn");
    }

    #[test]
    fn should_continue_tool_loop_requires_model_to_return_tool_calls() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({ "director_tool_loop_limit": 4 }));

        assert!(service.should_continue_tool_loop(
            &world,
            &serde_json::json!({
                "tool_calls": [{ "tool_name": "list_scenes", "arguments": {} }]
            }),
            1,
        ));
        assert!(!service.should_continue_tool_loop(
            &world,
            &serde_json::json!({
                "tool_results": [{ "tool_name": "list_scenes", "ok": true }]
            }),
            1,
        ));
        assert!(!service.should_continue_tool_loop(
            &world,
            &serde_json::json!({
                "tool_calls": [{ "tool_name": "list_scenes", "arguments": {} }]
            }),
            4,
        ));
    }

    #[test]
    fn parse_runtime_payload_respects_scene_transition_flag() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({ "allow_scene_transition": false }));
        let session = sample_session();
        let parsed = serde_json::json!({
            "next_location": "Tower",
            "next_scene_name": "Tower",
            "next_time_label": "Dawn",
            "scene_visible_characters": ["Alice"],
            "planned_speakers": ["Alice"],
        });

        let payload = service.parse_runtime_payload(&parsed, &session, &world, "move");
        assert_eq!(payload.next_location, "Dock");
        assert_eq!(payload.next_scene_name, "Dock");
        assert_eq!(payload.next_time_label, "Dawn");
    }

    #[test]
    fn director_interaction_waits_for_player_before_speakers_run() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({
            "director_interaction_kinds": ["choice"]
        }));
        let session = sample_session();
        let parsed = serde_json::json!({
            "planned_speakers": ["Alice"],
            "interaction": {
                "kind": "choice",
                "prompt": "Choose a route",
                "config": {
                    "options": [
                        { "id": "river", "label": "River path" },
                        { "id": "ridge", "label": "Ridge path" }
                    ]
                }
            }
        });

        let payload = service.parse_runtime_payload(&parsed, &session, &world, "continue");

        assert!(payload.planned_speakers.is_empty());
        let interaction = payload.interaction.expect("director interaction");
        assert_eq!(interaction.kind, "choice");
        assert_eq!(interaction.status, INTERACTION_STATUS_PENDING);
    }

    #[test]
    fn parse_runtime_payload_sanitizes_switch_character_proposal() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({ "allow_scene_transition": true }));
        let session = sample_session();
        let parsed = serde_json::json!({
            "switch_character_proposal": {
                "target_character_name": "Alice",
                "reason": "Need stealth expert",
                "scene_character_roster": ["Alice", "Player", "Bob"],
                "scene_name": "Warehouse"
            }
        });

        let payload = service.parse_runtime_payload(&parsed, &session, &world, "switch");
        let proposal = payload
            .switch_character_proposal
            .expect("switch proposal should exist");
        let visible = proposal
            .get("scene_character_roster")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let visible_names = visible
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>();

        assert!(!visible_names.contains(&"Player"));
        assert!(!visible_names.contains(&"Alice"));
        assert!(visible_names.contains(&"Bob"));
    }

    #[test]
    fn parse_runtime_payload_drops_switch_proposal_when_disabled() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({
            "allow_player_character_switch": false
        }));
        let session = sample_session();
        let parsed = serde_json::json!({
            "switch_character_proposal": {
                "target_character_name": "Alice",
                "reason": "Need stealth expert"
            }
        });

        let payload = service.parse_runtime_payload(&parsed, &session, &world, "switch");

        assert!(payload.switch_character_proposal.is_none());
    }

    #[test]
    fn build_runtime_turn_payload_uses_unambiguous_character_keys() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({}));
        let session = sample_session();
        let characters = vec![
            CharacterDefinition {
                id: "char-alice".to_string(),
                name: "Alice".to_string(),
                world_id: "world-1".to_string(),
                role: "Scout".to_string(),
                background_prompt: String::new(),
                model: "test-model".to_string(),
                memory_strategy: "recent".to_string(),
                recent_dialogue_rounds: 6,
                attributes: vec![],
                portrait_assets: vec![],
                avatar_asset: String::new(),
                system_prompt_template: String::new(),
                response_contract_prompt: String::new(),
                narration_prompt: String::new(),
                runtime_system_prompt: String::new(),
            },
            CharacterDefinition {
                id: "char-bob".to_string(),
                name: "Bob".to_string(),
                world_id: "world-1".to_string(),
                role: "Guard".to_string(),
                background_prompt: String::new(),
                model: "test-model".to_string(),
                memory_strategy: "recent".to_string(),
                recent_dialogue_rounds: 6,
                attributes: vec![],
                portrait_assets: vec![],
                avatar_asset: String::new(),
                system_prompt_template: String::new(),
                response_contract_prompt: String::new(),
                narration_prompt: String::new(),
                runtime_system_prompt: String::new(),
            },
        ];

        let payload =
            service.build_runtime_turn_payload(&world, &session, &characters, "hello", Vec::new());

        assert_eq!(
            payload
                .get("basic_setting")
                .and_then(|value| value.get("world_character_roster"))
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(2)
        );
        assert!(payload.get("available_characters").is_none());
        assert!(payload
            .get("current_state")
            .and_then(|value| value.get("current_scene_character_roster"))
            .is_some());
        assert!(payload
            .get("current_state")
            .and_then(|value| value.get("visible_characters"))
            .is_none());
        assert!(payload
            .get("current_state")
            .and_then(|value| value.get("scene_present_characters"))
            .is_none());
    }

    #[test]
    fn build_runtime_turn_payload_uses_minimal_director_contract() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({}));
        let session = sample_session();
        let payload =
            service.build_runtime_turn_payload(&world, &session, &[], "hello", Vec::new());

        let current_state = payload
            .get("current_state")
            .and_then(|value| value.as_object())
            .expect("current_state");
        assert!(payload
            .get("basic_setting")
            .and_then(|value| value.get("opening_scene"))
            .is_none());
        assert!(!current_state.contains_key("scene_name"));
        assert!(!current_state.contains_key("state_tags"));
        assert!(!current_state.contains_key("system_log"));

        let response_contract = payload
            .get("response_contract")
            .and_then(|value| value.as_object())
            .expect("response contract");
        assert_eq!(
            response_contract
                .get("return_policy")
                .and_then(|value| value.as_str()),
            Some("return_changed_fields_only")
        );
        assert_eq!(
            response_contract
                .get("forbidden_fields")
                .and_then(|value| value.as_array())
                .map(|items| items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()),
            Some(vec!["state_tags", "system_messages", "system_log"])
        );
        assert!(!response_contract.contains_key("tool_call_fallback_field"));
        let optional_fields = response_contract
            .get("optional_fields_when_changed")
            .and_then(|value| value.as_array())
            .map(|items| items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>())
            .unwrap_or_default();
        assert!(optional_fields.contains(&"session_attribute_updates"));
        assert!(optional_fields.contains(&"character_attribute_updates"));
        assert!(response_contract.contains_key("runtime_update_format"));
        assert!(response_contract
            .get("notes")
            .and_then(|value| value.as_array())
            .map(|items| items
                .iter()
                .filter_map(|item| item.as_str())
                .any(|item| item.contains(
                    "player character name in scene_visible_characters or planned_speakers"
                )))
            .unwrap_or(false));
    }

    #[test]
    fn director_response_schema_omits_removed_runtime_log_fields() {
        let service = WorldDirectorService::new();
        let world = sample_world(serde_json::json!({}));
        let schema = service.build_director_response_schema(&world);
        let properties = schema
            .get("properties")
            .and_then(|value| value.as_object())
            .expect("schema properties");

        assert!(properties.contains_key("planned_speakers"));
        assert!(properties.contains_key("next_scene_background_hint"));
        assert!(properties.contains_key("next_scene_tags"));
        assert!(properties.contains_key("generated_characters"));
        assert!(properties.contains_key("session_attribute_updates"));
        assert!(properties.contains_key("character_attribute_updates"));
        assert!(!properties.contains_key("tool_calls"));
        assert!(!properties.contains_key("state_tags"));
        assert!(!properties.contains_key("system_messages"));
        assert!(!properties.contains_key("system_log"));
        let generated = properties
            .get("generated_characters")
            .and_then(|value| value.get("items"))
            .and_then(|value| value.as_object())
            .expect("generated characters schema");
        let required = generated
            .get("required")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert!(required.contains(&"name"));
        assert!(required.contains(&"role"));
        assert!(required.contains(&"background_prompt"));
    }

    #[test]
    fn repair_common_json_issues_escapes_bare_newlines_inside_strings() {
        let broken = "{\"current_line\": \"第一行\n第二行\"}";
        let repaired = repair_common_json_issues(broken);
        let parsed =
            serde_json::from_str::<serde_json::Value>(&repaired).expect("修复后应能解析");
        assert_eq!(
            parsed.get("current_line").and_then(|value| value.as_str()),
            Some("第一行\n第二行")
        );
    }

    #[test]
    fn repair_common_json_issues_removes_bare_carriage_returns_in_strings() {
        let broken = "{\"current_line\": \"甲\r\n乙\"}";
        let repaired = repair_common_json_issues(broken);
        let parsed =
            serde_json::from_str::<serde_json::Value>(&repaired).expect("修复后应能解析");
        assert_eq!(
            parsed.get("current_line").and_then(|value| value.as_str()),
            Some("甲\n乙")
        );
    }

    #[test]
    fn repair_common_json_issues_completes_truncated_json() {
        let truncated = "{\"world_phase\": \"opening\", \"planned_speakers\": [\"Alice\"";
        let repaired = repair_common_json_issues(truncated);
        let parsed =
            serde_json::from_str::<serde_json::Value>(&repaired).expect("补全后应能解析");
        assert_eq!(
            parsed.get("world_phase").and_then(|value| value.as_str()),
            Some("opening")
        );
        assert_eq!(
            parsed
                .get("planned_speakers")
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(1)
        );
    }

    #[test]
    fn repair_common_json_issues_completes_unterminated_string() {
        let truncated = "{\"current_line\": \"雾从江面升起";
        let repaired = repair_common_json_issues(truncated);
        let parsed =
            serde_json::from_str::<serde_json::Value>(&repaired).expect("补全后应能解析");
        assert_eq!(
            parsed.get("current_line").and_then(|value| value.as_str()),
            Some("雾从江面升起")
        );
    }

    #[test]
    fn repair_common_json_issues_keeps_valid_json_untouched() {
        let valid = "{\"a\": 1, \"b\": [1, 2], \"c\": \"x\\n\"}";
        assert_eq!(repair_common_json_issues(valid), valid);
    }

    #[test]
    fn director_output_needs_json_repair_only_for_non_object_or_empty() {
        assert!(director_output_needs_json_repair(&serde_json::Value::Null));
        assert!(director_output_needs_json_repair(&serde_json::json!(
            "just text"
        )));
        assert!(director_output_needs_json_repair(&serde_json::json!({})));
        assert!(!director_output_needs_json_repair(&serde_json::json!({
            "planned_speakers": ["Alice"]
        })));
    }

    #[test]
    fn build_director_json_repair_request_appends_output_and_feedback() {
        let request = ChatRequest {
            model: "model".to_string(),
            messages: vec![crate::services::llm::client::ChatMessage {
                role: "system".to_string(),
                content: serde_json::json!("prompt"),
                reasoning_content: None,
                speaker: None,
                tool_call_id: None,
                tool_calls: None,
                metadata: None,
            }],
            generation: GenerationParams::default(),
            stream: Some(false),
            json_mode: Some(true),
            response_schema: None,
            tools: None,
            tool_choice: None,
            native_tool_calling: None,
        };

        let repaired = build_director_json_repair_request(&request, "{broken json");

        assert_eq!(repaired.messages.len(), 3);
        assert_eq!(repaired.messages[1].role, "assistant");
        assert_eq!(
            repaired.messages[1].content.as_str(),
            Some("{broken json")
        );
        assert_eq!(repaired.messages[2].role, "user");
        let feedback = repaired.messages[2]
            .content
            .as_str()
            .expect("feedback text");
        assert!(feedback.contains("无法解析为 JSON 对象"));
        assert!(feedback.contains("只输出 JSON 本身"));
        // 原请求的 model/json_mode 等参数保持不变
        assert_eq!(repaired.model, "model");
        assert_eq!(repaired.json_mode, Some(true));
    }