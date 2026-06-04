pub mod feihualing_world;
pub mod piao_world;

use rusqlite::{params, Connection};
use serde_json::Value;

use feihualing_world::*;
use piao_world::*;

const SEED_WORLD_TENSION_LABEL: &str = "世界紧张度";
const SEED_WORLD_TENSION_DESCRIPTION: &str = "用于描述当前世界叙事压力的数值属性。";
const SEED_CHARACTER_TRUST_LABEL: &str = "信任度";
const SEED_CHARACTER_TRUST_DESCRIPTION: &str = "角色对玩家或当前局势的信任数值。";
const SEED_RULE_NAME: &str = "气氛升温规则";
const SEED_RULE_DESCRIPTION: &str = "当世界紧张度较高时，进一步提升压力并追加阶段标签。";
pub(crate) const SEED_RULE_EFFECT_MESSAGE: &str = "规则：当前场上气氛进一步升温。";

fn sample_world_seeding_enabled() -> bool {
    true
}

fn seed_character_ids() -> [&'static str; 10] {
    [
        SEED_CHARACTER_SCARLETT_ID,
        SEED_CHARACTER_ASHLEY_ID,
        SEED_CHARACTER_RHETT_ID,
        SEED_CHARACTER_MELANIE_ID,
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
    custom_tabs_json: String,
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
            map_nodes_json, triggers_json, custom_tabs_json, time_config_json, director_config_json, ui_theme_config_json,
            director_system_prompt_base, director_runtime_system_prompt, opening_messages_json, opening_character_ids_json, player_character_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, '', '', ?14, ?15, ?16)
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
            custom_tabs_json,
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
    custom_tabs_json: String,
    time_config_json: String,
    director_config_json: String,
    ui_theme_config_json: String,
    opening_messages_json: String,
    opening_character_ids_json: String,
    player_character_id: Option<&str>,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE worlds SET name = ?1, genre = ?2, background_prompt = ?3, opening_scene = ?4, summary = ?5, time_system = ?6, map_nodes_json = ?7, triggers_json = ?8, custom_tabs_json = ?9, time_config_json = ?10, director_config_json = ?11, ui_theme_config_json = ?12, opening_messages_json = ?13, opening_character_ids_json = ?14, player_character_id = ?15 WHERE id = ?16",
        params![
            name,
            genre,
            background_prompt,
            opening_scene,
            summary,
            time_system,
            map_nodes_json,
            triggers_json,
            custom_tabs_json,
            time_config_json,
            director_config_json,
            ui_theme_config_json,
            opening_messages_json,
            opening_character_ids_json,
            player_character_id,
            id
        ],
    )?;
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
    custom_tabs_json: String,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "
        INSERT INTO characters (
            id, name, world_id, role, background_prompt, model, memory_strategy, recent_dialogue_rounds,
            attributes_json, portrait_assets_json, custom_tabs_json, runtime_system_prompt
        ) VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, 8, ?7, '[]', ?8, '')
        ",
        params![
            id,
            name,
            world_id,
            role,
            background_prompt,
            memory_strategy,
            attributes_json,
            custom_tabs_json
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
    custom_tabs_json: String,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE characters SET name = ?1, role = ?2, background_prompt = ?3, memory_strategy = ?4, attributes_json = ?5, custom_tabs_json = ?6 WHERE id = ?7",
        params![
            name,
            role,
            background_prompt,
            memory_strategy,
            attributes_json,
            custom_tabs_json,
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
            Value::String(DEFAULT_WORLD_DIRECTOR_PROMPT.to_string()),
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
            "战斗系统扩展",
            1,
            "提供可选的战斗结算钩子与战斗事件处理能力。",
            r#"["before_player_action","after_state_commit"]"#,
        ),
        (
            "inventory-plugin",
            "物品系统扩展",
            1,
            "负责物品更新、消耗处理，以及导出阶段的元数据辅助。",
            r#"["after_player_action","on_export"]"#,
        ),
        (
            "world-rule-plugin",
            "世界规则扩展",
            0,
            "注册额外的世界专属规则、状态变更钩子和触发器处理流程。",
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
            "生成图片",
            "根据世界主控提示生成场景背景或角色立绘，并保存到当前会话资源中。",
            "builtin-image-generation",
            "generate_image",
            1,
            "\"on-demand\"",
            "medium",
            r#"["background","portrait","generate image","scene image"]"#,
        ),
        (
            "mcp-tool-list-scenes",
            "列出场景",
            "在切换场景前返回可用场景、地图节点以及当前场景上下文。",
            "builtin-world-director",
            "list_scenes",
            1,
            "\"on-demand\"",
            "low",
            r#"["scene","map","location","list scenes"]"#,
        ),
        (
            "mcp-tool-list-characters",
            "列出角色",
            "返回世界角色、身份摘要与当前在场信息，用于辅助主控决策。",
            "builtin-world-director",
            "list_characters",
            1,
            "\"on-demand\"",
            "low",
            r#"["character","npc","list characters"]"#,
        ),
        (
            "mcp-tool-change-scene",
            "切换场景",
            "允许世界主控切换场景、更新场景描述，并同步调整当前在场角色。",
            "builtin-world-director",
            "change_scene",
            1,
            "\"on-demand\"",
            "medium",
            r#"["change scene","scene transition","enter scene"]"#,
        ),
        (
            "mcp-tool-switch-player-character",
            "切换玩家角色",
            "允许世界主控在不强制切场的情况下，把玩家控制权切换到现有世界角色。",
            "builtin-world-director",
            "switch_player_character",
            1,
            "\"on-demand\"",
            "medium",
            r#"["switch player","switch character","possession","control character"]"#,
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
                SEED_WORLD_GWTW_ID,
                SEED_WORLD_GWTW_NAME,
                SEED_WORLD_GWTW_GENRE,
                SEED_WORLD_GWTW_BACKGROUND_PROMPT,
                SEED_WORLD_GWTW_OPENING_SCENE,
                SEED_WORLD_GWTW_SUMMARY,
                SEED_WORLD_GWTW_TIME_SYSTEM,
                gwtw_world_map_nodes_json(),
                gwtw_world_triggers_json(),
                gwtw_world_custom_tabs_json(),
                gwtw_world_time_config_json(),
                default_seed_world_director_config_json(),
                gwtw_world_ui_theme_config_json(),
                gwtw_world_opening_messages_json(),
                gwtw_world_opening_character_ids_json(),
                Some(SEED_CHARACTER_SCARLETT_ID),
            )?;
            insert_seed_character(
                conn,
                SEED_CHARACTER_SCARLETT_ID,
                SEED_CHARACTER_SCARLETT_NAME,
                SEED_WORLD_GWTW_ID,
                SEED_CHARACTER_SCARLETT_ROLE,
                SEED_CHARACTER_SCARLETT_BACKGROUND,
                SEED_CHARACTER_SCARLETT_MEMORY,
                scarlett_attributes_json(),
                scarlett_custom_tabs_json(),
            )?;
            insert_seed_character(
                conn,
                SEED_CHARACTER_ASHLEY_ID,
                SEED_CHARACTER_ASHLEY_NAME,
                SEED_WORLD_GWTW_ID,
                SEED_CHARACTER_ASHLEY_ROLE,
                SEED_CHARACTER_ASHLEY_BACKGROUND,
                SEED_CHARACTER_ASHLEY_MEMORY,
                ashley_attributes_json(),
                ashley_custom_tabs_json(),
            )?;
            insert_seed_character(
                conn,
                SEED_CHARACTER_RHETT_ID,
                SEED_CHARACTER_RHETT_NAME,
                SEED_WORLD_GWTW_ID,
                SEED_CHARACTER_RHETT_ROLE,
                SEED_CHARACTER_RHETT_BACKGROUND,
                SEED_CHARACTER_RHETT_MEMORY,
                rhett_attributes_json(),
                rhett_custom_tabs_json(),
            )?;
            insert_seed_character(
                conn,
                SEED_CHARACTER_MELANIE_ID,
                SEED_CHARACTER_MELANIE_NAME,
                SEED_WORLD_GWTW_ID,
                SEED_CHARACTER_MELANIE_ROLE,
                SEED_CHARACTER_MELANIE_BACKGROUND,
                SEED_CHARACTER_MELANIE_MEMORY,
                melanie_attributes_json(),
                melanie_custom_tabs_json(),
            )?;
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
                poetry_world_custom_tabs_json(),
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
                poetry_guest_custom_tabs_json(),
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
                libai_custom_tabs_json(),
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
                dufu_custom_tabs_json(),
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
                wangwei_custom_tabs_json(),
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
                liqingzhao_custom_tabs_json(),
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
                sushi_custom_tabs_json(),
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
                "内置 Embedding：BAAI/bge-small-zh-v1.5",
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
    update_seed_world(
        conn,
        SEED_WORLD_GWTW_ID,
        SEED_WORLD_GWTW_NAME,
        SEED_WORLD_GWTW_GENRE,
        SEED_WORLD_GWTW_BACKGROUND_PROMPT,
        SEED_WORLD_GWTW_OPENING_SCENE,
        SEED_WORLD_GWTW_SUMMARY,
        SEED_WORLD_GWTW_TIME_SYSTEM,
        gwtw_world_map_nodes_json(),
        gwtw_world_triggers_json(),
        gwtw_world_custom_tabs_json(),
        gwtw_world_time_config_json(),
        default_seed_world_director_config_json(),
        gwtw_world_ui_theme_config_json(),
        gwtw_world_opening_messages_json(),
        gwtw_world_opening_character_ids_json(),
        Some(SEED_CHARACTER_SCARLETT_ID),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_SCARLETT_ID,
        SEED_CHARACTER_SCARLETT_NAME,
        SEED_CHARACTER_SCARLETT_ROLE,
        SEED_CHARACTER_SCARLETT_BACKGROUND,
        SEED_CHARACTER_SCARLETT_MEMORY,
        scarlett_attributes_json(),
        scarlett_custom_tabs_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_ASHLEY_ID,
        SEED_CHARACTER_ASHLEY_NAME,
        SEED_CHARACTER_ASHLEY_ROLE,
        SEED_CHARACTER_ASHLEY_BACKGROUND,
        SEED_CHARACTER_ASHLEY_MEMORY,
        ashley_attributes_json(),
        ashley_custom_tabs_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_RHETT_ID,
        SEED_CHARACTER_RHETT_NAME,
        SEED_CHARACTER_RHETT_ROLE,
        SEED_CHARACTER_RHETT_BACKGROUND,
        SEED_CHARACTER_RHETT_MEMORY,
        rhett_attributes_json(),
        rhett_custom_tabs_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_MELANIE_ID,
        SEED_CHARACTER_MELANIE_NAME,
        SEED_CHARACTER_MELANIE_ROLE,
        SEED_CHARACTER_MELANIE_BACKGROUND,
        SEED_CHARACTER_MELANIE_MEMORY,
        melanie_attributes_json(),
        melanie_custom_tabs_json(),
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
        poetry_world_custom_tabs_json(),
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
        poetry_guest_custom_tabs_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_LIBAI_ID,
        SEED_CHARACTER_LIBAI_NAME,
        SEED_CHARACTER_LIBAI_ROLE,
        SEED_CHARACTER_LIBAI_BACKGROUND,
        SEED_CHARACTER_LIBAI_MEMORY,
        libai_attributes_json(),
        libai_custom_tabs_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_DUFU_ID,
        SEED_CHARACTER_DUFU_NAME,
        SEED_CHARACTER_DUFU_ROLE,
        SEED_CHARACTER_DUFU_BACKGROUND,
        SEED_CHARACTER_DUFU_MEMORY,
        dufu_attributes_json(),
        dufu_custom_tabs_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_WANGWEI_ID,
        SEED_CHARACTER_WANGWEI_NAME,
        SEED_CHARACTER_WANGWEI_ROLE,
        SEED_CHARACTER_WANGWEI_BACKGROUND,
        SEED_CHARACTER_WANGWEI_MEMORY,
        wangwei_attributes_json(),
        wangwei_custom_tabs_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_LIQINGZHAO_ID,
        SEED_CHARACTER_LIQINGZHAO_NAME,
        SEED_CHARACTER_LIQINGZHAO_ROLE,
        SEED_CHARACTER_LIQINGZHAO_BACKGROUND,
        SEED_CHARACTER_LIQINGZHAO_MEMORY,
        liqingzhao_attributes_json(),
        liqingzhao_custom_tabs_json(),
    )?;
    update_seed_character(
        conn,
        SEED_CHARACTER_SUSHI_ID,
        SEED_CHARACTER_SUSHI_NAME,
        SEED_CHARACTER_SUSHI_ROLE,
        SEED_CHARACTER_SUSHI_BACKGROUND,
        SEED_CHARACTER_SUSHI_MEMORY,
        sushi_attributes_json(),
        sushi_custom_tabs_json(),
    )?;
    Ok(())
}

pub(crate) fn ensure_all(conn: &Connection) -> Result<(), rusqlite::Error> {
    ensure_default_plugins(conn)?;
    ensure_builtin_mcp_tools(conn)?;
    ensure_core_seed_data(conn)?;
    ensure_localized_builtin_content(conn)?;
    repair_corrupted_world_prompts(conn)?;
    clear_legacy_seed_character_model_overrides(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::feihualing_world;
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
            Some("Feihualing Mobile - Moonlit Poetry Album")
        );
    }
}
