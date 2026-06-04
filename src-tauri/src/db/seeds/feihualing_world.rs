use serde_json::{json, Value};

use super::SEED_RULE_EFFECT_MESSAGE;

const POETRY_DESKTOP_UI_FILE: &str = include_str!("assets/poetry-desktop-ui.jsonc");
const POETRY_MOBILE_UI_FILE: &str = include_str!("assets/poetry-mobile-ui.jsonc");

pub(crate) const SEED_WORLD_POETRY_ID: &str = "poetry";
pub(crate) const SEED_WORLD_POETRY_NAME: &str = "飞花令夜宴";
pub(crate) const SEED_WORLD_POETRY_GENRE: &str = "古典诗词 / 雅集行令 / 文辞交锋";
pub(crate) const SEED_WORLD_POETRY_BACKGROUND_PROMPT: &str = "这是一场围绕机锋、意象、典故与临场应对展开的诗词夜宴。每一句诗都可能改变席间地位、亲疏与气氛温度。";
pub(crate) const SEED_WORLD_POETRY_OPENING_SCENE: &str = "临水赏月亭";
pub(crate) const SEED_WORLD_POETRY_SUMMARY: &str =
    "玩家以赴宴来客的身份进入夜半诗会，每一次应答都可能成为试探、赞许，或一记含蓄而锋利的回击。";
pub(crate) const SEED_WORLD_POETRY_TIME_SYSTEM: &str = "夜宴更次 + 诗令轮转";

pub(crate) const SEED_CHARACTER_GUEST_ID: &str = "character-seed-poetry-guest";
pub(crate) const SEED_CHARACTER_GUEST_NAME: &str = "座上客";
pub(crate) const SEED_CHARACTER_GUEST_ROLE: &str = "新到来客 / 玩家视角 / 受邀发言者";
pub(crate) const SEED_CHARACTER_GUEST_BACKGROUND: &str =
    "刚入席不久，才情、记忆与胆气都还在接受席间诸人的试探。";
pub(crate) const SEED_CHARACTER_GUEST_MEMORY: &str =
    "会长期记住席间唱和、众人反应，以及自己赢得或失去的青眼。";

pub(crate) const SEED_CHARACTER_LIBAI_ID: &str = "character-seed-libai";
pub(crate) const SEED_CHARACTER_LIBAI_NAME: &str = "李白";
pub(crate) const SEED_CHARACTER_LIBAI_ROLE: &str = "豪放天才 / 即兴诗人 / 对月微醺";
pub(crate) const SEED_CHARACTER_LIBAI_BACKGROUND: &str =
    "能在纵酒豪气与天真惊叹之间自如转身，总用炫目的想象力回应压力。";
pub(crate) const SEED_CHARACTER_LIBAI_MEMORY: &str =
    "会长期记住最大胆的诗句、众人的喝彩，以及灵感过盛的高光时刻。";

pub(crate) const SEED_CHARACTER_DUFU_ID: &str = "character-seed-dufu";
pub(crate) const SEED_CHARACTER_DUFU_NAME: &str = "杜甫";
pub(crate) const SEED_CHARACTER_DUFU_ROLE: &str = "沉稳大家 / 克制评者 / 分量担当";
pub(crate) const SEED_CHARACTER_DUFU_BACKGROUND: &str =
    "听得极认真，比起辞采更看重分量，也总会把诗意引向现实与担当。";
pub(crate) const SEED_CHARACTER_DUFU_MEMORY: &str =
    "会长期记住真诚、功力，以及那些足以暴露一个人本色的细节。";

pub(crate) const SEED_CHARACTER_WANGWEI_ID: &str = "character-seed-wangwei";
pub(crate) const SEED_CHARACTER_WANGWEI_NAME: &str = "王维";
pub(crate) const SEED_CHARACTER_WANGWEI_ROLE: &str = "静观者 / 山水诗人 / 清定中心";
pub(crate) const SEED_CHARACTER_WANGWEI_BACKGROUND: &str =
    "偏爱准确而清静的表达，常常只凭一个意象就能让满席喧闹忽然转向。";
pub(crate) const SEED_CHARACTER_WANGWEI_MEMORY: &str =
    "会长期记住沉默、气氛，以及那些极细微的情绪转折。";

pub(crate) const SEED_CHARACTER_LIQINGZHAO_ID: &str = "character-seed-liqingzhao";
pub(crate) const SEED_CHARACTER_LIQINGZHAO_NAME: &str = "李清照";
pub(crate) const SEED_CHARACTER_LIQINGZHAO_ROLE: &str = "机锋敏锐 / 词学高手 / 情感精度极高";
pub(crate) const SEED_CHARACTER_LIQINGZHAO_BACKGROUND: &str =
    "她不必提高声调，也能既雅致又锋利，对虚浮矫情几乎一眼就能看穿。";
pub(crate) const SEED_CHARACTER_LIQINGZHAO_MEMORY: &str =
    "会长期记住情感层次、审美高下，以及一句话里情意是否真实。";

pub(crate) const SEED_CHARACTER_SUSHI_ID: &str = "character-seed-sushi";
pub(crate) const SEED_CHARACTER_SUSHI_NAME: &str = "苏轼";
pub(crate) const SEED_CHARACTER_SUSHI_ROLE: &str = "豁达主人 / 士大夫诗人 / 韧性幽默";
pub(crate) const SEED_CHARACTER_SUSHI_BACKGROUND: &str =
    "见识广、胸襟阔，擅长把一场紧绷的机锋转成更开阔也更有人情味的对话。";
