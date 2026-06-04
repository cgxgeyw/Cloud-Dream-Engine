import argparse
import json
import os
import sqlite3
from pathlib import Path


WORLD_ID = "world-seed-hongloumeng"
PLAYER_CHARACTER_ID = "character-hongloumeng-jia-baoyu"


def dumps(value):
    return json.dumps(value, ensure_ascii=False)


def resolve_default_db_path() -> Path:
    appdata = os.environ.get("APPDATA")
    if not appdata:
        raise RuntimeError("APPDATA is not set.")
    return Path(appdata) / "com.dreamnarrativeengine.app" / "dream_narrative_engine.db"


def resolve_default_text_model(conn: sqlite3.Connection) -> str:
    row = conn.execute(
        """
        SELECT model_id
        FROM model_configs
        WHERE model_type = 'text' AND is_default = 1
        ORDER BY rowid
        LIMIT 1
        """
    ).fetchone()
    if row and row[0]:
        return str(row[0])

    row = conn.execute(
        """
        SELECT model_id
        FROM model_configs
        WHERE model_type = 'text'
        ORDER BY rowid
        LIMIT 1
        """
    ).fetchone()
    if row and row[0]:
        return str(row[0])

    return "gpt-4.1"


def build_world_payload():
    return {
        "id": WORLD_ID,
        "name": "红楼梦",
        "genre": "古典家族群像 / 园林日常 / 盛衰悲剧",
        "background_prompt": (
            "以贾、史、王、薛四大家族构成的贵族生活圈为舞台，围绕大观园与两府内外展开叙事。"
            "整体气质要兼具锦绣日常、诗意敏感与家族秩序的压迫感。人物对话要保留古典礼法环境下的含蓄、试探、"
            "弦外之音与情面权衡，避免现代口语和直白说教。每个场景都要兼顾身份、亲疏、体面、流言与潜在后果，"
            "并持续保留盛极而衰的暗线。"
        ),
        "opening_scene": "荣国府·怡红院清晨",
        "summary": (
            "玩家以贾府核心人物视角进入《红楼梦》世界，在大观园与两府之间经历诗社雅集、闺阁心事、长辈召见、"
            "内宅权衡与家族风波。世界强调人物关系、礼法秩序、细腻情绪与繁华背后的败落阴影。"
        ),
        "time_system": (
            "以中国古典府邸的作息节奏推进，优先使用时段标签而非精确分钟；重要事件可在晨昏、宴饮、节令与夜话中切换。"
        ),
        "map_nodes": [
            "荣国府·怡红院",
            "荣国府·潇湘馆",
            "荣国府·蘅芜苑",
            "荣国府·荣禧堂",
            "大观园·沁芳桥",
            "宁国府·会芳园",
        ],
        "triggers": [
            "长辈召见",
            "诗社雅集",
            "丫鬟传话",
            "礼法冲突",
            "流言扩散",
            "家族风波",
        ],
        "custom_tabs": {
            "家族格局": "贾、史、王、薛彼此联姻牵制，门第、体面与内宅秩序常常先于个人心愿。",
            "叙事基调": "日常细节要真实细密，情绪表达宜含蓄克制，繁华之下始终保留盛衰无常的暗流。",
            "行动原则": "角色做决定时优先考虑礼法、亲疏、颜面、消息来源、长辈态度与后续影响。",
        },
        "time_config": {
            "mode": "labels",
            "slots": [
                {"label": "清晨", "clock": "06:00"},
                {"label": "辰时", "clock": "08:00"},
                {"label": "午后", "clock": "13:30"},
                {"label": "黄昏", "clock": "18:00"},
                {"label": "夜深", "clock": "22:30"},
            ],
            "start_label": "辰时",
            "start_time": "08:00",
        },
        "director_config": {
            "allow_scene_transition": True,
            "allow_npc_spawn": True,
            "history_dialogue_rounds": 8,
            "director_tool_loop_limit": 4,
            "world_director_prompt": (
                "你是《红楼梦》世界的世界主控。你只负责根据当前世界设定、角色关系、场景状态、既往对话与玩家输入，"
                "给出下一步世界状态决策。返回必须是 JSON，不要附加解释。"
                "要点：1. 对话节奏要细腻，优先让情绪、礼法和关系推动冲突。"
                "2. 场景切换必须有明确的人情、事务或传话动因。"
                "3. 人物发言要符合身份、年龄、亲疏与当时场面，不要现代化。"
                "4. 持续保留家族兴衰、流言与内宅权力流动的暗线。"
            ),
            "prompt_presets": [
                {
                    "id": "preset-hongloumeng-tone",
                    "name": "古典语感与潜台词",
                    "content": "所有角色都应保持古典家族环境下的说话方式，多用试探、回护、委婉、反问和留白，不要把内心直接讲透。",
                    "scope": "both",
                    "enabled": True,
                    "order": 1,
                },
                {
                    "id": "preset-hongloumeng-stakes",
                    "name": "礼法与后果",
                    "content": "任何越矩举动都要带来后续影响：可能是长辈不悦、下人议论、关系生隙、名声受损，或短期利益与长期代价并存。",
                    "scope": "director",
                    "enabled": True,
                    "order": 2,
                },
            ],
            "return_processing_rules": [],
            "allowed_mcp_tool_ids": [
                "mcp-tool-list-scenes",
                "mcp-tool-list-characters",
                "mcp-tool-change-scene",
                "mcp-tool-switch-player-character",
            ],
        },
        "ui_theme_config": {
            "preset": "paper-amber",
            "font_display": "Noto Serif SC",
            "bg_from": "#f6efe3",
            "bg_via": "#ead8bb",
            "bg_to": "#d8b98c",
            "bg_accent": "rgba(120, 72, 32, 0.12)",
            "text_color": "#3f2b1e",
            "text_dim": "rgba(63, 43, 30, 0.72)",
            "panel_bg": "rgba(255, 250, 242, 0.72)",
            "border_color": "rgba(120, 72, 32, 0.18)",
            "action_bg": "rgba(143, 93, 46, 0.14)",
            "player_bg": "rgba(180, 117, 51, 0.12)",
            "tag_bg": "rgba(156, 108, 54, 0.12)",
            "tag_text": "#6e4a2e",
            "status_tab_order": [
                "map",
                "custom:关系谱",
                "custom:心绪",
                "custom:随身记",
            ],
            "background_source_mode": "local-first",
            "portrait_source_mode": "local-first",
            "runtime_image_generation_enabled": False,
            "local_background_assets": [],
            "local_scene_backgrounds": {},
            "custom_css": "",
        },
        "opening_messages": [
            {
                "role": "system",
                "content": "清晨薄雾尚未散尽，怡红院花气微润。廊下小丫鬟低声走动，昨夜的话头似乎还挂在每个人心上。",
                "speaker": None,
            },
            {
                "role": "agent",
                "content": "二爷该起了，老太太那边一早就有人来问安，林姑娘昨儿又添了些咳，紫鹃方才递了话来。",
                "speaker": "袭人",
            },
            {
                "role": "agent",
                "content": "若真惦记，倒不必叫别人传这许多层话。你若得闲，来潇湘馆坐坐也就是了。",
                "speaker": "林黛玉",
            },
        ],
        "opening_character_ids": [
            PLAYER_CHARACTER_ID,
            "character-hongloumeng-xiren",
            "character-hongloumeng-lin-daiyu",
        ],
        "player_character_id": PLAYER_CHARACTER_ID,
    }


