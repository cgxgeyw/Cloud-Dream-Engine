pub mod feihualing_world;
pub mod schedule_assistant_world;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::models::mcp_tool::MCP_TOOL_SCHEDULE_NOTIFICATION_ID;

use feihualing_world::*;
use schedule_assistant_world::*;

fn default_seed_world_director_config_json() -> String {
    serde_json::json!({
        "allow_scene_transition": true,
        "allow_npc_spawn": true,
        "history_dialogue_rounds": 6,
        "director_tool_loop_limit": 6,
        "world_director_prompt": "",
        "prompt_presets": [],
        "return_processing_rules": [],
        "allowed_mcp_tool_ids": [
            "mcp-tool-list-scenes",
            "mcp-tool-list-characters",
            "mcp-tool-change-scene",
            "mcp-tool-switch-player-character",
            "mcp-tool-image-generation"
        ]
    })
    .to_string()
}

const SEED_WORLD_TENSION_LABEL: &str = "世界紧张度";
const SEED_WORLD_TENSION_DESCRIPTION: &str = "用于描述当前世界叙事压力的数值属性。";
const SEED_CHARACTER_TRUST_LABEL: &str = "信任度";
const SEED_CHARACTER_TRUST_DESCRIPTION: &str = "角色对玩家或当前局势的信任数值。";
const SCHEDULE_TODO_SCHEMA_ID: &str = "attr-schedule-assistant-todo-items";
const SCHEDULE_COMPLETED_SCHEMA_ID: &str = "attr-schedule-assistant-completed-items";
const SEED_RULE_NAME: &str = "气氛升温规则";
const SEED_RULE_DESCRIPTION: &str = "当世界紧张度较高时，进一步提升压力并追加阶段标签。";
fn sample_world_seeding_enabled() -> bool {
    true
}

fn seed_character_ids() -> [&'static str; 7] {
    [
        SEED_CHARACTER_SCHEDULE_ASSISTANT_ID,
        SEED_CHARACTER_GUEST_ID,
        SEED_CHARACTER_LIBAI_ID,
        SEED_CHARACTER_DUFU_ID,
        SEED_CHARACTER_WANGWEI_ID,
        SEED_CHARACTER_LIQINGZHAO_ID,
        SEED_CHARACTER_SUSHI_ID,
    ]
}

fn insert_seed_world(
    conn: &Connection,
    id: &str,
    name: &str,
    genre: &str,
    background_prompt: &str,
    opening_scene: &str,
    summary: &str,
    time_system: &str,
    map_nodes_json: String,
    triggers_json: String,
    time_config_json: String,
    director_config_json: String,
    ui_theme_config_json: String,
    opening_messages_json: String,
    opening_character_ids_json: String,
    player_character_id: Option<&str>,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "
        INSERT INTO worlds (
            id, name, genre, background_prompt, opening_scene, summary, time_system,
            map_nodes_json, triggers_json, time_config_json, director_config_json, ui_theme_config_json,
            director_system_prompt_base, director_runtime_system_prompt, opening_messages_json, opening_character_ids_json, player_character_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, '', '', ?13, ?14, ?15)
        ",
        params![
            id,
            name,
            genre,
            background_prompt,
            opening_scene,
            summary,
            time_system,
            map_nodes_json,
            triggers_json,
            time_config_json,
            director_config_json,
            ui_theme_config_json,
            opening_messages_json,
            opening_character_ids_json,
            player_character_id
        ],
    )?;
    Ok(())
}

