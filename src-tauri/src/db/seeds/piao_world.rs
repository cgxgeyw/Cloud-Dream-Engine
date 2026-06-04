use serde_json::json;

pub(crate) const DEFAULT_WORLD_DIRECTOR_PROMPT: &str = r#"你是世界主控。
你必须只返回一个 JSON 对象，用来描述当前回合之后的世界状态变化。
除非剧情确实成立，否则保持当前场景与地点不变。
只让当前场景里真实在场的角色进入可见角色或发言计划。
planned_speakers 通常应包含一个或多个会在本回合接话的 NPC。
不要输出任何解释、markdown 或额外文本，只返回 JSON。"#;

pub(crate) const SEED_WORLD_GWTW_ID: &str = "gwtw";
pub(crate) const SEED_WORLD_GWTW_NAME: &str = "乱世佳人：塔拉庄园回廊";
pub(crate) const SEED_WORLD_GWTW_GENRE: &str = "经典文学 / 南方庄园 / 战前情感戏";
pub(crate) const SEED_WORLD_GWTW_BACKGROUND_PROMPT: &str = "故事发生在战火改变一切之前的南方庄园。礼仪、流言、欲望、体面与压抑都应在每场对话里持续起作用。";
pub(crate) const SEED_WORLD_GWTW_OPENING_SCENE: &str = "塔拉庄园回廊";
pub(crate) const SEED_WORLD_GWTW_SUMMARY: &str =
    "玩家会进入一个光鲜却脆弱的社交世界，爱情、婚约传闻与战争阴影正在同时逼近。";
pub(crate) const SEED_WORLD_GWTW_TIME_SYSTEM: &str = "庄园日程 + 流言升温";

pub(crate) const SEED_CHARACTER_SCARLETT_ID: &str = "character-seed-scarlett";
pub(crate) const SEED_CHARACTER_SCARLETT_NAME: &str = "斯嘉丽";
pub(crate) const SEED_CHARACTER_SCARLETT_ROLE: &str = "庄园继承人 / 社交焦点 / 玩家视角";
pub(crate) const SEED_CHARACTER_SCARLETT_BACKGROUND: &str =
    "机敏、骄傲，也习惯让整间屋子的气氛都朝自己的意愿偏转。";
pub(crate) const SEED_CHARACTER_SCARLETT_MEMORY: &str =
    "会长期记住与爱情、地位、体面压力有关的事件与细微变化。";

pub(crate) const SEED_CHARACTER_ASHLEY_ID: &str = "character-seed-ashley";
pub(crate) const SEED_CHARACTER_ASHLEY_NAME: &str = "艾希礼";
pub(crate) const SEED_CHARACTER_ASHLEY_ROLE: &str = "温和理想主义者 / 家族继承人 / 心事深藏";
pub(crate) const SEED_CHARACTER_ASHLEY_BACKGROUND: &str =
    "有礼、沉静，面对艰难的感情时总是迟疑，不愿把话说得太直白。";
pub(crate) const SEED_CHARACTER_ASHLEY_MEMORY: &str = "会长期记住责任、婚约与情感犹疑之间的拉扯。";

pub(crate) const SEED_CHARACTER_RHETT_ID: &str = "character-seed-rhett";
pub(crate) const SEED_CHARACTER_RHETT_NAME: &str = "白瑞德";
pub(crate) const SEED_CHARACTER_RHETT_ROLE: &str = "旁观者 / 挑衅者 / 机会主义者";
pub(crate) const SEED_CHARACTER_RHETT_BACKGROUND: &str =
    "很快就能看穿做派背后的虚饰，也乐于把礼貌表层下的紧张直接挑出来。";
pub(crate) const SEED_CHARACTER_RHETT_MEMORY: &str =
    "会长期记住伪饰、筹码与危险吸引力带来的每一次裂缝。";

pub(crate) const SEED_CHARACTER_MELANIE_ID: &str = "character-seed-melanie";
pub(crate) const SEED_CHARACTER_MELANIE_NAME: &str = "湄兰妮";
pub(crate) const SEED_CHARACTER_MELANIE_ROLE: &str = "温柔支点 / 秩序维护者 / 道德中心";
pub(crate) const SEED_CHARACTER_MELANIE_BACKGROUND: &str =
    "真诚温和，但在气氛失控时也能不动声色地把整场局面重新稳住。";
pub(crate) const SEED_CHARACTER_MELANIE_MEMORY: &str = "会长期记住家族联系、礼法边界与人心暗流。";

