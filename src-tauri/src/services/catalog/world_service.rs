use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use rusqlite::Connection;

use crate::db::repositories::character_repo::CharacterRepository;
use crate::db::repositories::world_repo::WorldRepository;
use crate::models::character::{CharacterCreateRequest, CharacterDefinition};
use crate::models::session::{
    AssetSelection, CharacterVisualState, ChatMessage, MessageContent, SceneRuntime,
    SessionSnapshot, SessionState,
};
use crate::models::world::{
    BinaryFileResponse, CharacterPromptTracePreview, WorldCreateRequest, WorldDefinition,
    WorldOpeningPromptPreviewResponse, WorldUpdateRequest,
};
use crate::services::game_engine::dialogue::DialoguePipeline;
use crate::services::game_engine::director::WorldDirectorService;
use crate::services::game_engine::orchestrator::build_character_prompt_artifacts;
use crate::services::game_engine::prompting::{build_prompt_call, llm_chat_messages_to_values};
use crate::services::map_topology::compile_map_topology;
use crate::services::world_package::{ImportedWorldPackage, WorldPackageService};

pub struct WorldService {
    dialogue_pipeline: DialoguePipeline,
    director_service: WorldDirectorService,
}

impl WorldService {
    pub fn new() -> Self {
        Self {
            dialogue_pipeline: DialoguePipeline::new(),
            director_service: WorldDirectorService::new(),
        }
    }

    pub fn list_worlds(&self, conn: &Connection) -> Result<Vec<WorldDefinition>, String> {
        let repo = WorldRepository::new(conn);
        Ok(repo.list()?.into_iter().map(Self::enrich_world).collect())
    }

    pub fn get_world(&self, conn: &Connection, id: &str) -> Result<WorldDefinition, String> {
        let repo = WorldRepository::new(conn);
        repo.get(id)?
            .map(Self::enrich_world)
            .ok_or_else(|| "World not found".to_string())
    }

    pub fn create_world(
        &self,
        conn: &Connection,
        request: WorldCreateRequest,
    ) -> Result<WorldDefinition, String> {
        let repo = WorldRepository::new(conn);
        let request = WorldCreateRequest {
            director_config: Self::normalize_world_director_config(&request.director_config),
            ui_theme_config: Self::normalize_world_ui_theme_config(&request.ui_theme_config),
            ..request
        };
        repo.create(&request).map(Self::enrich_world)
    }

    pub fn update_world(
        &self,
        conn: &Connection,
        id: &str,
        request: WorldUpdateRequest,
    ) -> Result<WorldDefinition, String> {
        let repo = WorldRepository::new(conn);
        let request = WorldUpdateRequest {
            director_config: request
                .director_config
                .as_ref()
                .map(Self::normalize_world_director_config),
            ui_theme_config: request
                .ui_theme_config
                .as_ref()
                .map(Self::normalize_world_ui_theme_config),
            ..request
        };
        repo.update(id, &request).map(Self::enrich_world)
    }

    pub fn delete_world(&self, conn: &Connection, id: &str) -> Result<(), String> {
        let repo = WorldRepository::new(conn);
        repo.delete(id)
    }

    pub fn delete_all_worlds(&self, conn: &Connection) -> Result<serde_json::Value, String> {
        let repo = WorldRepository::new(conn);
        let count = repo.delete_all()?;
        Ok(serde_json::json!({ "ok": true, "deleted_count": count }))
    }