fn update_seed_world(
    conn: &Connection,
    id: &str,
    name: &str,
    genre: &str,
    background_prompt: &str,
    opening_scene: &str,
    summary: &str,
    time_system: &str,
    map_nodes_json: String,
    triggers_json: String,
    time_config_json: String,
    director_config_json: String,
    ui_theme_config_json: String,
    opening_messages_json: String,
    opening_character_ids_json: String,
    player_character_id: Option<&str>,
) -> Result<(), rusqlite::Error> {
    let updated = conn.execute(
        "UPDATE worlds SET name = ?1, genre = ?2, background_prompt = ?3, opening_scene = ?4, summary = ?5, time_system = ?6, map_nodes_json = ?7, triggers_json = ?8, time_config_json = ?9, director_config_json = ?10, ui_theme_config_json = ?11, opening_messages_json = ?12, opening_character_ids_json = ?13, player_character_id = ?14 WHERE id = ?15",
        params![
            name,
            genre,
            background_prompt,
            opening_scene,
            summary,
            time_system,
            map_nodes_json,
            triggers_json,
            time_config_json,
            director_config_json,
            ui_theme_config_json,
            opening_messages_json,
            opening_character_ids_json,
            player_character_id,
            id
        ],
    )?;
    if updated == 0 {
        insert_seed_world(
            conn,
            id,
            name,
            genre,
            background_prompt,
            opening_scene,
            summary,
            time_system,
            map_nodes_json,
            triggers_json,
            time_config_json,
            director_config_json,
            ui_theme_config_json,
            opening_messages_json,
            opening_character_ids_json,
            player_character_id,
        )?;
    }
    Ok(())
}

fn insert_seed_character(
    conn: &Connection,
    id: &str,
    name: &str,
    world_id: &str,
    role: &str,
    background_prompt: &str,
    memory_strategy: &str,
    attributes_json: String,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "
        INSERT OR IGNORE INTO characters (
            id, name, world_id, role, background_prompt, model, memory_strategy, recent_dialogue_rounds,
            attributes_json, portrait_assets_json, runtime_system_prompt
        ) VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, 8, ?7, '[]', '')
        ",
        params![
            id,
            name,
            world_id,
            role,
            background_prompt,
            memory_strategy,
            attributes_json
        ],
    )?;
    Ok(())
}

fn update_seed_character(
    conn: &Connection,
    id: &str,
    name: &str,
    role: &str,
    background_prompt: &str,
    memory_strategy: &str,
    attributes_json: String,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE characters SET name = ?1, role = ?2, background_prompt = ?3, memory_strategy = ?4, attributes_json = ?5 WHERE id = ?6",
        params![
            name,
            role,
            background_prompt,
            memory_strategy,
            attributes_json,
            id
        ],
    )?;
    Ok(())
}

fn looks_like_corrupted_prompt(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.len() < 16 {
        return false;
    }

    let question_marks = trimmed.chars().filter(|ch| *ch == '?').count();
    if question_marks < 8 {
        return false;
    }

    !trimmed
        .chars()
        .any(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch))
}

fn repair_corrupted_world_prompts(conn: &Connection) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT id, director_config_json FROM worlds")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    for row in rows {
        let (world_id, director_config_json) = row?;
        let Ok(mut director_config) = serde_json::from_str::<Value>(&director_config_json) else {
            continue;
        };
        let Some(config) = director_config.as_object_mut() else {
            continue;
        };
        let current_prompt = config
            .get("world_director_prompt")
            .and_then(Value::as_str)
            .unwrap_or("");

        if !looks_like_corrupted_prompt(current_prompt) {
            continue;
        }

        config.insert(
            "world_director_prompt".to_string(),
            Value::String(String::new()),
        );

        let Ok(repaired_json) = serde_json::to_string(&director_config) else {
            continue;
        };

        conn.execute(
            "UPDATE worlds SET director_config_json = ?1 WHERE id = ?2",
            params![repaired_json, world_id],
        )?;
    }

    Ok(())
}

fn remove_retired_gwtw_seed_world(conn: &Connection) -> Result<(), rusqlite::Error> {
    let world_name = conn
        .query_row(
            "SELECT name FROM worlds WHERE id = ?1",
            params!["gwtw"],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(world_name) = world_name else {
        return Ok(());
    };
    let session_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE world_name = ?1",
        params![world_name],
        |row| row.get(0),
    )?;
    if session_count > 0 {
        return Ok(());
    }

    conn.execute(
        "DELETE FROM attribute_values WHERE owner_id IN (?1, ?2, ?3, ?4)",
        params![
            "character-seed-scarlett",
            "character-seed-ashley",
            "character-seed-rhett",
            "character-seed-melanie"
        ],
    )?;
    conn.execute("DELETE FROM memories WHERE world_id = ?1", params!["gwtw"])?;
    conn.execute("DELETE FROM worlds WHERE id = ?1", params!["gwtw"])?;
    Ok(())
}

