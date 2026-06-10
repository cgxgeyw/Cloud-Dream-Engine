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
        "runtime_context_prompt": "当前时间：{{current_time}}\n请维护两个 session 级列表属性：todo_items 表示未完成待办事项，completed_items 表示已完成事项。\n用户提出新任务、提醒或安排时，把可执行事项加入 todo_items。\n用户说已经完成某些事项，或界面发送“我已完成以下待办事项：...”时，把对应事项从 todo_items 移除并加入 completed_items。\n两个列表都使用字符串数组，保持条目简短、去重、可直接显示。\n如果用户要求在明确时间、相对时间或稍后某个时刻提醒，必须调用 schedule_notification 工具创建系统提醒；不要只更新 todo_items 后声称会通知。\n只有在 schedule_notification 工具返回 ok=true 后，才能说提醒已创建或届时会通知。若工具失败或权限不足，请说明无法创建系统提醒，但仍可把事项保留在 todo_items。\n需要更新列表时，角色 JSON 回复必须同时包含 response 和 session_attribute_updates；response 是展示给用户的自然语言，不要把裸 JSON 当作聊天正文。例如：\n{\"response\":\"好的，已更新待办事项。\",\"session_attribute_updates\":[{\"key\":\"todo_items\",\"value\":[\"示例待办\"]},{\"key\":\"completed_items\",\"value\":[\"示例完成事项\"]}]}",
        "world_director_prompt": "你是行程助手世界的主控。请维护两个 session 级列表属性：todo_items 表示未完成待办事项，completed_items 表示已完成事项。用户提出新任务、提醒或安排时，把可执行事项加入 todo_items；用户说已经完成某些事项，或界面发送“我已完成以下待办事项：...”时，把对应事项从 todo_items 移除并加入 completed_items。两个列表都使用字符串数组，保持条目简短、去重、可直接显示。若创建了系统提醒，也可以把提醒相关事项保留在 todo_items，直到用户确认完成。",
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
