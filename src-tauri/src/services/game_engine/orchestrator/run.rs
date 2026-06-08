use crate::models::attribute::AttributeValue;
use crate::models::character::CharacterDefinition;
use crate::models::model_config::ModelConfig;
use crate::models::scheduled_notification::PendingScheduledNotification;
use crate::models::session::*;
use crate::models::world::WorldDefinition;
use crate::services::assets::resolver::AssetResolver;
use crate::services::game_engine::director::{
    DirectorLoopIterationTrace, DirectorLoopStreamProgress, ParsedDirectorRuntimePayload,
    WorldDirectorService,
};
use crate::services::game_engine::runtime_effects::DirectorRuntimeApplication;
use crate::services::game_engine::service_mode::{
    agent_chat_virtual_player_id, agent_chat_virtual_player_name, resolve_service_runtime_config,
    ServiceMode,
};
use crate::services::game_engine::structured_output::{
    validate_director_payload, StructuredOutputFailure,
};
use crate::services::llm::client::LlmClient;
use crate::services::map_topology::compile_map_topology;
use crate::services::notifications::NotificationToolRuntime;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::collections::HashMap;

use super::turn_context::*;
use super::writeback::*;

#[derive(Debug, Clone)]
pub struct DirectorDecision {
    pub world_phase: String,
    pub next_location: Option<String>,
    pub next_scene_name: Option<String>,
    pub next_scene_background_hint: Option<String>,
    pub scene_visible_characters: Vec<String>,
}

pub struct SessionOrchestrator;

pub struct DirectorTurnRecovery {
    pub resume_incomplete_turn: bool,
    pub recovered_completed_payload: Option<serde_json::Value>,
}

pub struct DirectorTurnRun {
    pub parsed: serde_json::Value,
    pub runtime_payload: ParsedDirectorRuntimePayload,
    pub traces: Vec<DirectorLoopIterationTrace>,
    pub trace_message: Option<DirectorTraceMessage>,
    pub model: ModelConfig,
    pub provider: String,
    pub tool_loop_limit: usize,
}

pub struct SpeakerTurnRunResult {
    pub messages: Vec<ChatMessage>,
    pub failure: Option<StructuredOutputFailure>,
    pub pending_notifications: Vec<PendingScheduledNotification>,
}

pub struct PreparedTurnContext {
    pub session: SessionSnapshot,
    pub world: WorldDefinition,
    pub characters: Vec<CharacterDefinition>,
    pub turn_index: i32,
    pub recovery_journal: Vec<serde_json::Value>,
    pub resume_incomplete_turn: bool,
    pub image_model: Option<ModelConfig>,
    pub director_model: ModelConfig,
    pub messages: Vec<ChatMessage>,
    pub director_completed_payload: Option<serde_json::Value>,
}

pub struct SessionAssetContext {
    pub session: SessionSnapshot,
    pub world: WorldDefinition,
    pub characters: Vec<CharacterDefinition>,
    pub image_model: Option<ModelConfig>,
}

pub struct SwitchPlayerCharacterContext {
    pub session: SessionSnapshot,
    pub world: WorldDefinition,
    pub characters: Vec<CharacterDefinition>,
    pub new_character: CharacterDefinition,
    pub image_model: Option<ModelConfig>,
}

#[derive(Debug, Clone)]

pub struct DirectorRuntimePreparation {
    pub parsed_runtime: serde_json::Value,
    pub next_location: String,
    pub next_scene_name: String,
    pub current_line: Option<String>,
    pub next_scene_background_hint: String,
    pub next_time_label: String,
    pub scene_visible_characters: Option<Vec<String>>,
    pub scene_visible_characters_explicit: bool,
    pub planned_speakers: Vec<String>,
    pub visible_chars: Vec<String>,
    pub pre_runtime_system_messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone)]

pub struct SpeakerTurnProgress {
    pub messages: Vec<ChatMessage>,
    pub speaker_name: String,
    pub narration: Option<String>,
    pub is_placeholder: bool,
    pub is_error: bool,
}

#[derive(Debug, Clone)]

pub struct DirectorTraceMessage {
    pub trace_text: String,
    pub trace_lines: Vec<String>,
    pub reasoning: Option<String>,
}

pub struct RuntimeMutationInput<'a> {
    pub asset_resolver: &'a AssetResolver,
    pub data_dir: &'a std::path::Path,
    pub session: &'a SessionSnapshot,
    pub messages: &'a [ChatMessage],
    pub world: &'a WorldDefinition,
    pub characters: &'a [CharacterDefinition],
    pub turn_index: i32,
    pub next_location: &'a str,
    pub next_time_label: &'a str,
    pub next_scene_name: &'a str,
    pub current_line: Option<&'a str>,
    pub next_scene_background_hint: String,
    pub planned_speakers: &'a [String],
    pub scene_visible_characters_explicit: bool,
    pub scene_visible_characters: &'a Option<Vec<String>>,
    pub visible_chars: &'a [String],
    pub runtime_application: &'a DirectorRuntimeApplication,
    pub image_model: Option<&'a ModelConfig>,
    pub parsed_runtime: &'a serde_json::Value,
}

pub struct SwitchPlayerCharacterInput<'a> {
    pub asset_resolver: &'a AssetResolver,
    pub data_dir: &'a std::path::Path,
    pub session: &'a SessionSnapshot,
    pub world: &'a WorldDefinition,
    pub characters: &'a [CharacterDefinition],
    pub new_character: &'a CharacterDefinition,
    pub proposal: Option<&'a SwitchCharacterProposal>,
    pub image_model: Option<&'a ModelConfig>,
}