fn clear_legacy_seed_character_model_overrides(conn: &Connection) -> Result<(), rusqlite::Error> {
    for character_id in seed_character_ids() {
        conn.execute(
            "
            UPDATE characters
            SET model = ''
            WHERE id = ?1
              AND TRIM(model) = 'gpt-4.1'
              AND NOT EXISTS (
                  SELECT 1
                  FROM model_configs
                  WHERE LOWER(TRIM(id)) = 'gpt-4.1'
                     OR LOWER(TRIM(model_id)) = 'gpt-4.1'
                     OR LOWER(TRIM(name)) = 'gpt-4.1'
              )
            ",
            params![character_id],
        )?;
    }

    Ok(())
}

fn ensure_default_plugins(conn: &Connection) -> Result<(), rusqlite::Error> {
    let default_plugins = [
        (
            "combat-plugin",
            "战斗扩展",
            1,
            "提供战斗相关的流程钩子，用于处理玩家行动前后的状态变化与战斗结算。",
            r#"["before_player_action","after_state_commit"]"#,
        ),
        (
            "inventory-plugin",
            "物品系统扩展",
            1,
            "提供物品记录与导出相关钩子，支持在玩家行动后同步更新背包文本内容。",
            r#"["after_player_action","on_export"]"#,
        ),
        (
            "world-rule-plugin",
            "世界规则扩展",
            0,
            "提供世界规则触发与说话人选择前的扩展钩子，用于承载额外规则逻辑。",
            r#"["before_speaker_selection","on_trigger_fired"]"#,
        ),
    ];

    for (id, name, enabled, description, hooks_json) in default_plugins {
        conn.execute(
            "
            INSERT INTO plugins (id, name, enabled, description, hooks_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                enabled = excluded.enabled,
                description = excluded.description,
                hooks_json = excluded.hooks_json
            ",
            params![id, name, enabled, description, hooks_json],
        )?;
    }

    Ok(())
}
fn ensure_builtin_mcp_tools(conn: &Connection) -> Result<(), rusqlite::Error> {
    let builtin_tools = [
        (
            "mcp-tool-image-generation",
            "图像生成",
            "根据当前世界上下文生成场景图、角色肖像或其他视觉素材。",
            "builtin-image-generation",
            "generate_image",
            1,
            r#""on-demand""#,
            "medium",
            r#"["background","portrait","generate image","scene image"]"#,
        ),
        (
            "mcp-tool-list-scenes",
            "列出场景",
            "读取当前世界地图与场景节点，列出可用场景及其基础信息。",
            "builtin-world-director",
            "list_scenes",
            1,
            r#""on-demand""#,
            "low",
            r#"["scene","map","location","list scenes"]"#,
        ),
        (
            "mcp-tool-list-characters",
            "列出角色",
            "读取当前世界中的角色列表，返回可参与互动的角色信息。",
            "builtin-world-director",
            "list_characters",
            1,
            r#""on-demand""#,
            "low",
            r#"["character","npc","list characters"]"#,
        ),
        (
            "mcp-tool-change-scene",
            "切换场景",
            "根据当前世界状态切换到目标场景，并同步场景描述与上下文。",
            "builtin-world-director",
            "change_scene",
            1,
            r#""on-demand""#,
            "medium",
            r#"["change scene","scene transition","enter scene"]"#,
        ),
        (
            "mcp-tool-switch-player-character",
            "切换玩家角色",
            "将当前玩家控制权切换到指定角色，并更新玩家角色上下文。",
            "builtin-world-director",
            "switch_player_character",
            1,
            r#""on-demand""#,
            "medium",
            r#"["switch player","switch character","possession","control character"]"#,
        ),
        (
            MCP_TOOL_SCHEDULE_NOTIFICATION_ID,
            "定时通知",
            "创建、查询、修改或删除系统级定时通知，用于提醒用户后续待办或行程安排。",
            "builtin-notification",
            "schedule_notification",
            1,
            r#""on-demand""#,
            "medium",
            r#"["notification","reminder","remind","notify","schedule","定时提醒","通知安排","行程提醒"]"#,
        ),
    ];

    for (
        id,
        name,
        description,
        server_name,
        tool_name,
        enabled,
        exposure_policy_json,
        risk_level,
        trigger_keywords_json,
    ) in builtin_tools
    {
        conn.execute(
            "
            INSERT INTO mcp_tools (
                id, name, description, server_name, tool_name, enabled, exposure_policy_json, risk_level, trigger_keywords_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                server_name = excluded.server_name,
                tool_name = excluded.tool_name,
                enabled = excluded.enabled,
                exposure_policy_json = excluded.exposure_policy_json,
                risk_level = excluded.risk_level,
                trigger_keywords_json = excluded.trigger_keywords_json
            ",
            params![
                id,
                name,
                description,
                server_name,
                tool_name,
                enabled,
                exposure_policy_json,
                risk_level,
                trigger_keywords_json
            ],
        )?;
    }

    Ok(())
}
fn ensure_core_seed_data(conn: &Connection) -> Result<(), rusqlite::Error> {
    if sample_world_seeding_enabled() {
        let world_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM worlds", [], |row| row.get(0))?;
        if world_count == 0 {
            insert_seed_world(
                conn,
                SEED_WORLD_POETRY_ID,
                SEED_WORLD_POETRY_NAME,
                SEED_WORLD_POETRY_GENRE,
                SEED_WORLD_POETRY_BACKGROUND_PROMPT,
                SEED_WORLD_POETRY_OPENING_SCENE,
                SEED_WORLD_POETRY_SUMMARY,
                SEED_WORLD_POETRY_TIME_SYSTEM,
                poetry_world_map_nodes_json(),
                poetry_world_triggers_json(),
                poetry_world_time_config_json(),
                default_seed_world_director_config_json(),
                poetry_world_ui_theme_config_json(),
                poetry_world_opening_messages_json(),
                poetry_world_opening_character_ids_json(),
                Some(SEED_CHARACTER_GUEST_ID),
            )?;
            insert_seed_character(
                conn,
                SEED_CHARACTER_GUEST_ID,
                SEED_CHARACTER_GUEST_NAME,
                SEED_WORLD_POETRY_ID,
                SEED_CHARACTER_GUEST_ROLE,
                SEED_CHARACTER_GUEST_BACKGROUND,
                SEED_CHARACTER_GUEST_MEMORY,
                poetry_guest_attributes_json(),
            )?;
            insert_seed_character(
                conn,
                SEED_CHARACTER_LIBAI_ID,
                SEED_CHARACTER_LIBAI_NAME,
                SEED_WORLD_POETRY_ID,
                SEED_CHARACTER_LIBAI_ROLE,
                SEED_CHARACTER_LIBAI_BACKGROUND,
                SEED_CHARACTER_LIBAI_MEMORY,
                libai_attributes_json(),
            )?;
            insert_seed_character(
                conn,
                SEED_CHARACTER_DUFU_ID,
                SEED_CHARACTER_DUFU_NAME,
                SEED_WORLD_POETRY_ID,
                SEED_CHARACTER_DUFU_ROLE,
                SEED_CHARACTER_DUFU_BACKGROUND,
                SEED_CHARACTER_DUFU_MEMORY,
                dufu_attributes_json(),
            )?;
            insert_seed_character(
                conn,
                SEED_CHARACTER_WANGWEI_ID,
                SEED_CHARACTER_WANGWEI_NAME,
                SEED_WORLD_POETRY_ID,
                SEED_CHARACTER_WANGWEI_ROLE,
                SEED_CHARACTER_WANGWEI_BACKGROUND,
                SEED_CHARACTER_WANGWEI_MEMORY,
                wangwei_attributes_json(),
            )?;
            insert_seed_character(
                conn,
                SEED_CHARACTER_LIQINGZHAO_ID,
                SEED_CHARACTER_LIQINGZHAO_NAME,
                SEED_WORLD_POETRY_ID,
                SEED_CHARACTER_LIQINGZHAO_ROLE,
                SEED_CHARACTER_LIQINGZHAO_BACKGROUND,
                SEED_CHARACTER_LIQINGZHAO_MEMORY,
                liqingzhao_attributes_json(),
            )?;
            insert_seed_character(
                conn,
                SEED_CHARACTER_SUSHI_ID,
                SEED_CHARACTER_SUSHI_NAME,
                SEED_WORLD_POETRY_ID,
                SEED_CHARACTER_SUSHI_ROLE,
                SEED_CHARACTER_SUSHI_BACKGROUND,
                SEED_CHARACTER_SUSHI_MEMORY,
                sushi_attributes_json(),
            )?;
        }
    }
    conn.execute(
        "DELETE FROM model_configs WHERE id IN (?1, ?2)",
        params!["model-seed-openai-text", "model-seed-a1111-image"],
    )?;
    let embedding_model_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM model_configs WHERE model_type = 'embedding'",
        [],
        |row| row.get(0),
    )?;
    if embedding_model_count == 0 {
        conn.execute(
            "
            INSERT INTO model_configs (id, name, model_type, provider, model_id, base_url, api_key, max_tokens, streaming_enabled, is_default)
            VALUES (?1, ?2, 'embedding', ?3, ?4, ?5, '', 512, 0, 1)
            ",
            params![
                "model-seed-bge-small-embedding",
                "闁告劕鎳愰悿?Embedding闁挎稒顑孉AI/bge-small-zh-v1.5",
                "builtin-local",
                "BAAI/bge-small-zh-v1.5",
                "",
            ],
        )?;
    }
    let schema_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM attribute_schemas", [], |row| {
            row.get(0)
        })?;
    if schema_count == 0 {
        conn.execute(
            "
            INSERT INTO attribute_schemas (
                id, scope, key, label, value_type, description, default_value_json, enum_options_json,
                display_policy_json, access_policy_json, mutation_policy_json, influence_policy_json, projection_policy_json
            ) VALUES (?1, 'world', 'world_tension', ?2, 'number', ?3, '0', '[]', ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                "attr-seed-world-tension",
                SEED_WORLD_TENSION_LABEL,
                SEED_WORLD_TENSION_DESCRIPTION,
                r#"{"editor_visible":true,"game_visible":true,"debug_visible":true}"#,
                r#"{"creator_read":true,"player_read":true,"director_read":true,"plugin_read":true}"#,
                r#"{"creator_write":true,"rule_write":true,"trigger_write":true,"player_action_write":true,"allowed_ops":["set","increment"]}"#,
                r#"{"speaker_selector":{"enabled":true,"mode":"weighted_factor","weight":0.6},"trigger_engine":{"enabled":true,"mode":"threshold"}}"#,
                r#"{"inherit_to_session":true,"session_owner_type":"session","mutable_in_session":true}"#,
            ],
        )?;
        conn.execute(
            "
            INSERT INTO attribute_schemas (
                id, scope, key, label, value_type, description, default_value_json, enum_options_json,
                display_policy_json, access_policy_json, mutation_policy_json, influence_policy_json, projection_policy_json
            ) VALUES (?1, 'character', 'trust_level', ?2, 'number', ?3, '0', '[]', ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                "attr-seed-character-trust",
                SEED_CHARACTER_TRUST_LABEL,
                SEED_CHARACTER_TRUST_DESCRIPTION,
                r#"{"editor_visible":true,"game_visible":true,"debug_visible":true}"#,
                r#"{"creator_read":true,"player_read":false,"agent_self_read":true,"director_read":true,"plugin_read":true}"#,
                r#"{"creator_write":true,"rule_write":true,"trigger_write":true,"player_action_write":true,"allowed_ops":["set","increment"]}"#,
                r#"{"prompt.character_self":{"enabled":true,"mode":"raw"},"speaker_selector":{"enabled":true,"mode":"weighted_factor","weight":0.8}}"#,
                r#"{"inherit_to_session":true,"session_owner_type":"session_character","mutable_in_session":true}"#,
            ],
        )?;
    }
    let rule_count: i64 = conn.query_row("SELECT COUNT(*) FROM rules", [], |row| row.get(0))?;
    if rule_count == 0 {
        conn.execute(
            "
            INSERT INTO rules (id, scope, name, enabled, priority, description, condition_json, effects_json)
            VALUES (?1, 'session', ?2, 1, 90, ?3, ?4, ?5)
            ",
            params![
                "rule-seed-lockdown-escalation",
                SEED_RULE_NAME,
                SEED_RULE_DESCRIPTION,
                r#"{"type":"attribute_threshold","attribute_key":"world_tension","operator":">=","value":40}"#,
                seed_rule_effects_json(),
            ],
        )?;
    }
    Ok(())
}
fn ensure_localized_builtin_content(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE attribute_schemas SET label = ?1, description = ?2 WHERE id = ?3",
        params![
            SEED_WORLD_TENSION_LABEL,
            SEED_WORLD_TENSION_DESCRIPTION,
            "attr-seed-world-tension"
        ],
    )?;
    conn.execute(
        "UPDATE attribute_schemas SET label = ?1, description = ?2 WHERE id = ?3",
        params![
            SEED_CHARACTER_TRUST_LABEL,
            SEED_CHARACTER_TRUST_DESCRIPTION,
            "attr-seed-character-trust"
        ],
    )?;
    conn.execute(
        "UPDATE rules SET name = ?1, description = ?2, effects_json = ?3 WHERE id = ?4",
        params![
            SEED_RULE_NAME,
            SEED_RULE_DESCRIPTION,
            seed_rule_effects_json(),
            "rule-seed-lockdown-escalation"
        ],
    )?;
    ensure_schedule_assistant_task_attribute_schemas(conn)?;
    update_seed_world(
        conn,
        SEED_WORLD_SCHEDULE_ASSISTANT_ID,
        SEED_WORLD_SCHEDULE_ASSISTANT_NAME,
        SEED_WORLD_SCHEDULE_ASSISTANT_GENRE,
        SEED_WORLD_SCHEDULE_ASSISTANT_BACKGROUND_PROMPT,
        SEED_WORLD_SCHEDULE_ASSISTANT_OPENING_SCENE,
        SEED_WORLD_SCHEDULE_ASSISTANT_SUMMARY,
        SEED_WORLD_SCHEDULE_ASSISTANT_TIME_SYSTEM,
        schedule_assistant_world_map_nodes_json(),
        schedule_assistant_world_triggers_json(),
        schedule_assistant_world_time_config_json(),
        schedule_assistant_world_director_config_json(),
        schedule_assistant_world_ui_theme_config_json(),
        schedule_assistant_world_opening_messages_json(),
        schedule_assistant_world_opening_character_ids_json(),
        None,
    )?;
    insert_seed_character(
        conn,
        SEED_CHARACTER_SCHEDULE_ASSISTANT_ID,
        SEED_CHARACTER_SCHEDULE_ASSISTANT_NAME,
        SEED_WORLD_SCHEDULE_ASSISTANT_ID,
        SEED_CHARACTER_SCHEDULE_ASSISTANT_ROLE,
        SEED_CHARACTER_SCHEDULE_ASSISTANT_BACKGROUND,
        SEED_CHARACTER_SCHEDULE_ASSISTANT_MEMORY,
        schedule_assistant_attributes_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_SCHEDULE_ASSISTANT_ID,
        SEED_CHARACTER_SCHEDULE_ASSISTANT_NAME,
        SEED_CHARACTER_SCHEDULE_ASSISTANT_ROLE,
        SEED_CHARACTER_SCHEDULE_ASSISTANT_BACKGROUND,
        SEED_CHARACTER_SCHEDULE_ASSISTANT_MEMORY,
        schedule_assistant_attributes_json(),
    )?;
    update_seed_world(
        conn,
        SEED_WORLD_POETRY_ID,
        SEED_WORLD_POETRY_NAME,
        SEED_WORLD_POETRY_GENRE,
        SEED_WORLD_POETRY_BACKGROUND_PROMPT,
        SEED_WORLD_POETRY_OPENING_SCENE,
        SEED_WORLD_POETRY_SUMMARY,
        SEED_WORLD_POETRY_TIME_SYSTEM,
        poetry_world_map_nodes_json(),
        poetry_world_triggers_json(),
        poetry_world_time_config_json(),
        default_seed_world_director_config_json(),
        poetry_world_ui_theme_config_json(),
        poetry_world_opening_messages_json(),
        poetry_world_opening_character_ids_json(),
        Some(SEED_CHARACTER_GUEST_ID),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_GUEST_ID,
        SEED_CHARACTER_GUEST_NAME,
        SEED_CHARACTER_GUEST_ROLE,
        SEED_CHARACTER_GUEST_BACKGROUND,
        SEED_CHARACTER_GUEST_MEMORY,
        poetry_guest_attributes_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_LIBAI_ID,
        SEED_CHARACTER_LIBAI_NAME,
        SEED_CHARACTER_LIBAI_ROLE,
        SEED_CHARACTER_LIBAI_BACKGROUND,
        SEED_CHARACTER_LIBAI_MEMORY,
        libai_attributes_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_DUFU_ID,
        SEED_CHARACTER_DUFU_NAME,
        SEED_CHARACTER_DUFU_ROLE,
        SEED_CHARACTER_DUFU_BACKGROUND,
        SEED_CHARACTER_DUFU_MEMORY,
        dufu_attributes_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_WANGWEI_ID,
        SEED_CHARACTER_WANGWEI_NAME,
        SEED_CHARACTER_WANGWEI_ROLE,
        SEED_CHARACTER_WANGWEI_BACKGROUND,
        SEED_CHARACTER_WANGWEI_MEMORY,
        wangwei_attributes_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_LIQINGZHAO_ID,
        SEED_CHARACTER_LIQINGZHAO_NAME,
        SEED_CHARACTER_LIQINGZHAO_ROLE,
        SEED_CHARACTER_LIQINGZHAO_BACKGROUND,
        SEED_CHARACTER_LIQINGZHAO_MEMORY,
        liqingzhao_attributes_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_SUSHI_ID,
        SEED_CHARACTER_SUSHI_NAME,
        SEED_CHARACTER_SUSHI_ROLE,
        SEED_CHARACTER_SUSHI_BACKGROUND,
        SEED_CHARACTER_SUSHI_MEMORY,
        sushi_attributes_json(),
    )?;
    Ok(())
}

