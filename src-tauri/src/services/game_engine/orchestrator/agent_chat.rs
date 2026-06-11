use rusqlite::Connection;

use crate::models::character::CharacterDefinition;
use crate::models::model_config::ModelConfig;
use crate::models::session::*;
use crate::models::world::WorldDefinition;
use crate::services::assets::resolver::AssetResolver;
use crate::services::game_engine::dialogue::DialoguePipeline;
use crate::services::game_engine::memory::MemoryService;
use crate::services::game_engine::runtime_effects::DirectorRuntimeApplication;
use crate::services::game_engine::service_mode::{
    agent_chat_virtual_player_id, agent_chat_virtual_player_name, ServiceRuntimeConfig,
};
use crate::services::llm::client::LlmClient;
use crate::services::notifications::NotificationToolRuntime;

use super::run::*;
use super::writeback::*;

pub(crate) struct AgentChatTurnInput<'a> {
    pub service_config: &'a ServiceRuntimeConfig,
    pub asset_resolver: &'a AssetResolver,
    pub data_dir: &'a std::path::Path,
    pub session: &'a SessionSnapshot,
    pub world: &'a WorldDefinition,
    pub characters: &'a [CharacterDefinition],
    pub turn_index: i32,
    pub messages: Vec<ChatMessage>,
    pub speaker_messages: &'a [ChatMessage],
    pub image_model: Option<&'a ModelConfig>,
}

pub(crate) struct AgentChatWritebackInput<'a> {
    pub conn: &'a Connection,
    pub recovery_journal: &'a [serde_json::Value],
    pub session_id: &'a str,
    pub turn_index: i32,
    pub runtime_application: &'a DirectorRuntimeApplication,
    pub updated: &'a SessionSnapshot,
    pub service_config: &'a ServiceRuntimeConfig,
    pub agent: &'a CharacterDefinition,
}

pub(crate) struct AgentChatTarget {
    pub agent: CharacterDefinition,
    pub planned_speakers: Vec<String>,
    pub visible_chars: Vec<String>,
    pub next_scene_name: String,
    pub next_location: String,
}

impl SessionOrchestrator {
    pub(crate) fn prepare_agent_chat_target(
        &self,
        service_config: &ServiceRuntimeConfig,
        session: &SessionSnapshot,
        characters: &[CharacterDefinition],
    ) -> Result<AgentChatTarget, String> {
        let agent = resolve_agent_character(service_config, session, characters)?;
        let visible_chars = vec![agent.name.clone()];
        let next_scene_name = if session.scene.name.trim().is_empty() {
            session.location.clone()
        } else {
            session.scene.name.clone()
        };
        let next_location = if session.location.trim().is_empty() {
            next_scene_name.clone()
        } else {
            session.location.clone()
        };

        Ok(AgentChatTarget {
            planned_speakers: vec![agent.name.clone()],
            visible_chars,
            next_scene_name,
            next_location,
            agent,
        })
    }

