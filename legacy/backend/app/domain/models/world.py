from dataclasses import dataclass, field
from typing import Any


DEFAULT_WORLD_DIRECTOR_PROMPT = """你是世界主控。你只根据玩家可编辑提示词、世界资料、当前状态、聊天记录和工具资料做状态决策。

你需要返回一个 JSON 对象，用于描述下一步世界状态。常用字段包括：
- world_phase
- next_scene_name
- next_time_label
- scene_visible_characters
- planned_speakers
- generated_characters
- character_visual_directives
- background_generation_prompt
- tool_calls

如果需要调用工具，返回 tool_calls；拿到工具执行结果后，再返回最终状态决策。
tool_calls 中每一项必须使用 {"tool_name": 工具名, "arguments": 参数对象}，不要使用 tool_id 或 input 作为返回字段。
当玩家明确要求转世、附身、扮演、成为某个具体人物时，必须优先通过工具把玩家操控人物切换到该人物：若该人物已存在，调用 switch_player_character；若该人物需要随新场景创建，调用 change_scene 并把 player_character 填为该人物。
不要在这里写角色台词。角色发言由角色自己的提示词决定。"""


DEFAULT_CHARACTER_PRESET_PROMPT = """角色返回一个 JSON 对象，字段为：
- speaker：说话角色名
- content：角色本轮发言或动作
- intent：本轮意图
- emotion：当前情绪

请只返回 JSON，不要返回 Markdown 代码块。"""


DEFAULT_PROMPT_PRESETS: list[dict[str, Any]] = [
    {
        "id": "default-character-output",
        "name": "角色通用返回格式",
        "content": DEFAULT_CHARACTER_PRESET_PROMPT,
        "scope": "character",
        "enabled": True,
        "order": 10,
    }
]


DEFAULT_WORLD_DIRECTOR_CONFIG: dict[str, Any] = {
    "allow_scene_transition": True,
    "allow_npc_spawn": True,
    "history_dialogue_rounds": 6,
    "world_director_prompt": DEFAULT_WORLD_DIRECTOR_PROMPT,
    "prompt_presets": DEFAULT_PROMPT_PRESETS,
    "return_processing_rules": [],
    "allowed_mcp_tool_ids": [],
}


def normalize_world_director_config(raw: dict[str, Any] | None) -> dict[str, Any]:
    resolved = {
        **DEFAULT_WORLD_DIRECTOR_CONFIG,
        "prompt_presets": [dict(item) for item in DEFAULT_PROMPT_PRESETS],
        "return_processing_rules": [],
        "allowed_mcp_tool_ids": [],
    }
    if not isinstance(raw, dict):
        return resolved

    for key in ("allow_scene_transition", "allow_npc_spawn"):
        value = raw.get(key)
        if isinstance(value, bool):
            resolved[key] = value

    history_dialogue_rounds = raw.get("history_dialogue_rounds")
    if isinstance(history_dialogue_rounds, int):
        resolved["history_dialogue_rounds"] = max(0, min(history_dialogue_rounds, 20))

    prompt = raw.get("world_director_prompt")
    if isinstance(prompt, str):
        resolved["world_director_prompt"] = prompt

    presets = raw.get("prompt_presets")
    if isinstance(presets, list):
        resolved["prompt_presets"] = [
            {
                "id": str(item.get("id") or "").strip() or f"preset-{index + 1}",
                "name": str(item.get("name") or "").strip() or "未命名预设",
                "content": str(item.get("content") or ""),
                "scope": str(item.get("scope") or "both").strip() if str(item.get("scope") or "").strip() in {"director", "character", "both"} else "both",
                "enabled": bool(item.get("enabled", True)),
                "order": int(item.get("order") or index + 1),
            }
            for index, item in enumerate(presets)
            if isinstance(item, dict)
        ]

    rules = raw.get("return_processing_rules")
    if isinstance(rules, list):
        resolved["return_processing_rules"] = [
            {
                "id": str(item.get("id") or "").strip() or f"rule-{index + 1}",
                "name": str(item.get("name") or "").strip() or "未命名规则",
                "scope": str(item.get("scope") or "both").strip() if str(item.get("scope") or "").strip() in {"director", "character", "both"} else "both",
                "pattern": str(item.get("pattern") or ""),
                "replacement": str(item.get("replacement") or ""),
                "enabled": bool(item.get("enabled", True)),
                "order": int(item.get("order") or index + 1),
            }
            for index, item in enumerate(rules)
            if isinstance(item, dict)
        ]

    allowed_tool_ids = raw.get("allowed_mcp_tool_ids")
    if isinstance(allowed_tool_ids, list):
        resolved["allowed_mcp_tool_ids"] = list(
            dict.fromkeys(
                str(item).strip()
                for item in allowed_tool_ids
                if str(item).strip()
            )
        )

    return resolved


@dataclass(frozen=True)
class WorldOpeningMessage:
    role: str
    content: str
    speaker: str | None = None


@dataclass(frozen=True)
class WorldDefinition:
    id: str
    name: str
    genre: str
    background_prompt: str
    opening_scene: str
    summary: str
    time_system: str
    map_nodes: list[str] = field(default_factory=list)
    triggers: list[str] = field(default_factory=list)
    custom_tabs: dict[str, str] = field(default_factory=dict)
    time_config: dict[str, Any] = field(default_factory=dict)
    director_config: dict[str, Any] = field(default_factory=dict)
    ui_theme_config: dict[str, Any] = field(default_factory=dict)
    opening_messages: list[WorldOpeningMessage] = field(default_factory=list)
    opening_character_ids: list[str] = field(default_factory=list)
    player_character_id: str | None = None
