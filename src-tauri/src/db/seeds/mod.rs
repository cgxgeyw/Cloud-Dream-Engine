pub mod bundled_worlds;
pub mod schedule_assistant_world;

use rusqlite::{params, Connection};

use crate::models::mcp_tool::{
    MCP_TOOL_BACKGROUND_GENERATION_ID, MCP_TOOL_CHANGE_SCENE_ID, MCP_TOOL_LIST_CHARACTERS_ID,
    MCP_TOOL_LIST_SCENES_ID, MCP_TOOL_PORTRAIT_GENERATION_ID, MCP_TOOL_SCHEDULE_NOTIFICATION_ID,
    MCP_TOOL_SEARCH_KNOWLEDGE_ID, MCP_TOOL_SWITCH_PLAYER_CHARACTER_ID,
};

use schedule_assistant_world::*;

const SCHEDULE_TODO_SCHEMA_ID: &str = "attr-schedule-assistant-todo-items";
const SCHEDULE_COMPLETED_SCHEMA_ID: &str = "attr-schedule-assistant-completed-items";

/// 内置（播种）世界的 id 列表，用于墓碑判定，避免误伤用户自建世界。
fn seed_world_ids() -> [&'static str; 1] {
    [SEED_WORLD_SCHEDULE_ASSISTANT_ID]
}