pub(crate) fn default_seed_world_director_config_json() -> String {
    json!({
        "allow_scene_transition": true,
        "allow_npc_spawn": true,
        "history_dialogue_rounds": 6,
        "director_tool_loop_limit": 6,
        "world_director_prompt": DEFAULT_WORLD_DIRECTOR_PROMPT,
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

pub(crate) fn gwtw_world_map_nodes_json() -> String {
    json!({
        "version": 1,
        "root": {
            "id": "tara-gallery",
            "label": "塔拉庄园回廊",
            "children": [
                {
                    "id": "tara-main-house",
                    "label": "塔拉庄园主宅",
                    "children": [
                        {
                            "id": "tara-parlor",
                            "label": "塔拉庄园会客室"
                        },
                        {
                            "id": "tara-porch",
                            "label": "塔拉庄园门廊"
                        }
                    ]
                },
                {
                    "id": "twelve-oaks",
                    "label": "十二橡树庄园",
                    "children": [
                        {
                            "id": "twelve-oaks-ballroom",
                            "label": "十二橡树舞厅"
                        },
                        {
                            "id": "wilkes-study",
                            "label": "威尔克斯书房"
                        }
                    ]
                },
                {
                    "id": "atlanta",
                    "label": "亚特兰大",
                    "children": [
                        {
                            "id": "atlanta-station",
                            "label": "亚特兰大车站"
                        },
                        {
                            "id": "war-road",
                            "label": "战时大道"
                        }
                    ]
                }
            ]
        },
        "edges": [
            { "source": "塔拉庄园门廊", "target": "十二橡树庄园" },
            { "source": "十二橡树舞厅", "target": "亚特兰大" },
            { "source": "亚特兰大车站", "target": "战时大道" }
        ]
    }).to_string()

}

pub(crate) fn gwtw_world_triggers_json() -> String {
    json!(["舞会流言", "婚约消息", "战事风声", "庄园来客"]).to_string()
}

pub(crate) fn gwtw_world_custom_tabs_json() -> String {
    json!({
        "关系": "爱情、地位与社交周旋的重要性，并不比正面冲突更低。",
        "礼仪": "很少有人把话说透。名声、家族期待与流言会塑造每一场戏。"
    })
    .to_string()
}

pub(crate) fn gwtw_world_time_config_json() -> String {
    json!({
        "mode": "labels",
        "start_label": "午后",
        "start_time": "14:00",
        "slots": [
            {"label": "午后", "clock": "14:00"},
            {"label": "傍晚", "clock": "18:00"},
            {"label": "夜间", "clock": "21:00"},
            {"label": "深夜", "clock": "23:30"}
        ]
    })
    .to_string()
}

const PIAO_DESKTOP_UI_FILE: &str = include_str!("assets/piao-desktop-ui.jsonc");
const PIAO_MOBILE_UI_FILE: &str = include_str!("assets/piao-mobile-ui.jsonc");

pub(crate) fn piao_desktop_ui_file() -> String {
    PIAO_DESKTOP_UI_FILE.to_string()
}

pub(crate) fn piao_mobile_ui_file() -> String {
    PIAO_MOBILE_UI_FILE.to_string()
}

pub(crate) fn gwtw_world_ui_theme_config_json() -> String {
    json!({
        "assets": {
            "background_source_mode": "local-first",
            "portrait_source_mode": "local-first",
            "runtime_image_generation_enabled": false,
            "local_background_assets": [],
            "local_scene_backgrounds": {}
        },
        "desktop_file": piao_desktop_ui_file(),
        "mobile_file": piao_mobile_ui_file()
    })
    .to_string()
}

pub(crate) fn gwtw_world_opening_messages_json() -> String {
    json!([
        {
            "role": "system",
            "content": "午后的光线落在塔拉庄园回廊上，闲谈、窥探与没有说出口的紧张正在悄悄聚拢。",
            "speaker": serde_json::Value::Null
        },
        {
            "role": "agent",
            "content": "所有人都像是在等，等谁先把那句危险的话真正说出口。",
            "speaker": SEED_CHARACTER_RHETT_NAME
        }
    ])
    .to_string()
}

pub(crate) fn gwtw_world_opening_character_ids_json() -> String {
    json!([
        SEED_CHARACTER_SCARLETT_ID,
        SEED_CHARACTER_ASHLEY_ID,
        SEED_CHARACTER_RHETT_ID,
        SEED_CHARACTER_MELANIE_ID
    ])
    .to_string()
}

pub(crate) fn scarlett_attributes_json() -> String {
    json!(["身份：庄园继承人", "长处：社交直觉", "情绪：躁动"]).to_string()
}

pub(crate) fn scarlett_custom_tabs_json() -> String {
    json!({
        "压力": "她想要掌控、仰慕，以及一个能朝自己心愿倾斜的未来。"
    })
    .to_string()
}

pub(crate) fn ashley_attributes_json() -> String {
    json!(["身份：继承人", "长处：克制", "情绪：矛盾"]).to_string()
}

pub(crate) fn ashley_custom_tabs_json() -> String {
    json!({
        "回避": "比起正面承认艰难真相，他更习惯把话说得柔和一些。"
    })
    .to_string()
}

pub(crate) fn rhett_attributes_json() -> String {
    json!(["身份：局外人", "长处：洞察", "情绪：玩味"]).to_string()
}

pub(crate) fn rhett_custom_tabs_json() -> String {
    json!({
        "观察": "他盯着每一层伪饰、筹码，以及体面最先出现裂缝的瞬间。"
    })
    .to_string()
}

pub(crate) fn melanie_attributes_json() -> String {
    json!(["身份：稳定支点", "长处：沉着", "情绪：温柔"]).to_string()
}

pub(crate) fn melanie_custom_tabs_json() -> String {
    json!({
        "平衡": "哪怕房间里的气氛开始撕裂，她也会努力把优雅与善意重新拉回来。"
    })
    .to_string()
}
