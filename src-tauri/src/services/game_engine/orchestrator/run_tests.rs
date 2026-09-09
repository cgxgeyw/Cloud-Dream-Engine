use super::*;
    use crate::db::schema;
    use crate::models::memory::MemoryEntry;
    use crate::models::session::{AssetSelection, SceneRuntime, SessionState};
    use crate::services::assets::resolver::AssetResolver;
    use chrono::Utc;
    use rusqlite::Connection;

    fn sample_world() -> WorldDefinition {
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
            director_config: serde_json::json!({ "allowed_mcp_tool_ids": [] }),
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
            current_line: "Ready.".to_string(),
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

    fn sample_model() -> ModelConfig {
        ModelConfig {
            id: "m1".to_string(),
            name: "test".to_string(),
            model_type: "text".to_string(),
            provider: "openai".to_string(),
            model_id: "gpt-test".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "".to_string(),
            max_tokens: 1200,
            streaming_enabled: true,
            is_default: true,
            input_modalities: Vec::new(),
        }
    }

    #[test]
    fn build_runtime_updated_session_snapshot_keeps_player_in_present_characters() {
        let session = sample_session();
        let world = sample_world();
        let asset_resolver = AssetResolver::new();
        let runtime_application = DirectorRuntimeApplication {
            scene_runtime: Some(SceneRuntime {
                scene_id: "warehouse".to_string(),
                name: "Warehouse".to_string(),
                background_hint: "fog".to_string(),
                temporary_tags: vec!["phase:opening".to_string()],
                present_characters: vec!["Bob".to_string(), "Player".to_string()],
            }),
            ..DirectorRuntimeApplication::default()
        };
        let updated = build_runtime_updated_session_snapshot(&RuntimeMutationInput {
            asset_resolver: &asset_resolver,
            data_dir: std::path::Path::new("."),
            session: &session,
            messages: &[],
            world: &world,
            characters: &[],
            turn_index: 1,
            next_location: "Warehouse",
            next_time_label: "Dawn",
            next_scene_name: "Warehouse District",
            current_line: Some("Fog rolls between the crates."),
            next_scene_background_hint: "fog".to_string(),
            planned_speakers: &["Bob".to_string()],
            scene_visible_characters_explicit: true,
            scene_visible_characters: &Some(vec!["Bob".to_string()]),
            visible_chars: &["Bob".to_string()],
            runtime_application: &runtime_application,
            image_model: None,
            parsed_runtime: &serde_json::json!({}),
        });

        assert_eq!(updated.scene.scene_id, "warehouse");
        assert!(updated
            .scene
            .present_characters
            .contains(&"Player".to_string()));
        assert!(updated
            .scene
            .present_characters
            .contains(&"Bob".to_string()));
        assert_eq!(updated.current_line, "Fog rolls between the crates.");
    }

    #[test]
    fn recovered_director_payload_prefers_full_runtime_payload() {
        let recovered = recovered_director_payload_to_result(&serde_json::json!({
            "director_runtime": {
                "world_phase": "crisis",
                "next_location": "Tower",
                "next_scene_name": "Tower Top",
                "next_scene_background_hint": "storm",
                "next_scene_tags": ["storm", "phase:crisis"],
                "scene_visible_characters": ["Alice"],
                "planned_speakers": ["Alice"]
            }
        }));

        assert_eq!(
            recovered
                .get("next_scene_background_hint")
                .and_then(|v| v.as_str()),
            Some("storm")
        );
        assert_eq!(
            recovered
                .get("next_scene_tags")
                .and_then(|v| v.as_array())
                .map(|items| items.len()),
            Some(2)
        );
    }

    #[test]
    fn build_character_turn_payload_expands_hit_turn_memory_windows() {
        let mut session = sample_session();
        session.messages = vec![
            ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "player".to_string(),
                content: MessageContent::Text("Check the code first.".to_string()),
                speaker: Some("Player".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 3 })),
            },
            ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "agent".to_string(),
                content: MessageContent::Text("Alice pointed toward the warehouse.".to_string()),
                speaker: Some("Alice".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 3 })),
            },
            ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "player".to_string(),
                content: MessageContent::Text("Was the door lock touched?".to_string()),
                speaker: Some("Player".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 4 })),
            },
            ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "agent".to_string(),
                content: MessageContent::Text("Bob said the lock has new scratches.".to_string()),
                speaker: Some("Bob".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 4 })),
            },
            ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "player".to_string(),
                content: MessageContent::Text("Continue tracking.".to_string()),
                speaker: Some("Player".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 5 })),
            },
            ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "agent".to_string(),
                content: MessageContent::Text("Alice followed into the warehouse.".to_string()),
                speaker: Some("Alice".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 5 })),
            },
        ];

        let world = WorldDefinition {
            director_config: serde_json::json!({
                "allowed_mcp_tool_ids": [],
                "character_memory_hit_turns": 1,
                "character_memory_event_window_rounds": 1,
                "character_memory_dialogue_window_rounds": 1
            }),
            ..sample_world()
        };
        let recalled_memories = vec![MemoryEntry {
            id: "mem-hit".to_string(),
            world_id: world.id.clone(),
            session_id: session.id.clone(),
            character_id: "char-a".to_string(),
            layer: "working".to_string(),
            content: "Bob mentioned new scratches on the lock.".to_string(),
            source: "speaker_response".to_string(),
            importance: 0.8,
            created_at: Utc::now().to_rfc3339(),
            turn_index: 4,
            conversation_id: Some(session.id.clone()),
            event_id: None,
            item_id: None,
            scene_id: Some(session.scene.scene_id.clone()),
            memory_type: "dialogue".to_string(),
            speaker: Some("Bob".to_string()),
            role: Some("agent".to_string()),
            location: Some(session.location.clone()),
            participants: vec!["Player".to_string(), "Alice".to_string(), "Bob".to_string()],
            keywords: vec![],
        }];
        let memory_pool = vec![
            MemoryEntry {
                id: "event-3".to_string(),
                world_id: world.id.clone(),
                session_id: session.id.clone(),
                character_id: "char-a".to_string(),
                layer: "short_term".to_string(),
                content: "The group noticed unusual movement near the warehouse.".to_string(),
                source: "director".to_string(),
                importance: 0.5,
                created_at: Utc::now().to_rfc3339(),
                turn_index: 3,
                conversation_id: Some(session.id.clone()),
                event_id: Some("event-3".to_string()),
                item_id: None,
                scene_id: Some(session.scene.scene_id.clone()),
                memory_type: "event".to_string(),
                speaker: None,
                role: Some("system".to_string()),
                location: Some(session.location.clone()),
                participants: vec!["Player".to_string(), "Alice".to_string(), "Bob".to_string()],
                keywords: vec![],
            },
            recalled_memories[0].clone(),
            MemoryEntry {
                id: "event-5".to_string(),
                world_id: world.id.clone(),
                session_id: session.id.clone(),
                character_id: "char-a".to_string(),
                layer: "short_term".to_string(),
                content: "The group decided to search the warehouse.".to_string(),
                source: "director".to_string(),
                importance: 0.5,
                created_at: Utc::now().to_rfc3339(),
                turn_index: 5,
                conversation_id: Some(session.id.clone()),
                event_id: Some("event-5".to_string()),
                item_id: None,
                scene_id: Some(session.scene.scene_id.clone()),
                memory_type: "event".to_string(),
                speaker: None,
                role: Some("system".to_string()),
                location: Some(session.location.clone()),
                participants: vec!["Player".to_string(), "Alice".to_string(), "Bob".to_string()],
                keywords: vec![],
            },
        ];

        let payload = build_character_turn_payload(
            &world,
            "Alice",
            None,
            &session,
            "Player",
            &session.location,
            &session.scene.name,
            "Continue tracking",
            &session.messages,
            &recalled_memories,
            &memory_pool,
            &[],
            &[],
            &[],
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&payload).expect("payload should be valid json");
        let memory_context = parsed
            .get("dialogue_history")
            .and_then(|value| value.get("memory_context"))
            .expect("memory_context should exist");

        assert_eq!(
            memory_context
                .get("hit_turns")
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(1)
        );
        assert_eq!(
            memory_context
                .get("event_timeline")
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(2)
        );
        // dialogue_focus 的窗口(命中轮 4 ±1 = 轮 3-5)与本测试的 recent_dialogue 完全重合，
        // 去重后整体省略，避免同一批对话发两遍。
        assert!(memory_context.get("dialogue_focus").is_none());
    }

    #[test]
    fn writeback_turn_snapshot_persists_core_journal_steps() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create schema");

        let session = sample_session();
        let mut updated = session.clone();
        updated.location = "Warehouse".to_string();
        updated.scene.scene_id = "warehouse-scene".to_string();
        updated.scene.name = "Warehouse".to_string();
        updated.scene.background_hint = "dark".to_string();
        updated.current_speaker = "Bob".to_string();
        updated.current_line = "I see movement near the crates.".to_string();
        updated.state.phase = "escalation".to_string();
        updated.state.tags = vec!["under_watch".to_string()];

        let runtime_application = DirectorRuntimeApplication {
            state_phase: "escalation".to_string(),
            state_tags: vec!["under_watch".to_string()],
            ..DirectorRuntimeApplication::default()
        };
        let orchestrator = SessionOrchestrator;
        orchestrator
            .writeback_turn_snapshot(TurnWritebackInput {
                conn: &conn,
                director_service: &WorldDirectorService::new(),
                recovery_journal: &[],
                session_id: &session.id,
                turn_index: 1,
                runtime_application: &runtime_application,
                updated: &updated,
                session: &session,
                world: &sample_world(),
                characters: &[],
                director_runtime: &serde_json::json!({
                    "world_phase": "escalation",
                    "next_location": "Warehouse",
                    "next_scene_name": "Warehouse",
                    "next_scene_background_hint": "dark",
                    "next_scene_tags": ["under_watch", "foggy"],
                    "next_time_label": "Night",
                    "scene_visible_characters": ["Bob"],
                    "planned_speakers": ["Bob"],
                }),
                planned_speakers: &["Bob".to_string()],
                scene_visible_characters: &Some(vec!["Bob".to_string()]),
                director_loop_traces: &[],
                director_provider: "openai",
                director_model: &sample_model(),
                player_input: "Move to warehouse",
                director_tool_loop_limit: 4,
            })
            .expect("writeback");

        let mut stmt = conn
            .prepare("SELECT step, payload_json FROM turn_journal WHERE session_id = ?1 AND turn_index = 1 ORDER BY created_at, id")
            .expect("prepare query");
        let rows = stmt
            .query_map(rusqlite::params![session.id.clone()], |row| {
                let step: String = row.get(0)?;
                let payload: String = row.get(1)?;
                Ok((step, payload))
            })
            .expect("query map")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect rows");

        let steps = rows
            .iter()
            .map(|(step, _)| step.as_str())
            .collect::<Vec<_>>();
        assert!(steps.contains(&"runtime_effects_applied"));
        assert!(steps.contains(&"scene_applied"));
        assert!(steps.contains(&"finished"));

        let scene_payload = rows
            .iter()
            .find(|(step, _)| step == "scene_applied")
            .map(|(_, payload)| {
                serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default()
            })
            .expect("scene_applied payload");
        assert_eq!(
            scene_payload.get("scene_name").and_then(|v| v.as_str()),
            Some("Warehouse")
        );
        assert_eq!(
            scene_payload.get("location").and_then(|v| v.as_str()),
            Some("Warehouse")
        );
        assert_eq!(
            scene_payload.get("state_phase").and_then(|v| v.as_str()),
            Some("escalation")
        );
        assert_eq!(
            scene_payload.get("time_label").and_then(|v| v.as_str()),
            Some("Night")
        );
        assert_eq!(
            scene_payload
                .get("planned_speakers")
                .and_then(|v| v.as_array())
                .map(|items| items.len()),
            Some(1)
        );
        assert_eq!(
            scene_payload
                .get("current_speaker")
                .and_then(|v| v.as_str()),
            Some("Bob")
        );

        let director_payload = rows
            .iter()
            .find(|(step, _)| step == "director_completed")
            .map(|(_, payload)| {
                serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default()
            })
            .expect("director_completed payload");
        assert_eq!(
            director_payload
                .get("director_runtime")
                .and_then(|value| value.get("next_scene_tags"))
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(2)
        );
    }

    // ---- 黄金测试：完整回合链路（生成 → 校验 → 落库）与校验拒绝 ----
    // 用本地 mock HTTP server 替代真实 LLM，ModelConfig.base_url 指向 127.0.0.1，
    // 走与生产完全相同的 openai provider 请求/解析/校验/写回代码路径。

    fn sample_character(id: &str, name: &str) -> CharacterDefinition {
        CharacterDefinition {
            id: id.to_string(),
            name: name.to_string(),
            world_id: "world-1".to_string(),
            role: "npc".to_string(),
            background_prompt: String::new(),
            model: String::new(),
            memory_strategy: String::new(),
            recent_dialogue_rounds: 5,
            attributes: vec![],
            portrait_assets: vec![],
            avatar_asset: String::new(),
            system_prompt_template: String::new(),
            response_contract_prompt: String::new(),
            narration_prompt: String::new(),
            runtime_system_prompt: String::new(),
        }
    }

    /// 最小 HTTP/1.1 mock：对任意请求返回固定的 OpenAI chat.completion 响应，
    /// content 为调用方给定的字符串。容量 4 次请求，足够工具循环/重试使用。
    fn spawn_mock_llm_server(response_content: String) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock llm");
        let addr = listener.local_addr().expect("local addr");
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for _ in 0..4 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buf = Vec::new();
                let mut temp = [0u8; 8192];
                loop {
                    let read = stream.read(&mut temp).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    buf.extend_from_slice(&temp[..read]);
                    if let Some(head_end) = buf
                        .windows(4)
                        .position(|window| window == b"\r\n\r\n")
                    {
                        let headers = String::from_utf8_lossy(&buf[..head_end]);
                        let content_length = headers
                            .lines()
                            .find_map(|line| {
                                let lower = line.to_ascii_lowercase();
                                lower
                                    .strip_prefix("content-length:")
                                    .and_then(|value| value.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if buf.len() >= head_end + 4 + content_length {
                            break;
                        }
                    }
                }
                let body = serde_json::json!({
                    "id": "chatcmpl-mock",
                    "object": "chat.completion",
                    "created": 0,
                    "model": "gpt-test",
                    "choices": [{
                        "index": 0,
                        "message": { "role": "assistant", "content": response_content },
                        "finish_reason": "stop"
                    }],
                    "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
                });
                let payload = serde_json::to_vec(&body).expect("serialize response");
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                if stream
                    .write_all(head.as_bytes())
                    .and_then(|_| stream.write_all(&payload))
                    .and_then(|_| stream.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        format!("http://{addr}")
    }

    /// 同上，但把收到的第一个请求体交出来——用于断言「参数真的发出去了 / 真的被过滤了」，
    /// 而不是只断言我们在内存里算对了。
    fn spawn_capturing_mock_llm_server(
        response_content: String,
    ) -> (String, std::sync::mpsc::Receiver<serde_json::Value>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock llm");
        let addr = listener.local_addr().expect("local addr");
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for _ in 0..4 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buf = Vec::new();
                let mut temp = [0u8; 8192];
                let mut body_start = 0usize;
                loop {
                    let read = stream.read(&mut temp).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    buf.extend_from_slice(&temp[..read]);
                    if let Some(head_end) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&buf[..head_end]);
                        let content_length = headers
                            .lines()
                            .find_map(|line| {
                                let lower = line.to_ascii_lowercase();
                                lower
                                    .strip_prefix("content-length:")
                                    .and_then(|value| value.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        body_start = head_end + 4;
                        if buf.len() >= body_start + content_length {
                            break;
                        }
                    }
                }
                if let Ok(request) = serde_json::from_slice::<serde_json::Value>(&buf[body_start..]) {
                    let _ = sender.send(request);
                }
                let body = serde_json::json!({
                    "id": "chatcmpl-mock",
                    "object": "chat.completion",
                    "created": 0,
                    "model": "gpt-test",
                    "choices": [{
                        "index": 0,
                        "message": { "role": "assistant", "content": response_content },
                        "finish_reason": "stop"
                    }],
                    "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
                });
                let payload = serde_json::to_vec(&body).expect("serialize response");
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                if stream
                    .write_all(head.as_bytes())
                    .and_then(|_| stream.write_all(&payload))
                    .and_then(|_| stream.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        (format!("http://{addr}"), receiver)
    }

    fn mock_model(base_url: String) -> ModelConfig {
        ModelConfig {
            base_url,
            streaming_enabled: false,
            ..sample_model()
        }
    }

    fn valid_director_content() -> String {
        serde_json::json!({
            "world_phase": "calm",
            "next_location": "Dock",
            "next_scene_name": "Dock",
            "next_scene_background_hint": "rain",
            "next_time_label": "Night",
            "next_scene_tags": ["harbor"],
            "scene_visible_characters": ["Alice", "Bob"],
            "planned_speakers": ["Alice"],
            "director_runtime": {
                "world_phase": "calm",
                "next_location": "Dock",
                "next_scene_name": "Dock",
                "next_scene_background_hint": "rain",
                "next_time_label": "Night",
                "next_scene_tags": ["harbor"],
                "scene_visible_characters": ["Alice", "Bob"],
                "planned_speakers": ["Alice"]
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn golden_full_turn_generates_validates_and_persists_journal() {
        let base_url = spawn_mock_llm_server(valid_director_content());
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create schema");

        let orchestrator = SessionOrchestrator;
        let director = WorldDirectorService::new();
        let llm = crate::services::llm::client::LlmClient::new();
        let session = sample_session();
        let world = sample_world();
        let characters = vec![
            sample_character("char-a", "Alice"),
            sample_character("char-b", "Bob"),
        ];

        // 生成 + 校验：走真实 HTTP 请求 → openai 解析 → 导演 payload 校验。
        let run = orchestrator
            .run_director_turn(
                &llm,
                &director,
                mock_model(base_url),
                DirectorTurnRecovery {
                    resume_incomplete_turn: false,
                    recovered_completed_payload: None,
                },
                &session,
                &world,
                &characters,
                1,
                "继续巡查码头",
                &[],
                &[],
                &[],
                &std::collections::HashMap::new(),
                &crate::models::session::SessionRuntimeAttributesResponse::default(),
                &crate::models::generation_params::GenerationParams::builtin_default_for_role(
                    crate::models::generation_params::GENERATION_ROLE_DIRECTOR,
                ),
                None,
                None,
            )
            .await
            .expect("director turn should succeed");

        assert_eq!(run.runtime_payload.planned_speakers, vec!["Alice".to_string()]);
        assert_eq!(run.runtime_payload.next_location, "Dock");
        assert_eq!(run.traces.len(), 1, "无工具调用时工具循环应一次完成");

        // 落库：用生成结果驱动写回，journal 必须包含导演完成与回合完成记录。
        let mut updated = session.clone();
        updated.location = run.runtime_payload.next_location.clone();
        updated.current_speaker = "Alice".to_string();
        updated.current_line = "雨下得更大了。".to_string();

        let director_runtime = run
            .parsed
            .get("director_runtime")
            .cloned()
            .unwrap_or_default();
        orchestrator
            .writeback_turn_snapshot(TurnWritebackInput {
                conn: &conn,
                director_service: &director,
                recovery_journal: &[],
                session_id: &session.id,
                turn_index: 1,
                runtime_application: &DirectorRuntimeApplication::default(),
                updated: &updated,
                session: &session,
                world: &world,
                characters: &characters,
                director_runtime: &director_runtime,
                planned_speakers: &run.runtime_payload.planned_speakers,
                scene_visible_characters: &run.runtime_payload.scene_visible_characters,
                director_loop_traces: &run.traces,
                director_provider: "openai",
                director_model: &run.model,
                player_input: "继续巡查码头",
                director_tool_loop_limit: run.tool_loop_limit,
            })
            .expect("writeback");

        let mut stmt = conn
            .prepare("SELECT step, payload_json FROM turn_journal WHERE session_id = ?1 AND turn_index = 1 ORDER BY created_at, id")
            .expect("prepare query");
        let rows = stmt
            .query_map(rusqlite::params![session.id.clone()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        let steps: Vec<&str> = rows.iter().map(|(step, _)| step.as_str()).collect();
        assert!(steps.contains(&"finished"), "journal 缺少 finished: {steps:?}");

        let director_payload = rows
            .iter()
            .find(|(step, _)| step == "director_completed")
            .map(|(_, payload)| serde_json::from_str::<serde_json::Value>(payload).unwrap())
            .expect("journal 缺少 director_completed");
        // 关键断言：落库的导演 payload 必须就是 mock LLM 生成的那段内容，
        // 证明 生成 → 校验 → 落库 是同一条链路。
        assert_eq!(
            director_payload
                .get("director_runtime")
                .and_then(|value| value.get("world_phase"))
                .and_then(|value| value.as_str()),
            Some("calm")
        );
    }

    #[tokio::test]
    async fn golden_turn_rejects_player_in_planned_speakers() {
        let base_url = spawn_mock_llm_server(
            serde_json::json!({
                "planned_speakers": ["Player"],
                "director_runtime": { "world_phase": "calm" }
            })
            .to_string(),
        );
        let orchestrator = SessionOrchestrator;
        let director = WorldDirectorService::new();
        let llm = crate::services::llm::client::LlmClient::new();
        let session = sample_session();
        let world = sample_world();
        let characters = vec![sample_character("char-a", "Alice")];

        let failure = match orchestrator
            .run_director_turn(
                &llm,
                &director,
                mock_model(base_url),
                DirectorTurnRecovery {
                    resume_incomplete_turn: false,
                    recovered_completed_payload: None,
                },
                &session,
                &world,
                &characters,
                1,
                "轮到我了",
                &[],
                &[],
                &[],
                &std::collections::HashMap::new(),
                &crate::models::session::SessionRuntimeAttributesResponse::default(),
                &crate::models::generation_params::GenerationParams::builtin_default_for_role(
                    crate::models::generation_params::GENERATION_ROLE_DIRECTOR,
                ),
                None,
                None,
            )
            .await
        {
            Ok(_) => panic!("planned_speakers 含玩家必须被校验拒绝"),
            Err(failure) => failure,
        };

        assert_eq!(failure.failure_code, "domain_validation_failed");
    }

    #[tokio::test]
    async fn golden_turn_rejects_non_json_model_output() {
        let base_url = spawn_mock_llm_server("这不是 JSON，模型跑偏了".to_string());
        let orchestrator = SessionOrchestrator;
        let director = WorldDirectorService::new();
        let llm = crate::services::llm::client::LlmClient::new();
        let session = sample_session();
        let world = sample_world();
        let characters = vec![sample_character("char-a", "Alice")];

        let failure = match orchestrator
            .run_director_turn(
                &llm,
                &director,
                mock_model(base_url),
                DirectorTurnRecovery {
                    resume_incomplete_turn: false,
                    recovered_completed_payload: None,
                },
                &session,
                &world,
                &characters,
                1,
                "随便说点什么",
                &[],
                &[],
                &[],
                &std::collections::HashMap::new(),
                &crate::models::session::SessionRuntimeAttributesResponse::default(),
                &crate::models::generation_params::GenerationParams::builtin_default_for_role(
                    crate::models::generation_params::GENERATION_ROLE_DIRECTOR,
                ),
                None,
                None,
            )
            .await
        {
            Ok(_) => panic!("非 JSON 输出必须被校验拒绝"),
            Err(failure) => failure,
        };

        assert!(
            failure.failure_code == "json_parse_failed"
                || failure.failure_code == "json_repair_failed",
            "unexpected failure_code: {}",
            failure.failure_code
        );
    }

    // ---- 生成参数三级覆盖（第 8 项）----

    #[tokio::test]
    async fn resolved_generation_params_reach_the_wire_and_unsupported_ones_are_filtered() {
        let (base_url, requests) = spawn_capturing_mock_llm_server(valid_director_content());
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create schema");

        let orchestrator = SessionOrchestrator;
        let director = WorldDirectorService::new();
        let llm = crate::services::llm::client::LlmClient::new();
        let session = sample_session();
        let mut world = sample_world();
        // 世界包配一组参数，其中 top_k 是 OpenAI 兼容端点不支持的。
        world.director_config = serde_json::json!({
            "generation_params": {
                "temperature": 0.15,
                "top_p": 0.8,
                "top_k": 40,
                "max_tokens": 777,
                "stop": ["【完】"],
                "seed": 1234
            }
        });
        let characters = vec![sample_character("char-a", "Alice")];
        let settings = crate::models::settings::AppSettings::default();
        let model = mock_model(base_url);
        let generation = resolve_generation_params_with_model(
            crate::models::generation_params::GENERATION_ROLE_DIRECTOR,
            &settings,
            &world,
            &session,
            &model,
        );

        orchestrator
            .run_director_turn(
                &llm,
                &director,
                model,
                DirectorTurnRecovery {
                    resume_incomplete_turn: false,
                    recovered_completed_payload: None,
                },
                &session,
                &world,
                &characters,
                1,
                "继续巡查码头",
                &[],
                &[],
                &[],
                &std::collections::HashMap::new(),
                &crate::models::session::SessionRuntimeAttributesResponse::default(),
                &generation,
                None,
                None,
            )
            .await
            .expect("director turn should succeed");

        let sent = requests
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("mock server should capture the request body");

        // 世界层配置直达请求体，且盖住了导演内置默认的 0.7。
        assert_eq!(sent["temperature"], 0.15);
        assert_eq!(sent["top_p"], 0.8);
        assert_eq!(sent["seed"], 1234);
        assert_eq!(sent["stop"], serde_json::json!(["【完】"]));
        // 世界配的 max_tokens 优先于模型连接配置的上限。
        assert_eq!(sent["max_tokens"], 777);
        // 不支持的参数被摘掉，而不是原样发出去让服务端报错。
        assert!(
            sent.get("top_k").is_none(),
            "OpenAI 兼容端点不支持 top_k，不该出现在请求体里"
        );
    }

    #[tokio::test]
    async fn player_media_reaches_the_wire_as_openai_multipart() {
        let (base_url, requests) = spawn_capturing_mock_llm_server(valid_director_content());
        let orchestrator = SessionOrchestrator;
        let director = WorldDirectorService::new();
        let llm = crate::services::llm::client::LlmClient::new();
        let session = sample_session();
        let world = sample_world();
        let characters = vec![sample_character("char-a", "Alice")];
        let model = ModelConfig {
            input_modalities: vec!["image".to_string(), "audio".to_string()],
            ..mock_model(base_url)
        };
        let media = vec![
            crate::models::session::ContentPart {
                part_type: "image_url".to_string(),
                text: None,
                image_url: Some(crate::models::session::ImageUrl {
                    url: "data:image/png;base64,QUJD".to_string(),
                }),
                input_audio: None,
            },
            crate::models::session::ContentPart {
                part_type: "input_audio".to_string(),
                text: None,
                image_url: None,
                input_audio: Some(crate::models::session::InputAudio {
                    data: "data:audio/wav;base64,QUJD".to_string(),
                    format: "wav".to_string(),
                    duration_secs: Some(4.0),
                }),
            },
        ];

        orchestrator
            .run_director_turn(
                &llm,
                &director,
                model,
                DirectorTurnRecovery {
                    resume_incomplete_turn: false,
                    recovered_completed_payload: None,
                },
                &session,
                &world,
                &characters,
                1,
                "看看这张图",
                &media,
                &[],
                &[],
                &std::collections::HashMap::new(),
                &crate::models::session::SessionRuntimeAttributesResponse::default(),
                &crate::models::generation_params::GenerationParams::builtin_default_for_role(
                    crate::models::generation_params::GENERATION_ROLE_DIRECTOR,
                ),
                None,
                None,
            )
            .await
            .expect("director turn should succeed");

        let sent = requests
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("mock server should capture the request body");
        let messages = sent["messages"].as_array().expect("messages array");
        let user_message = messages
            .iter()
            .find(|message| message["role"] == "user")
            .expect("user message");
        let parts = user_message["content"]
            .as_array()
            .expect("multipart user content");
        // 第一段是原 payload 文本，媒体 parts 紧随其后、OpenAI 格式直达 wire。
        assert_eq!(parts[0]["type"], "text");
        assert!(
            parts[0]["text"].as_str().unwrap_or("").contains("看看这张图"),
            "文本段应携带本回合输入"
        );
        assert_eq!(
            parts[1].pointer("/image_url/url").and_then(|v| v.as_str()),
            Some("data:image/png;base64,QUJD")
        );
        let audio = &parts[2]["input_audio"];
        assert_eq!(audio["data"].as_str(), Some("QUJD"), "data URL 前缀应被剥掉");
        assert!(audio.get("duration_secs").is_none(), "内部字段不应落线");
    }

    #[tokio::test]
    async fn session_layer_overrides_world_layer_on_the_wire() {
        let (base_url, requests) = spawn_capturing_mock_llm_server(valid_director_content());
        let orchestrator = SessionOrchestrator;
        let director = WorldDirectorService::new();
        let llm = crate::services::llm::client::LlmClient::new();
        let mut world = sample_world();
        world.director_config =
            serde_json::json!({ "generation_params": { "temperature": 1.4, "top_p": 0.7 } });
        let mut session = sample_session();
        // 玩家在本存档里把随机度调低：只覆盖 temperature，top_p 仍走世界层。
        session.generation_params = crate::models::generation_params::GenerationParams {
            temperature: Some(0.05),
            ..Default::default()
        };
        let characters = vec![sample_character("char-a", "Alice")];
        let model = mock_model(base_url);
        let generation = resolve_generation_params_with_model(
            crate::models::generation_params::GENERATION_ROLE_DIRECTOR,
            &crate::models::settings::AppSettings::default(),
            &world,
            &session,
            &model,
        );

        orchestrator
            .run_director_turn(
                &llm,
                &director,
                model,
                DirectorTurnRecovery {
                    resume_incomplete_turn: false,
                    recovered_completed_payload: None,
                },
                &session,
                &world,
                &characters,
                1,
                "继续巡查码头",
                &[],
                &[],
                &[],
                &std::collections::HashMap::new(),
                &crate::models::session::SessionRuntimeAttributesResponse::default(),
                &generation,
                None,
                None,
            )
            .await
            .expect("director turn should succeed");

        let sent = requests
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("mock server should capture the request body");

        assert_eq!(sent["temperature"], 0.05, "存档层优先级最高");
        assert_eq!(sent["top_p"], 0.7, "存档没配的项仍用世界层");
    }

    // ---- 消息稳定 ID（第 1 项）----

    #[test]
    fn message_ids_are_stable_unique_and_editable() {
        let first = ChatMessage::new(
            "player",
            MessageContent::Text("第一条".to_string()),
            Some("Player".to_string()),
        );
        let second = ChatMessage::new(
            "agent",
            MessageContent::Text("第二条".to_string()),
            Some("Alice".to_string()),
        );
        assert!(!first.message_id.is_empty());
        assert!(!first.created_at.is_empty());
        assert_ne!(first.message_id, second.message_id);

        let mut session = SessionSnapshot {
            messages: vec![first.clone(), second.clone()],
            ..sample_session()
        };
        let found = session.find_message(&second.message_id).expect("find by id");
        assert_eq!(found.content.as_str(), "第二条");

        assert!(session.edit_message_content(
            &second.message_id,
            MessageContent::Text("改过的第二条".to_string()),
        ));
        assert_eq!(
            session.find_message(&second.message_id).unwrap().content.as_str(),
            "改过的第二条"
        );
        // 不存在的 id 不得误伤其它消息
        assert!(!session.edit_message_content("msg-not-exist", MessageContent::Text("x".into())));
        assert_eq!(session.messages.len(), 2);
    }

    #[test]
    fn legacy_message_without_id_gets_fresh_id_on_deserialize() {
        // 旧数据没有 message_id/created_at 字段：反序列化时补发 id，created_at 留空。
        let legacy: ChatMessage = serde_json::from_str(
            r#"{"role":"agent","content":"旧消息","speaker":"Alice","metadata":null}"#,
        )
        .expect("deserialize legacy message");
        assert!(!legacy.message_id.is_empty());
        assert!(legacy.created_at.is_empty());
        assert!(legacy.parent_message_id.is_none());
    }

    #[test]
    fn message_runtime_proposals_follow_turn_index_linkage() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create schema");

        let mut session = sample_session();
        let message = ChatMessage::new(
            "agent",
            MessageContent::Text("这一回合属性变了".to_string()),
            Some("Alice".to_string()),
        )
        .with_metadata(serde_json::json!({ "turn_index": 7 }));
        let message_id = message.message_id.clone();
        session.messages = vec![message];
        crate::db::repositories::session_repo::SessionRepository::new(&conn)
            .upsert(&session)
            .expect("upsert session");

        let proposals = serde_json::json!({
            "state_phase": "escalation",
            "state_tags": ["under_watch"]
        });
        append_turn_journal(
            &conn,
            &session.id,
            7,
            "runtime_effects_applied",
            "completed",
            proposals.clone(),
        )
        .expect("append journal");

        let loaded = load_message_runtime_proposals(&conn, &session.id, &message_id)
            .expect("load proposals")
            .expect("proposals exist");
        assert_eq!(loaded, proposals);

        // 无 turn_index 的消息查不到提议；不存在的消息返回 None。
        let plain = ChatMessage::new(
            "player",
            MessageContent::Text("没有回合标记".to_string()),
            Some("Player".to_string()),
        );
        session.messages.push(plain.clone());
        crate::db::repositories::session_repo::SessionRepository::new(&conn)
            .upsert(&session)
            .expect("upsert session 2");
        assert!(
            load_message_runtime_proposals(&conn, &session.id, &plain.message_id)
                .expect("load ok")
                .is_none()
        );
        assert!(
            load_message_runtime_proposals(&conn, &session.id, "msg-not-exist")
                .expect("load ok")
                .is_none()
        );
    }
