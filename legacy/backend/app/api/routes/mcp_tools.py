import json
import re
import sqlite3
import uuid

from fastapi import APIRouter, HTTPException

from backend.app.api.schemas.mcp_tools import McpToolResponse, McpToolUpsertRequest
from backend.app.core.config import Settings

router = APIRouter(prefix="/api/mcp/tools", tags=["mcp-tools"])


def _connect() -> sqlite3.Connection:
    connection = sqlite3.connect(Settings().database_path)
    connection.row_factory = sqlite3.Row
    connection.execute(
        """
        CREATE TABLE IF NOT EXISTS mcp_tools (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL,
            server_name TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            exposure_policy TEXT NOT NULL DEFAULT 'on-demand',
            risk_level TEXT NOT NULL DEFAULT 'low',
            trigger_keywords_json TEXT NOT NULL DEFAULT '[]'
        )
        """
    )
    _ensure_builtin_tools(connection)
    return connection


def _ensure_builtin_tools(connection: sqlite3.Connection) -> None:
    builtin_tools = [
        (
            "mcp-tool-image-generation",
            "文生图",
            "根据世界主控给出的视觉提示词生成背景图或人物立绘，并把生成资产写入会话存档。",
            "builtin-image-generation",
            "generate_image",
            1,
            "on-demand",
            "medium",
            ["背景图", "立绘", "文生图", "生成图片", "场景图"],
        ),
        (
            "mcp-tool-list-scenes",
            "查询场景",
            "查询当前世界已有场景、地图节点和当前会话场景，供世界主控切换场景前参考。",
            "builtin-world-director",
            "list_scenes",
            1,
            "on-demand",
            "low",
            ["场景", "地图", "地点", "查询场景"],
        ),
        (
            "mcp-tool-list-characters",
            "查询角色",
            "查询当前世界已有角色、角色身份和在场状态，供世界主控安排 NPC 前参考。",
            "builtin-world-director",
            "list_characters",
            1,
            "on-demand",
            "low",
            ["角色", "人物", "NPC", "查询角色"],
        ),
        (
            "mcp-tool-change-scene",
            "切换场景",
            "由世界主控显式切换场景、填写场景描述、新增人物、在场人物和玩家操控人物。",
            "builtin-world-director",
            "change_scene",
            1,
            "on-demand",
            "medium",
            ["切换场景", "转世", "换身份", "进入场景", "新增人物"],
        ),
        (
            "mcp-tool-switch-player-character",
            "切换玩家角色",
            "由世界主控显式把玩家操控角色切换到当前世界已有角色，不强制切换场景。",
            "builtin-world-director",
            "switch_player_character",
            1,
            "on-demand",
            "medium",
            ["切换玩家", "切换角色", "换主角", "换身份", "操控角色", "附身"],
        ),
    ]
    connection.executemany(
        """
        INSERT OR IGNORE INTO mcp_tools (
            id, name, description, server_name, tool_name, enabled, exposure_policy, risk_level, trigger_keywords_json
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        [(*item[:8], json.dumps(item[8], ensure_ascii=False)) for item in builtin_tools],
    )


def _normalize_id(name: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")
    return f"mcp-tool-{slug or uuid.uuid4().hex[:8]}"


def _normalize_policy(value: str) -> str:
    return value if value in {"on-demand", "manual-only", "disabled"} else "on-demand"


def _normalize_risk(value: str) -> str:
    return value if value in {"low", "medium", "high"} else "low"


def _keywords(values: list[str]) -> list[str]:
    return list(dict.fromkeys(str(item).strip() for item in values if str(item).strip()))


def _row_to_response(row: sqlite3.Row) -> McpToolResponse:
    return McpToolResponse(
        id=row["id"],
        name=row["name"],
        description=row["description"],
        server_name=row["server_name"],
        tool_name=row["tool_name"],
        enabled=bool(row["enabled"]),
        exposure_policy=row["exposure_policy"],
        risk_level=row["risk_level"],
        trigger_keywords=json.loads(row["trigger_keywords_json"]),
    )


@router.get("", response_model=list[McpToolResponse])
def list_mcp_tools():
    with _connect() as connection:
        rows = connection.execute("SELECT * FROM mcp_tools ORDER BY enabled DESC, name").fetchall()
    return [_row_to_response(row) for row in rows]


@router.post("", response_model=McpToolResponse)
def create_mcp_tool(payload: McpToolUpsertRequest):
    tool_id = _normalize_id(payload.name)
    with _connect() as connection:
        candidate = tool_id
        while connection.execute("SELECT 1 FROM mcp_tools WHERE id = ?", (candidate,)).fetchone():
            candidate = f"{tool_id}-{uuid.uuid4().hex[:4]}"
        connection.execute(
            """
            INSERT INTO mcp_tools (
                id, name, description, server_name, tool_name, enabled, exposure_policy, risk_level, trigger_keywords_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                candidate,
                payload.name.strip(),
                payload.description.strip(),
                payload.server_name.strip(),
                payload.tool_name.strip(),
                1 if payload.enabled else 0,
                _normalize_policy(payload.exposure_policy),
                _normalize_risk(payload.risk_level),
                json.dumps(_keywords(payload.trigger_keywords), ensure_ascii=False),
            ),
        )
        row = connection.execute("SELECT * FROM mcp_tools WHERE id = ?", (candidate,)).fetchone()
    return _row_to_response(row)


@router.put("/{tool_id}", response_model=McpToolResponse)
def update_mcp_tool(tool_id: str, payload: McpToolUpsertRequest):
    with _connect() as connection:
        row = connection.execute("SELECT * FROM mcp_tools WHERE id = ?", (tool_id,)).fetchone()
        if row is None:
            raise HTTPException(status_code=404, detail="MCP tool not found")
        connection.execute(
            """
            UPDATE mcp_tools
            SET name = ?, description = ?, server_name = ?, tool_name = ?, enabled = ?, exposure_policy = ?, risk_level = ?, trigger_keywords_json = ?
            WHERE id = ?
            """,
            (
                payload.name.strip(),
                payload.description.strip(),
                payload.server_name.strip(),
                payload.tool_name.strip(),
                1 if payload.enabled else 0,
                _normalize_policy(payload.exposure_policy),
                _normalize_risk(payload.risk_level),
                json.dumps(_keywords(payload.trigger_keywords), ensure_ascii=False),
                tool_id,
            ),
        )
        updated = connection.execute("SELECT * FROM mcp_tools WHERE id = ?", (tool_id,)).fetchone()
    return _row_to_response(updated)


@router.delete("/{tool_id}")
def delete_mcp_tool(tool_id: str):
    with _connect() as connection:
        row = connection.execute("SELECT id FROM mcp_tools WHERE id = ?", (tool_id,)).fetchone()
        if row is None:
            raise HTTPException(status_code=404, detail="MCP tool not found")
        connection.execute("DELETE FROM mcp_tools WHERE id = ?", (tool_id,))
    return {"ok": True}
