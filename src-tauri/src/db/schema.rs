use rusqlite::Connection;

use crate::db::seeds;

pub fn create_tables(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS worlds (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            genre TEXT NOT NULL DEFAULT '',
            background_prompt TEXT NOT NULL DEFAULT '',
            opening_scene TEXT NOT NULL DEFAULT '',
            summary TEXT NOT NULL DEFAULT '',
            time_system TEXT NOT NULL DEFAULT '',
            map_nodes_json TEXT NOT NULL DEFAULT '[]',
            time_config_json TEXT NOT NULL DEFAULT '{}',
            director_config_json TEXT NOT NULL DEFAULT '{}',
            ui_theme_config_json TEXT NOT NULL DEFAULT '{}',
            director_system_prompt_base TEXT NOT NULL DEFAULT '',
            director_runtime_system_prompt TEXT NOT NULL DEFAULT '',
            opening_messages_json TEXT NOT NULL DEFAULT '[]',
            opening_character_ids_json TEXT NOT NULL DEFAULT '[]',
            player_character_id TEXT
        );

        CREATE TABLE IF NOT EXISTS world_records (
            id TEXT PRIMARY KEY,
            world_id TEXT NOT NULL,
            collection TEXT NOT NULL,
            data_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '',
            FOREIGN KEY (world_id) REFERENCES worlds(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS knowledge_bases (
            id TEXT PRIMARY KEY,
            world_id TEXT REFERENCES worlds(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1,
            chunk_strategy TEXT NOT NULL DEFAULT 'length',
            chunk_max_chars INTEGER NOT NULL DEFAULT 800,
            chunk_overlap_chars INTEGER NOT NULL DEFAULT 120,
            chunk_delimiter TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS knowledge_documents (
            id TEXT PRIMARY KEY,
            knowledge_base_id TEXT NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            source_filename TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL,
            audience TEXT NOT NULL DEFAULT 'public',
            audience_character_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_documents_base ON knowledge_documents(knowledge_base_id);
        CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_documents_fts USING fts5(
            document_id UNINDEXED,
            content,
            tokenize = 'trigram'
        );
        CREATE TABLE IF NOT EXISTS knowledge_chunk_embeddings (
            document_id TEXT NOT NULL REFERENCES knowledge_documents(id) ON DELETE CASCADE,
            chunk_index INTEGER NOT NULL,
            model_key TEXT NOT NULL,
            vector_json TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (document_id, chunk_index, model_key)
        );
        CREATE TABLE IF NOT EXISTS world_knowledge_bindings (
            world_id TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
            knowledge_base_id TEXT NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
            PRIMARY KEY (world_id, knowledge_base_id)
        );

        CREATE TABLE IF NOT EXISTS sticker_packs (
            id TEXT PRIMARY KEY,
            world_id TEXT REFERENCES worlds(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS stickers (
            id TEXT PRIMARY KEY,
            pack_id TEXT NOT NULL REFERENCES sticker_packs(id) ON DELETE CASCADE,
            key TEXT NOT NULL,
            label TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            asset_path TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '',
            UNIQUE (pack_id, key)
        );
        CREATE INDEX IF NOT EXISTS idx_stickers_pack ON stickers(pack_id, sort_order, id);
        CREATE TABLE IF NOT EXISTS world_sticker_bindings (
            world_id TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
            pack_id TEXT NOT NULL REFERENCES sticker_packs(id) ON DELETE CASCADE,
            PRIMARY KEY (world_id, pack_id)
        );

        CREATE INDEX IF NOT EXISTS idx_world_records_scope
            ON world_records(world_id, collection, updated_at);

        CREATE TABLE IF NOT EXISTS scoped_kv (
            owner_type TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            namespace TEXT NOT NULL,
            key TEXT NOT NULL,
            value_json TEXT NOT NULL DEFAULT 'null',
            updated_at TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (owner_type, owner_id, namespace, key)
        );

        CREATE TABLE IF NOT EXISTS world_feature_grants (
            world_id TEXT NOT NULL,
            feature TEXT NOT NULL,
            granted INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (world_id, feature)
        );

        CREATE INDEX IF NOT EXISTS idx_scoped_kv_scope
            ON scoped_kv(owner_type, owner_id, namespace, updated_at);

        CREATE TABLE IF NOT EXISTS characters (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            world_id TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT '',
            background_prompt TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            memory_strategy TEXT NOT NULL DEFAULT 'default',
            recent_dialogue_rounds INTEGER NOT NULL DEFAULT 30,
            attributes_json TEXT NOT NULL DEFAULT '[]',
            portrait_assets_json TEXT NOT NULL DEFAULT '[]',
            avatar_asset TEXT NOT NULL DEFAULT '',
            system_prompt_template TEXT NOT NULL DEFAULT '',
            response_contract_prompt TEXT NOT NULL DEFAULT '',
            narration_prompt TEXT NOT NULL DEFAULT '',
            runtime_system_prompt TEXT NOT NULL DEFAULT '',
            knowledge_base_ids_json TEXT NOT NULL DEFAULT '[]',
            FOREIGN KEY (world_id) REFERENCES worlds(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            world_name TEXT NOT NULL DEFAULT '',
            location TEXT NOT NULL DEFAULT '',
            time_label TEXT NOT NULL DEFAULT '',
            current_speaker TEXT NOT NULL DEFAULT '',
            current_line TEXT NOT NULL DEFAULT '',
            player_character_id TEXT NOT NULL DEFAULT '',
            player_character_name TEXT NOT NULL DEFAULT '',
            visible_characters_json TEXT NOT NULL DEFAULT '[]',
            messages_json TEXT NOT NULL DEFAULT '[]',
            player_stats_json TEXT NOT NULL DEFAULT '[]',
            map_graph_nodes_json TEXT NOT NULL DEFAULT '[]',
            map_graph_edges_json TEXT NOT NULL DEFAULT '[]',
            system_log_json TEXT NOT NULL DEFAULT '[]',
            scene_json TEXT NOT NULL DEFAULT '{}',
            assets_json TEXT NOT NULL DEFAULT '{}',
            state_json TEXT NOT NULL DEFAULT '{}',
            generation_params_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE TABLE IF NOT EXISTS saves (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL DEFAULT '',
            world_name TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '',
            progress TEXT NOT NULL DEFAULT '',
            summary TEXT NOT NULL DEFAULT '',
            player_character_name TEXT,
            parent_save_id TEXT,
            branch_root_save_id TEXT,
            branch_label TEXT,
            turn_index INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            world_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            character_id TEXT NOT NULL,
            layer TEXT NOT NULL DEFAULT 'working',
            content TEXT NOT NULL DEFAULT '',
            source TEXT NOT NULL DEFAULT '',
            importance REAL NOT NULL DEFAULT 0.5,
            created_at TEXT NOT NULL DEFAULT '',
            turn_index INTEGER NOT NULL DEFAULT 0,
            conversation_id TEXT,
            event_id TEXT,
            item_id TEXT,
            scene_id TEXT,
            memory_type TEXT NOT NULL DEFAULT 'dialogue',
            speaker TEXT,
            role TEXT,
            location TEXT,
            participants_json TEXT NOT NULL DEFAULT '[]',
            keywords_json TEXT NOT NULL DEFAULT '[]'
        );

        CREATE TABLE IF NOT EXISTS memory_embeddings (
            memory_id TEXT NOT NULL,
            model_key TEXT NOT NULL,
            vector_json TEXT NOT NULL DEFAULT '[]',
            updated_at TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (memory_id, model_key),
            FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
        );

        -- 固定桶记忆总结的元数据。总结正文仍存 memories(memory_type='summary')，
        -- 这样可以直接复用 memory_embeddings；这张表只记录桶/场景/回合范围。
        CREATE TABLE IF NOT EXISTS memory_summary_ranges (
            memory_id TEXT PRIMARY KEY,
            world_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            character_id TEXT NOT NULL,
            bucket_index INTEGER NOT NULL,
            bucket_start INTEGER NOT NULL,
            bucket_end INTEGER NOT NULL,
            scene_id TEXT NOT NULL DEFAULT '',
            scene_name TEXT NOT NULL DEFAULT '',
            turn_start INTEGER NOT NULL DEFAULT 0,
            turn_end INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT '',
            FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE,
            UNIQUE(session_id, character_id, bucket_index, scene_id)
        );

        CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_id);
        CREATE INDEX IF NOT EXISTS idx_memories_character ON memories(character_id);
        CREATE INDEX IF NOT EXISTS idx_memories_world ON memories(world_id);
        CREATE INDEX IF NOT EXISTS idx_memory_embeddings_model_key ON memory_embeddings(model_key);
        CREATE INDEX IF NOT EXISTS idx_memory_summary_ranges_lookup
            ON memory_summary_ranges(session_id, character_id, bucket_index);

        CREATE TABLE IF NOT EXISTS memory_entities (
            id TEXT PRIMARY KEY,
            world_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            name TEXT NOT NULL,
            name_normalized TEXT NOT NULL,
            entity_type TEXT NOT NULL DEFAULT '',
            aliases_json TEXT NOT NULL DEFAULT '[]',
            mention_count INTEGER NOT NULL DEFAULT 1,
            first_seen_turn INTEGER NOT NULL DEFAULT 0,
            last_seen_turn INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT '',
            UNIQUE(session_id, name_normalized)
        );

        CREATE TABLE IF NOT EXISTS memory_relations (
            id TEXT PRIMARY KEY,
            world_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            subject_entity_id TEXT NOT NULL,
            predicate TEXT NOT NULL,
            object_entity_id TEXT,
            object_text TEXT NOT NULL DEFAULT '',
            valid_from_turn INTEGER NOT NULL DEFAULT 0,
            invalid_at_turn INTEGER,
            source TEXT NOT NULL DEFAULT 'llm_extraction',
            confidence REAL NOT NULL DEFAULT 0.7,
            created_at TEXT NOT NULL DEFAULT '',
            asserted_by TEXT NOT NULL DEFAULT '',
            asserted_by_id TEXT,
            visibility TEXT NOT NULL DEFAULT 'scene',
            truth_status TEXT NOT NULL DEFAULT 'asserted',
            witnesses_json TEXT NOT NULL DEFAULT '[]',
            FOREIGN KEY (subject_entity_id) REFERENCES memory_entities(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_memory_entities_session ON memory_entities(session_id);
        CREATE INDEX IF NOT EXISTS idx_memory_relations_session ON memory_relations(session_id);
        CREATE INDEX IF NOT EXISTS idx_memory_relations_subject ON memory_relations(subject_entity_id);

        CREATE TABLE IF NOT EXISTS attribute_schemas (
            id TEXT PRIMARY KEY,
            scope TEXT NOT NULL,
            world_id TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0,
            key TEXT NOT NULL,
            label TEXT NOT NULL,
            value_type TEXT NOT NULL DEFAULT 'text',
            description TEXT NOT NULL DEFAULT '',
            default_value_json TEXT NOT NULL DEFAULT 'null',
            enum_options_json TEXT NOT NULL DEFAULT '[]',
            display_policy_json TEXT NOT NULL DEFAULT '{}',
            access_policy_json TEXT NOT NULL DEFAULT '{}',
            mutation_policy_json TEXT NOT NULL DEFAULT '{}',
            influence_policy_json TEXT NOT NULL DEFAULT '{}',
            projection_policy_json TEXT NOT NULL DEFAULT '{}',
            UNIQUE(scope, key, world_id)
        );

        CREATE TABLE IF NOT EXISTS attribute_values (
            id TEXT PRIMARY KEY,
            schema_id TEXT NOT NULL,
            owner_type TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            value_json TEXT NOT NULL DEFAULT 'null',
            source TEXT NOT NULL DEFAULT 'system',
            UNIQUE(schema_id, owner_type, owner_id),
            FOREIGN KEY (schema_id) REFERENCES attribute_schemas(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS model_configs (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            model_type TEXT NOT NULL DEFAULT 'text',
            provider TEXT NOT NULL DEFAULT '',
            model_id TEXT NOT NULL DEFAULT '',
            base_url TEXT NOT NULL DEFAULT '',
            api_key TEXT NOT NULL DEFAULT '',
            max_tokens INTEGER NOT NULL DEFAULT 1200,
            streaming_enabled INTEGER NOT NULL DEFAULT 1,
            is_default INTEGER NOT NULL DEFAULT 0,
            input_modalities TEXT NOT NULL DEFAULT '[]',
            json_mode_disabled INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS plugins (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            description TEXT NOT NULL DEFAULT '',
            hooks_json TEXT NOT NULL DEFAULT '[]'
        );

        CREATE TABLE IF NOT EXISTS mcp_tools (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            server_name TEXT NOT NULL DEFAULT '',
            tool_name TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1,
            exposure_policy_json TEXT NOT NULL DEFAULT '{}',
            risk_level TEXT NOT NULL DEFAULT 'low',
            trigger_keywords_json TEXT NOT NULL DEFAULT '[]',
            input_schema_json TEXT NOT NULL DEFAULT '{\"type\":\"object\",\"properties\":{}}',
            server_id TEXT NOT NULL DEFAULT '',
            impl_kind TEXT NOT NULL DEFAULT 'mcp',
            impl_config_json TEXT NOT NULL DEFAULT ''
        );

        -- MCP server 连接配置（第 7 项）。stdio 仅桌面端可用，http 全平台可用。
        CREATE TABLE IF NOT EXISTS mcp_servers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            transport TEXT NOT NULL DEFAULT 'http',
            command TEXT NOT NULL DEFAULT '',
            args_json TEXT NOT NULL DEFAULT '[]',
            env_json TEXT NOT NULL DEFAULT '{}',
            url TEXT NOT NULL DEFAULT '',
            headers_json TEXT NOT NULL DEFAULT '{}',
            auth_token TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1,
            timeout_ms INTEGER NOT NULL DEFAULT 15000,
            max_result_bytes INTEGER NOT NULL DEFAULT 32768
        );

        CREATE TABLE IF NOT EXISTS settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            text_model_provider TEXT NOT NULL DEFAULT 'openai',
            default_text_model TEXT NOT NULL DEFAULT 'gpt-4',
            image_model_provider TEXT NOT NULL DEFAULT 'automatic1111',
            default_image_workflow TEXT NOT NULL DEFAULT 'txt2img',
            embedding_enabled INTEGER NOT NULL DEFAULT 1,
            default_embedding_model TEXT NOT NULL DEFAULT 'BAAI/bge-small-zh-v1.5',
            home_background_strategy TEXT NOT NULL DEFAULT '',
            export_directory TEXT NOT NULL DEFAULT '',
            generation_params_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE TABLE IF NOT EXISTS agent_sessions (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            agent_type TEXT NOT NULL DEFAULT 'character',
            status TEXT NOT NULL DEFAULT 'pending_init',
            connection_state TEXT NOT NULL DEFAULT 'disconnected',
            scene_presence_state TEXT NOT NULL DEFAULT 'unknown',
            character_id TEXT,
            character_name TEXT,
            checkpoint_id TEXT,
            last_active_turn INTEGER NOT NULL DEFAULT 0,
            last_ack_message_index INTEGER NOT NULL DEFAULT 0,
            prompt_version TEXT NOT NULL DEFAULT '',
            runtime_key TEXT NOT NULL DEFAULT '',
            initialized_at TEXT,
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '',
            UNIQUE(session_id, runtime_key)
        );

        CREATE TABLE IF NOT EXISTS agent_checkpoints (
            id TEXT PRIMARY KEY,
            agent_session_id TEXT NOT NULL,
            turn_index INTEGER NOT NULL DEFAULT 0,
            checkpoint_type TEXT NOT NULL DEFAULT 'turn_state',
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT '',
            FOREIGN KEY (agent_session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS turn_journal (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            turn_index INTEGER NOT NULL DEFAULT 0,
            step TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'created',
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS prompt_call_traces (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            turn_index INTEGER NOT NULL DEFAULT 0,
            step TEXT NOT NULL DEFAULT '',
            recipient_type TEXT NOT NULL DEFAULT '',
            recipient_name TEXT NOT NULL DEFAULT '',
            prompt_call_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT ''
        );

        CREATE INDEX IF NOT EXISTS idx_prompt_call_traces_session_turn
            ON prompt_call_traces(session_id, turn_index, created_at);

        CREATE TABLE IF NOT EXISTS llm_call_traces (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            turn_index INTEGER NOT NULL DEFAULT 0,
            step TEXT NOT NULL DEFAULT '',
            speaker TEXT NOT NULL DEFAULT '',
            provider TEXT NOT NULL DEFAULT '',
            model_id TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'completed',
            latency_ms INTEGER NOT NULL DEFAULT 0,
            input_payload_json TEXT NOT NULL DEFAULT '{}',
            output_payload_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT ''
        );

        CREATE INDEX IF NOT EXISTS idx_llm_call_traces_session_turn
            ON llm_call_traces(session_id, turn_index, created_at);

        CREATE TABLE IF NOT EXISTS world_logic_traces (
            trace_id TEXT PRIMARY KEY,
            request_id TEXT NOT NULL,
            world_id TEXT NOT NULL DEFAULT '',
            session_id TEXT NOT NULL DEFAULT '',
            actor TEXT NOT NULL DEFAULT '',
            handler TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'completed',
            latency_ms INTEGER NOT NULL DEFAULT 0,
            input_json TEXT NOT NULL DEFAULT '{}',
            result_json TEXT NOT NULL DEFAULT '{}',
            error TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT ''
        );

        CREATE INDEX IF NOT EXISTS idx_world_logic_traces_session_time
            ON world_logic_traces(session_id, created_at, trace_id);

        CREATE TABLE IF NOT EXISTS game_turn_states (
            session_id TEXT PRIMARY KEY,
            world_id TEXT NOT NULL DEFAULT '',
            phase TEXT NOT NULL DEFAULT '',
            active_actor TEXT NOT NULL DEFAULT '',
            turn_index INTEGER NOT NULL DEFAULT 0,
            sequence INTEGER NOT NULL DEFAULT 0,
            terminal INTEGER NOT NULL DEFAULT 0,
            last_handler TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT ''
        );

        CREATE INDEX IF NOT EXISTS idx_game_turn_states_world
            ON game_turn_states(world_id, updated_at);

        CREATE TABLE IF NOT EXISTS llm_retry_capsules (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            turn_index INTEGER NOT NULL DEFAULT 0,
            message_id TEXT NOT NULL DEFAULT '',
            retry_token TEXT NOT NULL DEFAULT '',
            stage TEXT NOT NULL DEFAULT '',
            provider TEXT NOT NULL DEFAULT '',
            model_id TEXT NOT NULL DEFAULT '',
            request_json TEXT NOT NULL DEFAULT '{}',
            prompt_trace_json TEXT NOT NULL DEFAULT '{}',
            input_snapshot_json TEXT NOT NULL DEFAULT '{}',
            tool_context_json TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL DEFAULT 'active',
            created_at TEXT NOT NULL DEFAULT '',
            consumed_at TEXT
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_llm_retry_capsules_retry_token
            ON llm_retry_capsules(retry_token);

        CREATE INDEX IF NOT EXISTS idx_llm_retry_capsules_session_status
            ON llm_retry_capsules(session_id, status, created_at);

        CREATE TABLE IF NOT EXISTS scheduled_notifications (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL DEFAULT '',
            world_name TEXT NOT NULL DEFAULT '',
            source TEXT NOT NULL DEFAULT '',
            title TEXT NOT NULL DEFAULT '',
            body TEXT NOT NULL DEFAULT '',
            scheduled_at TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT '',
            fired_at TEXT,
            status TEXT NOT NULL DEFAULT 'scheduled',
            metadata_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE INDEX IF NOT EXISTS idx_scheduled_notifications_status_time
            ON scheduled_notifications(status, scheduled_at);

        CREATE INDEX IF NOT EXISTS idx_scheduled_notifications_session
            ON scheduled_notifications(session_id, status, scheduled_at);

        CREATE UNIQUE INDEX IF NOT EXISTS idx_scheduled_notifications_session_source
            ON scheduled_notifications(session_id, source);

        INSERT OR IGNORE INTO settings (id) VALUES (1);

        CREATE TABLE IF NOT EXISTS seed_world_tombstones (
            world_id TEXT PRIMARY KEY,
            created_at TEXT NOT NULL DEFAULT ''
        );
    ",
    )?;

    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_attribute_schemas_world ON attribute_schemas(scope, world_id, sort_order, key);")?;
    ensure_column(
        conn,
        "knowledge_bases",
        "chunk_strategy",
        "TEXT NOT NULL DEFAULT 'length'",
    )?;
    ensure_column(
        conn,
        "knowledge_bases",
        "chunk_max_chars",
        "INTEGER NOT NULL DEFAULT 800",
    )?;
    ensure_column(
        conn,
        "knowledge_bases",
        "chunk_overlap_chars",
        "INTEGER NOT NULL DEFAULT 120",
    )?;
    ensure_column(
        conn,
        "knowledge_documents",
        "audience",
        "TEXT NOT NULL DEFAULT 'public'",
    )?;
    ensure_column(conn, "knowledge_documents", "audience_character_id", "TEXT")?;
    ensure_column(
        conn,
        "knowledge_bases",
        "chunk_delimiter",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "characters",
        "knowledge_base_ids_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(
        conn,
        "memory_relations",
        "asserted_by",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(conn, "memory_relations", "asserted_by_id", "TEXT")?;
    ensure_column(
        conn,
        "memory_relations",
        "visibility",
        "TEXT NOT NULL DEFAULT 'scene'",
    )?;
    ensure_column(
        conn,
        "memory_relations",
        "truth_status",
        "TEXT NOT NULL DEFAULT 'asserted'",
    )?;
    ensure_column(
        conn,
        "memory_relations",
        "witnesses_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    migrate_world_knowledge_bindings(conn)?;
    seeds::ensure_all(conn)?;
    // 兜底补齐 FTS5 索引：放在 seeds 之后，覆盖内置世界带入的知识库文档。
    backfill_knowledge_fts(conn)?;

    Ok(())
}

/// 把 `knowledge_documents` 里缺失的行补进 FTS5 词法索引。
/// 索引存的是 `normalize_memory_text` 归一化后的正文（去掉空白/标点），与检索时的
/// 词法命中口径一致。正文靠文档增删改显式维护，这里只在启动时兜底补齐历史数据
/// （例如首次引入 FTS5 前的文档）。
fn backfill_knowledge_fts(conn: &Connection) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, title, content FROM knowledge_documents d
         WHERE d.id NOT IN (SELECT document_id FROM knowledge_documents_fts)",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for (id, title, content) in rows {
        let normalized = crate::services::game_engine::memory::knowledge_fts_index_text(&format!(
            "{title} {content}"
        ));
        if normalized.is_empty() {
            continue;
        }
        conn.execute(
            "INSERT INTO knowledge_documents_fts (document_id, content) VALUES (?1, ?2)",
            rusqlite::params![id, normalized],
        )?;
    }
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), rusqlite::Error> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
        [table, column],
        |row| row.get(0),
    )?;
    if exists == 0 {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition};"
        ))?;
    }
    Ok(())
}

/// 把旧配置迁到 `director_config.knowledge_base_ids`。
///
/// 旧版里世界自带的库自动可用、公共库靠 `world_knowledge_bindings` 绑定；
/// 新版两者都要在世界主控编辑页勾选。为了不让老存档「升级后突然查不到资料」，
/// 迁移时把「世界自带的全部库 + 旧绑定表里的公共库」一起写进选择列表。
/// 只在世界还没有该字段时执行，避免覆盖用户后来在编辑器里的选择。
fn migrate_world_knowledge_bindings(conn: &Connection) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, director_config_json FROM worlds WHERE json_type(director_config_json, '$.knowledge_base_ids') IS NULL",
    )?;
    let worlds = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if worlds.is_empty() {
        return Ok(());
    }

    // 世界自带的库：迁移后默认全部勾选，保持升级前「自动可用」的行为。
    let mut owned_stmt = conn.prepare(
        "SELECT id FROM knowledge_bases WHERE world_id = ?1 AND enabled = 1 ORDER BY id",
    )?;
    let mut binding_stmt = conn.prepare(
        "SELECT b.knowledge_base_id FROM world_knowledge_bindings b JOIN knowledge_bases k ON k.id = b.knowledge_base_id WHERE b.world_id = ?1 AND k.world_id IS NULL AND k.enabled = 1 ORDER BY b.knowledge_base_id",
    )?;
    for (world_id, config_json) in worlds {
        let mut ids = owned_stmt
            .query_map([&world_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in binding_stmt
            .query_map([&world_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<String>, _>>()?
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        let mut config: serde_json::Value =
            serde_json::from_str(&config_json).unwrap_or_else(|_| serde_json::json!({}));
        if !config.is_object() {
            config = serde_json::json!({});
        }
        config["knowledge_base_ids"] = serde_json::json!(ids);
        conn.execute(
            "UPDATE worlds SET director_config_json = ?2 WHERE id = ?1",
            rusqlite::params![world_id, serde_json::to_string(&config).unwrap_or_default()],
        )?;
    }
    Ok(())
}
