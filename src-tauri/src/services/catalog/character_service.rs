use rusqlite::{params, Connection};

use crate::db::repositories::character_repo::CharacterRepository;
use crate::db::repositories::session_repo::SessionRepository;
use crate::db::repositories::world_repo::WorldRepository;
use crate::models::character::*;
use crate::models::world::WorldUpdateRequest;

pub struct CharacterService;

impl CharacterService {
    pub fn new() -> Self {
        Self
    }

    pub fn list_world_characters(
        &self,
        conn: &Connection,
        world_id: &str,
    ) -> Result<Vec<CharacterDefinition>, String> {
        let repo = CharacterRepository::new(conn);
        Ok(repo
            .list_by_world(world_id)?
            .into_iter()
            .map(Self::enrich_character)
            .collect())
    }

    pub fn list_all_characters(
        &self,
        conn: &Connection,
    ) -> Result<Vec<CharacterDefinition>, String> {
        let repo = CharacterRepository::new(conn);
        Ok(repo
            .list_all()?
            .into_iter()
            .map(Self::enrich_character)
            .collect())
    }

    pub fn get_character(
        &self,
        conn: &Connection,
        id: &str,
    ) -> Result<CharacterDefinition, String> {
        let repo = CharacterRepository::new(conn);
        repo.get(id)?
            .map(Self::enrich_character)
            .ok_or_else(|| "Character not found".to_string())
    }

    pub fn create_world_character(
        &self,
        conn: &Connection,
        world_id: &str,
        request: CharacterCreateRequest,
    ) -> Result<CharacterDefinition, String> {
        let repo = CharacterRepository::new(conn);
        repo.create(world_id, &request).map(Self::enrich_character)
    }

    pub fn update_world_character(
        &self,
        conn: &Connection,
        id: &str,
        request: CharacterUpdateRequest,
    ) -> Result<CharacterDefinition, String> {
        let repo = CharacterRepository::new(conn);
        repo.update(id, &request).map(Self::enrich_character)
    }

    pub fn delete_world_character(&self, conn: &Connection, id: &str) -> Result<(), String> {
        let repo = CharacterRepository::new(conn);
        let world_repo = WorldRepository::new(conn);
        let session_repo = SessionRepository::new(conn);

        let character = repo
            .get(id)?
            .ok_or_else(|| "Character not found".to_string())?;
        let world = world_repo
            .get(&character.world_id)?
            .ok_or_else(|| "World not found".to_string())?;

        repo.delete(id)?;

        let remaining_characters = repo.list_by_world(&character.world_id)?;
        let fallback_player = remaining_characters.first().cloned();
        let fallback_player_id = fallback_player.as_ref().map(|item| item.id.clone());
        let fallback_player_name = fallback_player
            .as_ref()
            .map(|item| item.name.clone())
            .unwrap_or_default();

        let opening_character_ids = world
            .opening_character_ids
            .iter()
            .filter(|item| **item != id)
            .cloned()
            .collect::<Vec<_>>();
        let player_character_id = if world.player_character_id.as_deref() == Some(id) {
            Some(fallback_player_id.clone())
        } else {
            Some(world.player_character_id.clone())
        };
        world_repo.update(
            &world.id,
            &WorldUpdateRequest {
                name: None,
                genre: None,
                background_prompt: None,
                opening_scene: None,
                summary: None,
                time_system: None,
                map_nodes: None,
                triggers: None,
                time_config: None,
                director_config: None,
                ui_theme_config: None,
                opening_messages: None,
                opening_character_ids: Some(opening_character_ids),
                player_character_id,
            },
        )?;

        let mut stmt = conn
            .prepare("SELECT id FROM sessions WHERE world_name = ?1")
            .map_err(|e| e.to_string())?;
        let session_ids = stmt
            .query_map(params![world.name.as_str()], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        for session_id in session_ids {
            let mut session = match session_repo.get(&session_id)? {
                Some(value) => value,
                None => continue,
            };
            let mut changed = false;

            let next_visible = session
                .visible_characters
                .iter()
                .filter(|name| **name != character.name)
                .cloned()
                .collect::<Vec<_>>();
            if next_visible != session.visible_characters {
                session.visible_characters = next_visible;
                changed = true;
            }

            let next_present = session
                .scene
                .present_characters
                .iter()
                .filter(|name| **name != character.name)
                .cloned()
                .collect::<Vec<_>>();
            if next_present != session.scene.present_characters {
                session.scene.present_characters = next_present;
                changed = true;
            }

            if session.player_character_id == id {
                session.player_character_id = fallback_player_id.clone().unwrap_or_default();
                session.player_character_name = fallback_player_name.clone();
                session
                    .visible_characters
                    .retain(|name| *name != session.player_character_name);
                changed = true;
            }

            if session.current_speaker == character.name {
                session.current_speaker = session.player_character_name.clone();
                changed = true;
            }

            if changed {
                session_repo.upsert(&session)?;
            }
        }

        Ok(())
    }

    pub fn export_character_template(
        &self,
        conn: &Connection,
        world_id: &str,
        character_id: &str,
    ) -> Result<CharacterTemplateExport, String> {
        let world_repo = WorldRepository::new(conn);
        world_repo
            .get(world_id)?
            .ok_or_else(|| "World not found".to_string())?;
        let repo = CharacterRepository::new(conn);
        let character = repo
            .get(character_id)?
            .ok_or_else(|| "Character not found".to_string())?;
        if character.world_id != world_id {
            return Err("Character not found in this world".to_string());
        }
        repo.export_template(character_id)
    }

    pub fn create_character_in_world(
        &self,
        conn: &Connection,
        world_id: &str,
        character_id: &str,
        request: CharacterImportRequest,
    ) -> Result<CharacterDefinition, String> {
        let world_repo = WorldRepository::new(conn);
        world_repo
            .get(world_id)?
            .ok_or_else(|| "World not found".to_string())?;
        world_repo
            .get(&request.target_world_id)?
            .ok_or_else(|| "Target world not found".to_string())?;
        let repo = CharacterRepository::new(conn);
        let character = repo
            .get(character_id)?
            .ok_or_else(|| "Character not found".to_string())?;
        if character.world_id != world_id {
            return Err("Character not found in this world".to_string());
        }
        repo.create_from_template(character_id, &request.target_world_id, &request.name)
            .map(Self::enrich_character)
    }

    pub fn import_character_template(
        &self,
        conn: &Connection,
        world_id: &str,
        request: CharacterTemplateImportRequest,
    ) -> Result<CharacterDefinition, String> {
        let world_repo = WorldRepository::new(conn);
        world_repo
            .get(world_id)?
            .ok_or_else(|| "World not found".to_string())?;
        let repo = CharacterRepository::new(conn);
        repo.create(
            world_id,
            &CharacterCreateRequest {
                name: request.name,
                role: request.role,
                background_prompt: request.background_prompt,
                model: request.model,
                memory_strategy: request.memory_strategy,
                recent_dialogue_rounds: request.recent_dialogue_rounds,
                attributes: request.attributes,
                portrait_assets: request.portrait_assets,
                avatar_asset: request.avatar_asset,
                system_prompt_template: request.system_prompt_template,
                response_contract_prompt: request.response_contract_prompt,
                narration_prompt: request.narration_prompt,
                runtime_system_prompt: request.runtime_system_prompt,
            },
        )
        .map(Self::enrich_character)
    }

    pub fn enrich_character(character: CharacterDefinition) -> CharacterDefinition {
        character
    }
}
