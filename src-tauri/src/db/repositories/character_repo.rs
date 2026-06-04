use crate::models::character::*;
use rusqlite::{params, Connection, Row};
use std::collections::{HashMap, HashSet};

pub struct CharacterRepository<'a> {
    conn: &'a Connection,
}

const CHARACTER_SELECT_COLUMNS: &str = "id, name, world_id, role, background_prompt, model, memory_strategy, recent_dialogue_rounds, attributes_json, portrait_assets_json, custom_tabs_json, system_prompt_template, response_contract_prompt, narration_prompt, runtime_system_prompt";

impl<'a> CharacterRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list_by_world(&self, world_id: &str) -> Result<Vec<CharacterDefinition>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {CHARACTER_SELECT_COLUMNS} FROM characters WHERE world_id = ?1 ORDER BY name"
            ))
            .map_err(|e| e.to_string())?;

        let chars = stmt
            .query_map(params![world_id], map_character_definition)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(chars)
    }

    pub fn list_all(&self) -> Result<Vec<CharacterDefinition>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {CHARACTER_SELECT_COLUMNS} FROM characters ORDER BY name"
            ))
            .map_err(|e| e.to_string())?;

        let chars = stmt
            .query_map([], map_character_definition)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(chars)
    }

    pub fn get(&self, id: &str) -> Result<Option<CharacterDefinition>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {CHARACTER_SELECT_COLUMNS} FROM characters WHERE id = ?1"
            ))
            .map_err(|e| e.to_string())?;

        let mut chars = stmt
            .query_map(params![id], map_character_definition)
            .map_err(|e| e.to_string())?;

        chars.next().transpose().map_err(|e| e.to_string())
    }

    pub fn create(
        &self,
        world_id: &str,
        req: &CharacterCreateRequest,
    ) -> Result<CharacterDefinition, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let name = req.name.trim().to_string();
        let role = req.role.trim().to_string();
        let background_prompt = req.background_prompt.trim().to_string();
        let model = req.model.trim().to_string();
        let memory_strategy = req.memory_strategy.trim().to_string();
        let recent_dialogue_rounds = req.recent_dialogue_rounds.max(0);
        let attributes = normalize_list(&req.attributes);
        let portrait_assets = normalize_list(&req.portrait_assets);
        let custom_tabs = normalize_map(&req.custom_tabs);
        let system_prompt_template =
            resolve_character_system_prompt_template(Some(req.system_prompt_template.as_str()));
        let response_contract_prompt =
            resolve_character_response_contract_prompt(Some(req.response_contract_prompt.as_str()));
        let narration_prompt =
            resolve_character_narration_prompt(Some(req.narration_prompt.as_str()));
        self.conn.execute(
            "INSERT INTO characters (id, name, world_id, role, background_prompt, model, memory_strategy, recent_dialogue_rounds, attributes_json, portrait_assets_json, custom_tabs_json, system_prompt_template, response_contract_prompt, narration_prompt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                id,
                name,
                world_id,
                role,
                background_prompt,
                model,
                memory_strategy,
                recent_dialogue_rounds,
                serde_json::to_string(&attributes).unwrap_or_default(),
                serde_json::to_string(&portrait_assets).unwrap_or_default(),
                serde_json::to_string(&custom_tabs).unwrap_or_default(),
                system_prompt_template,
                response_contract_prompt,
                narration_prompt,
            ],
        )
        .map_err(|e| e.to_string())?;

        self.get(&id)?
            .ok_or_else(|| "Failed to create character".to_string())
    }

    pub fn update(
        &self,
        id: &str,
        req: &CharacterUpdateRequest,
    ) -> Result<CharacterDefinition, String> {
        let existing = self
            .get(id)?
            .ok_or_else(|| "Character not found".to_string())?;
        let name = req
            .name
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.name);
        let role = req
            .role
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.role);
        let background_prompt = req
            .background_prompt
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.background_prompt);
        let model = req
            .model
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.model);
        let memory_strategy = req
            .memory_strategy
            .as_ref()
            .map(|value| value.trim().to_string())
            .unwrap_or(existing.memory_strategy);
        let recent_dialogue_rounds = req
            .recent_dialogue_rounds
            .unwrap_or(existing.recent_dialogue_rounds)
            .max(0);
        let attributes = req
            .attributes
            .as_ref()
            .map(|items| normalize_list(items))
            .unwrap_or(existing.attributes);
        let portrait_assets = req
            .portrait_assets
            .as_ref()
            .map(|items| normalize_list(items))
            .unwrap_or(existing.portrait_assets);
        let custom_tabs = req
            .custom_tabs
            .as_ref()
            .map(|tabs| normalize_map(tabs))
            .unwrap_or(existing.custom_tabs);
        let system_prompt_template = req
            .system_prompt_template
            .as_ref()
            .map(|value| resolve_character_system_prompt_template(Some(value.as_str())))
            .unwrap_or(existing.system_prompt_template);
        let response_contract_prompt = req
            .response_contract_prompt
            .as_ref()
            .map(|value| resolve_character_response_contract_prompt(Some(value.as_str())))
            .unwrap_or(existing.response_contract_prompt);
        let narration_prompt = req
            .narration_prompt
            .as_ref()
            .map(|value| resolve_character_narration_prompt(Some(value.as_str())))
            .unwrap_or(existing.narration_prompt);

        self.conn.execute(
            "UPDATE characters SET name = ?1, role = ?2, background_prompt = ?3, model = ?4, memory_strategy = ?5, recent_dialogue_rounds = ?6, attributes_json = ?7, portrait_assets_json = ?8, custom_tabs_json = ?9, system_prompt_template = ?10, response_contract_prompt = ?11, narration_prompt = ?12 WHERE id = ?13",
            params![
                name,
                role,
                background_prompt,
                model,
                memory_strategy,
                recent_dialogue_rounds,
                serde_json::to_string(&attributes).unwrap_or_default(),
                serde_json::to_string(&portrait_assets).unwrap_or_default(),
                serde_json::to_string(&custom_tabs).unwrap_or_default(),
                system_prompt_template,
                response_contract_prompt,
                narration_prompt,
                id,
            ],
        )
        .map_err(|e| e.to_string())?;

        self.get(id)?
            .ok_or_else(|| "Failed to update character".to_string())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM characters WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn export_template(&self, id: &str) -> Result<CharacterTemplateExport, String> {
        let character = self
            .get(id)?
            .ok_or_else(|| "Character not found".to_string())?;
        Ok(CharacterTemplateExport {
            name: character.name,
            role: character.role,
            background_prompt: character.background_prompt,
            model: character.model,
            memory_strategy: character.memory_strategy,
            recent_dialogue_rounds: character.recent_dialogue_rounds,
            attributes: character.attributes,
            portrait_assets: character.portrait_assets,
            custom_tabs: character.custom_tabs,
            system_prompt_template: character.system_prompt_template,
            response_contract_prompt: character.response_contract_prompt,
            narration_prompt: character.narration_prompt,
        })
    }

    pub fn create_from_template(
        &self,
        source_character_id: &str,
        target_world_id: &str,
        name: &str,
    ) -> Result<CharacterDefinition, String> {
        let source = self
            .get(source_character_id)?
            .ok_or_else(|| "Character not found".to_string())?;
        let request = CharacterCreateRequest {
            name: name.to_string(),
            role: source.role,
            background_prompt: source.background_prompt,
            model: source.model,
            memory_strategy: source.memory_strategy,
            recent_dialogue_rounds: source.recent_dialogue_rounds,
            attributes: source.attributes,
            portrait_assets: source.portrait_assets,
            custom_tabs: source.custom_tabs,
            system_prompt_template: source.system_prompt_template,
            response_contract_prompt: source.response_contract_prompt,
            narration_prompt: source.narration_prompt,
        };
        self.create(target_world_id, &request)
    }
}

fn map_character_definition(row: &Row<'_>) -> rusqlite::Result<CharacterDefinition> {
    let system_prompt_template: String = row.get(11)?;
    let response_contract_prompt: String = row.get(12)?;
    let narration_prompt: String = row.get(13)?;
    Ok(CharacterDefinition {
        id: row.get(0)?,
        name: row.get(1)?,
        world_id: row.get(2)?,
        role: row.get(3)?,
        background_prompt: row.get(4)?,
        model: row.get(5)?,
        memory_strategy: row.get(6)?,
        recent_dialogue_rounds: row.get(7)?,
        attributes: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
        portrait_assets: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
        custom_tabs: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
        system_prompt_template: resolve_character_system_prompt_template(Some(
            system_prompt_template.as_str(),
        )),
        response_contract_prompt: resolve_character_response_contract_prompt(Some(
            response_contract_prompt.as_str(),
        )),
        narration_prompt: resolve_character_narration_prompt(Some(narration_prompt.as_str())),
        runtime_system_prompt: row.get(14)?,
    })
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
