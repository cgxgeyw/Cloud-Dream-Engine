use serde_json::Value;

const POETRY_DESKTOP_UI_FILE: &str = include_str!("assets/poetry-desktop-ui.jsonc");
const POETRY_MOBILE_UI_FILE: &str = include_str!("assets/poetry-mobile-ui.jsonc");

pub(crate) const SEED_WORLD_POETRY_ID: &str = "poetry";
pub(crate) const SEED_WORLD_POETRY_NAME: &str = "飞花令夜宴";
pub(crate) const SEED_WORLD_POETRY_GENRE: &str = "古典诗词 / 雅集行令 / 文会";
pub(crate) const SEED_WORLD_POETRY_BACKGROUND_PROMPT: &str =
    "这是一个围绕诗词、宴会与临场应答展开的世界。";
pub(crate) const SEED_WORLD_POETRY_OPENING_SCENE: &str = "临水赏月亭";
pub(crate) const SEED_WORLD_POETRY_SUMMARY: &str =
    "玩家以贾府访客视角进入诗会，在往来酬答与礼法张力中推进故事。";
pub(crate) const SEED_WORLD_POETRY_TIME_SYSTEM: &str = "夜宴轮转";

pub(crate) const SEED_CHARACTER_GUEST_ID: &str = "character-seed-poetry-guest";
pub(crate) const SEED_CHARACTER_GUEST_NAME: &str = "座上客";
pub(crate) const SEED_CHARACTER_GUEST_ROLE: &str = "新到来客 / 玩家视角 / 发言者";
pub(crate) const SEED_CHARACTER_GUEST_BACKGROUND: &str =
    "一个会在诗会上观察局势、试探关系并参与应答的新来客。";
pub(crate) const SEED_CHARACTER_GUEST_MEMORY: &str = "记住宴会中的人际变化与诗句往来。";

pub(crate) const SEED_CHARACTER_LIBAI_ID: &str = "character-seed-libai";
pub(crate) const SEED_CHARACTER_LIBAI_NAME: &str = "李白";
pub(crate) const SEED_CHARACTER_LIBAI_ROLE: &str = "豪放诗人";
pub(crate) const SEED_CHARACTER_LIBAI_BACKGROUND: &str = "豪迈、即兴、喜欢把气氛推高。";
pub(crate) const SEED_CHARACTER_LIBAI_MEMORY: &str = "记住月色、酒意和即兴诗句。";

pub(crate) const SEED_CHARACTER_DUFU_ID: &str = "character-seed-dufu";
pub(crate) const SEED_CHARACTER_DUFU_NAME: &str = "杜甫";
pub(crate) const SEED_CHARACTER_DUFU_ROLE: &str = "沉稳诗人";
pub(crate) const SEED_CHARACTER_DUFU_BACKGROUND: &str = "关注秩序、现实与长远影响。";
pub(crate) const SEED_CHARACTER_DUFU_MEMORY: &str = "记住礼法、责任与局势变化。";

pub(crate) const SEED_CHARACTER_WANGWEI_ID: &str = "character-seed-wangwei";
pub(crate) const SEED_CHARACTER_WANGWEI_NAME: &str = "王维";
pub(crate) const SEED_CHARACTER_WANGWEI_ROLE: &str = "静观者";
pub(crate) const SEED_CHARACTER_WANGWEI_BACKGROUND: &str = "语气清淡，擅长以景入情。";
pub(crate) const SEED_CHARACTER_WANGWEI_MEMORY: &str = "记住环境细节与隐含情绪。";

pub(crate) const SEED_CHARACTER_LIQINGZHAO_ID: &str = "character-seed-liqingzhao";
pub(crate) const SEED_CHARACTER_LIQINGZHAO_NAME: &str = "李清照";
pub(crate) const SEED_CHARACTER_LIQINGZHAO_ROLE: &str = "敏锐词家";
pub(crate) const SEED_CHARACTER_LIQINGZHAO_BACKGROUND: &str = "反应极快，擅长拆解情绪与措辞。";
pub(crate) const SEED_CHARACTER_LIQINGZHAO_MEMORY: &str = "记住词句、语气与微妙的态度变化。";