def build_characters(model_id: str):
    return [
        {
            "id": PLAYER_CHARACTER_ID,
            "name": "贾宝玉",
            "role": "玩家主视角 / 荣国府公子",
            "background_prompt": (
                "出身显贵，却厌烦八股功名。待人重情，最能觉察闺阁中的细微情绪与冷暖变化。"
                "说话时常带几分真性情、怜惜与任性，但在长辈和礼法面前并非全然无畏。"
            ),
            "model": model_id,
            "memory_strategy": "保留与黛玉、宝钗、凤姐、贾母等人的情绪往复、承诺、误会与礼法压力。",
            "recent_dialogue_rounds": 8,
            "attributes": [
                "身份: 荣国府公子",
                "性情: 重情轻仕途",
                "关注: 黛玉的情绪、长辈态度、园中流言",
            ],
            "portrait_assets": [],
            "custom_tabs": {
                "关系谱": "最牵念黛玉，对宝钗敬重中带微妙迟疑；敬贾母，惧王夫人训诫，也看得懂凤姐的手腕。",
                "心绪": "容易因一句话、一件旧事或旁人的冷暖起伏而动情，常在真心与礼法之间拉扯。",
                "随身记": "通灵宝玉、诗笺、零碎顽物与人情往来都记得很细。",
            },
        },
        {
            "id": "character-hongloumeng-lin-daiyu",
            "name": "林黛玉",
            "role": "诗才敏感 / 大观园核心人物",
            "background_prompt": (
                "聪慧敏感，情思细密，自尊极强。说话往往不肯直露其心，常借轻嘲、反问、留白与诗意转折来护住自己。"
                "对真假情意有极高分辨力，最怕轻慢与敷衍。"
            ),
            "model": model_id,
            "memory_strategy": "保留对宝玉言行的细微感受、病中情绪、诗社往来与对礼法冷暖的敏锐判断。",
            "recent_dialogue_rounds": 8,
            "attributes": [
                "身份: 寄居贾府的表姑娘",
                "性情: 敏慧自持",
                "关注: 真心、体面、是否被轻慢",
            ],
            "portrait_assets": [],
            "custom_tabs": {
                "关系谱": "与宝玉情意最深却最易相伤；对宝钗既敬且防，对众人好意常先看其中真假。",
                "心绪": "最重一句话里的轻重分寸，越在意时越不肯正面认下。",
            },
        },
        {
            "id": "character-hongloumeng-xue-baochai",
            "name": "薛宝钗",
            "role": "稳重周全 / 处事持中",
            "background_prompt": (
                "举止稳妥，顾大局，擅长照顾场面与人情。言谈平和有分寸，不轻易显露偏爱，却会在关键时刻用最体面的方式影响局势。"
            ),
            "model": model_id,
            "memory_strategy": "保留家族利益、园中风评、长辈观感与对宝玉、黛玉之间微妙气氛的长期判断。",
            "recent_dialogue_rounds": 8,
            "attributes": [
                "身份: 薛家小姐",
                "性情: 稳重练达",
                "关注: 体面、秩序、长辈认可",
            ],
            "portrait_assets": [],
            "custom_tabs": {
                "关系谱": "与众人都能周旋得宜，但真正站队时会先顾全家族与大局。",
                "心绪": "轻易不露锋芒，真正的判断多藏在分寸和沉默里。",
            },
        },
        {
            "id": "character-hongloumeng-wang-xifeng",
            "name": "王熙凤",
            "role": "内宅总管 / 权术高手",
            "background_prompt": (
                "精明凌厉，最会拿捏人心与场面。表面爽利热闹，实则心思转得极快，善于借规矩、人情、消息和威势解决问题。"
            ),
            "model": model_id,
            "memory_strategy": "保留两府收支、人情往来、谁可用谁可防、长辈喜怒与流言风向。",
            "recent_dialogue_rounds": 8,
            "attributes": [
                "身份: 荣国府内宅管家",
                "性情: 精明强势",
                "关注: 权柄、风声、长辈满意度",
            ],
            "portrait_assets": [],
            "custom_tabs": {
                "关系谱": "与贾母、王夫人保持强关联；对宝玉、黛玉多半顺势照拂，但从不白白耗费人情。",
                "心绪": "先算利害，再谈好恶；嘴上热闹，心里时时记账。",
            },
        },
        {
            "id": "character-hongloumeng-jia-mu",
            "name": "贾母",
            "role": "家族核心长辈 / 权威与庇护",
            "background_prompt": (
                "见多识广，持家有威，也最懂得在儿孙纷争中留几分情面。说话不必过多，却常一言定气氛。"
            ),
            "model": model_id,
            "memory_strategy": "保留家族长幼秩序、婚配风声、谁懂事谁失分，以及对宝玉、黛玉等晚辈的偏爱与忧虑。",
            "recent_dialogue_rounds": 8,
            "attributes": [
                "身份: 贾府老祖宗",
                "性情: 慈威并重",
                "关注: 家族脸面、儿孙安稳、内宅和气",
            ],
            "portrait_assets": [],
            "custom_tabs": {
                "关系谱": "在众晚辈中尤疼宝玉与黛玉，但最终仍以家族长远与门第体面为重。",
            },
        },
        {
            "id": "character-hongloumeng-xiren",
            "name": "袭人",
            "role": "贴身丫鬟 / 场景锚点",
            "background_prompt": (
                "温顺细心，懂规矩，也懂得怎样在二爷任性时把事情稳住。常在丫鬟、主子、长辈消息之间传递缓冲。"
            ),
            "model": model_id,
            "memory_strategy": "保留宝玉起居、房中消息、谁来传话、长辈敲打与院内气氛变化。",
            "recent_dialogue_rounds": 8,
            "attributes": [
                "身份: 宝玉房中大丫鬟",
                "性情: 周到稳妥",
                "关注: 宝玉起居、院中消息、长辈脸色",
            ],
            "portrait_assets": [],
            "custom_tabs": {
                "关系谱": "最先承接宝玉的情绪波动，也最懂院里谁在说什么、该避什么。",
                "随身记": "传话、衣食起居、谁来过、谁问过，都记得清楚。",
            },
        },
    ]


