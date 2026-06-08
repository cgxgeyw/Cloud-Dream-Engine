use crate::models::world::*;
use crate::services::map_topology::normalize_map_topology;
use rusqlite::{params, Connection};
use std::collections::HashSet;

pub struct WorldRepository<'a> {
    conn: &'a Connection,
}

const WORLD_SELECT_COLUMNS: &str = "id, name, genre, background_prompt, opening_scene, summary, time_system, map_nodes_json, triggers_json, time_config_json, director_config_json, ui_theme_config_json, director_system_prompt_base, director_runtime_system_prompt, opening_messages_json, opening_character_ids_json, player_character_id";

impl<'a> WorldRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<WorldDefinition>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {WORLD_SELECT_COLUMNS} FROM worlds ORDER BY name"))
            .map_err(|e| e.to_string())?;

        let worlds = stmt
            .query_map([], |row| {
                Ok(WorldDefinition {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    genre: row.get(2)?,
                    background_prompt: row.get(3)?,
                    opening_scene: row.get(4)?,
                    summary: row.get(5)?,
                    time_system: row.get(6)?,
                    map_nodes: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                    triggers: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
                    time_config: serde_json::from_str(&row.get::<_, String>(9)?)
                        .unwrap_or_default(),
                    director_config: serde_json::from_str(&row.get::<_, String>(10)?)
                        .unwrap_or_default(),
                    ui_theme_config: serde_json::from_str(&row.get::<_, String>(11)?)
                        .unwrap_or_default(),
                    director_system_prompt_base: row.get(12)?,
                    director_runtime_system_prompt: row.get(13)?,
                    opening_messages: serde_json::from_str(&row.get::<_, String>(14)?)
                        .unwrap_or_default(),
                    opening_character_ids: serde_json::from_str(&row.get::<_, String>(15)?)
                        .unwrap_or_default(),
                    player_character_id: row.get(16)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(worlds)
    }

    pub fn get(&self, id: &str) -> Result<Option<WorldDefinition>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {WORLD_SELECT_COLUMNS} FROM worlds WHERE id = ?1"))
            .map_err(|e| e.to_string())?;

        let mut worlds = stmt
            .query_map(params![id], |row| {
                Ok(WorldDefinition {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    genre: row.get(2)?,
                    background_prompt: row.get(3)?,
                    opening_scene: row.get(4)?,
                    summary: row.get(5)?,
                    time_system: row.get(6)?,
                    map_nodes: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                    triggers: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
                    time_config: serde_json::from_str(&row.get::<_, String>(9)?)
                        .unwrap_or_default(),
                    director_config: serde_json::from_str(&row.get::<_, String>(10)?)
                        .unwrap_or_default(),
                    ui_theme_config: serde_json::from_str(&row.get::<_, String>(11)?)
                        .unwrap_or_default(),
                    director_system_prompt_base: row.get(12)?,
                    director_runtime_system_prompt: row.get(13)?,
                    opening_messages: serde_json::from_str(&row.get::<_, String>(14)?)
                        .unwrap_or_default(),
                    opening_character_ids: serde_json::from_str(&row.get::<_, String>(15)?)
                        .unwrap_or_default(),
                    player_character_id: row.get(16)?,
                })
            })
            .map_err(|e| e.to_string())?;

        worlds.next().transpose().map_err(|e| e.to_string())
    }

    pub fn create(&self, world: &WorldCreateRequest) -> Result<WorldDefinition, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let name = world.name.trim().to_string();
        let genre = world.genre.trim().to_string();
        let background_prompt = world.background_prompt.trim().to_string();
        let opening_scene = world.opening_scene.trim().to_string();
        let summary = world.summary.trim().to_string();
        let time_system = world.time_system.trim().to_string();
        let map_nodes = normalize_map_topology(&world.map_nodes);
        let triggers = normalize_list(&world.triggers);
        let opening_messages = normalize_messages(&world.opening_messages);
        let opening_character_ids = normalize_list(&world.opening_character_ids);
        let player_character_id = world
            .player_character_id
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.conn.execute(
            "INSERT INTO worlds (id, name, genre, background_prompt, opening_scene, summary, time_system, map_nodes_json, triggers_json, time_config_json, director_config_json, ui_theme_config_json, opening_messages_json, opening_character_ids_json, player_character_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                id,
                name,
                genre,
                background_prompt,
                opening_scene,
                summary,
                time_system,
                serde_json::to_string(&map_nodes).unwrap_or_default(),
                serde_json::to_string(&triggers).unwrap_or_default(),
                serde_json::to_string(&world.time_config).unwrap_or_default(),
                serde_json::to_string(&world.director_config).unwrap_or_default(),
                serde_json::to_string(&world.ui_theme_config).unwrap_or_default(),
                serde_json::to_string(&opening_messages).unwrap_or_default(),
                serde_json::to_string(&opening_character_ids).unwrap_or_default(),
                player_character_id,
            ],
        )
        .map_err(|e| e.to_string())?;

        self.get(&id)?
            .ok_or_else(|| "Failed to create world".to_string())
    }

    pub fn update(&self, id: &str, req: &WorldUpdateRequest) -> Result<WorldDefinition, String> {
        let existing = self.get(id)?.ok_or_else(|| "World not found".to_string())?;
        let name = req
            .name
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.name);
        let genre = req
            .genre
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.genre);
        let background_prompt = req
            .background_prompt
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.background_prompt);
        let opening_scene = req
            .opening_scene
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.opening_scene);
        let summary = req
            .summary
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.summary);
        let time_system = req
            .time_system
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.time_system);
        let map_nodes = req
            .map_nodes
            .as_ref()
            .map(normalize_map_topology)
            .unwrap_or(existing.map_nodes);
        let triggers = req
            .triggers
            .as_ref()
            .map(|values| normalize_list(values))
            .unwrap_or(existing.triggers);
        let opening_messages = req
            .opening_messages
            .as_ref()
            .map(|values| normalize_messages(values))
            .unwrap_or(existing.opening_messages);
        let opening_character_ids = req
            .opening_character_ids
            .as_ref()
            .map(|values| normalize_list(values))
            .unwrap_or(existing.opening_character_ids);
        let player_character_id = req.player_character_id.as_ref().map(|value| {
            value
                .as_ref()
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
        });

        self.conn.execute(
            "UPDATE worlds SET name = ?1, genre = ?2, background_prompt = ?3, opening_scene = ?4, summary = ?5, time_system = ?6, map_nodes_json = ?7, triggers_json = ?8, time_config_json = ?9, director_config_json = ?10, ui_theme_config_json = ?11, opening_messages_json = ?12, opening_character_ids_json = ?13, player_character_id = ?14 WHERE id = ?15",
            params![
                name,
                genre,
                background_prompt,
                opening_scene,
                summary,
                time_system,
                serde_json::to_string(&map_nodes).unwrap_or_default(),
                serde_json::to_string(&triggers).unwrap_or_default(),
                serde_json::to_string(req.time_config.as_ref().unwrap_or(&existing.time_config)).unwrap_or_default(),
                serde_json::to_string(req.director_config.as_ref().unwrap_or(&existing.director_config)).unwrap_or_default(),
                serde_json::to_string(req.ui_theme_config.as_ref().unwrap_or(&existing.ui_theme_config)).unwrap_or_default(),
                serde_json::to_string(&opening_messages).unwrap_or_default(),
                serde_json::to_string(&opening_character_ids).unwrap_or_default(),
                player_character_id
                    .as_ref()
                    .and_then(|inner| inner.as_ref())
                    .or(existing.player_character_id.as_ref()),
                id,
            ],
        )
        .map_err(|e| e.to_string())?;

        self.get(id)?
            .ok_or_else(|| "Failed to update world".to_string())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM worlds WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_all(&self) -> Result<u64, String> {
        let count = self
            .conn
            .execute("DELETE FROM worlds", [])
            .map_err(|e| e.to_string())?;
        Ok(count as u64)
    }
}

fn normalize_list(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .filter_map(|value| {
            if seen.insert(value.to_string()) {
                Some(value.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn normalize_messages(values: &[WorldOpeningMessage]) -> Vec<WorldOpeningMessage> {
    values
        .iter()
        .filter_map(|message| {
            let role = message.role.trim().to_string();
            let content = message.content.trim().to_string();
            if role.is_empty() || content.is_empty() {
                return None;
            }
            Some(WorldOpeningMessage {
                role,
                content,
                speaker: message
                    .speaker
                    .as_ref()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
            })
        })
        .collect()
}
