from dataclasses import dataclass, field


@dataclass(frozen=True)
class MemoryEntry:
    id: str
    world_id: str
    session_id: str
    character_id: str
    layer: str
    content: str
    source: str
    importance: float
    created_at: str
    turn_index: int = 0
    conversation_id: str | None = None
    event_id: str | None = None
    item_id: str | None = None
    scene_id: str | None = None
    memory_type: str = "dialogue"
    speaker: str | None = None
    role: str | None = None
    location: str | None = None
    participants: list[str] = field(default_factory=list)
    keywords: list[str] = field(default_factory=list)
