use crate::models::session::*;
use crate::services::game_engine::structured_output::StructuredOutputFailure;
use chrono::Utc;
use rusqlite::{params, Connection};

use super::run::*;
use super::writeback::*;

impl SessionOrchestrator {
    pub fn load_resume_player_request(
        &self,
        conn: &Connection,
        session_id: &str,
    ) -> Result<Option<PlayerActionRequest>, String> {
        let latest_turn_index = load_latest_turn_index(conn, session_id)?;
        if latest_turn_index <= 0 {
            return Ok(None);
        }
        // 从最新回合向前找第一个"未完成且有 created 载荷"的回合。
        // 旧版本曾把重试写进错位的新回合(只有 structured_output_failed、没有 created),
        // 对这类存档只检查最新回合会永远报 Missing created payload;向前扫描可自愈。
        let mut turn_index = latest_turn_index;
        while turn_index > 0 {
            let recovery_journal = load_turn_journal(conn, session_id, turn_index)?;
            if recovery_journal.is_empty() {
                break;
            }
            if journal_has_completed_step(&recovery_journal, "finished") {
                // 最新回合已完成:更早的回合视为历史,与旧行为一致返回 None。
                return Ok(None);
            }
            if let Some(created_payload) = journal_payload(&recovery_journal, "created") {
                let content = created_payload
                    .get("player_input")
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Incomplete turn has no player input".to_string())?;
                return Ok(Some(PlayerActionRequest {
                    content: MessageContent::Text(content),
                    action_mode: PlayerActionMode::Resend,
                    resend_from_turn_index: Some(turn_index),
                }));
            }
            // 未完成但缺 created 的错位回合:跳过,继续向前找。
            turn_index -= 1;
        }
        Err("Missing created payload for incomplete turn".to_string())
    }