    pub fn duplicate_world(&self, conn: &Connection, id: &str) -> Result<WorldDefinition, String> {
        let world_repo = WorldRepository::new(conn);
        let char_repo = CharacterRepository::new(conn);

        let source_world = world_repo
            .get(id)?
            .ok_or_else(|| "World not found".to_string())?;
        let source_characters = char_repo.list_by_world(id)?;

        let duplicated_world = world_repo.create(&WorldCreateRequest {
            name: format!("{} (Copy)", source_world.name),
            genre: source_world.genre.clone(),
            background_prompt: source_world.background_prompt.clone(),
            opening_scene: source_world.opening_scene.clone(),
            summary: source_world.summary.clone(),
            time_system: source_world.time_system.clone(),
            map_nodes: source_world.map_nodes.clone(),
            triggers: source_world.triggers.clone(),
            custom_tabs: source_world.custom_tabs.clone(),
            world_custom_attribute_definitions: source_world
                .world_custom_attribute_definitions
                .clone(),
            character_custom_attribute_definitions: source_world
                .character_custom_attribute_definitions
                .clone(),
            time_config: source_world.time_config.clone(),
            director_config: source_world.director_config.clone(),
            ui_theme_config: Self::normalize_world_ui_theme_config(&source_world.ui_theme_config),
            opening_messages: source_world.opening_messages.clone(),
            opening_character_ids: Vec::new(),
            player_character_id: None,
        })?;

        let mut character_id_map = HashMap::new();
        for source_character in source_characters {
            let duplicated_character = char_repo.create(
                &duplicated_world.id,
                &CharacterCreateRequest {
                    name: source_character.name.clone(),
                    role: source_character.role.clone(),
                    background_prompt: source_character.background_prompt.clone(),
                    model: source_character.model.clone(),
                    memory_strategy: source_character.memory_strategy.clone(),
                    recent_dialogue_rounds: source_character.recent_dialogue_rounds,
                    attributes: source_character.attributes.clone(),
                    portrait_assets: source_character.portrait_assets.clone(),
                    custom_tabs: source_character.custom_tabs.clone(),
                    system_prompt_template: source_character.system_prompt_template.clone(),
                    response_contract_prompt: source_character.response_contract_prompt.clone(),
                    narration_prompt: source_character.narration_prompt.clone(),
                },
            )?;
            character_id_map.insert(source_character.id, duplicated_character.id);
        }

        let updated_world = world_repo.update(
            &duplicated_world.id,
            &WorldUpdateRequest {
                name: None,
                genre: None,
                background_prompt: None,
                opening_scene: None,
                summary: None,
                time_system: None,
                map_nodes: None,
                triggers: None,
                custom_tabs: None,
                world_custom_attribute_definitions: None,
                character_custom_attribute_definitions: None,
                time_config: None,
                director_config: None,
                ui_theme_config: None,
                opening_messages: None,
                opening_character_ids: Some(
                    source_world
                        .opening_character_ids
                        .iter()
                        .filter_map(|character_id| character_id_map.get(character_id).cloned())
                        .collect(),
                ),
                player_character_id: Some(
                    source_world
                        .player_character_id
                        .as_ref()
                        .and_then(|character_id| character_id_map.get(character_id).cloned()),
                ),
            },
        )?;

        Ok(Self::enrich_world(updated_world))
    }

