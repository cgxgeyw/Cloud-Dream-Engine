from dataclasses import dataclass, field
from typing import Any

from backend.app.domain.models.asset import AssetSelection
from backend.app.domain.models.inventory import InventoryItem
from backend.app.domain.models.scene import SceneRuntime
from backend.app.domain.models.state import SessionState


@dataclass(frozen=True)
class ChatMessage:
    role: str
    content: str
    speaker: str | None = None
    metadata: dict[str, object] | None = None


ContentPart = dict[str, Any]
MessageInput = str | list[ContentPart]


def extract_message_text(content: MessageInput) -> str:
    if isinstance(content, str):
        return content.strip()
    text_parts: list[str] = []
    media_counts = {"image": 0, "audio": 0}
    for part in content:
        if not isinstance(part, dict):
            continue
        part_type = str(part.get("type") or "")
        if part_type == "text":
            text = str(part.get("text") or "").strip()
            if text:
                text_parts.append(text)
        elif part_type == "image_url":
            media_counts["image"] += 1
        elif part_type in {"input_audio", "audio_url"}:
            media_counts["audio"] += 1
    media_labels = []
    if media_counts["image"]:
        media_labels.append(f"{media_counts['image']} 张图片")
    if media_counts["audio"]:
        media_labels.append(f"{media_counts['audio']} 段音频")
    if media_labels:
        text_parts.append(f"[附件：{'，'.join(media_labels)}]")
    return "\n".join(text_parts).strip()


def extract_media_parts(content: MessageInput) -> list[ContentPart]:
    if not isinstance(content, list):
        return []
    media_parts: list[ContentPart] = []
    for part in content:
        if not isinstance(part, dict):
            continue
        part_type = str(part.get("type") or "")
        if part_type in {"image_url", "input_audio", "audio_url"}:
            media_parts.append(part)
    return media_parts


@dataclass(frozen=True)
class SessionMapNode:
    node_id: str
    label: str
    discovered: bool = True
    current: bool = False


@dataclass(frozen=True)
class SessionMapEdge:
    edge_id: str
    source_node_id: str
    target_node_id: str


@dataclass(frozen=True)
class SessionSnapshot:
    id: str
    world_name: str
    location: str
    time_label: str
    current_speaker: str
    current_line: str
    player_character_id: str | None = None
    player_character_name: str | None = None
    visible_characters: list[str] = field(default_factory=list)
    messages: list[ChatMessage] = field(default_factory=list)
    player_stats: list[str] = field(default_factory=list)
    map_graph_nodes: list[SessionMapNode] = field(default_factory=list)
    map_graph_edges: list[SessionMapEdge] = field(default_factory=list)
    inventory_items: list[InventoryItem] = field(default_factory=list)
    system_log: list[str] = field(default_factory=list)
    scene: SceneRuntime = field(default_factory=lambda: SceneRuntime(scene_id="default", name="default", background_hint="default"))
    assets: AssetSelection = field(default_factory=lambda: AssetSelection(background_hint="default", active_speaker_portrait="default"))
    state: SessionState = field(default_factory=SessionState)
