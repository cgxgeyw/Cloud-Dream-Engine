use crate::models::world::*;
use crate::services::map_topology::normalize_map_topology;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};

pub struct WorldRepository<'a> {
    conn: &'a Connection,
}

const WORLD_SELECT_COLUMNS: &str = "id, name, genre, background_prompt, opening_scene, summary, time_system, map_nodes_json, triggers_json, custom_tabs_json, world_custom_attribute_definitions_json, character_custom_attribute_definitions_json, time_config_json, director_config_json, ui_theme_config_json, director_system_prompt_base, director_runtime_system_prompt, opening_messages_json, opening_character_ids_json, player_character_id";

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
                    custom_tabs: serde_json::from_str(&row.get::<_, String>(9)?)
                        .unwrap_or_default(),
                    world_custom_attribute_definitions: serde_json::from_str(&row.get::<_, String>(10)?)
                        .unwrap_or_default(),
                    character_custom_attribute_definitions: serde_json::from_str(&row.get::<_, String>(11)?)
                        .unwrap_or_default(),
                    time_config: serde_json::from_str(&row.get::<_, String>(12)?)
                        .unwrap_or_default(),
                    director_config: serde_json::from_str(&row.get::<_, String>(13)?)
                        .unwrap_or_default(),
                    ui_theme_config: serde_json::from_str(&row.get::<_, String>(14)?)
                        .unwrap_or_default(),
                    director_system_prompt_base: row.get(15)?,
                    director_runtime_system_prompt: row.get(16)?,
                    opening_messages: serde_json::from_str(&row.get::<_, String>(17)?)
                        .unwrap_or_default(),
                    opening_character_ids: serde_json::from_str(&row.get::<_, String>(18)?)
                        .unwrap_or_default(),
                    player_character_id: row.get(19)?,
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
                    custom_tabs: serde_json::from_str(&row.get::<_, String>(9)?)
                        .unwrap_or_default(),
                    world_custom_attribute_definitions: serde_json::from_str(&row.get::<_, String>(10)?)
                        .unwrap_or_default(),
                    character_custom_attribute_definitions: serde_json::from_str(&row.get::<_, String>(11)?)
                        .unwrap_or_default(),
                    time_config: serde_json::from_str(&row.get::<_, String>(12)?)
                        .unwrap_or_default(),
                    director_config: serde_json::from_str(&row.get::<_, String>(13)?)
                        .unwrap_or_default(),
                    ui_theme_config: serde_json::from_str(&row.get::<_, String>(14)?)
                        .unwrap_or_default(),
                    director_system_prompt_base: row.get(15)?,
                    director_runtime_system_prompt: row.get(16)?,
                    opening_messages: serde_json::from_str(&row.get::<_, String>(17)?)
                        .unwrap_or_default(),
                    opening_character_ids: serde_json::from_str(&row.get::<_, String>(18)?)
                        .unwrap_or_default(),
                    player_character_id: row.get(19)?,
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
        let custom_tabs = normalize_map(&world.custom_tabs);
        let world_custom_attribute_definitions =
            normalize_custom_attribute_definitions(&world.world_custom_attribute_definitions);
        let character_custom_attribute_definitions =
            normalize_custom_attribute_definitions(&world.character_custom_attribute_definitions);
        let opening_messages = normalize_messages(&world.opening_messages);
        let opening_character_ids = normalize_list(&world.opening_character_ids);
        let player_character_id = world
            .player_character_id
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.conn.execute(
            "INSERT INTO worlds (id, name, genre, background_prompt, opening_scene, summary, time_system, map_nodes_json, triggers_json, custom_tabs_json, world_custom_attribute_definitions_json, character_custom_attribute_definitions_json, time_config_json, director_config_json, ui_theme_config_json, opening_messages_json, opening_character_ids_json, player_character_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
                serde_json::to_string(&custom_tabs).unwrap_or_default(),
                serde_json::to_string(&world_custom_attribute_definitions).unwrap_or_default(),
                serde_json::to_string(&character_custom_attribute_definitions).unwrap_or_default(),
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
        let custom_tabs = req
            .custom_tabs
            .as_ref()
            .map(|values| normalize_map(values))
            .unwrap_or(existing.custom_tabs);
        let world_custom_attribute_definitions = req
            .world_custom_attribute_definitions
            .as_ref()
            .map(|values| normalize_custom_attribute_definitions(values))
            .unwrap_or(existing.world_custom_attribute_definitions);
        let character_custom_attribute_definitions = req
            .character_custom_attribute_definitions
            .as_ref()
            .map(|values| normalize_custom_attribute_definitions(values))
            .unwrap_or(existing.character_custom_attribute_definitions);
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
            "UPDATE worlds SET name = ?1, genre = ?2, background_prompt = ?3, opening_scene = ?4, summary = ?5, time_system = ?6, map_nodes_json = ?7, triggers_json = ?8, custom_tabs_json = ?9, world_custom_attribute_definitions_json = ?10, character_custom_attribute_definitions_json = ?11, time_config_json = ?12, director_config_json = ?13, ui_theme_config_json = ?14, opening_messages_json = ?15, opening_character_ids_json = ?16, player_character_id = ?17 WHERE id = ?18",
            params![
                name,
                genre,
                background_prompt,
                opening_scene,
                summary,
                time_system,
                serde_json::to_string(&map_nodes).unwrap_or_default(),
                serde_json::to_string(&triggers).unwrap_or_default(),
                serde_json::to_string(&custom_tabs).unwrap_or_default(),
                serde_json::to_string(&world_custom_attribute_definitions).unwrap_or_default(),
                serde_json::to_string(&character_custom_attribute_definitions).unwrap_or_default(),
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

fn normalize_map(values: &HashMap<String, String>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (key, value) in values {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        map.insert(key.to_string(), value.trim().to_string());
    }
    map
}

fn normalize_custom_attribute_definitions(
    values: &[CustomAttributeDefinition],
) -> Vec<CustomAttributeDefinition> {
    let mut seen = HashSet::new();
    let mut definitions = Vec::new();

    for (index, value) in values.iter().enumerate() {
        let name = value.name.trim();
        if name.is_empty() {
            continue;
        }

        let id = value.id.trim();
        let fallback_id = normalize_attribute_id(name);
        let id = if id.is_empty() { fallback_id } else { normalize_attribute_id(id) };
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }

        let value_type = match value.value_type.trim() {
            "text" | "longText" => value.value_type.trim().to_string(),
            _ => "longText".to_string(),
        };

        definitions.push(CustomAttributeDefinition {
            id,
            name: name.to_string(),
            value_type,
            order: if value.order >= 0 {
                value.order
            } else {
                index as i32
            },
            enabled: value.enabled,
            required: value.required,
            placeholder: value.placeholder.trim().to_string(),
            default_value: value.default_value.trim().to_string(),
        });
    }

    definitions.sort_by_key(|item| item.order);
    for (index, item) in definitions.iter_mut().enumerate() {
        item.order = index as i32;
    }
    definitions
}

fn normalize_attribute_id(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else if character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
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
