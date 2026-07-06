use crate::models::save::*;
use crate::models::session::SessionSnapshot;
use chrono::Local;
use rusqlite::{params, Connection, OptionalExtension};

pub struct SaveRepository<'a> {
    conn: &'a Connection,
}

impl<'a> SaveRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<SaveSummary>, String> {
        // M6: 此前逐个 save 读 turn_journal、比较、再回写 turn_index,并发 list 时既不一致
        // 又产生写放大。改为单条查询用子查询取 turn_journal 最新值,只读不写。
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.id, s.session_id, s.title, s.world_name, s.updated_at, s.progress, \
                        s.summary, s.player_character_name, s.parent_save_id, s.branch_root_save_id, \
                        s.branch_label, \
                        MAX(s.turn_index, COALESCE((SELECT MAX(j.turn_index) FROM turn_journal j WHERE j.session_id = s.session_id), 0)) \
                 FROM saves s ORDER BY s.updated_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let saves = stmt
            .query_map([], |row| {
                Ok(SaveSummary {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    title: row.get(2)?,
                    world_name: row.get(3)?,
                    updated_at: row.get(4)?,
                    progress: row.get(5)?,
                    summary: row.get(6)?,
                    player_character_name: row.get(7)?,
                    parent_save_id: row.get(8)?,
                    branch_root_save_id: row.get(9)?,
                    branch_label: row.get(10)?,
                    turn_index: row.get(11)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(saves)
    }

    pub fn get(&self, id: &str) -> Result<Option<SaveSummary>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM saves WHERE id = ?1")
            .map_err(|e| e.to_string())?;

        let mut rows = stmt
            .query_map(params![id], |row| {
                Ok(SaveSummary {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    title: row.get(2)?,
                    world_name: row.get(3)?,
                    updated_at: row.get(4)?,
                    progress: row.get(5)?,
                    summary: row.get(6)?,
                    player_character_name: row.get(7)?,
                    parent_save_id: row.get(8)?,
                    branch_root_save_id: row.get(9)?,
                    branch_label: row.get(10)?,
                    turn_index: row.get(11)?,
                })
            })
            .map_err(|e| e.to_string())?;

        match rows.next() {
            Some(row) => Ok(Some(row.map_err(|e| e.to_string())?)),
            None => Ok(None),
        }
    }

    pub fn upsert(&self, save: &SaveSummary) -> Result<(), String> {
        let mut save = save.clone(); // 创建可变副本

        // 从 turn_journal 同步最新的 turn_index
        let latest_turn_index = self.load_latest_turn_index(&save.session_id)?;
        if latest_turn_index > save.turn_index {
            save.turn_index = latest_turn_index;
        }

        let title = save.title.trim().to_string();
        let world_name = save.world_name.trim().to_string();
        let progress = save.progress.trim().to_string();
        let summary = save.summary.trim().to_string();
        let player_character_name = save
            .player_character_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let parent_save_id = save
            .parent_save_id
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let branch_root_save_id = save
            .branch_root_save_id
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let branch_label = save
            .branch_label
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.conn.execute(
            "INSERT OR REPLACE INTO saves (id, session_id, title, world_name, updated_at, progress, summary, player_character_name, parent_save_id, branch_root_save_id, branch_label, turn_index) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                save.id,
                save.session_id,
                title,
                world_name,
                save.updated_at,
                progress,
                summary,
                player_character_name,
                parent_save_id,
                branch_root_save_id,
                branch_label,
                save.turn_index,
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn branch_save(
        &self,
        save_id: &str,
        source_session: &SessionSnapshot,
    ) -> Result<SaveSummary, String> {
        let source_save = self
            .get(save_id)?
            .ok_or_else(|| "Save not found".to_string())?;
        let branched_session_id = format!("session-{}", uuid::Uuid::new_v4().simple());
        let branched_save_id = format!("save-{}", uuid::Uuid::new_v4().simple());
        let branch_label = "新分支".to_string();
        let branch_title = format!("{} / {}", source_save.title, branch_label);

        let branched_session = SessionSnapshot {
            id: branched_session_id.clone(),
            world_name: source_session.world_name.clone(),
            location: source_session.location.clone(),
            time_label: source_session.time_label.clone(),
            current_speaker: source_session.current_speaker.clone(),
            current_line: source_session.current_line.clone(),
            player_character_id: source_session.player_character_id.clone(),
            player_character_name: source_session.player_character_name.clone(),
            visible_characters: source_session.visible_characters.clone(),
            messages: source_session.messages.clone(),
            player_stats: source_session.player_stats.clone(),
            map_graph_nodes: source_session.map_graph_nodes.clone(),
            map_graph_edges: source_session.map_graph_edges.clone(),
            inventory_items: source_session.inventory_items.clone(),
            system_log: source_session.system_log.clone(),
            scene: source_session.scene.clone(),
            assets: source_session.assets.clone(),
            state: source_session.state.clone(),
        };

        // M2: 分支涉及 sessions + memories + memory_embeddings + attribute_values + saves
        // 多表写入,用事务包裹:中途任何一步失败都整体回滚,不留孤儿 session/半套记忆。
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;

        self.conn.execute(
            "INSERT INTO sessions (id, world_name, location, time_label, current_speaker, current_line, player_character_id, player_character_name, visible_characters_json, messages_json, player_stats_json, map_graph_nodes_json, map_graph_edges_json, inventory_items_json, system_log_json, scene_json, assets_json, state_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                branched_session.id,
                branched_session.world_name,
                branched_session.location,
                branched_session.time_label,
                branched_session.current_speaker,
                branched_session.current_line,
                branched_session.player_character_id,
                branched_session.player_character_name,
                serde_json::to_string(&branched_session.visible_characters).unwrap_or_default(),
                serde_json::to_string(&branched_session.messages).unwrap_or_default(),
                serde_json::to_string(&branched_session.player_stats).unwrap_or_default(),
                serde_json::to_string(&branched_session.map_graph_nodes).unwrap_or_default(),
                serde_json::to_string(&branched_session.map_graph_edges).unwrap_or_default(),
                serde_json::to_string(&branched_session.inventory_items).unwrap_or_default(),
                serde_json::to_string(&branched_session.system_log).unwrap_or_default(),
                serde_json::to_string(&branched_session.scene).unwrap_or_default(),
                serde_json::to_string(&branched_session.assets).unwrap_or_default(),
                serde_json::to_string(&branched_session.state).unwrap_or_default(),
            ],
        )
        .map_err(|e| e.to_string())?;

        self.copy_branch_memories(&source_save.session_id, &branched_session_id)?;
        self.copy_branch_attributes(&source_save.session_id, &branched_session_id)?;

        let branched_save = SaveSummary {
            id: branched_save_id,
            session_id: branched_session_id,
            title: branch_title,
            world_name: source_save.world_name,
            updated_at: Local::now().format("%Y-%m-%d %H:%M").to_string(),
            progress: source_save.progress,
            summary: source_save.summary,
            player_character_name: source_save.player_character_name,
            parent_save_id: Some(source_save.id.clone()),
            branch_root_save_id: Some(source_save.branch_root_save_id.unwrap_or(source_save.id)),
            branch_label: Some(branch_label),
            turn_index: source_save.turn_index,
        };

        self.upsert(&branched_save)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(branched_save)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM saves WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_all(&self) -> Result<u64, String> {
        let count = self
            .conn
            .execute("DELETE FROM saves", [])
            .map_err(|e| e.to_string())?;
        Ok(count as u64)
    }

    /// H1: 列出所有存档指向的会话 id,供删除存档时清理底层会话数据。
    pub fn list_session_ids(&self) -> Result<Vec<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT session_id FROM saves")
            .map_err(|e| e.to_string())?;
        let ids = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(ids)
    }

    fn copy_branch_memories(
        &self,
        source_session_id: &str,
        branched_session_id: &str,
    ) -> Result<(), String> {
        let mut stmt = self.conn
            .prepare("SELECT id, world_id, character_id, layer, content, source, importance, created_at, turn_index, conversation_id, event_id, item_id, scene_id, memory_type, speaker, role, location, participants_json, keywords_json FROM memories WHERE session_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![source_session_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, f64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<String>>(15)?,
                    row.get::<_, Option<String>>(16)?,
                    row.get::<_, String>(17)?,
                    row.get::<_, String>(18)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        // M3: 记忆换新 id 时,memory_embeddings(FK→memory_id)必须一并按新 id 复制,
        // 否则分支会话的记忆缺向量,语义检索返回空、角色"失忆"直到重新嵌入。
        // 这里记录 旧 id → 新 id 映射,稍后复制对应的嵌入行。
        let mut id_map: Vec<(String, String)> = Vec::with_capacity(rows.len());

        for row in rows {
            let (
                old_id,
                world_id,
                character_id,
                layer,
                content,
                source,
                importance,
                created_at,
                turn_index,
                conversation_id,
                event_id,
                item_id,
                scene_id,
                memory_type,
                speaker,
                role,
                location,
                participants_json,
                keywords_json,
            ) = row;
            let new_id = format!("memory-{}", uuid::Uuid::new_v4().simple());

            self.conn.execute(
                "INSERT INTO memories (id, world_id, session_id, character_id, layer, content, source, importance, created_at, turn_index, conversation_id, event_id, item_id, scene_id, memory_type, speaker, role, location, participants_json, keywords_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    new_id,
                    world_id,
                    branched_session_id,
                    character_id,
                    layer,
                    content,
                    source,
                    importance,
                    created_at,
                    turn_index,
                    conversation_id,
                    event_id,
                    item_id,
                    scene_id,
                    memory_type,
                    speaker,
                    role,
                    location,
                    participants_json,
                    keywords_json,
                ],
            )
            .map_err(|e| e.to_string())?;
            id_map.push((old_id, new_id));
        }

        // 复制每条记忆的嵌入向量到新 id。
        for (old_id, new_id) in &id_map {
            self.conn
                .execute(
                    "INSERT INTO memory_embeddings (memory_id, model_key, vector_json, updated_at) \
                     SELECT ?1, model_key, vector_json, updated_at FROM memory_embeddings WHERE memory_id = ?2",
                    params![new_id, old_id],
                )
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn copy_branch_attributes(
        &self,
        source_session_id: &str,
        branched_session_id: &str,
    ) -> Result<(), String> {
        let pattern = format!("{}:%", source_session_id);
        let mut stmt = self.conn
            .prepare("SELECT schema_id, owner_type, owner_id, value_json, source FROM attribute_values WHERE (owner_type = 'session' AND owner_id = ?1) OR (owner_type = 'session_character' AND owner_id LIKE ?2)")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![source_session_id, pattern], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        for row in rows {
            let (schema_id, owner_type, owner_id, value_json, source) =
                row.map_err(|e| e.to_string())?;
            let next_owner_id = if owner_type == "session" {
                branched_session_id.to_string()
            } else {
                let character_id = owner_id
                    .split_once(':')
                    .map(|(_, suffix)| suffix.to_string())
                    .ok_or_else(|| "Invalid session_character owner_id".to_string())?;
                format!("{}:{}", branched_session_id, character_id)
            };

            self.conn.execute(
                "INSERT INTO attribute_values (id, schema_id, owner_type, owner_id, value_json, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    format!("attrval-{}", uuid::Uuid::new_v4().simple()),
                    schema_id,
                    owner_type,
                    next_owner_id,
                    value_json,
                    source,
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn load_latest_turn_index(&self, session_id: &str) -> Result<i32, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT COALESCE(MAX(turn_index), 0) FROM turn_journal WHERE session_id = ?1")
            .map_err(|e| e.to_string())?;
        let result: Option<i32> = stmt
            .query_row(params![session_id], |row| row.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(result.unwrap_or(0))
    }
}