    pub fn preview_opening_prompt(
        &self,
        conn: &Connection,
        world_id: &str,
        params: Option<serde_json::Value>,
    ) -> Result<WorldOpeningPromptPreviewResponse, String> {
        let player_character_id = params
            .as_ref()
            .and_then(|value| value.get("playerCharacterId"))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .or_else(|| {
                params
                    .as_ref()
                    .and_then(|value| value.get("player_character_id"))
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
            });
        let player_input = params
            .as_ref()
            .and_then(|value| value.get("playerInput"))
            .and_then(|value| value.as_str())
            .or_else(|| {
                params
                    .as_ref()
                    .and_then(|value| value.get("player_input"))
                    .and_then(|value| value.as_str())
            })
            .unwrap_or("continue")
            .to_string();

        let world_repo = WorldRepository::new(conn);
        let char_repo = CharacterRepository::new(conn);
        let world = Self::enrich_world(
            world_repo
                .get(world_id)?
                .ok_or_else(|| "World not found".to_string())?,
        );
        let characters = char_repo.list_by_world(world_id)?;

        let player_character =
            self.resolve_player_character(&world, &characters, player_character_id.as_deref());
        let planned_characters =
            self.resolve_opening_characters(&world, &characters, player_character.as_ref());
        let opening_messages = world
            .opening_messages
            .iter()
            .filter(|message| !message.content.trim().is_empty())
            .map(|message| ChatMessage {
                role: Self::normalize_message_role(&message.role),
                content: MessageContent::Text(message.content.clone()),
                speaker: message.speaker.clone(),
                metadata: None,
            })
            .collect::<Vec<_>>();

        let opening_session = self.build_opening_preview_session(
            &world,
            player_character.as_ref(),
            &planned_characters,
            &opening_messages,
        );

        Ok(WorldOpeningPromptPreviewResponse {
            opening_calls_llm: false,
            sample_player_input: player_input.clone(),
            planned_speakers: planned_characters
                .iter()
                .map(|character| character.name.clone())
                .collect::<Vec<_>>(),
            world_director_prompt_trace: self.director_service.build_runtime_prompt_call(
                &world,
                &opening_session,
                &characters,
                &player_input,
                "opening_preview",
                None,
            ),
            character_prompt_traces: planned_characters
                .iter()
                .map(|character| {
                    self.build_character_preview_trace(
                        &world,
                        character,
                        &opening_session,
                        &player_input,
                    )
                })
                .collect(),
            opening_messages,
            notes: vec![
                "Opening preview does not call the LLM when the session is first created; opening messages come directly from world.opening_messages.".to_string(),
                "This preview shows the first director and character prompts after the player sends the first input.".to_string(),
                "The preview includes runtime prompt assembly, recent dialogue, and visual context, but does not execute tools or write back state.".to_string(),
            ],
        })
    }

    pub fn build_world_package(
        &self,
        conn: &Connection,
        data_dir: &Path,
        world_id: &str,
    ) -> Result<BinaryFileResponse, String> {
        let world_repo = WorldRepository::new(conn);
        let char_repo = CharacterRepository::new(conn);
        let world = Self::enrich_world(
            world_repo
                .get(world_id)?
                .ok_or_else(|| "World not found".to_string())?,
        );
        let characters = char_repo.list_by_world(world_id)?;

        WorldPackageService::build_package(data_dir, &world, &characters)
    }

    pub fn import_world_package_archive(
        &self,
        conn: &Connection,
        data_dir: &Path,
        data: Vec<u8>,
    ) -> Result<WorldDefinition, String> {
        let imported = WorldPackageService::import_package_archive(data_dir, data)?;
        self.persist_imported_world_package(conn, imported)
    }

