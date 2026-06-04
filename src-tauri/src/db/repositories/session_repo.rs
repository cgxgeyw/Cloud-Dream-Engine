use crate::models::session::*;
use rusqlite::{params, Connection};
use std::collections::HashSet;

pub struct SessionRepository<'a> {
    conn: &'a Connection,
}

impl<'a> SessionRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> Result<Option<SessionSnapshot>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM sessions WHERE id = ?1")
            .map_err(|e| e.to_string())?;

        let mut sessions = stmt
            .query_map(params![id], |row| {
                Ok(SessionSnapshot {
                    id: row.get(0)?,
                    world_name: row.get(1)?,
                    location: row.get(2)?,
                    time_label: row.get(3)?,
                    current_speaker: row.get(4)?,
                    current_line: row.get(5)?,
                    player_character_id: row.get(6)?,
                    player_character_name: row.get(7)?,
                    visible_characters: serde_json::from_str(&row.get::<_, String>(8)?)
                        .unwrap_or_default(),
                    messages: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
                    player_stats: serde_json::from_str(&row.get::<_, String>(10)?)
                        .unwrap_or_default(),
                    map_graph_nodes: serde_json::from_str(&row.get::<_, String>(11)?)
                        .unwrap_or_default(),
                    map_graph_edges: serde_json::from_str(&row.get::<_, String>(12)?)
                        .unwrap_or_default(),
                    inventory_items: serde_json::from_str(&row.get::<_, String>(13)?)
                        .unwrap_or_default(),
                    system_log: serde_json::from_str(&row.get::<_, String>(14)?)
                        .unwrap_or_default(),
                    scene: serde_json::from_str(&row.get::<_, String>(15)?).unwrap_or_default(),
                    assets: serde_json::from_str(&row.get::<_, String>(16)?).unwrap_or_default(),
                    state: serde_json::from_str(&row.get::<_, String>(17)?).unwrap_or_default(),
                })
            })
            .map_err(|e| e.to_string())?;

        sessions.next().transpose().map_err(|e| e.to_string())
    }

    pub fn upsert(&self, session: &SessionSnapshot) -> Result<(), String> {
        let world_name = session.world_name.trim().to_string();
        let location = session.location.trim().to_string();
        let time_label = session.time_label.trim().to_string();
        let current_speaker = session.current_speaker.trim().to_string();
        let current_line = session.current_line.trim().to_string();
        let player_character_id = session.player_character_id.trim().to_string();
        let player_character_name = session.player_character_name.trim().to_string();
        let visible_characters = normalize_list(&session.visible_characters);
        let player_stats = normalize_list(&session.player_stats);
        let system_log = normalize_list(&session.system_log);
        self.conn.execute(
            "INSERT OR REPLACE INTO sessions (id, world_name, location, time_label, current_speaker, current_line, player_character_id, player_character_name, visible_characters_json, messages_json, player_stats_json, map_graph_nodes_json, map_graph_edges_json, inventory_items_json, system_log_json, scene_json, assets_json, state_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                session.id,
                world_name,
                location,
                time_label,
                current_speaker,
                current_line,
                player_character_id,
                player_character_name,
                serde_json::to_string(&visible_characters).unwrap_or_default(),
                serde_json::to_string(&session.messages).unwrap_or_default(),
                serde_json::to_string(&player_stats).unwrap_or_default(),
                serde_json::to_string(&session.map_graph_nodes).unwrap_or_default(),
                serde_json::to_string(&session.map_graph_edges).unwrap_or_default(),
                serde_json::to_string(&session.inventory_items).unwrap_or_default(),
                serde_json::to_string(&system_log).unwrap_or_default(),
                serde_json::to_string(&session.scene).unwrap_or_default(),
                serde_json::to_string(&session.assets).unwrap_or_default(),
                serde_json::to_string(&session.state).unwrap_or_default(),
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
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