    pub(crate) async fn build_agent_chat_updated_session(
        &self,
        input: AgentChatTurnInput<'_>,
    ) -> SessionSnapshot {
        let current_speaker = input
            .speaker_messages
            .iter()
            .rev()
            .find(|message| message.role == "agent")
            .and_then(|message| message.speaker.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                input
                    .service_config
                    .default_agent_id
                    .as_deref()
                    .and_then(|id| input.characters.iter().find(|character| character.id == id))
                    .map(|character| character.name.clone())
            })
            .unwrap_or_else(|| input.session.current_speaker.clone());
        let current_line = input
            .speaker_messages
            .iter()
            .rev()
            .filter(|message| message.role == "agent")
            .filter_map(|message| {
                message
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("narration"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
            .next()
            .or_else(|| {
                input
                    .speaker_messages
                    .iter()
                    .rev()
                    .find(|message| message.role == "agent" && !message.content.trim().is_empty())
                    .map(|message| message.content.trim())
            })
            .unwrap_or_else(|| input.session.current_line.clone());
        let target = self
            .prepare_agent_chat_target(input.service_config, input.session, input.characters)
            .unwrap_or_else(|_| AgentChatTarget {
                agent: input
                    .characters
                    .first()
                    .cloned()
                    .unwrap_or_else(|| fallback_agent_character(input.world)),
                planned_speakers: vec![current_speaker.clone()],
                visible_chars: vec![current_speaker.clone()],
                next_scene_name: input.session.scene.name.clone(),
                next_location: input.session.location.clone(),
            });
        let present_characters =
            build_turn_participants(&target.visible_chars, &input.session.player_character_name);
        let mut system_log = input.session.system_log.clone();
        system_log.push(format!(
            "Turn {}: mode=agent_chat, agent={}",
            input.turn_index, target.agent.name
        ));
        let updated = SessionSnapshot {
            id: input.session.id.clone(),
            world_name: input.session.world_name.clone(),
            location: target.next_location.clone(),
            time_label: input.session.time_label.clone(),
            current_speaker,
            current_line,
            player_character_id: input.session.player_character_id.clone(),
            player_character_name: input.session.player_character_name.clone(),
            visible_characters: target.visible_chars.clone(),
            messages: input.messages,
            player_stats: input.session.player_stats.clone(),
            map_graph_nodes: input.session.map_graph_nodes.clone(),
            map_graph_edges: input.session.map_graph_edges.clone(),
            inventory_items: input.session.inventory_items.clone(),
            system_log,
            scene: SceneRuntime {
                scene_id: if input.session.scene.scene_id.trim().is_empty() {
                    slugify_agent_scene_id(&target.next_scene_name)
                } else {
                    input.session.scene.scene_id.clone()
                },
                name: target.next_scene_name,
                background_hint: input.session.scene.background_hint.clone(),
                temporary_tags: input.session.scene.temporary_tags.clone(),
                present_characters,
            },
            assets: input.session.assets.clone(),
            state: input.session.state.clone(),
        };

        self.resolve_session_assets(
            input.asset_resolver,
            input.data_dir,
            &updated,
            input.world,
            input.characters,
            input.image_model,
        )
        .await
    }

    pub(crate) fn ensure_agent_chat_runtime_session(
        &self,
        conn: &Connection,
        session_id: &str,
        turn_index: i32,
        _service_config: &ServiceRuntimeConfig,
        agent: &CharacterDefinition,
    ) -> Result<String, String> {
        let runtime_key = format!("character:{}", agent.id);
        ensure_agent_session(
            conn,
            session_id,
            "character",
            &runtime_key,
            Some(agent.id.as_str()),
            Some(agent.name.as_str()),
            "present",
            turn_index,
        )?;
        Ok(runtime_key)
    }

    pub(crate) fn normalize_agent_chat_player_identity(
        &self,
        session: &SessionSnapshot,
        messages: Vec<ChatMessage>,
        agent: &CharacterDefinition,
    ) -> (SessionSnapshot, Vec<ChatMessage>) {
        if session.player_character_id != agent.id && session.player_character_name != agent.name {
            return (session.clone(), messages);
        }

        let mut normalized_session = session.clone();
        normalized_session.player_character_id = agent_chat_virtual_player_id().to_string();
        normalized_session.player_character_name = agent_chat_virtual_player_name();
        normalized_session.visible_characters = vec![agent.name.clone()];
        normalized_session.scene.present_characters = build_turn_participants(
            &normalized_session.visible_characters,
            &normalized_session.player_character_name,
        );
        let mut normalized_messages = messages;
        for message in &mut normalized_messages {
            if message.role == "player" {
                message.speaker = Some(normalized_session.player_character_name.clone());
            }
        }
        (normalized_session, normalized_messages)
    }

    pub(crate) fn writeback_agent_chat_turn(
        &self,
        input: AgentChatWritebackInput<'_>,
    ) -> Result<(), String> {
        crate::db::repositories::session_repo::SessionRepository::new(input.conn)
            .upsert(input.updated)?;
        if !journal_has_completed_step(input.recovery_journal, "agent_chat_completed") {
            append_turn_journal(
                input.conn,
                input.session_id,
                input.turn_index,
                "agent_chat_completed",
                "completed",
                serde_json::json!({
                    "service_mode": "agent_chat",
                    "agent_id": input.agent.id,
                    "agent_name": input.agent.name,
                    "memory_write_mode": format!("{:?}", input.service_config.memory_write_mode),
                    "message_count": input.updated.messages.len(),
                }),
            )?;
        }
        append_finished_journal(
            input.conn,
            input.recovery_journal,
            input.session_id,
            input.turn_index,
            input.updated,
        )?;
        let runtime_key = format!("character:{}", input.agent.id);
        if !has_agent_checkpoint(
            input.conn,
            input.session_id,
            &runtime_key,
            input.turn_index,
            "turn_state",
        )? {
            record_agent_checkpoint(
                input.conn,
                input.session_id,
                &runtime_key,
                input.turn_index,
                "turn_state",
                serde_json::json!({
                    "session_snapshot": input.updated.clone(),
                    "phase": "finished",
                    "service_mode": "agent_chat",
                    "runtime_effects": {
                        "memory_entries_count": input.runtime_application.memory_entries.len(),
                        "system_messages_count": input.runtime_application.system_messages.len(),
                    }
                }),
            )?;
        }
        Ok(())
    }
}

fn resolve_agent_character(
    service_config: &ServiceRuntimeConfig,
    session: &SessionSnapshot,
    characters: &[CharacterDefinition],
) -> Result<CharacterDefinition, String> {
    if let Some(agent_id) = service_config.default_agent_id.as_deref() {
        if let Some(character) = characters.iter().find(|character| character.id == agent_id) {
            return Ok(character.clone());
        }
    }
    characters
        .iter()
        .find(|character| {
            character.id != session.player_character_id
                && character.name != session.player_character_name
        })
        .cloned()
        .or_else(|| characters.first().cloned())
        .ok_or_else(|| "agent_chat requires a default agent character".to_string())
}

fn fallback_agent_character(world: &WorldDefinition) -> CharacterDefinition {
    CharacterDefinition {
        id: "agent-chat-fallback".to_string(),
        name: "Agent".to_string(),
        world_id: world.id.clone(),
        role: "assistant".to_string(),
        background_prompt: String::new(),
        model: String::new(),
        memory_strategy: String::new(),
        recent_dialogue_rounds: 2,
        attributes: Vec::new(),
        portrait_assets: Vec::new(),
        avatar_asset: String::new(),
        system_prompt_template: String::new(),
        response_contract_prompt: String::new(),
        narration_prompt: String::new(),
        runtime_system_prompt: String::new(),
    }
}

fn slugify_agent_scene_id(value: &str) -> String {
    let mut normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while normalized.contains("--") {
        normalized = normalized.replace("--", "-");
    }
    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        "agent-chat".to_string()
    } else {
        normalized
    }
}

pub(crate) async fn run_agent_chat_speaker_turn(
    orchestrator: &SessionOrchestrator,
    db: &tokio::sync::Mutex<crate::db::Database>,
    llm_client: &LlmClient,
    dialogue_pipeline: &DialoguePipeline,
    memory_service: &MemoryService,
    session_id: &str,
    turn_index: i32,
    recovery_journal: &[serde_json::Value],
    session: &SessionSnapshot,
    world: &WorldDefinition,
    characters: &[CharacterDefinition],
    messages: Vec<ChatMessage>,
    target: &AgentChatTarget,
    player_input: &str,
    notification_runtime: Option<NotificationToolRuntime<'_>>,
    progress_callback: Option<&mut (dyn FnMut(SpeakerTurnProgress) + Send)>,
) -> Result<SpeakerTurnRunResult, String> {
    orchestrator
        .run_speaker_turns(
            db,
            llm_client,
            dialogue_pipeline,
            memory_service,
            session_id,
            turn_index,
            recovery_journal,
            session,
            world,
            characters,
            messages,
            &target.planned_speakers,
            player_input,
            &target.next_scene_name,
            &target.next_location,
            &target.visible_chars,
            notification_runtime,
            progress_callback,
        )
        .await
}