pub struct TurnWritebackInput<'a> {
    pub conn: &'a Connection,
    pub director_service: &'a WorldDirectorService,
    pub recovery_journal: &'a [serde_json::Value],
    pub session_id: &'a str,
    pub turn_index: i32,
    pub runtime_application: &'a DirectorRuntimeApplication,
    pub updated: &'a SessionSnapshot,
    pub session: &'a SessionSnapshot,
    pub world: &'a WorldDefinition,
    pub characters: &'a [CharacterDefinition],
    pub director_runtime: &'a serde_json::Value,
    pub planned_speakers: &'a [String],
    pub scene_visible_characters: &'a Option<Vec<String>>,
    pub director_loop_traces: &'a [DirectorLoopIterationTrace],
    pub director_provider: &'a str,
    pub director_model: &'a ModelConfig,
    pub player_input: &'a str,
    pub director_tool_loop_limit: usize,
}

pub fn build_director_trace_message_from_stream_progress(
    progress: &DirectorLoopStreamProgress,
) -> DirectorTraceMessage {
    let tool_calls = progress
        .tool_enriched
        .get("tool_calls")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let planned_speakers = progress
        .tool_enriched
        .get("planned_speakers")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    let next_scene_name = progress
        .tool_enriched
        .get("next_scene_name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let next_location = progress
        .tool_enriched
        .get("next_location")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let world_phase = progress
        .tool_enriched
        .get("world_phase")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let mut trace_lines = Vec::new();
    if !world_phase.is_empty() {
        trace_lines.push(format!("\u{9636}\u{6bb5}\u{ff1a}{world_phase}"));
    }
    if !next_scene_name.is_empty() {
        trace_lines.push(format!("\u{573a}\u{666f}\u{ff1a}{next_scene_name}"));
    }
    if !next_location.is_empty() {
        trace_lines.push(format!("\u{5730}\u{70b9}\u{ff1a}{next_location}"));
    }
    if !planned_speakers.is_empty() {
        trace_lines.push(format!(
            "\u{53d1}\u{8a00}\u{987a}\u{5e8f}\u{ff1a}{}",
            planned_speakers.join(" / ")
        ));
    }
    if !tool_calls.is_empty() {
        let tool_names = tool_calls
            .iter()
            .filter_map(|item| {
                item.get("tool_name")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        if !tool_names.is_empty() {
            trace_lines.push(format!(
                "\u{5de5}\u{5177}\u{8c03}\u{7528}\u{ff1a}{}",
                tool_names.join(" / ")
            ));
        }
    }
    DirectorTraceMessage {
        trace_text: if trace_lines.is_empty() {
            "\u{4e16}\u{754c}\u{4e3b}\u{63a7}\u{6b63}\u{5728}\u{601d}\u{8003}...".to_string()
        } else {
            trace_lines.join("\n")
        },
        trace_lines,
        reasoning: progress
            .reasoning
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    }
}

pub fn build_streaming_director_trace_chat_message(
    trace_message: &DirectorTraceMessage,
    turn_index: i32,
) -> ChatMessage {
    ChatMessage {
        role: "system".to_string(),
        content: MessageContent::Text(trace_message.trace_text.clone()),
        speaker: None,
        metadata: Some(serde_json::json!({
            "turn_index": turn_index,
            "action_type": "director_trace",
            "message_kind": "director_trace",
            "trace_source": "world_director",
            "trace_text": trace_message.trace_text,
            "trace_lines": trace_message.trace_lines,
            "reasoning": trace_message.reasoning,
            "reasoning_expanded": true,
            "world_phase": "",
            "next_scene_name": "",
            "next_location": "",
            "next_time_label": "",
            "planned_speakers": [],
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::character_repo::CharacterRepository;
    use crate::db::repositories::world_repo::WorldRepository;
    use crate::db::schema;
    use crate::models::character::CharacterCreateRequest;
    use crate::models::memory::MemoryEntry;
    use crate::models::session::{AssetSelection, SceneRuntime, SessionState};
    use crate::models::world::WorldCreateRequest;
    use crate::services::assets::resolver::AssetResolver;
    use crate::services::game_engine::structured_output::StructuredFailureStage;
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
        }
    }

    #[test]
    fn create_agent_chat_session_uses_virtual_player_when_no_player_is_configured() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        schema::create_tables(&conn).expect("create schema");
        let world = WorldRepository::new(&conn)
            .create(&WorldCreateRequest {
                name: "Helper".to_string(),
                genre: "".to_string(),
                background_prompt: "".to_string(),
                opening_scene: "Desk".to_string(),
                summary: "".to_string(),
                time_system: "".to_string(),
                map_nodes: serde_json::json!({ "version": 1, "nodes": [] }),
                triggers: vec![],
                time_config: serde_json::json!({}),
                director_config: serde_json::json!({
                    "service_mode": "agent_chat",
                    "default_agent_id": "agent-source"
                }),
                ui_theme_config: serde_json::json!({}),
                opening_messages: vec![],
                opening_character_ids: vec![],
                player_character_id: None,
            })
            .expect("create world");
        let agent = CharacterRepository::new(&conn)
            .create(
                &world.id,
                &CharacterCreateRequest {
                    name: "Agent".to_string(),
                    role: "assistant".to_string(),
                    background_prompt: String::new(),
                    model: String::new(),
                    memory_strategy: "default".to_string(),
                    recent_dialogue_rounds: 2,
                    attributes: vec![],
                    portrait_assets: vec![],
                    system_prompt_template: String::new(),
                    response_contract_prompt: String::new(),
                    narration_prompt: String::new(),
                },
            )
            .expect("create character");
        let updated_world = WorldRepository::new(&conn)
            .update(
                &world.id,
                &crate::models::world::WorldUpdateRequest {
                    name: None,
                    genre: None,
                    background_prompt: None,
                    opening_scene: None,
                    summary: None,
                    time_system: None,
                    map_nodes: None,
                    triggers: None,
                    time_config: None,
                    director_config: Some(serde_json::json!({
                        "service_mode": "agent_chat",
                        "default_agent_id": agent.id,
                    })),
                    ui_theme_config: None,
                    opening_messages: None,
                    opening_character_ids: None,
                    player_character_id: Some(None),
                },
            )
            .expect("update world");

        let session = SessionOrchestrator::create_session(&conn, &updated_world.id, None)
            .expect("create session");

        assert_eq!(session.player_character_id, agent_chat_virtual_player_id());
        assert_eq!(
            session.player_character_name,
            agent_chat_virtual_player_name()
        );
        assert_eq!(session.visible_characters, vec![agent.name.clone()]);
        assert!(session.scene.present_characters.contains(&agent.name));
        assert!(session
            .scene
            .present_characters
            .contains(&agent_chat_virtual_player_name()));
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
                role: "player".to_string(),
                content: MessageContent::Text("Check the code first.".to_string()),
                speaker: Some("Player".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 3 })),
            },
            ChatMessage {
                role: "agent".to_string(),
                content: MessageContent::Text("Alice pointed toward the warehouse.".to_string()),
                speaker: Some("Alice".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 3 })),
            },
            ChatMessage {
                role: "player".to_string(),
                content: MessageContent::Text("Was the door lock touched?".to_string()),
                speaker: Some("Player".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 4 })),
            },
            ChatMessage {
                role: "agent".to_string(),
                content: MessageContent::Text("Bob said the lock has new scratches.".to_string()),
                speaker: Some("Bob".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 4 })),
            },
            ChatMessage {
                role: "player".to_string(),
                content: MessageContent::Text("Continue tracking.".to_string()),
                speaker: Some("Player".to_string()),
                metadata: Some(serde_json::json!({ "turn_index": 5 })),
            },
            ChatMessage {
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
        assert_eq!(
            memory_context
                .get("dialogue_focus")
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(6)
        );
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
}

