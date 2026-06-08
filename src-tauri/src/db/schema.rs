use rusqlite::Connection;

use crate::db::migrations;
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
            triggers_json TEXT NOT NULL DEFAULT '[]',
            time_config_json TEXT NOT NULL DEFAULT '{}',
            director_config_json TEXT NOT NULL DEFAULT '{}',
            ui_theme_config_json TEXT NOT NULL DEFAULT '{}',
            director_system_prompt_base TEXT NOT NULL DEFAULT '',
            director_runtime_system_prompt TEXT NOT NULL DEFAULT '',
            opening_messages_json TEXT NOT NULL DEFAULT '[]',
            opening_character_ids_json TEXT NOT NULL DEFAULT '[]',
            player_character_id TEXT
        );

        CREATE TABLE IF NOT EXISTS characters (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            world_id TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT '',
            background_prompt TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            memory_strategy TEXT NOT NULL DEFAULT 'default',
            recent_dialogue_rounds INTEGER NOT NULL DEFAULT 10,
            attributes_json TEXT NOT NULL DEFAULT '[]',
            portrait_assets_json TEXT NOT NULL DEFAULT '[]',
            system_prompt_template TEXT NOT NULL DEFAULT '',
            response_contract_prompt TEXT NOT NULL DEFAULT '',
            narration_prompt TEXT NOT NULL DEFAULT '',
            runtime_system_prompt TEXT NOT NULL DEFAULT '',
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
            inventory_items_json TEXT NOT NULL DEFAULT '[]',
            system_log_json TEXT NOT NULL DEFAULT '[]',
            scene_json TEXT NOT NULL DEFAULT '{}',
            assets_json TEXT NOT NULL DEFAULT '{}',
            state_json TEXT NOT NULL DEFAULT '{}'
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

        CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_id);
        CREATE INDEX IF NOT EXISTS idx_memories_character ON memories(character_id);
        CREATE INDEX IF NOT EXISTS idx_memories_world ON memories(world_id);
        CREATE INDEX IF NOT EXISTS idx_memory_embeddings_model_key ON memory_embeddings(model_key);

        CREATE TABLE IF NOT EXISTS attribute_schemas (
            id TEXT PRIMARY KEY,
            scope TEXT NOT NULL,
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
            UNIQUE(scope, key)
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
            is_default INTEGER NOT NULL DEFAULT 0
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
            trigger_keywords_json TEXT NOT NULL DEFAULT '[]'
        );

        CREATE TABLE IF NOT EXISTS rules (
            id TEXT PRIMARY KEY,
            scope TEXT NOT NULL DEFAULT '',
            name TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            priority INTEGER NOT NULL DEFAULT 0,
            description TEXT NOT NULL DEFAULT '',
            condition_json TEXT NOT NULL DEFAULT '{}',
            effects_json TEXT NOT NULL DEFAULT '[]'
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
            export_directory TEXT NOT NULL DEFAULT ''
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
    ",
    )?;

    migrations::run(conn)?;
    seeds::ensure_all(conn)?;

    Ok(())
}
