use crate::models::character::CharacterDefinition;
use crate::models::model_config::ModelConfig;
use crate::models::session::*;
use crate::models::world::WorldDefinition;
use crate::services::assets::resolver::AssetResolver;
use crate::services::game_engine::service_mode::{
    agent_chat_virtual_player_id, agent_chat_virtual_player_name, resolve_service_runtime_config,
    ServiceMode,
};
use crate::services::map_topology::{compile_map_topology, resolve_map_label};
use rusqlite::Connection;

use super::request_building::*;
use super::run::*;
use super::turn_context::*;
use super::writeback::*;

impl SessionOrchestrator {
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
        let use_agent_chat_virtual_player = service_config.service_mode == ServiceMode::AgentChat;
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
                message_id: ChatMessage::generate_id(),
                created_at: chrono::Utc::now().to_rfc3339(),
                parent_message_id: None,
                role: msg.role.clone(),
                content: MessageContent::Text(msg.content.clone()),
                speaker: msg.speaker.clone(),
                metadata: None,
            });
        }
        let present_characters = std::iter::once(player_character_name.clone())
            .chain(visible_chars.iter().cloned())
            .collect::<Vec<_>>();

        let opening_scene = resolve_map_label(&world.map_nodes, &world.opening_scene)
            .unwrap_or_else(|| world.opening_scene.trim().to_string());
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
            inventory_items: world
                .ui_theme_config
                .get("initial_inventory_items")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .unwrap_or_default(),
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
            // 新存档不覆盖任何采样参数：走世界与应用两层。
            generation_params: Default::default(),
        };

        let session_repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
        session_repo.upsert(&session)?;

        let package_attribute_schemas = world
            .ui_theme_config
            .get("attribute_schemas")
            .cloned()
            .and_then(|value| serde_json::from_value::<Vec<crate::models::attribute::AttributeSchemaCreateRequest>>(value).ok())
            .unwrap_or_default();
        if !package_attribute_schemas.is_empty() {
            let attribute_repo = crate::db::repositories::attribute_repo::AttributeRepository::new(conn);
            let registered = attribute_repo.list_schemas(None)?;
            for declared in package_attribute_schemas {
                if declared.default_value.is_null() {
                    continue;
                }
                let Some(schema) = registered.iter().find(|schema| {
                    schema.scope.trim() == declared.scope.trim()
                        && schema.key.trim() == declared.key.trim()
                }) else {
                    return Err(format!("World attribute schema is not registered: {}", declared.key));
                };
                let (owner_type, owner_id) = match schema.scope.as_str() {
                    "session" => ("session", session.id.clone()),
                    "session_character" => (
                        "session_character",
                        format!("{}:{}", session.id, player_character_id),
                    ),
                    _ => continue,
                };
                attribute_repo.upsert_value(&crate::models::attribute::AttributeValueUpsertRequest {
                    schema_id: schema.id.clone(),
                    owner_type: owner_type.to_string(),
                    owner_id,
                    value: schema.default_value.clone(),
                    source: "world_package_default".to_string(),
                })?;
            }
        }

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
        let session_repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
        let mut session = session_repo
            .get(session_id)?
            .ok_or_else(|| "Session not found".to_string())?;
        let world = resolve_world_for_session(conn, &session)?;
        let canonical_location = resolve_map_label(&world.map_nodes, &session.location)
            .unwrap_or_else(|| session.location.trim().to_string());
        let map_topology = compile_map_topology(&world.map_nodes, &canonical_location);
        let needs_map_repair = session.location != canonical_location
            || session
                .map_graph_nodes
                .iter()
                .filter(|node| node.current)
                .count()
                != map_topology.nodes.iter().filter(|node| node.current).count()
            || session
                .map_graph_nodes
                .iter()
                .any(|node| {
                    node.current
                        && !map_topology.nodes.iter().any(|candidate| {
                            candidate.node_id == node.node_id && candidate.current
                        })
                });
        if needs_map_repair {
            session.location = canonical_location.clone();
            if session.scene.name.trim().is_empty()
                || resolve_map_label(&world.map_nodes, &session.scene.name)
                    .as_deref()
                    == Some(canonical_location.as_str())
            {
                session.scene.name = canonical_location;
            }
            session.map_graph_nodes = map_topology.nodes;
            session.map_graph_edges = map_topology.edges;
            session_repo.upsert(&session)?;
        }
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

    /// C2: 资产解析在锁外异步进行;回写时不能整体覆盖快照,否则会丢失这期间
    /// 其它命令对同一 session 的并发修改。此处在第二次持锁内重读最新快照,
    /// 仅合并 `resolve_session_assets` 真正改动的 `assets` 字段后回写。
    pub fn persist_resolved_session_assets(
        &self,
        conn: &Connection,
        session_id: &str,
        resolved: &SessionSnapshot,
    ) -> Result<SessionSnapshot, String> {
        let repo = crate::db::repositories::session_repo::SessionRepository::new(conn);
        let mut latest = repo
            .get(session_id)?
            .ok_or_else(|| "Session not found".to_string())?;
        latest.assets = resolved.assets.clone();
        repo.upsert(&latest)?;
        Ok(latest)
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
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::character_repo::CharacterRepository;
    use crate::db::repositories::world_repo::WorldRepository;
    use crate::db::schema;
    use crate::models::character::CharacterCreateRequest;
    use crate::models::world::WorldCreateRequest;

    #[test]
    fn create_agent_chat_session_always_uses_virtual_player() {
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
                    avatar_asset: String::new(),
                    system_prompt_template: String::new(),
                    response_contract_prompt: String::new(),
                    narration_prompt: String::new(),
                    runtime_system_prompt: String::new(),
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
                    player_character_id: Some(Some(agent.id.clone())),
                },
            )
            .expect("update world");

        let session =
            SessionOrchestrator::create_session(&conn, &updated_world.id, Some(&agent.id))
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
}