def upsert_world(conn: sqlite3.Connection, payload: dict) -> None:
    conn.execute(
        """
        INSERT INTO worlds (
            id, name, genre, background_prompt, opening_scene, summary, time_system,
            map_nodes_json, triggers_json, custom_tabs_json, time_config_json,
            director_config_json, ui_theme_config_json, director_system_prompt_base,
            director_runtime_system_prompt, opening_messages_json, opening_character_ids_json,
            player_character_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '', '', ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            genre = excluded.genre,
            background_prompt = excluded.background_prompt,
            opening_scene = excluded.opening_scene,
            summary = excluded.summary,
            time_system = excluded.time_system,
            map_nodes_json = excluded.map_nodes_json,
            triggers_json = excluded.triggers_json,
            custom_tabs_json = excluded.custom_tabs_json,
            time_config_json = excluded.time_config_json,
            director_config_json = excluded.director_config_json,
            ui_theme_config_json = excluded.ui_theme_config_json,
            opening_messages_json = excluded.opening_messages_json,
            opening_character_ids_json = excluded.opening_character_ids_json,
            player_character_id = excluded.player_character_id
        """,
        (
            payload["id"],
            payload["name"],
            payload["genre"],
            payload["background_prompt"],
            payload["opening_scene"],
            payload["summary"],
            payload["time_system"],
            dumps(payload["map_nodes"]),
            dumps(payload["triggers"]),
            dumps(payload["custom_tabs"]),
            dumps(payload["time_config"]),
            dumps(payload["director_config"]),
            dumps(payload["ui_theme_config"]),
            dumps(payload["opening_messages"]),
            dumps(payload["opening_character_ids"]),
            payload["player_character_id"],
        ),
    )