/// 内置世界被用户删除后记一笔墓碑，避免每次启动重新播种把它"复活"。
fn seed_world_is_tombstoned(conn: &Connection, world_id: &str) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM seed_world_tombstones WHERE world_id = ?1",
        params![world_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub(crate) fn mark_seed_world_tombstoned(
    conn: &Connection,
    world_id: &str,
) -> Result<(), rusqlite::Error> {
    // 只为内置世界记墓碑；用户自建世界不会被播种，无需标记。
    if !seed_world_ids().contains(&world_id) {
        return Ok(());
    }
    conn.execute(
        "INSERT OR IGNORE INTO seed_world_tombstones (world_id, created_at) VALUES (?1, datetime('now'))",
        params![world_id],
    )?;
    Ok(())
}

pub(crate) fn mark_all_seed_worlds_tombstoned(conn: &Connection) -> Result<(), rusqlite::Error> {
    for world_id in seed_world_ids() {
        mark_seed_world_tombstoned(conn, world_id)?;
    }
    Ok(())
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
    time_config_json: String,
    director_config_json: String,
    ui_theme_config_json: String,
    opening_messages_json: String,
    opening_character_ids_json: String,
    player_character_id: Option<&str>,
) -> Result<(), rusqlite::Error> {
    // 用户删除过的内置世界不再自动重建，尊重删除意图。
    if seed_world_is_tombstoned(conn, id)? {
        return Ok(());
    }
    conn.execute(
        "
        INSERT INTO worlds (
            id, name, genre, background_prompt, opening_scene, summary, time_system,
            map_nodes_json, time_config_json, director_config_json, ui_theme_config_json,
            director_system_prompt_base, director_runtime_system_prompt, opening_messages_json, opening_character_ids_json, player_character_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, '', '', ?12, ?13, ?14)
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
    time_config_json: String,
    director_config_json: String,
    ui_theme_config_json: String,
    opening_messages_json: String,
    opening_character_ids_json: String,
    player_character_id: Option<&str>,
) -> Result<(), rusqlite::Error> {
    // 用户删除过的内置世界不再自动重建，尊重删除意图。
    if seed_world_is_tombstoned(conn, id)? {
        return Ok(());
    }
    let updated = conn.execute(
        "UPDATE worlds SET name = ?1, genre = ?2, background_prompt = ?3, opening_scene = ?4, summary = ?5, time_system = ?6, map_nodes_json = ?7, time_config_json = ?8, director_config_json = ?9, ui_theme_config_json = ?10, opening_messages_json = ?11, opening_character_ids_json = ?12, player_character_id = ?13 WHERE id = ?14",
        params![
            name,
            genre,
            background_prompt,
            opening_scene,
            summary,
            time_system,
            map_nodes_json,
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
    // 所属内置世界已被删除（墓碑）时，不再重建其角色，避免出现孤立角色。
    if seed_world_is_tombstoned(conn, world_id)? {
        return Ok(());
    }
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
    conn.execute("DELETE FROM mcp_tools WHERE id = 'mcp-tool-image-generation'", [])?;
    let builtin_tools = [
        (
            MCP_TOOL_BACKGROUND_GENERATION_ID,
            "生成场景背景",
            "仅供世界主控按当前剧情与场景生成或更新背景图，不是通用生图工具。",
            "builtin-world-director",
            "generate_background",
            1,
            r#""on-demand""#,
            "medium",
            r#"["background","scene image","场景背景","背景图"]"#,
        ),
        (
            MCP_TOOL_PORTRAIT_GENERATION_ID,
            "生成角色立绘",
            "仅供世界主控为指定角色生成或更新立绘，不是通用生图工具。",
            "builtin-world-director",
            "generate_portrait",
            1,
            r#""on-demand""#,
            "medium",
            r#"["portrait","character image","角色立绘","立绘"]"#,
        ),
        (
            MCP_TOOL_LIST_SCENES_ID,
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
            MCP_TOOL_LIST_CHARACTERS_ID,
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
            MCP_TOOL_CHANGE_SCENE_ID,
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
            MCP_TOOL_SWITCH_PLAYER_CHARACTER_ID,
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
            MCP_TOOL_SEARCH_KNOWLEDGE_ID,
            "知识库检索",
            "按需检索本世界已授权的知识库，返回来源标题与正文片段。",
            "builtin-knowledge",
            "search_knowledge",
            1,
            r#""on-demand""#,
            "low",
            r#"["knowledge","知识库","search","检索","资料"]"#,
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

    // 基础 HTTP 工具（本地工具框架）：由核心直接执行，桌面与 Android 均可用。
    // 默认只种子这一个本地工具；股票等数据接口工具以工具包 JSON 形式按需导入。
    conn.execute(
        "
        INSERT INTO mcp_tools (
            id, name, description, server_name, tool_name, enabled, exposure_policy_json, risk_level, trigger_keywords_json, input_schema_json, server_id, impl_kind, impl_config_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, '', ?11, ?12)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            server_name = excluded.server_name,
            tool_name = excluded.tool_name,
            enabled = excluded.enabled,
            exposure_policy_json = excluded.exposure_policy_json,
            risk_level = excluded.risk_level,
            trigger_keywords_json = excluded.trigger_keywords_json,
            input_schema_json = excluded.input_schema_json,
            impl_kind = excluded.impl_kind,
            impl_config_json = excluded.impl_config_json
        ",
        params![
            "mcp-tool-http-request",
            "HTTP 请求",
            "发起一次 HTTP 请求并返回响应内容。仅在用户或世界明确要求访问某个地址时使用；优先使用 https。",
            "builtin-local",
            "http_request",
            1,
            r#""on-demand""#,
            "high",
            r#"["http","url","fetch","request","网页","接口"]"#,
            r#"{"type":"object","required":["url"],"properties":{"url":{"type":"string","description":"完整请求地址，http 或 https"},"method":{"type":"string","description":"HTTP 方法，默认 GET"},"headers":{"type":"object","description":"请求头"},"params":{"type":"object","description":"URL 查询参数"},"body":{"type":"object","description":"JSON 请求体（POST/PUT 时）"}}}"#,
            "builtin_http",
            r#"{"mode":"generic"}"#,
        ],
    )?;

    Ok(())
}

fn ensure_core_seed_data(conn: &Connection) -> Result<(), rusqlite::Error> {
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
                "内置本地 Embedding / bge-small-zh-v1.5",
                "builtin-local",
                "BAAI/bge-small-zh-v1.5",
                "",
            ],
        )?;
    }
    Ok(())
}
fn ensure_localized_builtin_content(conn: &Connection) -> Result<(), rusqlite::Error> {
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
    Ok(())
}

fn ensure_schedule_assistant_task_attribute_schemas(
    conn: &Connection,
) -> Result<(), rusqlite::Error> {
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
    ensure_core_seed_data(conn)?;
    ensure_localized_builtin_content(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::schedule_assistant_world;
    use crate::models::mcp_tool::MCP_TOOL_SCHEDULE_NOTIFICATION_ID;
    use serde_json::Value;

    #[test]
    fn builtin_image_tools_replace_the_retired_combined_tool() {
        use crate::db::schema;
        use crate::models::mcp_tool::{
            MCP_TOOL_BACKGROUND_GENERATION_ID, MCP_TOOL_PORTRAIT_GENERATION_ID,
        };

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        conn.execute(
            "INSERT INTO mcp_tools (id, name, tool_name) VALUES ('mcp-tool-image-generation', '旧生图', 'generate_image')",
            [],
        )
        .unwrap();
        super::ensure_builtin_mcp_tools(&conn).unwrap();

        let retired: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mcp_tools WHERE id = 'mcp-tool-image-generation'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retired, 0);
        for (id, tool_name) in [
            (MCP_TOOL_BACKGROUND_GENERATION_ID, "generate_background"),
            (MCP_TOOL_PORTRAIT_GENERATION_ID, "generate_portrait"),
        ] {
            let seeded: String = conn
                .query_row("SELECT tool_name FROM mcp_tools WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(seeded, tool_name);
        }
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

        assert_eq!(
            config.get("service_mode").and_then(Value::as_str),
            Some("agent_chat")
        );
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
    fn schedule_assistant_seed_owns_dedicated_html_ui_files() {
        let config = serde_json::from_str::<Value>(
            &schedule_assistant_world::schedule_assistant_world_ui_theme_config_json(),
        )
        .unwrap();
        assert_eq!(
            config.get("runtime_version").and_then(Value::as_u64),
            Some(3)
        );
        let desktop_source = config
            .pointer("/entries/desktop/document")
            .and_then(Value::as_str)
            .expect("schedule assistant desktop UI document");
        let mobile_source = config
            .pointer("/entries/mobile/document")
            .and_then(Value::as_str)
            .expect("schedule assistant mobile UI document");

        assert!(desktop_source.trim_start().starts_with("<!doctype html>"));
        assert!(mobile_source.trim_start().starts_with("<!doctype html>"));
        assert!(desktop_source.contains(r#"data-platform="desktop""#));
        assert!(mobile_source.contains(r#"data-platform="mobile""#));
        assert!(desktop_source.contains("id=\"messages\""));
        assert!(desktop_source.contains("id=\"todo-list\""));
        assert!(desktop_source.contains("id=\"confirm-done\""));
        assert!(mobile_source.contains("id=\"messages\""));
        assert!(mobile_source.contains("id=\"todo-list\""));
        assert!(mobile_source.contains("data-tab=\"tasks\""));
        assert!(desktop_source.contains("<script data-world-script>"));
        assert!(mobile_source.contains("<script data-world-script>"));
        assert!(!desktop_source.contains("\"schema_version\""));
        assert!(!mobile_source.contains("\"schema_version\""));
    }
}