impl SessionOrchestrator {
    pub async fn run_director_turn(
        &self,
        llm_client: &LlmClient,
        world_director: &WorldDirectorService,
        model: ModelConfig,
        recovery: DirectorTurnRecovery,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        turn_index: i32,
        player_input: &str,
        notification_runtime: Option<NotificationToolRuntime<'_>>,
        mut progress_callback: Option<&mut (dyn FnMut(DirectorLoopStreamProgress) + Send)>,
    ) -> Result<DirectorTurnRun, StructuredOutputFailure> {
        let provider = normalize_provider_name(&model.provider);
        let tool_loop_limit = world_director.resolve_tool_loop_limit(world);

        let parsed = if recovery.resume_incomplete_turn {
            if let Some(payload) = recovery.recovered_completed_payload {
                payload
            } else {
                let prompt_call = world_director.build_runtime_prompt_call(
                    world,
                    session,
                    characters,
                    player_input,
                    "director_decision",
                    None,
                );
                let request = world_director.build_chat_request_from_prompt_call(
                    &prompt_call,
                    &model.model_id,
                    model.max_tokens,
                    model.streaming_enabled,
                );
                if let Some(callback) = progress_callback.as_deref_mut() {
                    world_director
                        .run_director_tool_loop(
                            llm_client,
                            &provider,
                            &model,
                            session,
                            world,
                            characters,
                            request,
                            tool_loop_limit,
                            turn_index,
                            notification_runtime,
                            Some(callback),
                        )
                        .await
                        .map_err(|error| {
                            build_director_transport_failure(
                                &provider,
                                &model,
                                turn_index,
                                player_input,
                                &error,
                            )
                        })?
                        .parsed
                } else {
                    world_director
                        .run_director_tool_loop(
                            llm_client,
                            &provider,
                            &model,
                            session,
                            world,
                            characters,
                            request,
                            tool_loop_limit,
                            turn_index,
                            notification_runtime,
                            None,
                        )
                        .await
                        .map_err(|error| {
                            build_director_transport_failure(
                                &provider,
                                &model,
                                turn_index,
                                player_input,
                                &error,
                            )
                        })?
                        .parsed
                }
            }
        } else {
            let prompt_call = world_director.build_runtime_prompt_call(
                world,
                session,
                characters,
                player_input,
                "director_decision",
                None,
            );
            let request = world_director.build_chat_request_from_prompt_call(
                &prompt_call,
                &model.model_id,
                model.max_tokens,
                model.streaming_enabled,
            );
            let loop_result = if let Some(callback) = progress_callback.as_deref_mut() {
                world_director
                    .run_director_tool_loop(
                        llm_client,
                        &provider,
                        &model,
                        session,
                        world,
                        characters,
                        request,
                        tool_loop_limit,
                        turn_index,
                        notification_runtime,
                        Some(callback),
                    )
                    .await
                    .map_err(|error| {
                        build_director_transport_failure(
                            &provider,
                            &model,
                            turn_index,
                            player_input,
                            &error,
                        )
                    })?
            } else {
                world_director
                    .run_director_tool_loop(
                        llm_client,
                        &provider,
                        &model,
                        session,
                        world,
                        characters,
                        request,
                        tool_loop_limit,
                        turn_index,
                        notification_runtime,
                        None,
                    )
                    .await
                    .map_err(|error| {
                        build_director_transport_failure(
                            &provider,
                            &model,
                            turn_index,
                            player_input,
                            &error,
                        )
                    })?
            };
            let raw_text = loop_result
                .traces
                .last()
                .and_then(|trace| trace.response_value.get("response"))
                .and_then(|value| value.get("content"))
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let world_character_roster = characters
                .iter()
                .map(|character| character.name.clone())
                .collect::<Vec<_>>();
            validate_director_payload(
                &loop_result.parsed,
                &session.player_character_name,
                &session.visible_characters,
                &world_character_roster,
                &provider,
                &model.model_id,
                turn_index,
                raw_text,
                None,
            )?;
            let runtime_payload = world_director.parse_runtime_payload(
                &loop_result.parsed,
                session,
                world,
                player_input,
            );
            let trace_message = loop_result.traces.last().map(build_director_trace_message);
            return Ok(DirectorTurnRun {
                parsed: loop_result.parsed,
                runtime_payload,
                traces: loop_result.traces,
                trace_message,
                model,
                provider,
                tool_loop_limit,
            });
        };
        let world_character_roster = characters
            .iter()
            .map(|character| character.name.clone())
            .collect::<Vec<_>>();
        validate_director_payload(
            &parsed,
            &session.player_character_name,
            &session.visible_characters,
            &world_character_roster,
            &provider,
            &model.model_id,
            turn_index,
            "",
            None,
        )?;
        let runtime_payload =
            world_director.parse_runtime_payload(&parsed, session, world, player_input);
        Ok(DirectorTurnRun {
            parsed,
            runtime_payload,
            traces: Vec::new(),
            trace_message: None,
            model,
            provider,
            tool_loop_limit,
        })
    }

