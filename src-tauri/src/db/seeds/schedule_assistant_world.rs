use serde_json::json;

use crate::models::mcp_tool::MCP_TOOL_SCHEDULE_NOTIFICATION_ID;

const SCHEDULE_ASSISTANT_DESKTOP_UI_FILE: &str =
    include_str!("assets/schedule-assistant-desktop-ui.jsonc");
const SCHEDULE_ASSISTANT_MOBILE_UI_FILE: &str =
    include_str!("assets/schedule-assistant-mobile-ui.jsonc");

pub(crate) const SEED_WORLD_SCHEDULE_ASSISTANT_ID: &str = "schedule-assistant";
pub(crate) const SEED_WORLD_SCHEDULE_ASSISTANT_NAME: &str = "行程助手";
pub(crate) const SEED_WORLD_SCHEDULE_ASSISTANT_GENRE: &str = "单助手 / 行程提醒 / 系统通知";
pub(crate) const SEED_WORLD_SCHEDULE_ASSISTANT_BACKGROUND_PROMPT: &str =
    "这是一个帮助用户拆解任务并创建系统提醒的世界。";
pub(crate) const SEED_WORLD_SCHEDULE_ASSISTANT_OPENING_SCENE: &str = "行程助手";
pub(crate) const SEED_WORLD_SCHEDULE_ASSISTANT_SUMMARY: &str =
    "用户告诉助手要做什么和大致时间，助手会分析安排并生成几条系统级定时通知。";
pub(crate) const SEED_WORLD_SCHEDULE_ASSISTANT_TIME_SYSTEM: &str = "真实时间 + 系统通知";

pub(crate) const SEED_CHARACTER_SCHEDULE_ASSISTANT_ID: &str =
    "character-seed-schedule-assistant";
pub(crate) const SEED_CHARACTER_SCHEDULE_ASSISTANT_NAME: &str = "行程助手";
pub(crate) const SEED_CHARACTER_SCHEDULE_ASSISTANT_ROLE: &str =
    "任务拆解助手 / 定时通知规划者";
pub(crate) const SEED_CHARACTER_SCHEDULE_ASSISTANT_BACKGROUND: &str =
    "你是一个务实、可靠的行程助手，负责把用户需求拆成可执行的提醒安排。";
pub(crate) const SEED_CHARACTER_SCHEDULE_ASSISTANT_MEMORY: &str =
    "记住用户常用时间段、提醒偏好、近期安排以及已经创建过的通知意图。";

pub(crate) fn schedule_assistant_desktop_ui_file() -> String {
    SCHEDULE_ASSISTANT_DESKTOP_UI_FILE.to_string()
}

pub(crate) fn schedule_assistant_mobile_ui_file() -> String {
    SCHEDULE_ASSISTANT_MOBILE_UI_FILE.to_string()
}

pub(crate) fn schedule_assistant_world_map_nodes_json() -> String {
    json!({
        "version": 1,
        "root": {
            "id": "schedule-assistant-root",
            "label": "行程助手",
            "children": [
                { "id": "task-planning", "label": "任务拆解" },
                { "id": "notification-plan", "label": "提醒规划" }
            ]
        },
        "edges": [
            { "source": "任务拆解", "target": "提醒规划" }
        ]
    })
    .to_string()
}

pub(crate) fn schedule_assistant_world_triggers_json() -> String {
    json!(["提醒", "通知", "日程", "待办", "截止", "准备", "复盘"]).to_string()
}

pub(crate) fn schedule_assistant_world_time_config_json() -> String {
    json!({
        "mode": "realtime",
        "label": "实时"
    })
    .to_string()
}

pub(crate) fn schedule_assistant_world_director_config_json() -> String {
    json!({
        "service_mode": "agent_chat",
        "default_agent_id": SEED_CHARACTER_SCHEDULE_ASSISTANT_ID,
        "allow_scene_transition": false,
        "allow_npc_spawn": false,
        "history_dialogue_rounds": 8,
        "director_tool_loop_limit": 4,
        "world_director_prompt": "",
        "prompt_presets": [],
        "return_processing_rules": [],
        "runtime_policy": {
            "memory_write_mode": "session"
        },
        "allowed_mcp_tool_ids": [
            MCP_TOOL_SCHEDULE_NOTIFICATION_ID
        ]
    })
    .to_string()
}

pub(crate) fn schedule_assistant_world_ui_theme_config_json() -> String {
    json!({
        "assets": {
            "background_source_mode": "local-first",
            "portrait_source_mode": "local-first",
            "runtime_image_generation_enabled": false,
            "local_background_assets": [],
            "local_scene_backgrounds": {}
        },
        "desktop_file": schedule_assistant_desktop_ui_file(),
        "mobile_file": schedule_assistant_mobile_ui_file()
    })
    .to_string()
}

pub(crate) fn schedule_assistant_world_opening_messages_json() -> String {
    json!([
        {
            "role": "system",
            "content": "告诉行程助手你接下来要做什么、什么时候做，以及希望提前多久收到提醒。",
            "speaker": serde_json::Value::Null
        }
    ])
    .to_string()
}

pub(crate) fn schedule_assistant_world_opening_character_ids_json() -> String {
    json!([SEED_CHARACTER_SCHEDULE_ASSISTANT_ID]).to_string()
}

pub(crate) fn schedule_assistant_attributes_json() -> String {
    json!([
        "服务类型：行程提醒",
        "默认策略：生成 2 到 5 条系统通知",
        "限制：时间不明确时先追问再安排"
    ])
    .to_string()
}