fn ensure_schedule_assistant_task_attribute_schemas(conn: &Connection) -> Result<(), rusqlite::Error> {
    for (id, key, label, description) in [
        (
            SCHEDULE_TODO_SCHEMA_ID,
            "todo_items",
            "待办事项",
            "行程助手当前会话的未完成待办事项列表。",
        ),
        (
            SCHEDULE_COMPLETED_SCHEMA_ID,
            "completed_items",
            "已完成事项",
            "行程助手当前会话中用户已确认完成的事项列表。",
        ),
    ] {
        conn.execute(
            "INSERT INTO attribute_schemas (
                id, scope, key, label, value_type, description, default_value_json, enum_options_json,
                display_policy_json, access_policy_json, mutation_policy_json, influence_policy_json, projection_policy_json
            )
            VALUES (?1, 'session', ?2, ?3, 'list', ?4, '[]', '[]', ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                scope = 'session',
                key = excluded.key,
                label = excluded.label,
                value_type = 'list',
                description = excluded.description,
                default_value_json = excluded.default_value_json,
                display_policy_json = excluded.display_policy_json,
                access_policy_json = excluded.access_policy_json,
                mutation_policy_json = excluded.mutation_policy_json,
                influence_policy_json = excluded.influence_policy_json,
                projection_policy_json = excluded.projection_policy_json",
            params![
                id,
                key,
                label,
                description,
                r#"{"editor_visible":true,"game_visible":true,"debug_visible":true}"#,
                r#"{"creator_read":true,"player_read":true,"agent_self_read":true,"director_read":true,"plugin_read":true}"#,
                r#"{"creator_write":true,"rule_write":true,"trigger_write":true,"player_action_write":true,"allowed_ops":["set"]}"#,
                r#"{"prompt.director":{"enabled":true,"mode":"raw"},"ui.status_panel":{"enabled":true}}"#,
                r#"{"inherit_to_session":true,"session_owner_type":"session","mutable_in_session":true}"#,
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn ensure_all(conn: &Connection) -> Result<(), rusqlite::Error> {
    ensure_default_plugins(conn)?;
    ensure_builtin_mcp_tools(conn)?;
    remove_retired_gwtw_seed_world(conn)?;
    ensure_core_seed_data(conn)?;
    ensure_localized_builtin_content(conn)?;
    repair_corrupted_world_prompts(conn)?;
    clear_legacy_seed_character_model_overrides(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::feihualing_world;
    use super::schedule_assistant_world;
    use crate::models::mcp_tool::MCP_TOOL_SCHEDULE_NOTIFICATION_ID;
    use serde_json::Value;

    #[test]
    fn poetry_seed_ui_config_keeps_world_owned_ui_files() {
        let config =
            serde_json::from_str::<Value>(&feihualing_world::poetry_world_ui_theme_config_json())
                .unwrap();
        let desktop_source = config
            .get("desktop_file")
            .and_then(Value::as_str)
            .expect("poetry desktop UI source");
        let mobile_source = config
            .get("mobile_file")
            .and_then(Value::as_str)
            .expect("poetry mobile UI source");
        let desktop_file = serde_json::from_str::<Value>(desktop_source).unwrap();
        let mobile_file = serde_json::from_str::<Value>(mobile_source).unwrap();

        assert_eq!(
            desktop_file.pointer("/meta/name").and_then(Value::as_str),
            Some("Poetry Desktop UI")
        );
        assert_eq!(
            mobile_file.pointer("/meta/name").and_then(Value::as_str),
            Some("Default Mobile Narrative UI")
        );
    }

    #[test]
    fn schedule_assistant_seed_uses_agent_chat_notification_tool() {
        let config = serde_json::from_str::<Value>(
            &schedule_assistant_world::schedule_assistant_world_director_config_json(),
        )
        .unwrap();
        let allowed = config
            .get("allowed_mcp_tool_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        assert_eq!(config.get("service_mode").and_then(Value::as_str), Some("agent_chat"));
        assert_eq!(
            config.get("default_agent_id").and_then(Value::as_str),
            Some(schedule_assistant_world::SEED_CHARACTER_SCHEDULE_ASSISTANT_ID)
        );
        assert_eq!(allowed.len(), 1);
        assert!(allowed
            .iter()
            .any(|value| value.as_str() == Some(MCP_TOOL_SCHEDULE_NOTIFICATION_ID)));
    }

    #[test]
    fn schedule_assistant_seed_owns_dedicated_ui_files() {
        let config = serde_json::from_str::<Value>(
            &schedule_assistant_world::schedule_assistant_world_ui_theme_config_json(),
        )
        .unwrap();
        let desktop_source = config
            .get("desktop_file")
            .and_then(Value::as_str)
            .expect("schedule assistant desktop UI source");
        let mobile_source = config
            .get("mobile_file")
            .and_then(Value::as_str)
            .expect("schedule assistant mobile UI source");
        let desktop_file = serde_json::from_str::<Value>(desktop_source).unwrap();
        let mobile_file = serde_json::from_str::<Value>(mobile_source).unwrap();

        assert_eq!(
            desktop_file.pointer("/meta/name").and_then(Value::as_str),
            Some("Schedule Assistant Desktop UI")
        );
        assert_eq!(
            mobile_file.pointer("/meta/name").and_then(Value::as_str),
            Some("Schedule Assistant Mobile UI")
        );
    }
}