    pub fn record_structured_output_failure(
        &self,
        conn: &Connection,
        session_id: &str,
        turn_index: i32,
        request: &PlayerActionRequest,
        failure: &StructuredOutputFailure,
    ) -> Result<ChatMessage, String> {
        conn.execute(
            "UPDATE llm_retry_capsules SET status = 'invalidated' WHERE session_id = ?1 AND status = 'active'",
            params![session_id],
        )
        .map_err(|e| e.to_string())?;

        let retry_token = uuid::Uuid::new_v4().to_string();
        let message_id = uuid::Uuid::new_v4().to_string();
        let display_message =
            self.build_structured_failure_chat_message(failure, turn_index, Some(&retry_token));
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO llm_retry_capsules (id, session_id, turn_index, message_id, retry_token, stage, provider, model_id, request_json, prompt_trace_json, input_snapshot_json, tool_context_json, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'active', ?13)",
            params![
                uuid::Uuid::new_v4().to_string(),
                session_id,
                turn_index,
                message_id,
                retry_token,
                failure.stage.retry_kind(),
                failure.provider,
                failure.model_id,
                serde_json::to_string(request).unwrap_or_else(|_| "{}".to_string()),
                "{}",
                "{}",
                "{}",
                now,
            ],
        )
        .map_err(|e| e.to_string())?;
        append_turn_journal(
            conn,
            session_id,
            turn_index,
            "structured_output_failed",
            "completed",
            serde_json::json!({
                "failure": serde_json::to_value(failure).unwrap_or_default(),
                "display_message": serde_json::to_value(&display_message).unwrap_or_default(),
                "retry_token": retry_token,
                "message_id": message_id,
            }),
        )?;
        Ok(display_message)
    }

    pub fn build_structured_failure_chat_message(
        &self,
        failure: &StructuredOutputFailure,
        turn_index: i32,
        retry_token: Option<&str>,
    ) -> ChatMessage {
        let mut metadata = serde_json::json!({
            "turn_index": turn_index,
            "message_kind": failure.message_kind(),
            "action_type": failure.action_type(),
            "failure_code": failure.failure_code,
            "failure_stage": failure.stage.retry_kind(),
            "provider": failure.provider,
            "model_id": failure.model_id,
            "summary": failure.summary,
            "repair_summary": failure.repair_summary,
            "schema_errors": failure.schema_errors,
            "domain_errors": failure.domain_errors,
            "raw_excerpt": failure.raw_text_excerpt,
            "speaker_name": failure.speaker_name,
            "title": failure.display_title(),
        });
        if let Some(object) = metadata.as_object_mut() {
            if let Some(token) = retry_token {
                object.insert(
                    "retry_token".to_string(),
                    serde_json::Value::String(token.to_string()),
                );
            }
        }
        ChatMessage {
            message_id: ChatMessage::generate_id(),
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_message_id: None,
            role: "system".to_string(),
            content: MessageContent::Text(failure.display_content()),
            speaker: None,
            metadata: Some(metadata),
        }
    }

    pub fn build_incomplete_turn_overlay(
        &self,
        conn: &Connection,
        session: &SessionSnapshot,
    ) -> Result<Option<SessionSnapshot>, String> {
        let latest_turn_index = load_latest_turn_index(conn, &session.id)?;
        if latest_turn_index <= 0 {
            return Ok(None);
        }
        let recovery_journal = load_turn_journal(conn, &session.id, latest_turn_index)?;
        if recovery_journal.is_empty() || journal_has_completed_step(&recovery_journal, "finished")
        {
            return Ok(None);
        }
        let failure_payload = journal_payload(&recovery_journal, "structured_output_failed");
        let Some(failure_payload) = failure_payload else {
            return Ok(None);
        };
        let created_payload = journal_payload(&recovery_journal, "created").unwrap_or_default();
        let mut messages = session.messages.clone();
        let player_input = created_payload
            .get("player_input")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .unwrap_or_default();
        if !player_input.trim().is_empty() {
            messages.push(ChatMessage {
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: "player".to_string(),
                content: MessageContent::Text(player_input),
                speaker: Some(session.player_character_name.clone()),
                metadata: Some(serde_json::json!({
                    "turn_index": latest_turn_index,
                    "message_kind": "player_action"
                })),
            });
        }
        messages.extend(materialize_completed_speaker_messages(
            &recovery_journal,
            latest_turn_index,
        ));
        if let Some(message_value) = failure_payload.get("display_message").cloned() {
            if let Ok(message) = serde_json::from_value::<ChatMessage>(message_value) {
                messages.push(message);
            }
        }
        Ok(Some(SessionSnapshot {
            messages,
            ..session.clone()
        }))
    }

    /// H3: 原子认领胶囊。把 active → consuming 用单条带 status 守卫的 UPDATE 完成,
    /// 仅当本次确实改动一行(`changes() == 1`)才算认领成功。两个并发同 token 调用中
    /// 只有一个能赢得这行,另一个看到 0 行被改、立即报错,从而杜绝双重消费/双重付费回合。
    pub fn claim_retry_capsule(
        &self,
        conn: &Connection,
        session_id: &str,
        retry_token: &str,
    ) -> Result<(), String> {
        conn.execute(
            "UPDATE llm_retry_capsules SET status = 'consuming' WHERE session_id = ?1 AND retry_token = ?2 AND status = 'active'",
            params![session_id, retry_token],
        )
        .map_err(|e| e.to_string())?;
        if conn.changes() == 0 {
            return Err("Retry token is missing or no longer active".to_string());
        }
        Ok(())
    }

    /// H3: 认领成功的回合跑完后,把 consuming → consumed 落定。
    pub fn finalize_retry_capsule(
        &self,
        conn: &Connection,
        session_id: &str,
        retry_token: &str,
    ) -> Result<(), String> {
        conn.execute(
            "UPDATE llm_retry_capsules SET status = 'consumed', consumed_at = ?3 WHERE session_id = ?1 AND retry_token = ?2 AND status = 'consuming'",
            params![session_id, retry_token, Utc::now().to_rfc3339()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// H3: 回合执行失败(返回 Err)时把胶囊从 consuming 退回 active,允许重试。
    pub fn release_retry_capsule(
        &self,
        conn: &Connection,
        session_id: &str,
        retry_token: &str,
    ) -> Result<(), String> {
        conn.execute(
            "UPDATE llm_retry_capsules SET status = 'active' WHERE session_id = ?1 AND retry_token = ?2 AND status = 'consuming'",
            params![session_id, retry_token],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::world_repo::WorldRepository;
    use crate::db::schema;
    use crate::models::world::WorldCreateRequest;
    use crate::services::game_engine::structured_output::StructuredFailureStage;

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

    #[test]
    fn build_structured_failure_chat_message_uses_expected_metadata_shapes() {
        let orchestrator = SessionOrchestrator;
        let director_failure = StructuredOutputFailure {
            stage: StructuredFailureStage::DirectorMain,
            failure_code: "domain_validation_failed".to_string(),
            summary: "director payload invalid".to_string(),
            provider: "openai".to_string(),
            model_id: "gpt-test".to_string(),
            turn_index: 2,
            speaker_name: None,
            raw_text_excerpt: "{\"planned_speakers\":[\"Ghost\"]}".to_string(),
            repair_summary: Some("json extracted but domain validation failed".to_string()),
            schema_errors: Vec::new(),
            domain_errors: vec![
                "planned_speakers contains unknown or not-visible character: Ghost".to_string(),
            ],
        };

        let director_message = orchestrator.build_structured_failure_chat_message(
            &director_failure,
            2,
            Some("retry-director"),
        );
        let director_metadata = director_message
            .metadata
            .as_ref()
            .expect("director metadata");

        assert_eq!(director_message.role, "system");
        assert_eq!(
            director_metadata
                .get("message_kind")
                .and_then(|value| value.as_str()),
            Some("system_action")
        );
        assert_eq!(
            director_metadata
                .get("action_type")
                .and_then(|value| value.as_str()),
            Some("director_retry_required")
        );
        assert_eq!(
            director_metadata
                .get("retry_token")
                .and_then(|value| value.as_str()),
            Some("retry-director")
        );

        let speaker_failure = StructuredOutputFailure {
            stage: StructuredFailureStage::SpeakerResponse,
            failure_code: "schema_validation_failed".to_string(),
            summary: "speaker payload invalid".to_string(),
            provider: "openai".to_string(),
            model_id: "gpt-test".to_string(),
            turn_index: 2,
            speaker_name: Some("Alice".to_string()),
            raw_text_excerpt: "{\"speaker\":\"Alice\"}".to_string(),
            repair_summary: None,
            schema_errors: vec!["content is required".to_string()],
            domain_errors: Vec::new(),
        };
        let speaker_message = orchestrator.build_structured_failure_chat_message(
            &speaker_failure,
            2,
            Some("retry-speaker"),
        );
        let speaker_metadata = speaker_message.metadata.as_ref().expect("speaker metadata");

        assert_eq!(
            speaker_metadata
                .get("message_kind")
                .and_then(|value| value.as_str()),
            Some("llm_structured_error")
        );
        assert_eq!(
            speaker_metadata
                .get("action_type")
                .and_then(|value| value.as_str()),
            Some("structured_output_error")
        );
        assert_eq!(
            speaker_metadata
                .get("speaker_name")
                .and_then(|value| value.as_str()),
            Some("Alice")
        );
    }

    #[test]
    fn record_structured_output_failure_persists_retry_capsule_and_invalidates_previous_one() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create schema");
        let orchestrator = SessionOrchestrator;
        let session = sample_session();
        let request = PlayerActionRequest {
            content: MessageContent::Text("Open the sealed door".to_string()),
            action_mode: PlayerActionMode::Submit,
            resend_from_turn_index: None,
        };
        conn.execute(
            "INSERT INTO llm_retry_capsules (id, session_id, turn_index, message_id, retry_token, stage, provider, model_id, request_json, prompt_trace_json, input_snapshot_json, tool_context_json, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '{}', '{}', '{}', '{}', 'active', ?9)",
            rusqlite::params![
                "capsule-old",
                session.id.clone(),
                1,
                "msg-old",
                "retry-old",
                "director_main",
                "openai",
                "gpt-old",
                Utc::now().to_rfc3339(),
            ],
        )
        .expect("seed active capsule");

        let failure = StructuredOutputFailure {
            stage: StructuredFailureStage::DirectorMain,
            failure_code: "json_repair_failed".to_string(),
            summary: "director json could not be repaired".to_string(),
            provider: "openai".to_string(),
            model_id: "gpt-test".to_string(),
            turn_index: 2,
            speaker_name: None,
            raw_text_excerpt: "```json {broken".to_string(),
            repair_summary: Some("code fence stripped but no valid object remained".to_string()),
            schema_errors: vec!["response object is empty".to_string()],
            domain_errors: Vec::new(),
        };

        let message = orchestrator
            .record_structured_output_failure(&conn, &session.id, 2, &request, &failure)
            .expect("record failure");
        let metadata = message.metadata.expect("failure metadata");
        let retry_token = metadata
            .get("retry_token")
            .and_then(|value| value.as_str())
            .expect("retry token")
            .to_string();

        let old_status: String = conn
            .query_row(
                "SELECT status FROM llm_retry_capsules WHERE id = 'capsule-old'",
                [],
                |row| row.get(0),
            )
            .expect("load previous capsule");
        assert_eq!(old_status, "invalidated");

        let (status, stage, stored_request_json): (String, String, String) = conn
            .query_row(
                "SELECT status, stage, request_json FROM llm_retry_capsules WHERE session_id = ?1 AND retry_token = ?2",
                rusqlite::params![session.id.clone(), retry_token.clone()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("load new capsule");
        assert_eq!(status, "active");
        assert_eq!(stage, "director_main");
        assert!(stored_request_json.contains("Open the sealed door"));

        let journal_payload: String = conn
            .query_row(
                "SELECT payload_json FROM turn_journal WHERE session_id = ?1 AND turn_index = 2 AND step = 'structured_output_failed'",
                rusqlite::params![session.id.clone()],
                |row| row.get(0),
            )
            .expect("journal payload");
        assert!(journal_payload.contains(&retry_token));
    }

    #[test]
    fn build_incomplete_turn_overlay_rehydrates_player_and_failure_messages() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create schema");
        let orchestrator = SessionOrchestrator;
        let session = sample_session();
        let turn_index = 3;

        append_turn_journal(
            &conn,
            &session.id,
            turn_index,
            "created",
            "completed",
            serde_json::json!({
                "player_input": "Force the lock",
                "action_mode": "submit",
                "player_character_name": session.player_character_name.clone(),
            }),
        )
        .expect("append created");
        append_turn_journal(
            &conn,
            &session.id,
            turn_index,
            "speaker_0_completed",
            "completed",
            serde_json::json!({
                "llm_output": {
                    "speaker": "Alice",
                    "content": "Alice checks the tumblers and nods.",
                    "intent": "inspect",
                    "emotion": "focused",
                    "narration": "She kneels by the lock.",
                    "raw_content": "{\"speaker\":\"Alice\",\"content\":\"Alice checks the tumblers and nods.\"}"
                }
            }),
        )
        .expect("append speaker");

        let failure = StructuredOutputFailure {
            stage: StructuredFailureStage::SpeakerResponse,
            failure_code: "schema_validation_failed".to_string(),
            summary: "speaker payload invalid".to_string(),
            provider: "openai".to_string(),
            model_id: "gpt-test".to_string(),
            turn_index,
            speaker_name: Some("Bob".to_string()),
            raw_text_excerpt: "{\"speaker\":\"Bob\"}".to_string(),
            repair_summary: None,
            schema_errors: vec!["content is required".to_string()],
            domain_errors: Vec::new(),
        };
        let failure_message = orchestrator.build_structured_failure_chat_message(
            &failure,
            turn_index,
            Some("retry-3"),
        );
        append_turn_journal(
            &conn,
            &session.id,
            turn_index,
            "structured_output_failed",
            "completed",
            serde_json::json!({
                "display_message": failure_message,
            }),
        )
        .expect("append failure");

        let overlay = orchestrator
            .build_incomplete_turn_overlay(&conn, &session)
            .expect("build overlay")
            .expect("overlay exists");

        assert_eq!(overlay.messages.len(), 3);
        assert_eq!(overlay.messages[0].role, "player");
        assert_eq!(overlay.messages[0].content, "Force the lock");
        assert_eq!(overlay.messages[1].role, "agent");
        assert_eq!(overlay.messages[1].speaker.as_deref(), Some("Alice"));
        assert_eq!(
            overlay.messages[1]
                .metadata
                .as_ref()
                .and_then(|value| value.get("recovered"))
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            overlay.messages[2]
                .metadata
                .as_ref()
                .and_then(|value| value.get("action_type"))
                .and_then(|value| value.as_str()),
            Some("structured_output_error")
        );
    }

    #[test]
    fn load_resume_player_request_scans_back_past_misaligned_turn() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create schema");
        let orchestrator = SessionOrchestrator;
        let session_id = "sess-misaligned";

        // 回合 1:正常开局后导演结构化输出失败(无 finished)。
        append_turn_journal(
            &conn,
            session_id,
            1,
            "created",
            "completed",
            serde_json::json!({
                "player_input": "推开密室的门",
                "action_mode": "submit",
            }),
        )
        .expect("append created turn 1");
        append_turn_journal(
            &conn,
            session_id,
            1,
            "structured_output_failed",
            "completed",
            serde_json::json!({}),
        )
        .expect("append failure turn 1");
        // 旧 bug 的错位存档:回合 2 只有 structured_output_failed,没有 created。
        append_turn_journal(
            &conn,
            session_id,
            2,
            "structured_output_failed",
            "completed",
            serde_json::json!({}),
        )
        .expect("append failure turn 2");

        let request = orchestrator
            .load_resume_player_request(&conn, session_id)
            .expect("错位存档应自愈,不再报 Missing created payload")
            .expect("应找到可恢复的回合 1");
        assert_eq!(request.resend_from_turn_index, Some(1));
        assert!(matches!(
            request.action_mode,
            PlayerActionMode::Resend
        ));
        assert_eq!(request.content.as_str(), "推开密室的门");
    }

    #[test]
    fn load_resume_player_request_returns_none_when_latest_turn_finished() {
        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create schema");
        let orchestrator = SessionOrchestrator;
        let session_id = "sess-finished";

        append_turn_journal(
            &conn,
            session_id,
            1,
            "created",
            "completed",
            serde_json::json!({ "player_input": "look around" }),
        )
        .expect("append created");
        append_turn_journal(
            &conn,
            session_id,
            1,
            "finished",
            "completed",
            serde_json::json!({}),
        )
        .expect("append finished");

        assert!(orchestrator
            .load_resume_player_request(&conn, session_id)
            .expect("load ok")
            .is_none());
    }

    #[test]
    fn resume_retry_reuses_incomplete_turn_index_and_stays_resumable() {
        use crate::db::repositories::model_repo::ModelRepository;
        use crate::models::model_config::ModelConfigCreateRequest;

        let conn = Connection::open_in_memory().expect("open sqlite");
        schema::create_tables(&conn).expect("create schema");
        WorldRepository::new(&conn)
            .create(&WorldCreateRequest {
                name: "World".to_string(),
                genre: "".to_string(),
                background_prompt: "".to_string(),
                opening_scene: "Dock".to_string(),
                summary: "".to_string(),
                time_system: "".to_string(),
                map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
                triggers: vec![],
                time_config: serde_json::json!({}),
                director_config: serde_json::json!({}),
                ui_theme_config: serde_json::json!({}),
                opening_messages: vec![],
                opening_character_ids: vec![],
                player_character_id: None,
            })
            .expect("create world");
        ModelRepository::new(&conn)
            .create(&ModelConfigCreateRequest {
                name: "test".to_string(),
                model_type: "text".to_string(),
                provider: "openai".to_string(),
                model_id: "gpt-test".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: "".to_string(),
                max_tokens: 1200,
                streaming_enabled: false,
                is_default: true,
                input_modalities: vec![],
            })
            .expect("create model");
        let session = sample_session();
        crate::db::repositories::session_repo::SessionRepository::new(&conn)
            .upsert(&session)
            .expect("upsert session");
        let orchestrator = SessionOrchestrator;

        let submit = PlayerActionRequest {
            content: MessageContent::Text("推开密室的门".to_string()),
            action_mode: PlayerActionMode::Submit,
            resend_from_turn_index: None,
        };
        let failure = StructuredOutputFailure {
            stage: StructuredFailureStage::DirectorMain,
            failure_code: "json_parse_failed".to_string(),
            summary: "director output is not valid json".to_string(),
            provider: "openai".to_string(),
            model_id: "gpt-test".to_string(),
            turn_index: 1,
            speaker_name: None,
            raw_text_excerpt: "{broken".to_string(),
            repair_summary: None,
            schema_errors: vec!["response must be a JSON object".to_string()],
            domain_errors: Vec::new(),
        };

        // 回合 1:开局后导演结构化输出失败。
        let first = orchestrator
            .prepare_turn_context(&conn, &session.id, &submit)
            .expect("prepare turn 1");
        assert_eq!(first.turn_index, 1);
        assert!(!first.resume_incomplete_turn);
        orchestrator
            .record_structured_output_failure(&conn, &session.id, first.turn_index, &submit, &failure)
            .expect("record failure 1");

        // 第一次重发:resume 必须复用回合号 1,而不是另开回合 2。
        let resume = orchestrator
            .load_resume_player_request(&conn, &session.id)
            .expect("load resume")
            .expect("resume request");
        assert_eq!(resume.resend_from_turn_index, Some(1));
        let resumed = orchestrator
            .prepare_turn_context(&conn, &session.id, &resume)
            .expect("prepare resume");
        assert!(resumed.resume_incomplete_turn);
        assert_eq!(resumed.turn_index, 1, "resume 必须复用原回合号");

        // 第二次失败仍记到回合 1;第三次重发仍能拿到 resume 请求
        // (修复前此处因回合错位报 Missing created payload,表现为"无法二次重发")。
        orchestrator
            .record_structured_output_failure(&conn, &session.id, resumed.turn_index, &resume, &failure)
            .expect("record failure 2");
        let resume_again = orchestrator
            .load_resume_player_request(&conn, &session.id)
            .expect("第三次重发必须能拿到 resume 请求")
            .expect("resume request after second failure");
        assert_eq!(resume_again.resend_from_turn_index, Some(1));
        assert_eq!(resume_again.content.as_str(), "推开密室的门");
    }
}
