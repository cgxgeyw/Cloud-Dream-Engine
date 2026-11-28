use crate::models::save::*;
use crate::models::session::SessionSnapshot;
use chrono::Local;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

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
        let source_save = self.get(save_id)?.ok_or_else(|| "存档不存在".to_string())?;
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
            system_log: source_session.system_log.clone(),
            scene: source_session.scene.clone(),
            assets: source_session.assets.clone(),
            state: source_session.state.clone(),
            // 分支继承源存档的会话级采样参数（玩家已调好的偏好不该在分支后丢掉）。
            generation_params: source_session.generation_params.clone(),
        };

        // M2: 分支涉及 sessions + memories + memory_embeddings + attribute_values + saves
        // 多表写入,用事务包裹:中途任何一步失败都整体回滚,不留孤儿 session/半套记忆。
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;

        self.conn.execute(
            "INSERT INTO sessions (id, world_name, location, time_label, current_speaker, current_line, player_character_id, player_character_name, visible_characters_json, messages_json, player_stats_json, map_graph_nodes_json, map_graph_edges_json, system_log_json, scene_json, assets_json, state_json, generation_params_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
                serde_json::to_string(&branched_session.system_log).unwrap_or_default(),
                serde_json::to_string(&branched_session.scene).unwrap_or_default(),
                serde_json::to_string(&branched_session.assets).unwrap_or_default(),
                serde_json::to_string(&branched_session.state).unwrap_or_default(),
                serde_json::to_string(&branched_session.generation_params)
                    .unwrap_or_else(|_| "{}".to_string()),
            ],
        )
        .map_err(|e| e.to_string())?;

        let memory_ids =
            self.copy_branch_memories(&source_save.session_id, &branched_session_id)?;
        self.copy_branch_summary_ranges(
            &source_save.session_id,
            &branched_session_id,
            &memory_ids,
        )?;
        let memory_graph_ids =
            self.copy_branch_memory_graph(&source_save.session_id, &branched_session_id)?;
        self.copy_branch_attributes(&source_save.session_id, &branched_session_id)?;
        let oag_ids = crate::services::oag::lifecycle::copy_session(
            &tx,
            &source_save.session_id,
            &branched_session_id,
            &memory_ids,
        )?;
        self.copy_branch_turn_journal(
            &source_save.session_id,
            &branched_session_id,
            &memory_ids,
            &memory_graph_ids,
            &oag_ids,
        )?;

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
    ) -> Result<HashMap<String, String>, String> {
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
        let mut id_map: HashMap<String, String> = HashMap::with_capacity(rows.len());

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
            id_map.insert(old_id, new_id);
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

        Ok(id_map)
    }

    fn copy_branch_summary_ranges(
        &self,
        source_session_id: &str,
        branched_session_id: &str,
        memory_ids: &HashMap<String, String>,
    ) -> Result<(), String> {
        let mut stmt = self
            .conn
            .prepare("SELECT memory_id, world_id, character_id, bucket_index, bucket_start, bucket_end, scene_id, scene_name, turn_start, turn_end, created_at FROM memory_summary_ranges WHERE session_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![source_session_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, String>(10)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        for (
            memory_id,
            world_id,
            character_id,
            bucket_index,
            bucket_start,
            bucket_end,
            scene_id,
            scene_name,
            turn_start,
            turn_end,
            created_at,
        ) in rows
        {
            let new_memory_id = memory_ids
                .get(&memory_id)
                .ok_or_else(|| "总结区间引用了不存在的源记忆".to_string())?;
            self.conn
                .execute(
                    "INSERT INTO memory_summary_ranges (memory_id, world_id, session_id, character_id, bucket_index, bucket_start, bucket_end, scene_id, scene_name, turn_start, turn_end, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        new_memory_id,
                        world_id,
                        branched_session_id,
                        character_id,
                        bucket_index,
                        bucket_start,
                        bucket_end,
                        scene_id,
                        scene_name,
                        turn_start,
                        turn_end,
                        created_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn copy_branch_memory_graph(
        &self,
        source_session_id: &str,
        branched_session_id: &str,
    ) -> Result<HashMap<String, String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, world_id, name, name_normalized, entity_type, aliases_json, mention_count, first_seen_turn, last_seen_turn, created_at FROM memory_entities WHERE session_id = ?1")
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
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut entity_ids = HashMap::with_capacity(rows.len());
        for (
            old_id,
            world_id,
            name,
            name_normalized,
            entity_type,
            aliases_json,
            mention_count,
            first_seen_turn,
            last_seen_turn,
            created_at,
        ) in rows
        {
            let new_id = format!("entity-{}", uuid::Uuid::new_v4().simple());
            self.conn
                .execute(
                    "INSERT INTO memory_entities (id, world_id, session_id, name, name_normalized, entity_type, aliases_json, mention_count, first_seen_turn, last_seen_turn, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        new_id,
                        world_id,
                        branched_session_id,
                        name,
                        name_normalized,
                        entity_type,
                        aliases_json,
                        mention_count,
                        first_seen_turn,
                        last_seen_turn,
                        created_at,
                    ],
                )
                .map_err(|e| e.to_string())?;
            entity_ids.insert(old_id, new_id);
        }

        let mut stmt = self
            .conn
            .prepare("SELECT id, world_id, subject_entity_id, predicate, object_entity_id, object_text, valid_from_turn, invalid_at_turn, source, confidence, created_at, asserted_by, asserted_by_id, visibility, truth_status, witnesses_json FROM memory_relations WHERE session_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![source_session_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, f64>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, String>(15)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (
            old_id,
            world_id,
            subject_entity_id,
            predicate,
            object_entity_id,
            object_text,
            valid_from_turn,
            invalid_at_turn,
            source,
            confidence,
            created_at,
            asserted_by,
            asserted_by_id,
            visibility,
            truth_status,
            witnesses_json,
        ) in rows
        {
            let new_subject_id = entity_ids
                .get(&subject_entity_id)
                .cloned()
                .ok_or_else(|| "事实引用了不存在的源主体实体".to_string())?;
            let new_object_id = object_entity_id
                .as_ref()
                .map(|id| {
                    entity_ids
                        .get(id)
                        .cloned()
                        .ok_or_else(|| "事实引用了不存在的源客体实体".to_string())
                })
                .transpose()?;
            let new_relation_id = format!("relation-{}", uuid::Uuid::new_v4().simple());
            entity_ids.insert(old_id, new_relation_id.clone());
            self.conn
                .execute(
                    "INSERT INTO memory_relations (id, world_id, session_id, subject_entity_id, predicate, object_entity_id, object_text, valid_from_turn, invalid_at_turn, source, confidence, created_at, asserted_by, asserted_by_id, visibility, truth_status, witnesses_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                    params![
                        new_relation_id,
                        world_id,
                        branched_session_id,
                        new_subject_id,
                        predicate,
                        new_object_id,
                        object_text,
                        valid_from_turn,
                        invalid_at_turn,
                        source,
                        confidence,
                        created_at,
                        asserted_by,
                        asserted_by_id,
                        visibility,
                        truth_status,
                        witnesses_json,
                    ],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(entity_ids)
    }

    fn copy_branch_turn_journal(
        &self,
        source_session_id: &str,
        branched_session_id: &str,
        memory_ids: &HashMap<String, String>,
        memory_graph_ids: &HashMap<String, String>,
        oag_ids: &HashMap<String, String>,
    ) -> Result<(), String> {
        let rows = crate::services::oag::lifecycle::query_rows_json(
            self.conn,
            "SELECT * FROM turn_journal WHERE session_id = ?1 ORDER BY turn_index, created_at, id",
            &[&source_session_id],
        )?;
        let mut id_map = memory_ids.clone();
        id_map.extend(
            memory_graph_ids
                .iter()
                .map(|(old, new)| (old.clone(), new.clone())),
        );
        id_map.extend(oag_ids.iter().map(|(old, new)| (old.clone(), new.clone())));
        let mut copied = Vec::with_capacity(rows.len());
        for mut value in rows {
            let row = value
                .as_object_mut()
                .ok_or_else(|| "回合日志行不是对象".to_string())?;
            let old_id = row
                .get("id")
                .and_then(|value| value.as_str())
                .ok_or_else(|| "回合日志缺少 ID".to_string())?
                .to_string();
            row.insert(
                "id".to_string(),
                serde_json::Value::String(
                    uuid::Uuid::new_v5(
                        &uuid::Uuid::NAMESPACE_URL,
                        format!("dream-journal-branch:{source_session_id}:{branched_session_id}:{old_id}").as_bytes(),
                    )
                    .to_string(),
                ),
            );
            row.insert(
                "session_id".to_string(),
                serde_json::Value::String(branched_session_id.to_string()),
            );
            if let Some(serde_json::Value::String(payload)) = row.get_mut("payload_json") {
                let mut value: serde_json::Value = serde_json::from_str(payload)
                    .map_err(|error| format!("分支回合日志 JSON 无效：{error}"))?;
                crate::services::oag::lifecycle::remap_branch_value(
                    &mut value,
                    &id_map,
                    source_session_id,
                    branched_session_id,
                );
                *payload = value.to_string();
            }
            copied.push(value);
        }
        crate::services::oag::lifecycle::insert_rows(self.conn, "turn_journal", &copied)
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
                    .ok_or_else(|| "会话角色归属无效".to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::{AssetSelection, SceneRuntime, SessionState};

    fn setup() -> (Connection, SessionSnapshot) {
        let conn = Connection::open_in_memory().expect("open sqlite");
        crate::db::schema::create_tables(&conn).expect("create schema");
        crate::db::oag_schema::create(&conn).expect("create OAG schema");
        conn.pragma_update(None, "foreign_keys", true)
            .expect("enable foreign keys");
        let session = SessionSnapshot {
            id: "session-source".to_string(),
            world_name: "测试世界".to_string(),
            location: "Dock".to_string(),
            time_label: "Day 1".to_string(),
            current_speaker: "Alice".to_string(),
            current_line: "Hello".to_string(),
            player_character_id: "player".to_string(),
            player_character_name: "Player".to_string(),
            visible_characters: vec!["Alice".to_string()],
            messages: vec![],
            player_stats: vec![],
            map_graph_nodes: vec![],
            map_graph_edges: vec![],
            system_log: vec![],
            scene: SceneRuntime::default(),
            assets: AssetSelection::default(),
            state: SessionState::default(),
            generation_params: Default::default(),
        };
        crate::db::repositories::session_repo::SessionRepository::new(&conn)
            .upsert(&session)
            .expect("upsert session");
        (conn, session)
    }

    #[test]
    fn branch_copies_memories_summaries_and_fact_graph_with_new_ids() {
        let (conn, session) = setup();
        let repository = SaveRepository::new(&conn);
        let source_save = SaveSummary {
            id: "save-source".to_string(),
            session_id: session.id.clone(),
            title: "Source".to_string(),
            world_name: session.world_name.clone(),
            updated_at: "2026-11-26 10:00".to_string(),
            progress: "Opening".to_string(),
            summary: "Summary".to_string(),
            player_character_name: Some("Player".to_string()),
            parent_save_id: None,
            branch_root_save_id: None,
            branch_label: None,
            turn_index: 4,
        };
        repository.upsert(&source_save).expect("upsert save");
        conn.execute_batch(
            "INSERT INTO memories (id, world_id, session_id, character_id, content, turn_index, memory_type, scene_id)
             VALUES ('memory-summary', 'world-1', 'session-source', 'char-alice', '旧桶总结', 10, 'summary', 'scene-1'),
                    ('memory-raw', 'world-1', 'session-source', 'char-alice', '原始记忆', 4, 'dialogue', 'scene-1');
             INSERT INTO memory_embeddings (memory_id, model_key, vector_json, updated_at)
             VALUES ('memory-summary', 'model-1', '[1,0]', 'now'), ('memory-raw', 'model-1', '[0,1]', 'now');
             INSERT INTO memory_summary_ranges (memory_id, world_id, session_id, character_id, bucket_index, bucket_start, bucket_end, scene_id, scene_name, turn_start, turn_end, created_at)
             VALUES ('memory-summary', 'world-1', 'session-source', 'char-alice', 0, 1, 10, 'scene-1', 'Dock', 1, 10, 'now');
             INSERT INTO memory_entities (id, world_id, session_id, name, name_normalized, created_at)
             VALUES ('entity-key', 'world-1', 'session-source', '银钥匙', '银钥匙', 'now'),
                    ('entity-box', 'world-1', 'session-source', '柜子', '柜子', 'now');
             INSERT INTO memory_relations (id, world_id, session_id, subject_entity_id, predicate, object_entity_id, object_text, valid_from_turn, source, confidence, created_at, asserted_by, asserted_by_id, visibility, truth_status, witnesses_json)
             VALUES ('relation-key', 'world-1', 'session-source', 'entity-key', '藏在', 'entity-box', '', 4, 'llm_extraction', 0.9, 'now', 'Alice', 'char-alice', 'private', 'asserted', '[\"Alice\",\"Player\"]');
             INSERT INTO oag_objects (id, world_id, session_id, object_type, canonical_key, name, name_normalized, created_turn, updated_turn)
             VALUES ('oag-source', 'world-1', 'session-source', 'person', 'runtime-character:char-alice', 'Alice', 'alice', 1, 1),
                    ('oag-canon', 'world-1', '', 'person', 'canon-alice', 'Canon Alice', 'canon alice', 0, 0);
             INSERT INTO oag_events (id, world_id, session_id, turn_index, dedupe_key)
             VALUES ('event-source', 'world-1', 'session-source', 4, 'event-source');
             UPDATE oag_objects SET source_event_id = 'event-source' WHERE id = 'oag-source';
             INSERT INTO oag_attributes (id, world_id, session_id, object_id, key, value_json, valid_from_turn, valid_to_turn)
             VALUES ('attr-source', 'world-1', 'session-source', 'oag-source', 'health', '10', 1, 3);
             INSERT INTO oag_memory_objects (memory_id, object_id, role) VALUES ('memory-raw', 'oag-source', 'mentioned');",
        )
        .expect("insert memory graph");
        conn.execute(
            "INSERT INTO turn_journal (id, session_id, turn_index, step, status, payload_json, created_at)
             VALUES ('journal-source', 'session-source', 4, 'snapshot_created', 'completed', ?1, 'now')",
            params![serde_json::json!({
                "session_snapshot": session,
                "memory_entities": [{ "id": "entity-key" }],
                "memory_relations": [{ "id": "relation-key", "subject_entity_id": "entity-key" }],
                "oag_snapshot": { "object_id": "oag-source", "event_id": "event-source", "memory_id": "memory-raw" }
            }).to_string()],
        )
        .expect("insert source journal");

        let branch = repository
            .branch_save("save-source", &session)
            .expect("branch save");
        let branch_id = branch.session_id.as_str();
        assert_ne!(branch_id, session.id);
        assert_eq!(branch.turn_index, 4);
        let journal: (i64, String) = conn
            .query_row(
                "SELECT MAX(turn_index), payload_json FROM turn_journal WHERE session_id = ?1 AND step = 'snapshot_created'",
                [branch_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("branch journal");
        assert_eq!(journal.0, 4);
        assert!(!journal.1.contains("session-source"));
        assert!(!journal.1.contains("oag-source"));
        assert!(!journal.1.contains("memory-raw"));
        let journal_payload: serde_json::Value = serde_json::from_str(&journal.1).unwrap();
        assert_eq!(journal_payload["session_snapshot"]["id"], branch_id);
        assert_ne!(
            journal_payload["memory_entities"][0]["id"].as_str(),
            Some("entity-key")
        );
        let oag_object: (String, String, String) = conn
            .query_row(
                "SELECT o.id, o.source_event_id, a.object_id FROM oag_objects o JOIN oag_attributes a ON a.object_id = o.id WHERE o.session_id = ?1",
                [branch_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("branch OAG object");
        assert_ne!(oag_object.0, "oag-source");
        assert_ne!(oag_object.1, "event-source");
        assert_eq!(oag_object.0, oag_object.2);
        let oag_event: (String, String) = conn
            .query_row(
                "SELECT id, dedupe_key FROM oag_events WHERE session_id = ?1",
                [branch_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("branch OAG event");
        assert_eq!(oag_event.0, oag_event.1);
        let memory_link: (String, String) = conn
            .query_row(
                "SELECT m.id, mo.object_id FROM oag_memory_objects mo JOIN memories m ON m.id = mo.memory_id WHERE m.session_id = ?1",
                [branch_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("branch OAG memory link");
        assert_ne!(memory_link.0, "memory-raw");
        assert_eq!(memory_link.1, oag_object.0);
        let canon_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oag_objects WHERE id = 'oag-canon' AND session_id = ''",
                [],
                |row| row.get(0),
            )
            .expect("canon count");
        assert_eq!(canon_count, 1);
        let memory_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE session_id = ?1",
                [branch_id],
                |row| row.get(0),
            )
            .expect("memory count");
        let embedding_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM memory_embeddings e JOIN memories m ON m.id = e.memory_id WHERE m.session_id = ?1", [branch_id], |row| row.get(0))
            .expect("embedding count");
        assert_eq!((memory_count, embedding_count), (2, 2));
        let (range_memory_id, bucket_end): (String, i64) = conn
            .query_row(
                "SELECT memory_id, bucket_end FROM memory_summary_ranges WHERE session_id = ?1",
                [branch_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("summary range");
        assert_ne!(range_memory_id, "memory-summary");
        assert_eq!(bucket_end, 10);
        let summary_memory_type: String = conn
            .query_row(
                "SELECT memory_type FROM memories WHERE id = ?1",
                [&range_memory_id],
                |row| row.get(0),
            )
            .expect("summary memory");
        assert_eq!(summary_memory_type, "summary");

        let graph: (String, String, Option<String>, String, Option<String>, String, String) = conn
            .query_row(
                "SELECT r.id, r.subject_entity_id, r.object_entity_id, r.asserted_by, r.asserted_by_id, r.visibility, r.truth_status FROM memory_relations r WHERE r.session_id = ?1",
                [branch_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
            )
            .expect("branch relation");
        assert_eq!(graph.3, "Alice");
        assert_eq!(graph.4.as_deref(), Some("char-alice"));
        assert_eq!(graph.5, "private");
        assert_eq!(graph.6, "asserted");
        assert_ne!(graph.0, "relation-key");
        assert_ne!(graph.1, "entity-key");
        assert_ne!(graph.2.as_deref(), Some("entity-box"));
        let subject_session: String = conn
            .query_row(
                "SELECT session_id FROM memory_entities WHERE id = ?1",
                [&graph.1],
                |row| row.get(0),
            )
            .expect("subject session");
        let object_session: String = conn
            .query_row(
                "SELECT session_id FROM memory_entities WHERE id = ?1",
                [graph.2.as_deref().expect("object entity")],
                |row| row.get(0),
            )
            .expect("object session");
        assert_eq!(
            (subject_session.as_str(), object_session.as_str()),
            (branch_id, branch_id)
        );
        let witnesses: String = conn
            .query_row(
                "SELECT witnesses_json FROM memory_relations WHERE session_id = ?1",
                [branch_id],
                |row| row.get(0),
            )
            .expect("witnesses");
        assert_eq!(witnesses, "[\"Alice\",\"Player\"]");

        conn.execute(
            "UPDATE memory_relations SET invalid_at_turn = 12 WHERE session_id = ?1",
            [branch_id],
        )
        .expect("change branch fact");
        let source_invalid_at: Option<i64> = conn
            .query_row(
                "SELECT invalid_at_turn FROM memory_relations WHERE id = 'relation-key'",
                [],
                |row| row.get(0),
            )
            .expect("source fact");
        assert_eq!(source_invalid_at, None);
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("integrity check");
        assert_eq!(integrity, "ok");
    }
}