    pub fn create_session(
        conn: &Connection,
        world_id: &str,
        player_character_id: Option<&str>,
    ) -> Result<SessionSnapshot, String> {
        let world_repo = crate::db::repositories::world_repo::WorldRepository::new(conn);
        let world = world_repo.get(world_id)?.ok_or("World not found")?;

        let char_repo = crate::db::repositories::character_repo::CharacterRepository::new(conn);
        let characters = char_repo.list_by_world(world_id)?;

        let service_config = resolve_service_runtime_config(&world);
        let use_agent_chat_virtual_player = service_config.service_mode == ServiceMode::AgentChat
            && player_character_id.is_none()
            && world
                .player_character_id
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty();
        let agent_chat_default_agent = service_config
            .default_agent_id
            .as_deref()
            .and_then(|cid| characters.iter().find(|c| c.id == cid).cloned())
            .or_else(|| characters.first().cloned());

        let (player_character_id, player_character_name, visible_chars) =
            if use_agent_chat_virtual_player {
                let agent_char = agent_chat_default_agent
                    .ok_or("agent_chat requires at least one agent character")?;
                (
                    agent_chat_virtual_player_id().to_string(),
                    agent_chat_virtual_player_name(),
                    vec![agent_char.name.clone()],
                )
            } else {
                let player_char = player_character_id
                    .and_then(|cid| characters.iter().find(|c| c.id == cid).cloned())
                    .or_else(|| {
                        world
                            .player_character_id
                            .as_deref()
                            .and_then(|cid| characters.iter().find(|c| c.id == cid).cloned())
                    })
                    .or_else(|| characters.first().cloned())
                    .ok_or("No player character found")?;
                let visible_chars = characters
                    .iter()
                    .filter(|c| c.id != player_char.id)
                    .filter(|c| world.opening_character_ids.iter().any(|id| id == &c.id))
                    .map(|c| c.name.clone())
                    .collect();
                (
                    player_char.id.clone(),
                    player_char.name.clone(),
                    visible_chars,
                )
            };

        let mut messages = Vec::new();
        for msg in &world.opening_messages {
            messages.push(ChatMessage {
                role: msg.role.clone(),
                content: MessageContent::Text(msg.content.clone()),
                speaker: msg.speaker.clone(),
                metadata: None,
            });
        }
        let present_characters = std::iter::once(player_character_name.clone())
            .chain(visible_chars.iter().cloned())
            .collect::<Vec<_>>();

        let opening_scene = world.opening_scene.trim().to_string();
        let map_topology = compile_map_topology(&world.map_nodes, &opening_scene);

        let session = SessionSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            world_name: world.name.clone(),
            location: opening_scene.clone(),
            time_label: String::new(),
            current_speaker: String::new(),
            current_line: String::new(),
            player_character_id: player_character_id.clone(),
            player_character_name: player_character_name.clone(),
            visible_characters: visible_chars,
            messages,
            player_stats: vec![],
            map_graph_nodes: map_topology.nodes,
            map_graph_edges: map_topology.edges,
            inventory_items: vec![],
            system_log: vec![],
            scene: SceneRuntime {
                scene_id: "opening".to_string(),
                name: opening_scene,
                background_hint: String::new(),
                temporary_tags: vec![],
                present_characters,
            },
            assets: AssetSelection::default(),
            state: SessionState::default(),
        };