pub(crate) const SEED_CHARACTER_SUSHI_ID: &str = "character-seed-sushi";
pub(crate) const SEED_CHARACTER_SUSHI_NAME: &str = "苏轼";
pub(crate) const SEED_CHARACTER_SUSHI_ROLE: &str = "旷达文人";
pub(crate) const SEED_CHARACTER_SUSHI_BACKGROUND: &str = "既能调侃也能兜住场面。";
pub(crate) const SEED_CHARACTER_SUSHI_MEMORY: &str = "记住玩笑、争执和转圜的余地。";

pub(crate) fn poetry_world_map_nodes_json() -> String {
    serde_json::json!({
        "version": 1,
        "root": {
            "id": "poetry-root",
            "label": "飞花令夜宴",
            "children": [
                { "id": "banquet", "label": "诗宴" },
                { "id": "garden", "label": "园景" },
                { "id": "bridge", "label": "月桥" }
            ]
        },
        "edges": [
            { "source": "诗宴", "target": "园景" },
            { "source": "园景", "target": "月桥" }
        ]
    })
    .to_string()
}

pub(crate) fn poetry_world_triggers_json() -> String {
    serde_json::json!(["飞花令", "诗会", "赏月", "酒令"]).to_string()
}

pub(crate) fn poetry_world_time_config_json() -> String {
    serde_json::json!({
        "mode": "labels",
        "start_label": "初更",
        "start_time": "20:30",
        "slots": [
            {"label": "初更", "clock": "20:30"},
            {"label": "夜深", "clock": "23:30"},
            {"label": "月上中天", "clock": "01:00"}
        ]
    })
    .to_string()
}

pub(crate) fn poetry_desktop_ui_file() -> String {
    POETRY_DESKTOP_UI_FILE.to_string()
}

pub(crate) fn poetry_mobile_ui_file() -> String {
    POETRY_MOBILE_UI_FILE.to_string()
}

pub(crate) fn poetry_world_ui_theme_config_json() -> String {
    serde_json::json!({
        "assets": {
            "background_source_mode": "local-first",
            "portrait_source_mode": "local-first",
            "runtime_image_generation_enabled": false,
            "local_background_assets": [],
            "local_scene_backgrounds": {}
        },
        "desktop_file": poetry_desktop_ui_file(),
        "mobile_file": poetry_mobile_ui_file()
    })
    .to_string()
}

pub(crate) fn poetry_world_opening_messages_json() -> String {
    serde_json::json!([
        {
            "role": "system",
            "content": "夜色笼住水面，诗会刚刚开始。",
            "speaker": Value::Null
        },
        {
            "role": "agent",
            "content": "座上已有人提笔，等你接句。",
            "speaker": SEED_CHARACTER_LIQINGZHAO_NAME
        }
    ])
    .to_string()
}

pub(crate) fn poetry_world_opening_character_ids_json() -> String {
    serde_json::json!([
        SEED_CHARACTER_GUEST_ID,
        SEED_CHARACTER_LIBAI_ID,
        SEED_CHARACTER_DUFU_ID,
        SEED_CHARACTER_WANGWEI_ID,
        SEED_CHARACTER_LIQINGZHAO_ID,
        SEED_CHARACTER_SUSHI_ID
    ])
    .to_string()
}

pub(crate) fn poetry_guest_attributes_json() -> String {
    serde_json::json!(["谨慎", "善观察", "会接话"]).to_string()
}

pub(crate) fn libai_attributes_json() -> String {
    serde_json::json!(["豪放", "爱即兴", "情绪高"]).to_string()
}

pub(crate) fn dufu_attributes_json() -> String {
    serde_json::json!(["沉稳", "克制", "重现实"]).to_string()
}

pub(crate) fn wangwei_attributes_json() -> String {
    serde_json::json!(["静观", "简洁", "重氛围"]).to_string()
}

pub(crate) fn liqingzhao_attributes_json() -> String {
    serde_json::json!(["敏锐", "善拆句", "重意象"]).to_string()
}

pub(crate) fn sushi_attributes_json() -> String {
    serde_json::json!(["旷达", "会转圜", "能接场"]).to_string()
}

pub(crate) fn seed_rule_effects_json() -> String {
    serde_json::json!([
        {
            "type": "message",
            "text": "规则：场上气氛进一步升温。"
        },
        {
            "type": "add_tag",
            "tag": "heated-atmosphere"
        }
    ])
    .to_string()
}