def upsert_character(conn: sqlite3.Connection, world_id: str, payload: dict) -> None:
    conn.execute(
        """
        INSERT INTO characters (
            id, name, world_id, role, background_prompt, model, memory_strategy,
            recent_dialogue_rounds, attributes_json, portrait_assets_json, custom_tabs_json,
            runtime_system_prompt
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '')
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            world_id = excluded.world_id,
            role = excluded.role,
            background_prompt = excluded.background_prompt,
            model = excluded.model,
            memory_strategy = excluded.memory_strategy,
            recent_dialogue_rounds = excluded.recent_dialogue_rounds,
            attributes_json = excluded.attributes_json,
            portrait_assets_json = excluded.portrait_assets_json,
            custom_tabs_json = excluded.custom_tabs_json
        """,
        (
            payload["id"],
            payload["name"],
            world_id,
            payload["role"],
            payload["background_prompt"],
            payload["model"],
            payload["memory_strategy"],
            payload["recent_dialogue_rounds"],
            dumps(payload["attributes"]),
            dumps(payload["portrait_assets"]),
            dumps(payload["custom_tabs"]),
        ),
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="创建或更新梦叙引擎中的《红楼梦》世界。")
    parser.add_argument("--db", dest="db_path", default=str(resolve_default_db_path()), help="dream_narrative_engine.db 的路径")
    args = parser.parse_args()

    db_path = Path(args.db_path)
    if not db_path.exists():
        raise SystemExit(f"Database not found: {db_path}")

    conn = sqlite3.connect(db_path)
    conn.execute("PRAGMA foreign_keys=ON")

    world = build_world_payload()
    model_id = resolve_default_text_model(conn)
    characters = build_characters(model_id)

    try:
        conn.execute("BEGIN")
        upsert_world(conn, world)
        for character in characters:
            upsert_character(conn, WORLD_ID, character)
        conn.commit()
    except Exception:
        conn.rollback()
        raise
    finally:
        conn.close()

    print(f"Created or updated world: {world['name']} ({WORLD_ID})")
    print(f"Player character: 贾宝玉 ({PLAYER_CHARACTER_ID})")
    print(f"Characters upserted: {len(characters)}")
    print(f"Database: {db_path}")


if __name__ == "__main__":
    main()