        let session_repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
        session_repo.upsert(&session)?;

        let save = crate::models::save::SaveSummary {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session.id.clone(),
            title: format!("{} - {}", world.name, player_character_name),
            world_name: world.name.clone(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            progress: String::new(),
            summary: String::new(),
            player_character_name: Some(player_character_name),
            parent_save_id: None,
            branch_root_save_id: None,
            branch_label: None,
            turn_index: 0,
        };
        let save_repo = crate::db::repositories::save_repo::SaveRepository::new(conn);
        save_repo.upsert(&save)?;

        Ok(session)
    }

    pub fn prepare_create_session_context(
        &self,
        conn: &Connection,
        world_id: &str,
        player_character_id: Option<&str>,
    ) -> Result<SessionAssetContext, String> {
        let session = Self::create_session(conn, world_id, player_character_id)?;
        let world = resolve_world_for_session(conn, &session)?;
        let characters = crate::db::repositories::character_repo::CharacterRepository::new(conn)
            .list_by_world(&world.id)?;
        let settings = resolve_settings(conn)?;
        let image_model = resolve_default_image_model(conn, &settings)?;
        Ok(SessionAssetContext {
            session,
            world,
            characters,
            image_model,
        })
    }

    pub fn prepare_get_session_context(
        &self,
        conn: &Connection,
        session_id: &str,
    ) -> Result<SessionAssetContext, String> {
        let session = crate::db::repositories::session_repo::SessionRepository::new(conn)
            .get(session_id)?
            .ok_or_else(|| "Session not found".to_string())?;
        let world = resolve_world_for_session(conn, &session)?;
        let characters = crate::db::repositories::character_repo::CharacterRepository::new(conn)
            .list_by_world(&world.id)?;
        let settings = resolve_settings(conn)?;
        let image_model = resolve_default_image_model(conn, &settings)?;
        Ok(SessionAssetContext {
            session,
            world,
            characters,
            image_model,
        })
    }

    pub async fn resolve_session_assets(
        &self,
        asset_resolver: &AssetResolver,
        data_dir: &std::path::Path,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        image_model: Option<&ModelConfig>,
    ) -> SessionSnapshot {
        let resolved_assets = asset_resolver
            .resolve(
                data_dir,
                session,
                &session.scene,
                &session.current_speaker,
                Some(world),
                characters,
                image_model,
                None,
                world_allows_mcp_tool(world, "mcp-tool-image-generation"),
            )
            .await;
        SessionSnapshot {
            assets: resolved_assets,
            ..session.clone()
        }
    }

    pub fn persist_session_snapshot(
        &self,
        conn: &Connection,
        session: &SessionSnapshot,
    ) -> Result<(), String> {
        crate::db::repositories::session_repo::SessionRepository::new(conn).upsert(session)
    }