    pub fn normalize_world_director_config(raw: &serde_json::Value) -> serde_json::Value {
        let object = raw.as_object().cloned().unwrap_or_default();

        let allow_scene_transition = object
            .get("allow_scene_transition")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let allow_npc_spawn = object
            .get("allow_npc_spawn")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let history_dialogue_rounds = object
            .get("history_dialogue_rounds")
            .and_then(|value| value.as_i64())
            .map(|value| value.clamp(0, 20))
            .unwrap_or(6);
        let director_tool_loop_limit = object
            .get("director_tool_loop_limit")
            .and_then(|value| value.as_i64())
            .map(|value| value.clamp(1, 12))
            .unwrap_or(4);
        let character_memory_hit_turns = object
            .get("character_memory_hit_turns")
            .and_then(|value| value.as_i64())
            .map(|value| value.clamp(1, 6))
            .unwrap_or(2);
        let character_memory_event_window_rounds = object
            .get("character_memory_event_window_rounds")
            .and_then(|value| value.as_i64())
            .map(|value| value.clamp(0, 20))
            .unwrap_or(10);
        let character_memory_dialogue_window_rounds = object
            .get("character_memory_dialogue_window_rounds")
            .and_then(|value| value.as_i64())
            .map(|value| value.clamp(0, 6))
            .unwrap_or(2);
        let character_memory_retrieval_mode = object
            .get("character_memory_retrieval_mode")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| matches!(value.as_str(), "lexical_only" | "hybrid" | "semantic_only"))
            .unwrap_or_else(|| "hybrid".to_string());
        let character_memory_candidate_limit = object
            .get("character_memory_candidate_limit")
            .and_then(|value| value.as_i64())
            .map(|value| value.clamp(20, 600))
            .unwrap_or(200);
        let character_memory_semantic_weight = object
            .get("character_memory_semantic_weight")
            .and_then(|value| value.as_f64())
            .map(|value| value.clamp(0.0, 1.0))
            .unwrap_or(0.65);
        let director_tool_call_limit = object
            .get("director_tool_call_limit")
            .and_then(|value| value.as_i64())
            .map(|value| value.clamp(1, 8))
            .unwrap_or(4);
        let director_tool_loop_termination = object
            .get("director_tool_loop_termination")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| value == "tool_calls_present")
            .unwrap_or_else(|| "tool_calls_present".to_string());
        let director_stage_labels = object
            .get("director_stage_labels")
            .and_then(|value| value.as_object())
            .map(|labels| {
                serde_json::json!({
                    "default_turn": labels
                        .get("default_turn")
                        .and_then(|value| value.as_str())
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| "Default Turn".to_string()),
                    "tool_loop_turn": labels
                        .get("tool_loop_turn")
                        .and_then(|value| value.as_str())
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| "Tool Loop Turn".to_string()),
                })
            })
            .unwrap_or_else(|| {
                serde_json::json!({
                    "default_turn": "Default Turn",
                    "tool_loop_turn": "Tool Loop Turn",
                })
            });
        let world_director_prompt = object
            .get("world_director_prompt")
            .and_then(|value| value.as_str())
            .map(Self::resolve_world_director_prompt)
            .unwrap_or_else(Self::minimal_world_director_prompt);
        let director_model = object
            .get("director_model")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_default();
        let prompt_presets = object
            .get("prompt_presets")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let return_processing_rules = object
            .get("return_processing_rules")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let allowed_mcp_tool_ids = object
            .get("allowed_mcp_tool_ids")
            .and_then(|value| value.as_array())
            .map(|items| {
                let mut seen = BTreeSet::new();
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(|value| value.trim().to_string()))
                    .filter(|value| !value.is_empty() && seen.insert(value.clone()))
                    .map(serde_json::Value::String)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        serde_json::json!({
            "allow_scene_transition": allow_scene_transition,
            "allow_npc_spawn": allow_npc_spawn,
            "history_dialogue_rounds": history_dialogue_rounds,
            "director_tool_loop_limit": director_tool_loop_limit,
            "character_memory_hit_turns": character_memory_hit_turns,
            "character_memory_event_window_rounds": character_memory_event_window_rounds,
            "character_memory_dialogue_window_rounds": character_memory_dialogue_window_rounds,
            "character_memory_retrieval_mode": character_memory_retrieval_mode,
            "character_memory_candidate_limit": character_memory_candidate_limit,
            "character_memory_semantic_weight": character_memory_semantic_weight,
            "director_tool_call_limit": director_tool_call_limit,
            "director_tool_loop_termination": director_tool_loop_termination,
            "director_stage_labels": director_stage_labels,
            "director_model": director_model,
            "world_director_prompt": world_director_prompt,
            "prompt_presets": prompt_presets,
            "return_processing_rules": return_processing_rules,
            "allowed_mcp_tool_ids": allowed_mcp_tool_ids,
        })
    }

    pub fn normalize_world_ui_theme_config(raw: &serde_json::Value) -> serde_json::Value {
        let mut object = raw.as_object().cloned().unwrap_or_default();
        let desktop_file = object
            .get("desktop_file")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(Self::default_desktop_ui_file);
        let mobile_file = object
            .get("mobile_file")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(Self::default_mobile_ui_file);
        let empty = serde_json::json!({});
        let assets = Self::normalize_world_ui_assets_config(object.get("assets").unwrap_or(&empty));

        object.insert("assets".to_string(), assets);
        object.insert(
            "desktop_file".to_string(),
            serde_json::Value::String(desktop_file),
        );
        object.insert(
            "mobile_file".to_string(),
            serde_json::Value::String(mobile_file),
        );

        serde_json::Value::Object(object)
    }

    pub fn normalize_world_ui_assets_config(raw: &serde_json::Value) -> serde_json::Value {
        let mut object = raw.as_object().cloned().unwrap_or_default();
        let background_source_mode = Self::normalize_asset_source_mode(
            object
                .get("background_source_mode")
                .and_then(|value| value.as_str()),
            "local-first",
        );
        let portrait_source_mode = Self::normalize_asset_source_mode(
            object
                .get("portrait_source_mode")
                .and_then(|value| value.as_str()),
            "local-first",
        );
        let runtime_image_generation_enabled = object
            .get("runtime_image_generation_enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        object.insert(
            "background_source_mode".to_string(),
            serde_json::Value::String(background_source_mode),
        );
        object.insert(
            "portrait_source_mode".to_string(),
            serde_json::Value::String(portrait_source_mode),
        );
        object.insert(
            "runtime_image_generation_enabled".to_string(),
            serde_json::Value::Bool(runtime_image_generation_enabled),
        );
        if !object.contains_key("local_background_assets") {
            object.insert(
                "local_background_assets".to_string(),
                serde_json::Value::Array(vec![]),
            );
        }
        if !object.contains_key("local_scene_backgrounds") {
            object.insert("local_scene_backgrounds".to_string(), serde_json::json!({}));
        }
        serde_json::Value::Object(object)
    }

    pub fn enrich_world(mut world: WorldDefinition) -> WorldDefinition {
        let normalized = Self::normalize_world_director_config(&world.director_config);
        world.director_runtime_system_prompt = normalized
            .get("world_director_prompt")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        world.director_system_prompt_base = String::new();
        world.director_config = normalized;
        world.ui_theme_config = Self::normalize_world_ui_theme_config(&world.ui_theme_config);
        world
    }

    fn minimal_world_director_prompt() -> String {
        r#"You are the world director.
Return exactly one JSON object describing only the next state changes that are needed for this turn.
Typical fields include planned_speakers, tool_calls, switch_character_proposal, world_phase,
next_scene_name, next_location, next_time_label, scene_visible_characters, current_line,
next_scene_background_hint, next_scene_tags, and character_visual_directives.

Rules:
1. Return JSON only.
2. Omit unchanged fields.
3. current_scene_character_roster and scene_visible_characters describe only characters present in the current scene.
4. planned_speakers should usually include one or more NPC speakers for the current turn.
5. Keep the current scene and location unless a transition is justified.
6. Do not emit state_tags, system_messages, or system_log."#
            .to_string()
    }

    fn resolve_world_director_prompt(prompt: &str) -> String {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            Self::minimal_world_director_prompt()
        } else {
            trimmed.to_string()
        }
    }

    fn default_desktop_ui_file() -> String {
        include_str!("../../db/seeds/assets/default-desktop-ui.jsonc").to_string()
    }

    fn default_mobile_ui_file() -> String {
        include_str!("../../db/seeds/assets/default-mobile-ui.jsonc").to_string()
    }

    fn persist_imported_world_package(
        &self,
        conn: &Connection,
        imported: ImportedWorldPackage,
    ) -> Result<WorldDefinition, String> {
        let world_repo = WorldRepository::new(conn);
        let char_repo = CharacterRepository::new(conn);

        let imported_world = imported.world;
        let desktop_ui_source = imported.desktop_ui_source;
        let mobile_ui_source = imported.mobile_ui_source;
        let imported_characters = imported.characters;
        let asset_map = imported.asset_map;

        let created_world = world_repo.create(&WorldCreateRequest {
            name: imported_world.name.clone(),
            genre: imported_world.genre.clone(),
            background_prompt: imported_world.background_prompt.clone(),
            opening_scene: imported_world.opening_scene.clone(),
            summary: imported_world.summary.clone(),
            time_system: imported_world.time_system.clone(),
            map_nodes: imported_world.map_nodes.clone(),
            triggers: imported_world.triggers.clone(),
            custom_tabs: imported_world.custom_tabs.clone(),
            world_custom_attribute_definitions: imported_world
                .world_custom_attribute_definitions
                .clone(),
            character_custom_attribute_definitions: imported_world
                .character_custom_attribute_definitions
                .clone(),
            time_config: imported_world.time_config.clone(),
            director_config: Self::normalize_world_director_config(&imported_world.director_config),
            ui_theme_config: Self::normalize_world_ui_theme_config(&serde_json::json!({
                "assets": WorldPackageService::remap_world_ui_theme_assets(
                    Self::normalize_world_ui_assets_config(&imported_world.ui_assets_config),
                    &asset_map,
                ),
                "desktop_file": desktop_ui_source,
                "mobile_file": mobile_ui_source,
            })),
            opening_messages: imported_world.opening_messages.clone(),
            opening_character_ids: Vec::new(),
            player_character_id: None,
        })?;

        let mut id_map = HashMap::new();
        let mut name_map = HashMap::new();
        for character in imported_characters {
            let source_character_id = if character.source_character_id.trim().is_empty() {
                character.name.clone()
            } else {
                character.source_character_id.clone()
            };
            let mut character = character;
            character.portrait_assets = character
                .portrait_assets
                .iter()
                .map(|path| asset_map.get(path).cloned().unwrap_or_else(|| path.clone()))
                .collect();
            let created_character = char_repo.create(
                &created_world.id,
                &CharacterCreateRequest {
                    name: character.name.clone(),
                    role: character.role.clone(),
                    background_prompt: character.background_prompt.clone(),
                    model: character.model.clone(),
                    memory_strategy: character.memory_strategy.clone(),
                    recent_dialogue_rounds: character.recent_dialogue_rounds,
                    attributes: character.attributes.clone(),
                    portrait_assets: character.portrait_assets.clone(),
                    custom_tabs: character.custom_tabs.clone(),
                    system_prompt_template: character.system_prompt_template.clone(),
                    response_contract_prompt: character.response_contract_prompt.clone(),
                    narration_prompt: character.narration_prompt.clone(),
                },
            )?;
            let created_character_id = created_character.id.clone();
            id_map.insert(source_character_id, created_character_id.clone());
            name_map.insert(character.name.clone(), created_character_id);
        }

        let opening_character_ids = if !imported_world.opening_character_source_ids.is_empty() {
            imported_world
                .opening_character_source_ids
                .iter()
                .filter_map(|id| id_map.get(id).cloned())
                .collect()
        } else {
            imported_world
                .opening_character_names
                .iter()
                .filter_map(|name| name_map.get(name).cloned())
                .collect()
        };

        let player_character_id =
            if let Some(source_id) = &imported_world.player_character_source_id {
                id_map.get(source_id).cloned()
            } else {
                imported_world
                    .player_character_name
                    .as_ref()
                    .and_then(|name| name_map.get(name).cloned())
            };

        let updated_world = world_repo.update(
            &created_world.id,
            &WorldUpdateRequest {
                name: None,
                genre: None,
                background_prompt: None,
                opening_scene: None,
                summary: None,
                time_system: None,
                map_nodes: None,
                triggers: None,
                custom_tabs: None,
                world_custom_attribute_definitions: None,
                character_custom_attribute_definitions: None,
                time_config: None,
                director_config: None,
                ui_theme_config: None,
                opening_messages: None,
                opening_character_ids: Some(opening_character_ids),
                player_character_id: Some(player_character_id),
            },
        )?;

        Ok(Self::enrich_world(updated_world))
    }

    fn resolve_player_character(
        &self,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        requested_player_character_id: Option<&str>,
    ) -> Option<CharacterDefinition> {
        let target_id = requested_player_character_id
            .or(world.player_character_id.as_deref())
            .unwrap_or_default();
        characters
            .iter()
            .find(|character| character.id == target_id)
            .cloned()
            .or_else(|| characters.first().cloned())
    }

    fn resolve_opening_characters(
        &self,
        world: &WorldDefinition,
        characters: &[CharacterDefinition],
        player_character: Option<&CharacterDefinition>,
    ) -> Vec<CharacterDefinition> {
        let player_id = player_character.map(|character| character.id.as_str());
        let mut selected = Vec::new();
        for character_id in &world.opening_character_ids {
            if Some(character_id.as_str()) == player_id {
                continue;
            }
            if let Some(character) = characters
                .iter()
                .find(|character| &character.id == character_id)
            {
                selected.push(character.clone());
            }
        }
        selected
    }

    fn build_opening_preview_session(
        &self,
        world: &WorldDefinition,
        player_character: Option<&CharacterDefinition>,
        planned_characters: &[CharacterDefinition],
        opening_messages: &[ChatMessage],
    ) -> SessionSnapshot {
        let opening_scene = Self::normalize_opening_scene(world);
        let player_character_id = player_character
            .map(|character| character.id.clone())
            .unwrap_or_default();
        let player_character_name = player_character
            .map(|character| character.name.clone())
            .unwrap_or_else(|| "Player".to_string());
        let visible_characters = planned_characters
            .iter()
            .map(|character| character.name.clone())
            .collect::<Vec<_>>();
        let map_topology = compile_map_topology(&world.map_nodes, &opening_scene);

        SessionSnapshot {
            id: format!("preview-{}", world.id),
            world_name: world.name.clone(),
            location: opening_scene.clone(),
            time_label: Self::resolve_preview_time_label(world),
            current_speaker: opening_messages
                .iter()
                .rev()
                .find_map(|message| message.speaker.clone())
                .unwrap_or_default(),
            current_line: opening_messages
                .iter()
                .rev()
                .find_map(|message| {
                    let content = message.content.trim();
                    if content.is_empty() {
                        None
                    } else {
                        Some(content.to_string())
                    }
                })
                .unwrap_or_default(),
            player_character_id,
            player_character_name,
            visible_characters: visible_characters.clone(),
            messages: opening_messages.to_vec(),
            player_stats: Vec::new(),
            map_graph_nodes: map_topology.nodes,
            map_graph_edges: map_topology.edges,
            inventory_items: Vec::new(),
            system_log: Vec::new(),
            scene: SceneRuntime {
                scene_id: Self::slugify_scene_id(&opening_scene),
                name: opening_scene.clone(),
                background_hint: opening_scene,
                temporary_tags: Vec::new(),
                present_characters: visible_characters,
            },
            assets: AssetSelection {
                background_hint: String::new(),
                active_speaker_portrait: String::new(),
                background_asset_path: None,
                active_speaker_portrait_path: None,
                background_generation_prompt: String::new(),
                active_speaker_generation_prompt: String::new(),
                visible_character_portraits: planned_characters
                    .iter()
                    .map(|character| CharacterVisualState {
                        character_name: character.name.clone(),
                        portrait_hint: character.name.clone(),
                        portrait_asset_path: character.portrait_assets.first().cloned(),
                        generation_prompt: String::new(),
                    })
                    .collect(),
            },
            state: SessionState {
                metrics: HashMap::new(),
                tags: Vec::new(),
                phase: "opening".to_string(),
            },
        }
    }

    fn build_character_preview_trace(
        &self,
        world: &WorldDefinition,
        character: &CharacterDefinition,
        session: &SessionSnapshot,
        player_input: &str,
    ) -> CharacterPromptTracePreview {
        let recent_messages = Self::slice_character_history(
            &session.messages,
            character.recent_dialogue_rounds,
            Some(session.player_character_name.as_str()),
        );
        let artifacts = build_character_prompt_artifacts(
            &self.dialogue_pipeline,
            world,
            &character.name,
            Some(character),
            session,
            &session.player_character_name,
            &session.location,
            &session.scene.name,
            player_input,
            &recent_messages,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &session.scene.name,
            &session.location,
            &session.visible_characters,
        );
        CharacterPromptTracePreview {
            speaker: Some(character.name.clone()),
            prompt_trace: build_prompt_call(
                "prompt_call_v2",
                "character",
                &character.name,
                "opening_preview",
                "Preview the first character response after opening messages",
                &artifacts.system_prompt,
                &artifacts.user_prompt,
                llm_chat_messages_to_values(&artifacts.messages),
                artifacts.modules,
                artifacts.response_contract,
                serde_json::json!({
                    "preview": true,
                    "speaker": character.name,
                    "role": character.role,
                    "background_prompt": character.background_prompt,
                    "runtime_system_prompt": character.runtime_system_prompt,
                    "scene_name": session.scene.name,
                    "location": session.location,
                    "init_payload": artifacts.init_payload,
                    "turn_payload": artifacts.turn_payload,
                    "scene_state": artifacts.scene_state,
                    "visibility_context": artifacts.visibility_context,
                }),
            ),
        }
    }

    fn slice_character_history(
        messages: &[ChatMessage],
        previous_rounds: i32,
        current_player_name: Option<&str>,
    ) -> Vec<ChatMessage> {
        let max_previous_rounds = previous_rounds.max(0);
        let dialogue_messages = messages
            .iter()
            .filter(|message| {
                !message.content.trim().is_empty()
                    && (message.role == "player"
                        || message.role == "agent"
                        || Self::is_player_message(message, current_player_name))
            })
            .cloned()
            .collect::<Vec<_>>();
        if dialogue_messages.is_empty() {
            return Vec::new();
        }

        let mut selected = Vec::new();
        let mut player_messages_seen = 0;
        for message in dialogue_messages.into_iter().rev() {
            let is_player =
                message.role == "player" || Self::is_player_message(&message, current_player_name);
            selected.push(message);
            if is_player {
                player_messages_seen += 1;
                if player_messages_seen > max_previous_rounds {
                    break;
                }
            }
        }
        selected.reverse();
        selected
    }

    fn is_player_message(message: &ChatMessage, current_player_name: Option<&str>) -> bool {
        current_player_name
            .map(|player_name| {
                message
                    .speaker
                    .as_deref()
                    .map(|speaker| speaker.trim() == player_name.trim())
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    fn normalize_asset_source_mode(value: Option<&str>, fallback: &str) -> String {
        let mode = value
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| fallback.to_string());
        match mode.as_str() {
            "generated-first" | "generated-only" | "local-only" | "local-first" => mode,
            _ => fallback.to_string(),
        }
    }

    fn resolve_preview_time_label(world: &WorldDefinition) -> String {
        if let Some(label) = world
            .time_config
            .get("start_label")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return label;
        }

        if let Some(label) = world
            .time_config
            .get("labels")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return label;
        }

        "Opening".to_string()
    }

    fn normalize_opening_scene(world: &WorldDefinition) -> String {
        let scene = world.opening_scene.trim();
        if scene.is_empty() {
            "Opening".to_string()
        } else {
            scene.to_string()
        }
    }

    fn normalize_message_role(role: &str) -> String {
        match role.trim().to_ascii_lowercase().as_str() {
            "assistant" | "npc" => "agent".to_string(),
            "user" => "player".to_string(),
            "system" => "system".to_string(),
            "player" | "agent" => role.trim().to_ascii_lowercase(),
            _ => role.trim().to_ascii_lowercase(),
        }
    }

    fn slugify_scene_id(value: &str) -> String {
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
        normalized.trim_matches('-').to_string()
    }
}