pub(crate) const SEED_CHARACTER_SUSHI_MEMORY: &str =
    "会长期记住同席情分、人生起伏，以及如何把局面重新铺开。";

pub(crate) fn poetry_world_map_nodes_json() -> String {
    json!({
        "version": 1,
        "root": {
            "id": "waterside-moon",
            "label": "临水赏月亭",
            "children": [
                {
                    "id": "poetry-banquet",
                    "label": "诗会主席",
                    "children": [
                        {
                            "id": "host-seat",
                            "label": "主客席"
                        },
                        {
                            "id": "guest-seat",
                            "label": "来客席"
                        },
                        {
                            "id": "wine-table",
                            "label": "斟酒小几"
                        }
                    ]
                },
                {
                    "id": "garden-corridor",
                    "label": "庭园步道",
                    "children": [
                        {
                            "id": "bamboo-gallery",
                            "label": "竹影长廊"
                        },
                        {
                            "id": "orchid-study",
                            "label": "兰月书房"
                        }
                    ]
                },
                {
                    "id": "river-lantern-ferry",
                    "label": "河灯渡口",
                    "children": [
                        {
                            "id": "moon-bridge",
                            "label": "映月小桥"
                        },
                        {
                            "id": "mist-bank",
                            "label": "水雾河岸"
                        }
                    ]
                }
            ]
        },
        "edges": [
            { "source": "斟酒小几", "target": "竹影长廊" },
            { "source": "兰月书房", "target": "映月小桥" },
            { "source": "河灯渡口", "target": "水雾河岸" }
        ]
    }).to_string()

}

pub(crate) fn poetry_world_triggers_json() -> String {
    json!(["飞花令点题", "典故会意", "斟酒续杯", "月升忽静"]).to_string()
}

pub(crate) fn poetry_world_custom_tabs_json() -> String {
    json!({
        "??": "???????????????????????????????????????"
    })
    .to_string()
}
pub(crate) fn poetry_world_time_config_json() -> String {
    json!({
        "mode": "labels",
        "start_label": "初更",
        "start_time": "20:30",
        "slots": [
            {"label": "初更", "clock": "20:30"},
            {"label": "夜宴正酣", "clock": "22:00"},
            {"label": "月上中天", "clock": "23:30"},
            {"label": "最后一杯", "clock": "01:00"}
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
    json!({
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
    json!([
        {
            "role": "system",
            "content": "夜色覆上水亭，席间主人提起新一轮飞花令，众人的目光都慢慢聚了过来。",
            "speaker": Value::Null
        },
        {
            "role": "agent",
            "content": "来客的第一句诗，不能怯。就算写的是静景，也要知道它将落在谁心上。",
            "speaker": SEED_CHARACTER_LIQINGZHAO_NAME
        }
    ])
    .to_string()
}

pub(crate) fn poetry_world_opening_character_ids_json() -> String {
    json!([
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
    json!(["身份：席间来客", "长处：见招拆招", "情绪：凝神"]).to_string()
}

pub(crate) fn poetry_guest_custom_tabs_json() -> String {
    json!({
        "席位": "还在赢得认可，但只要一句出彩，就足以迅速改写众人的期待。"
    })
    .to_string()
}

pub(crate) fn libai_attributes_json() -> String {
    json!(["身份：谪仙诗客", "长处：意象奔涌", "情绪：飞扬"]).to_string()
}

pub(crate) fn libai_custom_tabs_json() -> String {
    json!({
        "锋芒": "他总能把一轮唱和推高，让想象力显得轻松又危险。"
    })
    .to_string()
}

pub(crate) fn dufu_attributes_json() -> String {
    json!(["身份：沉郁工匠", "长处：分量沉实", "情绪：沉定"]).to_string()
}

pub(crate) fn dufu_custom_tabs_json() -> String {
    json!({
        "尺度": "他看重节制、诚实，以及一行经得起推敲的句子。"
    })
    .to_string()
}

pub(crate) fn wangwei_attributes_json() -> String {
    json!(["身份：静观之人", "长处：气氛铺陈", "情绪：清和"]).to_string()
}

pub(crate) fn wangwei_custom_tabs_json() -> String {
    json!({
        "留白": "他珍惜空间感、声调平衡，以及不费力就能抵达的意象。"
    })
    .to_string()
}

pub(crate) fn liqingzhao_attributes_json() -> String {
    json!(["身份：词意裁判", "长处：情感精读", "情绪：敏锐"]).to_string()
}

pub(crate) fn liqingzhao_custom_tabs_json() -> String {
    json!({
        "辨味": "她能立刻分辨一句话里的感情究竟真切、做作、闪躲，还是锋利到位。"
    })
    .to_string()
}

pub(crate) fn sushi_attributes_json() -> String {
    json!(["身份：豁达主人", "长处：格局开阔", "情绪：温厚"]).to_string()
}

pub(crate) fn sushi_custom_tabs_json() -> String {
    json!({
        "开阔": "他总能把一场局促的对答重新铺成情分、眼界与新的兴致。"
    })
    .to_string()
}

pub(crate) fn seed_rule_effects_json() -> String {
    json!([
        {
            "type": "message",
            "text": SEED_RULE_EFFECT_MESSAGE
        },
        {
            "type": "add_tag",
            "tag": "heated-atmosphere"
        }
    ])
    .to_string()
}