    pub fn load_resume_player_request(
        &self,
        conn: &Connection,
        session_id: &str,
    ) -> Result<Option<PlayerActionRequest>, String> {
        let latest_turn_index = load_latest_turn_index(conn, session_id)?;
        if latest_turn_index <= 0 {
            return Ok(None);
        }
        let recovery_journal = load_turn_journal(conn, session_id, latest_turn_index)?;
        if recovery_journal.is_empty() || journal_has_completed_step(&recovery_journal, "finished")
        {
            return Ok(None);
        }
        let created_payload = journal_payload(&recovery_journal, "created")
            .ok_or_else(|| "Missing created payload for incomplete turn".to_string())?;
        let content = created_payload
            .get("player_input")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Incomplete turn has no player input".to_string())?;
        Ok(Some(PlayerActionRequest {
            content: MessageContent::Text(content),
            action_mode: PlayerActionMode::Resend,
            resend_from_turn_index: Some(latest_turn_index),
        }))
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

    pub fn verify_retry_capsule(
        &self,
        conn: &Connection,
        session_id: &str,
        retry_token: &str,
    ) -> Result<(), String> {
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM llm_retry_capsules WHERE session_id = ?1 AND retry_token = ?2 AND status = 'active'")
            .map_err(|e| e.to_string())?;
        let count: i64 = stmt
            .query_row(params![session_id, retry_token], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if count <= 0 {
            return Err("Retry token is missing or no longer active".to_string());
        }
        Ok(())
    }

    pub fn consume_retry_capsule(
        &self,
        conn: &Connection,
        session_id: &str,
        retry_token: &str,
    ) -> Result<(), String> {
        conn.execute(
            "UPDATE llm_retry_capsules SET status = 'consumed', consumed_at = ?3 WHERE session_id = ?1 AND retry_token = ?2 AND status = 'active'",
            params![session_id, retry_token, Utc::now().to_rfc3339()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_session_runtime_attributes(
        &self,
        conn: &Connection,
        session_id: &str,
    ) -> Result<SessionRuntimeAttributesResponse, String> {
        let session_repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
        let session = session_repo
            .get(session_id)?
            .ok_or_else(|| "Session not found".to_string())?;
        let attribute_repo =
            crate::db::repositories::attribute_repo::AttributeRepository::new(conn);
        let schema_map = attribute_repo
            .list_schemas(None)?
            .into_iter()
            .map(|schema| (schema.id.clone(), schema))
            .collect::<HashMap<_, _>>();
        let session_attributes =
            attribute_repo.list_values(Some("session"), Some(session_id), None)?;
        let character_attributes = attribute_repo
            .list_values(Some("session_character"), None, None)?
            .into_iter()
            .filter(|value| value.owner_id.starts_with(&(session.id.clone() + ":")))
            .collect::<Vec<_>>();
        let runtime_session_attributes = session_attributes
            .iter()
            .filter_map(|value| build_runtime_attribute_item(value, &schema_map))
            .collect::<Vec<_>>();
        let mut grouped_values = HashMap::<String, Vec<AttributeValue>>::new();
        for value in character_attributes {
            grouped_values
                .entry(value.owner_id.clone())
                .or_default()
                .push(value);
        }
        let mut runtime_character_groups = Vec::new();
        for (owner_id, values) in grouped_values {
            let owner_label = owner_id
                .split(':')
                .nth(1)
                .map(|value| value.to_string())
                .unwrap_or_else(|| owner_id.clone());
            let items = values
                .iter()
                .filter_map(|value| build_runtime_attribute_item(value, &schema_map))
                .collect::<Vec<_>>();
            runtime_character_groups.push(RuntimeAttributeGroup {
                owner_type: "session_character".to_string(),
                owner_id,
                owner_label,
                items,
            });
        }
        Ok(SessionRuntimeAttributesResponse {
            session_attributes: vec![RuntimeAttributeGroup {
                owner_type: "session".to_string(),
                owner_id: session.id,
                owner_label: session.world_name,
                items: runtime_session_attributes,
            }],
            character_attributes: runtime_character_groups,
        })
    }

    pub fn prepare_turn_context(
        &self,
        conn: &Connection,
        session_id: &str,
        request: &PlayerActionRequest,
    ) -> Result<PreparedTurnContext, String> {
        let replay_turn_index = if request.action_mode.requires_replay() {
            let turn_index = request
                .resend_from_turn_index
                .ok_or_else(|| "Replay action requires resend_from_turn_index".to_string())?;
            if turn_index <= 0 {
                return Err("Replay action requires a positive resend_from_turn_index".to_string());
            }
            Some(turn_index)
        } else {
            if request.resend_from_turn_index.is_some() {
                return Err("Submit action does not accept resend_from_turn_index".to_string());
            }
            None
        };
        let loaded_session = {
            let session_repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
            session_repo
                .get(session_id)?
                .ok_or_else(|| "Session not found".to_string())?
        };
        let recovery_journal = if let Some(turn_index) = replay_turn_index {
            load_turn_journal(conn, session_id, turn_index)?
        } else {
            Vec::new()
        };
        let resume_incomplete_turn = replay_turn_index.is_some()
            && !recovery_journal.is_empty()
            && !journal_has_completed_step(&recovery_journal, "finished");
        let session = if let Some(turn_index) = replay_turn_index {
            if resume_incomplete_turn {
                loaded_session.clone()
            } else {
                rollback_session_to_turn(conn, &loaded_session, turn_index)?
            }
        } else {
            loaded_session
        };
        let world = resolve_world_for_session(conn, &session)?;
        let characters = {
            let char_repo = crate::db::repositories::character_repo::CharacterRepository::new(conn);
            char_repo.list_by_world(&world.id)?
        };
        let settings = resolve_settings(conn)?;
        let image_model = resolve_default_image_model(conn, &settings)?;
        let director_model = resolve_text_model(
            conn,
            world
                .director_config
                .get("director_model")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )?;
        let turn_index = next_turn_index(conn, session_id)?;
        if !journal_has_completed_step(&recovery_journal, "created") {
            append_turn_journal(
                conn,
                session_id,
                turn_index,
                "created",
                "completed",
                serde_json::json!({
                    "player_input": request.content.clone(),
                    "action_mode": request.action_mode.as_str(),
                    "player_character_name": session.player_character_name.clone(),
                }),
            )?;
        }
        if !journal_has_completed_step(&recovery_journal, "snapshot_created") {
            append_turn_journal(
                conn,
                session_id,
                turn_index,
                "snapshot_created",
                "completed",
                serde_json::json!({
                    "session_snapshot": session.clone(),
                    "attribute_values": collect_runtime_attribute_values(conn, &session.id)?,
                }),
            )?;
        }
        ensure_agent_session(
            conn,
            &session.id,
            "director",
            "director",
            None,
            None,
            "present",
            turn_index,
        )?;
        for visible_name in session
            .visible_characters
            .iter()
            .chain(std::iter::once(&session.player_character_name))
        {
            if let Some(character) = characters.iter().find(|item| item.name == *visible_name) {
                ensure_agent_session(
                    conn,
                    &session.id,
                    "character",
                    &format!("character:{}", character.id),
                    Some(character.id.as_str()),
                    Some(character.name.as_str()),
                    "present",
                    turn_index,
                )?;
            }
        }
        let effective_recovery_journal = if resume_incomplete_turn {
            recovery_journal
        } else {
            Vec::new()
        };
        let mut messages = session.messages.clone();
        messages.push(ChatMessage {
            role: "player".to_string(),
            content: request.content.clone(),
            speaker: Some(session.player_character_name.clone()),
            metadata: Some(serde_json::json!({
                "turn_index": turn_index,
                "message_kind": "player_action"
            })),
        });
        let director_completed_payload = if resume_incomplete_turn
            && journal_has_completed_step(&effective_recovery_journal, "director_completed")
        {
            journal_payload(&effective_recovery_journal, "director_completed")
                .map(|value| recovered_director_payload_to_result(&value))
        } else {
            None
        };
        Ok(PreparedTurnContext {
            session,
            world,
            characters,
            turn_index,
            recovery_journal: effective_recovery_journal,
            resume_incomplete_turn,
            image_model,
            director_model,
            messages,
            director_completed_payload,
        })
    }

    pub async fn resolve_runtime_assets(
        &self,
        asset_resolver: &AssetResolver,
        data_dir: &std::path::Path,
        updated: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        image_model: Option<&ModelConfig>,
        parsed_runtime: &serde_json::Value,
    ) -> crate::models::session::AssetSelection {
        asset_resolver
            .resolve(
                data_dir,
                updated,
                &updated.scene,
                &updated.current_speaker,
                Some(world),
                characters,
                image_model,
                Some(parsed_runtime),
                world_allows_mcp_tool(world, "mcp-tool-image-generation"),
            )
            .await
    }

    pub fn prepare_director_runtime(
        &self,
        conn: &Connection,
        director_service: &WorldDirectorService,
        recovery_journal: &[serde_json::Value],
        session_id: &str,
        turn_index: i32,
        session: &SessionSnapshot,
        world: &WorldDefinition,
        characters: &mut Vec<CharacterDefinition>,
        parsed: &serde_json::Value,
        director_runtime_payload: &ParsedDirectorRuntimePayload,
        director_trace_message: Option<&DirectorTraceMessage>,
    ) -> Result<DirectorRuntimePreparation, String> {
        let mut next_location = director_runtime_payload.next_location.clone();
        let mut next_scene_name = director_runtime_payload.next_scene_name.clone();
        let current_line = director_runtime_payload.current_line.clone();
        let mut next_scene_background_hint = director_runtime_payload
            .next_scene_background_hint
            .clone()
            .unwrap_or_else(|| session.scene.background_hint.clone());
        let next_time_label = director_runtime_payload.next_time_label.clone();
        let scene_visible_characters = director_runtime_payload.scene_visible_characters.clone();
        let scene_visible_characters_explicit = scene_visible_characters.is_some();
        let planned_speakers = director_runtime_payload.planned_speakers.clone();
        let mut visible_chars = session.visible_characters.clone();
        if scene_visible_characters.is_none() {
            for generated in &director_runtime_payload.generated_character_payloads {
                if let Some(name) = generated.get("name").and_then(|value| value.as_str()) {
                    let name = name.trim();
                    if !name.is_empty()
                        && name != session.player_character_name
                        && !visible_chars.iter().any(|item| item == name)
                    {
                        visible_chars.push(name.to_string());
                    }
                }
            }
        } else if let Some(explicit_visible) = scene_visible_characters.as_ref() {
            visible_chars = Vec::new();
            for char_name in explicit_visible {
                if !visible_chars.contains(char_name) && char_name != &session.player_character_name
                {
                    visible_chars.push(char_name.clone());
                }
            }
        }

        let mut parsed_runtime = parsed.clone();
        if let Some(object) = parsed_runtime.as_object_mut() {
            object.insert(
                "world_phase".to_string(),
                serde_json::Value::String(director_runtime_payload.world_phase.clone()),
            );
            object.insert(
                "next_location".to_string(),
                serde_json::Value::String(next_location.clone()),
            );
            object.insert(
                "next_scene_name".to_string(),
                serde_json::Value::String(next_scene_name.clone()),
            );
            if let Some(current_line) = current_line.clone() {
                object.insert(
                    "current_line".to_string(),
                    serde_json::Value::String(current_line),
                );
            }
            object.insert(
                "next_time_label".to_string(),
                serde_json::Value::String(next_time_label.clone()),
            );
            object.insert(
                "scene_visible_characters".to_string(),
                serde_json::Value::Array(
                    scene_visible_characters
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
            object.insert(
                "planned_speakers".to_string(),
                serde_json::Value::Array(
                    planned_speakers
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
            if let Some(background_hint) =
                director_runtime_payload.next_scene_background_hint.clone()
            {
                object.insert(
                    "next_scene_background_hint".to_string(),
                    serde_json::Value::String(background_hint),
                );
            }
            if !director_runtime_payload.next_scene_tags.is_empty() {
                object.insert(
                    "next_scene_tags".to_string(),
                    serde_json::Value::Array(
                        director_runtime_payload
                            .next_scene_tags
                            .iter()
                            .cloned()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
            }
            if let Some(value) = director_runtime_payload.background_asset_name.clone() {
                object.insert(
                    "background_asset_name".to_string(),
                    serde_json::Value::String(value),
                );
            }
            if let Some(value) = director_runtime_payload.background_asset_path.clone() {
                object.insert(
                    "background_asset_path".to_string(),
                    serde_json::Value::String(value),
                );
            }
            if let Some(value) = director_runtime_payload
                .background_generation_prompt
                .clone()
            {
                object.insert(
                    "background_generation_prompt".to_string(),
                    serde_json::Value::String(value),
                );
            }
            if !director_runtime_payload
                .character_visual_directives
                .is_empty()
            {
                object.insert(
                    "character_visual_directives".to_string(),
                    serde_json::Value::Array(
                        director_runtime_payload.character_visual_directives.clone(),
                    ),
                );
            }
            if let Some(value) = director_runtime_payload.switch_character_proposal.clone() {
                object.insert("switch_character_proposal".to_string(), value);
            }
        }

        let mut pre_runtime_system_messages = Vec::<ChatMessage>::new();
        if let Some(trace_message) = director_trace_message {
            pre_runtime_system_messages.push(build_director_trace_chat_message(
                trace_message,
                turn_index,
                director_runtime_payload,
                false,
            ));
        }
        let mut created_character_ids_this_turn = Vec::<String>::new();
        let mut proposal_scene_tags: Option<Vec<String>> = None;
        let switch_target_name = director_runtime_payload
            .switch_character_proposal
            .as_ref()
            .and_then(|value| value.get("target_character_name"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        for generated in director_runtime_payload
            .generated_character_payloads
            .clone()
        {
            if let Some(created) = director_service
                .create_generated_character_if_missing(conn, world, characters, &generated)?
            {
                let for_switch_character = switch_target_name
                    .as_deref()
                    .map(|name| name == created.name)
                    .unwrap_or(false);
                if created.name != session.player_character_name
                    && !visible_chars.contains(&created.name)
                {
                    visible_chars.push(created.name.clone());
                }
                if !created_character_ids_this_turn.contains(&created.id) {
                    created_character_ids_this_turn.push(created.id.clone());
                }
                pre_runtime_system_messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text(format!("character created: {}", created.name)),
                    speaker: None,
                    metadata: Some(serde_json::json!({
                        "turn_index": turn_index,
                        "action_type": "character_created",
                        "character_id": created.id,
                        "character_name": created.name,
                        "character_role": created.role,
                        "character_background_prompt": created.background_prompt,
                        "for_switch_character": for_switch_character,
                    })),
                });
            }
        }

        if let Some((creation_messages, proposal_message)) = director_service
            .materialize_switch_proposal_message(
                conn,
                world,
                session,
                characters,
                turn_index,
                director_runtime_payload.switch_character_proposal.as_ref(),
            )?
        {
            for message in &creation_messages {
                if message
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.get("action_type"))
                    .and_then(|value| value.as_str())
                    == Some("character_created")
                {
                    if let Some(character_id) = message
                        .metadata
                        .as_ref()
                        .and_then(|meta| meta.get("character_id"))
                        .and_then(|value| value.as_str())
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                    {
                        if !created_character_ids_this_turn.contains(&character_id) {
                            created_character_ids_this_turn.push(character_id);
                        }
                    }
                }
            }
            pre_runtime_system_messages.extend(creation_messages);
            if let Some(metadata) = proposal_message.metadata.as_ref() {
                if let Some(location) = metadata.get("location").and_then(|value| value.as_str()) {
                    let location = location.trim();
                    if !location.is_empty() {
                        next_location = location.to_string();
                    }
                }
                if let Some(scene_name) =
                    metadata.get("scene_name").and_then(|value| value.as_str())
                {
                    let scene_name = scene_name.trim();
                    if !scene_name.is_empty() {
                        next_scene_name = scene_name.to_string();
                    }
                }
                if let Some(scene_background_hint) = metadata
                    .get("scene_background_hint")
                    .and_then(|value| value.as_str())
                {
                    let scene_background_hint = scene_background_hint.trim();
                    if !scene_background_hint.is_empty() {
                        next_scene_background_hint = scene_background_hint.to_string();
                    }
                }
                proposal_scene_tags = metadata
                    .get("scene_tags")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str())
                            .map(|item| item.trim().to_string())
                            .filter(|item| !item.is_empty())
                            .fold(Vec::<String>::new(), |mut acc, item| {
                                if !acc.contains(&item) {
                                    acc.push(item);
                                }
                                acc
                            })
                    });
                let proposal_visible = metadata
                    .get("scene_character_roster")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str())
                            .map(|item| item.trim().to_string())
                            .filter(|item| {
                                !item.is_empty() && *item != session.player_character_name
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                visible_chars = merge_visible_characters(
                    &visible_chars,
                    proposal_visible,
                    &session.player_character_name,
                );
            }
            pre_runtime_system_messages.push(proposal_message);
        }

        if !created_character_ids_this_turn.is_empty()
            && !journal_has_completed_step(recovery_journal, "characters_created")
        {
            append_turn_journal(
                conn,
                session_id,
                turn_index,
                "characters_created",
                "completed",
                serde_json::json!({
                    "character_ids": created_character_ids_this_turn,
                }),
            )?;
        }
        if let Some(object) = parsed_runtime.as_object_mut() {
            object.insert(
                "next_location".to_string(),
                serde_json::Value::String(next_location.clone()),
            );
            object.insert(
                "next_scene_name".to_string(),
                serde_json::Value::String(next_scene_name.clone()),
            );
            object.insert(
                "next_scene_background_hint".to_string(),
                serde_json::Value::String(next_scene_background_hint.clone()),
            );
            object.insert(
                "scene_visible_characters".to_string(),
                serde_json::Value::Array(
                    visible_chars
                        .iter()
                        .filter(|name| *name != &session.player_character_name)
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
            if let Some(scene_tags) = proposal_scene_tags {
                object.insert(
                    "next_scene_tags".to_string(),
                    serde_json::Value::Array(
                        scene_tags
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
            }
        }
        Ok(DirectorRuntimePreparation {
            parsed_runtime,
            next_location,
            next_scene_name,
            current_line,
            next_scene_background_hint,
            next_time_label,
            scene_visible_characters,
            scene_visible_characters_explicit,
            planned_speakers,
            visible_chars,
            pre_runtime_system_messages,
        })
    }
}
